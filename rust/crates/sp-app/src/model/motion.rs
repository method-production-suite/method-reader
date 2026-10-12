//! How the window moves: CSS's timing functions, every animation's
//! duration and curve as the old window's stylesheets and scripts set them
//! (desktop/ui/*.css and page-show.js at git a366d08), and the rule for when
//! the window draws its next frame.
//!
//! The window draws a frame only when something asks for one. While an
//! animation is mid-flight it asks for the very next one, so egui presents
//! on the display's own refresh (120 Hz on a ProMotion Mac), as
//! requestAnimationFrame did; a timer that is due later (the devices poll,
//! the Settings settle, the heads-up's eight seconds) asks to be woken then;
//! and with neither the window sleeps until the OS or a worker wakes it, so
//! a window at rest costs nothing.

/// A CSS `cubic-bezier(x1, y1, x2, y2)`, solved the way WebKit's
/// UnitBezier solves it (Newton's method, then bisection).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// CSS `linear`.
pub const LINEAR: Curve = Curve::new(0.0, 0.0, 1.0, 1.0);
/// CSS `ease`.
pub const EASE: Curve = Curve::new(0.25, 0.1, 0.25, 1.0);
/// CSS `ease-in`.
pub const EASE_IN: Curve = Curve::new(0.42, 0.0, 1.0, 1.0);
/// CSS `ease-out`: tokens.css's `--ease`, the window's one curve.
pub const EASE_OUT: Curve = Curve::new(0.0, 0.0, 0.58, 1.0);
/// CSS `ease-in-out`.
pub const EASE_IN_OUT: Curve = Curve::new(0.42, 0.0, 0.58, 1.0);

impl Curve {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// The curve a CSS timing keyword names.
    pub fn named(word: &str) -> Option<Curve> {
        Some(match word {
            "linear" => LINEAR,
            "ease" => EASE,
            "ease-in" => EASE_IN,
            "ease-out" => EASE_OUT,
            "ease-in-out" => EASE_IN_OUT,
            _ => return None,
        })
    }

    fn coefficients(p1: f64, p2: f64) -> (f64, f64, f64) {
        let c = 3.0 * p1;
        let b = 3.0 * (p2 - p1) - c;
        let a = 1.0 - c - b;
        (a, b, c)
    }

    fn sample(p1: f64, p2: f64, t: f64) -> f64 {
        let (a, b, c) = Self::coefficients(p1, p2);
        ((a * t + b) * t + c) * t
    }

    fn slope_x(&self, t: f64) -> f64 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        (3.0 * a * t + 2.0 * b) * t + c
    }

    /// The parameter t whose x is `x`.
    fn solve_x(&self, x: f64) -> f64 {
        const EPSILON: f64 = 1e-7;
        let mut t = x;
        for _ in 0..8 {
            let err = Self::sample(self.x1, self.x2, t) - x;
            if err.abs() < EPSILON {
                return t;
            }
            let d = self.slope_x(t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        t = x;
        while lo < hi {
            let v = Self::sample(self.x1, self.x2, t);
            if (v - x).abs() < EPSILON {
                return t;
            }
            if x > v {
                lo = t;
            } else {
                hi = t;
            }
            let next = (lo + hi) / 2.0;
            if (next - t).abs() < f64::EPSILON {
                break;
            }
            t = next;
        }
        t
    }

    /// The eased progress at `x` (time's fraction, clamped to 0..=1).
    pub fn at(&self, x: f64) -> f64 {
        if x.is_nan() || x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        Self::sample(self.y1, self.y2, self.solve_x(x))
    }
}

/// One animation's clock: how long, after what delay, along which curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub ms: f64,
    pub delay: f64,
    pub curve: Curve,
}

impl Timing {
    pub const fn new(ms: f64, curve: Curve) -> Self {
        Self {
            ms,
            delay: 0.0,
            curve,
        }
    }

    pub const fn delayed(self, delay: f64) -> Self {
        Self { delay, ..self }
    }

    /// Eased progress `elapsed` ms after the start, 0 through the delay.
    pub fn progress(&self, elapsed: f64) -> f32 {
        if self.ms <= 0.0 {
            return 1.0;
        }
        self.curve.at((elapsed - self.delay) / self.ms) as f32
    }

    /// Whether it is still moving `elapsed` ms after the start.
    pub fn running(&self, elapsed: f64) -> bool {
        elapsed < self.delay + self.ms
    }
}

