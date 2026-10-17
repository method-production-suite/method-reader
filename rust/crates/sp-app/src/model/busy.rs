//! Whether the engine is somebody's work right now: what was
//! `desktop/ui/app.js`'s count of counted calls and holds, the quiet period
//! an update's restart waits for, and the refusal once an update has installed.
//!
//! The count lives on the window's thread; an update's run waits for quiet
//! on its own thread. So every counted call and every hold is mirrored into
//! sp-update's [`EngineGate`] as a ticket, and the gate is what the run
//! retires and waits on: the two can never disagree about who is busy.

use sp_update::gate::{EngineGate, Ticket};

/// Idle means nothing counted has run for this long.
pub const ENGINE_QUIET_MS: f64 = 500.0;

/// What every engine call is told once an update has been installed.
pub const UPDATED_MESSAGE: &str = "Method Reader has been updated. Restart it to keep going.";

/// Which calls count toward busy. The read-only ones do not: the devices
/// poll, the KFX probe, the route list, reading app settings, the Kindle
/// file check and reveal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Version,
    Convert,
    Reconvert,
    Devices,
    Eject,
    Send,
    Settings,
    Export,
    KindleCheck,
    KfxStatus,
    KfxInstall,
    AppSettingsRead,
    AppSettingsWrite,
    Reveal,
    Routes,
    Route,
    EmailSetup,
}

pub fn counts_toward_busy(call: Call) -> bool {
    !matches!(
        call,
        Call::Devices
            | Call::KfxStatus
            | Call::Routes
            | Call::AppSettingsRead
            | Call::KindleCheck
            | Call::Reveal
    )
}

/// The count, the holds and the clock. Times are milliseconds on a
/// monotonic clock the caller supplies.
pub struct Busy {
    in_flight: u32,
    last_ended: f64,
    retired: bool,
    next_hold: u64,
    holds: Vec<(u64, Ticket)>,
    calls: Vec<Ticket>,
    gate: EngineGate,
}

impl std::fmt::Debug for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Busy")
            .field("in_flight", &self.in_flight)
            .field("retired", &self.retired())
            .finish()
    }
}

impl Default for Busy {
    fn default() -> Self {
        Self {
            in_flight: 0,
            last_ended: f64::NEG_INFINITY,
            retired: false,
            next_hold: 0,
            holds: Vec::new(),
            calls: Vec::new(),
            gate: EngineGate::new(),
        }
    }
}

/// A hold, released once however many times it is let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold(u64);

impl Busy {
    /// A call is starting. Refused (Err with UPDATED_MESSAGE) once retired;
    /// a refused call is no work and is not counted. Returns whether it is
    /// counted, which the caller hands back to `ended`.
    pub fn starting(&mut self, call: Call) -> Result<bool, &'static str> {
        if self.retired {
            return Err(UPDATED_MESSAGE);
        }
        let counted = counts_toward_busy(call);
        // The gate checks retirement and counts the call under ONE lock:
        // asking `retired()` and then taking a hold left a gap in which the
        // update's thread could retire the gate, find it quiet and restart,
        // with this call starting after all (a hold is taken even when
        // retired).
        let ticket = self.gate.enter(counted).map_err(|_| UPDATED_MESSAGE)?;
        if counted {
            self.in_flight += 1;
            self.calls.push(ticket);
        }
        Ok(counted)
    }

    pub fn ended(&mut self, counted: bool, now: f64) {
        if counted {
            self.calls.pop();
            self.work_ended(now);
        }
    }

    /// Count as busy while no call runs: a surface owes the engine work.
    pub fn hold(&mut self) -> Hold {
        self.in_flight += 1;
        self.next_hold += 1;
        self.holds.push((self.next_hold, self.gate.hold()));
        Hold(self.next_hold)
    }

    /// Let a hold go. A second release does nothing.
    pub fn release(&mut self, hold: Hold, now: f64) {
        if let Some(i) = self.holds.iter().position(|(h, _)| *h == hold.0) {
            self.holds.remove(i);
            self.work_ended(now);
        }
    }

    /// The gate an update's run retires and waits on.
    pub fn gate(&self) -> EngineGate {
        self.gate.clone()
    }

    fn work_ended(&mut self, now: f64) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.last_ended = now;
    }

    /// The instantaneous fact: a counted call or a hold is out.
    pub fn engine_busy(&self) -> bool {
        self.in_flight > 0
    }

    /// Quiet for ENGINE_QUIET_MS: what a restart waits for.
    pub fn idle(&self, now: f64) -> bool {
        self.in_flight == 0 && now - self.last_ended >= ENGINE_QUIET_MS
    }

    /// An update has been installed: refuse every call from now on.
    pub fn retire(&mut self) {
        self.retired = true;
        self.gate.retire();
    }

    /// Retired here, or by an update's run through the gate.
    pub fn retired(&self) -> bool {
        self.retired || self.gate.retired()
    }
}

