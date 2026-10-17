//! The one door every engine call goes through, as `desktop/ui/app.js`
//! kept it (`runEngine`, `holdEngine`, `whenIdle`, `retireEngine`):
//! counted work in flight, the moment it last ended, and whether an update
//! has retired the engine.
//!
//! In the Rust app the engine is linked in, so the hazard app.js guarded
//! (a NEW engine answering this window's arguments) is narrower: only what
//! the running copy still starts from its bundle path (the MTP helper, any
//! resource read late) can meet the new bundle. The rule is kept whole
//! anyway (inventory row U7): once the bundle is swapped, no new engine
//! call starts, with the window's own sentence.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// What every engine call is told once an update has been installed
/// (`UPDATED_MESSAGE` in desktop/ui/app.js).
pub const UPDATED_MESSAGE: &str = "Method Reader has been updated. Restart it to keep going.";

/// How long the engine must have been idle before a restart: 500 ms
/// (`ENGINE_QUIET_MS`), which covers the gap between two calls of one job.
pub const ENGINE_QUIET: Duration = Duration::from_millis(500);

/// How long a restart waits for quiet before handing the choice over
/// (`RESTART_WAIT_CAP_MS`, then "Restart now").
pub const RESTART_WAIT_CAP: Duration = Duration::from_secs(120);

#[derive(Default)]
struct State {
    in_flight: u32,
    last_ended: Option<Instant>,
    retired: bool,
}

/// Shared by every caller; cheap to clone.
#[derive(Clone, Default)]
pub struct EngineGate {
    inner: Arc<(Mutex<State>, Condvar)>,
}

/// Held while a counted call (or a hold) runs; dropping it ends the work.
pub struct Ticket {
    gate: Option<EngineGate>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(gate) = self.gate.take() {
            let (lock, cv) = &*gate.inner;
            let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
            s.in_flight = s.in_flight.saturating_sub(1);
            s.last_ended = Some(Instant::now());
            cv.notify_all();
        }
    }
}

impl EngineGate {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// An engine call is starting. Refused once retired; `counted` is
    /// whether it is somebody's work (app.js `countsTowardBusy`: a
    /// conversion, a send, a save, but not the devices poll or a read).
    pub fn enter(&self, counted: bool) -> Result<Ticket, &'static str> {
        let mut s = self.state();
        if s.retired {
            return Err(UPDATED_MESSAGE);
        }
        if !counted {
            return Ok(Ticket { gate: None });
        }
        s.in_flight += 1;
        Ok(Ticket {
            gate: Some(self.clone()),
        })
    }

    /// Count as busy with no call running yet (`holdEngine`): a settings
    /// change still in its settle, a native dialog still open. Holds are
    /// taken even after retirement: they only delay a restart.
    pub fn hold(&self) -> Ticket {
        self.state().in_flight += 1;
        Ticket {
            gate: Some(self.clone()),
        }
    }

    /// The bundle on disk is the new one: refuse every call from now on.
    /// Calls already running finish.
    pub fn retire(&self) {
        self.state().retired = true;
    }

    pub fn retired(&self) -> bool {
        self.state().retired
    }

    /// Whether counted work is running right now ("Restarting after this
    /// finishes…" is said only then).
    pub fn busy(&self) -> bool {
        self.state().in_flight > 0
    }

    /// Wait until no counted work has run for `quiet`, at most `cap`.
    /// True when it settled, false when the cap ran out ("Restart now").
    pub fn wait_quiet(&self, quiet: Duration, cap: Duration) -> bool {
        let deadline = Instant::now() + cap;
        let (lock, cv) = &*self.inner;
        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            let now = Instant::now();
            let ready_at = if s.in_flight > 0 {
                None
            } else {
                Some(s.last_ended.map_or(now, |t| t + quiet))
            };
            if let Some(at) = ready_at
                && at <= now
            {
                return true;
            }
            if now >= deadline {
                return false;
            }
            let until = ready_at.map_or(deadline, |at| at.min(deadline));
            let (next, _) = cv
                .wait_timeout(s, until.saturating_duration_since(now))
                .unwrap_or_else(|p| p.into_inner());
            s = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Duration = Duration::from_millis(40);

    #[test]
    fn a_gate_that_never_ran_anything_is_quiet_at_once() {
        let g = EngineGate::new();
        let t = Instant::now();
        assert!(g.wait_quiet(Q, Duration::from_secs(5)));
        assert!(t.elapsed() < Q);
    }

    #[test]
    fn quiet_means_idle_for_the_whole_period_after_the_last_call() {
        let g = EngineGate::new();
        drop(g.enter(true).unwrap());
        let t = Instant::now();
        assert!(g.wait_quiet(Q, Duration::from_secs(5)));
        assert!(t.elapsed() >= Q - Duration::from_millis(5));
    }

    #[test]
    fn a_call_that_never_ends_stalls_at_the_cap() {
        let g = EngineGate::new();
        let _stuck = g.enter(true).unwrap();
        assert!(g.busy());
        let t = Instant::now();
        assert!(!g.wait_quiet(Q, Duration::from_millis(120)));
        assert!(t.elapsed() >= Duration::from_millis(120));
    }

    #[test]
    fn a_call_ending_on_another_thread_releases_the_wait() {
        let g = EngineGate::new();
        let ticket = g.enter(true).unwrap();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            drop(ticket);
        });
        assert!(g.wait_quiet(Q, Duration::from_secs(5)));
        h.join().unwrap();
    }

    #[test]
    fn uncounted_calls_and_retirement() {
        let g = EngineGate::new();
        let _poll = g.enter(false).unwrap();
        assert!(!g.busy(), "the devices poll is not somebody's work");
        let hold = g.hold();
        assert!(g.busy());
        g.retire();
        assert_eq!(g.enter(true).err(), Some(UPDATED_MESSAGE));
        assert_eq!(g.enter(false).err(), Some(UPDATED_MESSAGE));
        drop(hold);
        assert!(!g.busy());
    }
}