/// tokens.css: `--motion-press: 0.1s`, `--motion-state: 0.2s`, `--ease`.
pub const PRESS_MS: f64 = 100.0;
pub const STATE_MS: f64 = 200.0;
pub const STATE: Timing = Timing::new(STATE_MS, EASE_OUT);

/// style.css `tab-arrive`: fade and rise 5px, `--motion-state --ease`,
/// the third tab 70 ms later (`.tab:nth-child(3)`).
pub const TAB_ARRIVE: Timing = STATE;
pub const TAB_STAGGER_MS: f64 = 70.0;
pub const TAB_RISE: f32 = 5.0;
/// surfaces.css `heads-up-in`: from translateY(-50%) and transparent.
pub const HEADS_UP_IN: Timing = STATE;
/// style.css `sheet-rise`: the release notes rise 16px as they fade in.
pub const SHEET_RISE: Timing = STATE;
pub const SHEET_RISE_PX: f32 = 16.0;
/// surfaces.css `.scene-rail`: slides 0.8rem (space-3) and fades.
pub const INDEX_SLIDE: Timing = STATE;
/// surfaces.css `.knob-group-title::before`: the triangle turns 90deg.
pub const GROUP_TURN: Timing = STATE;
/// surfaces.css `.fill`: the progress bar's width.
pub const PROGRESS_FILL: Timing = STATE;
/// surfaces.css `build-sweep`: 1.4s ease-in-out infinite, a 30% bar from
/// translateX(-100%) to translateX(334%) of its own width.
pub const BUILD_SWEEP: Timing = Timing::new(1400.0, EASE_IN_OUT);
/// The same under Reduce Motion (`build-breathe`): 1.6s ease-in-out
/// infinite alternate, opacity 0.35 to 1.
pub const BUILD_BREATHE: Timing = Timing::new(1600.0, EASE_IN_OUT);
/// page-show.js: `page-show-glow ${GLOW_MS}ms ease-out`.
pub const GLOW: Timing = Timing::new(super::page_timing::GLOW_MS, EASE_OUT);
/// surfaces.css `.page-show-frame`: `opacity 0.25s var(--ease)`.
pub const PAGE_LIFT: Timing = Timing::new(super::page_show::FADE_MS, EASE_OUT);

/// The build sweep's bar, as a fraction of the track: its left edge and its
/// width, `now` ms into the run.
pub fn sweep_at(now: f64) -> (f32, f32) {
    let width = 0.3;
    let k = BUILD_SWEEP.progress(now.rem_euclid(BUILD_SWEEP.ms));
    // translateX(-100%) to translateX(334%) of the bar's own width.
    let left = width * (-1.0 + 4.34 * k);
    (left, width)
}

/// The track's opacity under Reduce Motion, `now` ms in: there and back.
pub fn breathe_at(now: f64) -> f32 {
    let cycle = now.rem_euclid(2.0 * BUILD_BREATHE.ms);
    let x = if cycle < BUILD_BREATHE.ms {
        cycle
    } else {
        2.0 * BUILD_BREATHE.ms - cycle
    };
    0.35 + 0.65 * BUILD_BREATHE.progress(x)
}

/// WebKit's smooth scroll (ScrollAnimationSmooth.cpp), what
/// `scrollIntoView({behavior: 'smooth'})` did in the old window: 1000
/// points a second, never longer than 200 ms, ease-in-out.
pub fn scroll_timing(distance: f32) -> Timing {
    let ms = (f64::from(distance.abs()) / 1000.0 * 1000.0).min(200.0);
    Timing::new(ms, EASE_IN_OUT)
}

/// A value easing toward where it was last sent, the way a CSS transition
/// follows a property: a new target starts a fresh transition from wherever
/// the value is now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    from: f32,
    to: f32,
    start: f64,
    timing: Timing,
}

impl Transition {
    /// Resting at `value`.
    pub fn at(value: f32, timing: Timing) -> Self {
        Self {
            from: value,
            to: value,
            start: f64::NEG_INFINITY,
            timing,
        }
    }