/// One native dialog at a time, holding the engine while it is up.
#[derive(Debug, Default)]
pub struct Dialogs {
    open: Option<Hold>,
}

impl Dialogs {
    /// May a dialog open now? If so it holds the engine until `closed`.
    pub fn try_open(&mut self, busy: &mut Busy) -> bool {
        if self.open.is_some() {
            return false;
        }
        self.open = Some(busy.hold());
        true
    }

    pub fn closed(&mut self, busy: &mut Busy, now: f64) {
        if let Some(hold) = self.open.take() {
            busy.release(hold, now);
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_read_only_calls_never_count() {
        for call in [
            Call::Devices,
            Call::KfxStatus,
            Call::Routes,
            Call::AppSettingsRead,
            Call::KindleCheck,
            Call::Reveal,
        ] {
            assert!(!counts_toward_busy(call), "{call:?}");
        }
        for call in [
            Call::Convert,
            Call::Eject,
            Call::Send,
            Call::Export,
            Call::KfxInstall,
            Call::AppSettingsWrite,
            Call::Route,
        ] {
            assert!(counts_toward_busy(call), "{call:?}");
        }
    }

    #[test]
    fn idle_means_quiet_for_half_a_second_not_merely_zero() {
        let mut b = Busy::default();
        assert!(b.idle(0.0), "nothing has ever run");
        let counted = b.starting(Call::Settings).unwrap();
        assert!(b.engine_busy());
        b.ended(counted, 1000.0);
        assert!(!b.engine_busy());
        assert!(
            !b.idle(1200.0),
            "the next call in an awaited chain is still coming"
        );
        assert!(b.idle(1500.0));
        let quiet = b.starting(Call::Devices).unwrap();
        assert!(!quiet && b.idle(1500.0), "a poll is nobody's work");
    }

    #[test]
    fn a_hold_counts_like_a_call_and_releases_once() {
        let mut b = Busy::default();
        let h = b.hold();
        assert!(b.engine_busy());
        let counted = b.starting(Call::Settings).unwrap();
        b.release(h, 10.0);
        b.release(h, 11.0);
        assert!(
            b.engine_busy(),
            "a second release cannot drive the count below the call"
        );
        b.ended(counted, 20.0);
        assert!(!b.engine_busy());
    }

    #[test]
    fn after_an_install_every_call_is_refused_and_counts_nothing() {
        let mut b = Busy::default();
        b.retire();
        assert_eq!(b.starting(Call::Routes), Err(UPDATED_MESSAGE));
        assert!(!b.engine_busy());
    }

    #[test]
    fn one_dialog_at_a_time_and_it_holds_the_engine() {
        let mut b = Busy::default();
        let mut d = Dialogs::default();
        assert!(d.try_open(&mut b));
        assert!(!d.try_open(&mut b), "a second ask opens nothing");
        assert!(b.engine_busy());
        d.closed(&mut b, 5.0);
        assert!(!b.engine_busy() && !d.is_open());
        assert!(d.try_open(&mut b));
    }

    #[test]
    fn the_gate_an_update_waits_on_sees_the_same_count() {
        let mut b = Busy::default();
        let gate = b.gate();
        assert_eq!(b.starting(Call::Devices), Ok(false));
        assert!(!gate.busy(), "an uncounted call is nobody's work");
        assert_eq!(b.starting(Call::Convert), Ok(true));
        let hold = b.hold();
        assert!(gate.busy());
        b.ended(true, 1.0);
        assert!(gate.busy(), "the hold is still out");
        b.release(hold, 2.0);
        assert!(!gate.busy());
        // A run on another thread retires the gate: the window refuses.
        gate.retire();
        assert!(b.retired());
        assert_eq!(b.starting(Call::Send), Err(UPDATED_MESSAGE));
    }
}