    /// Head for `to` from where it is at `now`; at once with `instant`
    /// (Reduce Motion: `transition: none`).
    pub fn set(&mut self, to: f32, now: f64, instant: bool) {
        if instant {
            *self = Self::at(to, self.timing);
            return;
        }
        if (to - self.to).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value(now);
        self.to = to;
        self.start = now;
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn value(&self, now: f64) -> f32 {
        let k = self.timing.progress(now - self.start);
        self.from + (self.to - self.from) * k
    }

    pub fn running(&self, now: f64) -> bool {
        self.start.is_finite() && self.timing.running(now - self.start)
    }
}

/// What the window hands egui for [`Wake::Next`]: any delay shorter than
/// a frame is "the next frame" (egui takes a frame's length off it), and,
/// unlike a zero delay, it is asked for once rather than twice.
pub const NEXT_FRAME: std::time::Duration = std::time::Duration::from_millis(1);

/// What the next frame is owed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wake {
    /// Nothing moves and nothing is due: sleep until something happens.
    Idle,
    /// Something is moving: the very next frame, on the display's refresh.
    Next,
    /// A timer is due this many ms from now.
    In(f64),
}

impl Wake {
    /// The recorder's word for it (framelog.rs).
    pub fn word(self) -> String {
        match self {
            Wake::Idle => "idle".into(),
            Wake::Next => "next".into(),
            Wake::In(ms) => format!("after:{ms:.0}"),
        }
    }
}

/// What this frame learnt about the next: gathered while it draws.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wakes {
    moving: bool,
    due: Option<f64>,
}

impl Wakes {
    /// Something on screen is mid-animation.
    pub fn moving(&mut self) {
        self.moving = true;
    }

    /// Something is due at `at` (the window's clock, ms).
    pub fn at(&mut self, at: f64) {
        self.due = Some(self.due.map_or(at, |d| d.min(at)));
    }

    /// The rule: a moving frame wants the next one; otherwise the earliest
    /// timer, at once if it is already due; otherwise nothing.
    pub fn wake(&self, now: f64) -> Wake {
        if self.moving {
            return Wake::Next;
        }
        match self.due {
            Some(at) if at <= now => Wake::Next,
            Some(at) => Wake::In(at - now),
            None => Wake::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    // Values from the CSS definitions, computed independently (the
    // control points' Bezier evaluated at the parameter whose x matches).
    #[test]
    fn the_css_keywords_solve_like_a_browser() {
        assert!(close(EASE_OUT.at(0.5), 0.6847));
        assert!(close(EASE_IN_OUT.at(0.5), 0.5));
        assert!(close(EASE_IN_OUT.at(0.25), 0.1291));
        assert!(close(EASE.at(0.5), 0.8024));
        assert!(close(EASE_IN.at(0.5), 0.3153));
        assert!(close(LINEAR.at(0.3), 0.3));
        for c in [LINEAR, EASE, EASE_IN, EASE_OUT, EASE_IN_OUT] {
            assert_eq!(c.at(0.0), 0.0);
            assert_eq!(c.at(1.0), 1.0);
            assert_eq!(c.at(-1.0), 0.0);
            assert_eq!(c.at(2.0), 1.0);
            assert_eq!(c.at(f64::NAN), 0.0);
            let mut last = 0.0;
            for i in 0..=100 {
                let y = c.at(f64::from(i) / 100.0);
                assert!(y + 1e-9 >= last, "monotonic");
                last = y;
            }
        }
    }

    #[test]
    fn ease_out_leads_linear_the_whole_way() {
        // What reads as smooth rather than mechanical: by a quarter of the
        // time an ease-out is over a third of the way.
        assert!(EASE_OUT.at(0.25) > 0.37);
        for i in 1..100 {
            let x = f64::from(i) / 100.0;
            assert!(EASE_OUT.at(x) > x);
        }
    }

    fn tokens() -> Vec<(String, String)> {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../brand/tokens.css"
        ))
        .expect("brand/tokens.css");
        text.lines()
            .filter_map(|l| {
                let rest = l.trim().strip_prefix("--")?;
                let (name, value) = rest.split_once(':')?;
                Some((name.into(), value.split(';').next()?.trim().into()))
            })
            .collect()
    }

    // W22: the motion tokens. Every state change in the old window was
    // `var(--motion-state) var(--ease)`.
    #[test]
    fn the_state_timing_is_tokens_css_s() {
        let t = tokens();
        let value = |n: &str| t.iter().find(|(k, _)| k == n).expect(n).1.clone();
        assert_eq!(Curve::named(&value("ease")), Some(STATE.curve));
        assert_eq!(value("motion-state"), format!("{}s", STATE_MS / 1000.0));
        assert_eq!(value("motion-press"), format!("{}s", PRESS_MS / 1000.0));
    }

    // Each animation as the old window's CSS and JS wrote it (a366d08).
    #[test]
    fn every_animation_has_the_old_window_s_duration_and_curve() {
        let state = Timing::new(200.0, EASE_OUT);
        for (what, t) in [
            ("tab-arrive", TAB_ARRIVE),
            ("heads-up-in", HEADS_UP_IN),
            ("sheet-rise", SHEET_RISE),
            (".scene-rail", INDEX_SLIDE),
            (".knob-group-title::before", GROUP_TURN),
            (".fill", PROGRESS_FILL),
        ] {
            assert_eq!(t, state, "{what}");
        }
        assert_eq!(TAB_STAGGER_MS, 70.0);
        assert_eq!(TAB_RISE, 5.0);
        assert_eq!(SHEET_RISE_PX, 16.0);
        assert_eq!(BUILD_SWEEP, Timing::new(1400.0, EASE_IN_OUT));
        assert_eq!(BUILD_BREATHE, Timing::new(1600.0, EASE_IN_OUT));
        assert_eq!(GLOW, Timing::new(600.0, EASE_OUT));
        assert_eq!(PAGE_LIFT, Timing::new(250.0, EASE_OUT));
    }

    #[test]
    fn a_delay_holds_the_start() {
        let t = TAB_ARRIVE.delayed(TAB_STAGGER_MS);
        assert_eq!(t.progress(0.0), 0.0);
        assert_eq!(t.progress(70.0), 0.0);
        assert!(t.progress(170.0) > 0.5);
        assert_eq!(t.progress(270.0), 1.0);
        assert!(t.running(269.0) && !t.running(270.0));
    }

    #[test]
    fn the_sweep_crosses_the_track_and_the_breath_comes_back() {
        let (left, w) = sweep_at(0.0);
        assert!(close(f64::from(left), -0.3) && close(f64::from(w), 0.3));
        let (left, _) = sweep_at(1399.999);
        assert!(close(f64::from(left), 0.3 * 3.34));
        // Half way through, ease-in-out is half way.
        let (mid, _) = sweep_at(700.0);
        assert!(close(f64::from(mid), 0.3 * (-1.0 + 2.17)));
        assert!(close(f64::from(breathe_at(0.0)), 0.35));
        assert!(close(f64::from(breathe_at(1600.0)), 1.0));
        assert!(close(f64::from(breathe_at(3200.0)), 0.35));
        assert!(close(
            f64::from(breathe_at(800.0)),
            f64::from(breathe_at(2400.0))
        ));
    }

    #[test]
    fn a_smooth_scroll_is_webkit_s() {
        assert_eq!(scroll_timing(100.0), Timing::new(100.0, EASE_IN_OUT));
        assert_eq!(scroll_timing(-5000.0), Timing::new(200.0, EASE_IN_OUT));
        assert_eq!(scroll_timing(0.0).progress(0.0), 1.0, "nothing to move");
    }

    #[test]
    fn a_transition_retargets_from_where_it_is() {
        let mut t = Transition::at(0.0, STATE);
        assert!(!t.running(0.0));
        t.set(1.0, 1000.0, false);
        assert!(t.running(1100.0));
        let mid = t.value(1100.0);
        assert!(mid > 0.5 && mid < 1.0, "ease-out is past half at half time");
        t.set(0.0, 1100.0, false);
        assert!((t.value(1100.0) - mid).abs() < 1e-6, "no jump on retarget");
        assert_eq!(t.value(1300.0), 0.0);
        assert!(!t.running(1300.0));
        t.set(1.0, 2000.0, true);
        assert_eq!(t.value(2000.0), 1.0, "Reduce Motion: at once");
        assert!(!t.running(2000.0));
    }

    #[test]
    fn the_window_sleeps_when_nothing_moves_or_is_due() {
        assert_eq!(Wakes::default().wake(0.0), Wake::Idle);
    }

    #[test]
    fn a_moving_frame_asks_for_the_next() {
        let mut w = Wakes::default();
        w.at(5000.0);
        w.moving();
        assert_eq!(w.wake(0.0), Wake::Next);
    }

    #[test]
    fn a_timer_wakes_the_window_when_it_is_due_and_not_before() {
        let mut w = Wakes::default();
        w.at(2300.0);
        w.at(1300.0);
        assert_eq!(w.wake(1000.0), Wake::In(300.0));
        assert_eq!(w.wake(1300.0), Wake::Next);
        assert_eq!(Wake::In(300.0).word(), "after:300");
    }
}
