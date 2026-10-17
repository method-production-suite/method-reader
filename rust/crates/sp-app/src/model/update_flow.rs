//! The window's one update: which moment it is in, and the single run that
//! downloads, installs and restarts (was `desktop/ui/update-flow.js`).
//!
//! The label beside the stamp and the release notes both read this one
//! phase, so they cannot tell two stories about the same update. The run
//! itself (update.js's installAndRestart: the download, the install, the
//! restart) is the updater transport, wave 3's; this is the state machine
//! around it. A run is `start`, then any `during` phases, then `finished`
//! (or `threw`, for a failure before the run could report anything).

use super::update::{CheckOutcome, Phase, Storage, remember_found};

/// A version a check found. `live` stands for update.js's `update` handle:
/// whether this offer still holds one, or a retry must fetch a fresh one.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub version: String,
    pub body: String,
    pub live: bool,
}

/// How installAndRestart ended. `offer` is the offer it actually used,
/// which can be newer than the one the run started with (a label drawn
/// from memory makes it check again first).
#[derive(Clone, Debug, PartialEq)]
pub enum RunOutcome {
    Restarting,
    Installed { offer: Option<Offer> },
    Stalled { offer: Option<Offer> },
    Current,
    Error { offer: Option<Offer> },
}

impl RunOutcome {
    /// The process is on its way out, or there is nothing left for a
    /// second click to do but restart.
    fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Restarting | Self::Installed { .. } | Self::Stalled { .. }
        )
    }
}

/// What a click on the label or the notes' button does.
#[derive(Clone, Debug, PartialEq)]
pub enum Start {
    /// Run installAndRestart with this offer.
    Run(Offer),
    /// The bundle is already swapped: restart into this version.
    RestartNow(String),
    /// A run is under way, or there is nothing to install.
    Nothing,
}

#[derive(Debug, Default)]
pub struct Flow {
    phase: Option<Phase>,
    offer: Option<Offer>,
    /// Which offer object is current: update-flow.js compares offers by
    /// identity, so an equal offer found again is still a new one.
    offer_seq: u64,
    attempted: Option<(u64, Offer)>,
    running: bool,
    booted: bool,
    restarting: bool,
    /// Counts every emit, repeats included: a listener waiting on a change
    /// (the notes' "Looking…") hears a re-emit of the same phase too.
    pub heard: u64,
}

impl Flow {
    pub fn phase(&self) -> Option<&Phase> {
        self.phase.as_ref()
    }

    pub fn current_offer(&self) -> Option<&Offer> {
        self.offer.as_ref()
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_phase(&mut self, next: Option<Phase>) {
        self.phase = next;
        self.heard += 1;
    }

    /// A check found a newer version: the phase if nothing runs, else
    /// remembered for afterwards with the current phase emitted again.
    pub fn offer_found(&mut self, offer: Offer) {
        let phase = if self.running {
            self.phase.clone()
        } else {
            Some(Phase::Offer {
                version: offer.version.clone(),
                body: offer.body.clone(),
                retrying: false,
            })
        };
        self.offer = Some(offer);
        self.offer_seq += 1;
        self.set_phase(phase);
    }

    /// A quiet check's update, downloaded and verified ahead of the click:
    /// the phase is "Update ready" unless a run is under way (then it waits,
    /// as [`Flow::offer_found`]'s offer does). A click runs it as any offer.
    pub fn ready_found(&mut self, offer: Offer) {
        let phase = if self.running {
            self.phase.clone()
        } else {
            Some(Phase::Ready {
                version: offer.version.clone(),
                body: offer.body.clone(),
            })
        };
        self.offer = Some(offer);
        self.offer_seq += 1;
        self.set_phase(phase);
    }

    /// Whether the update `version` is already downloaded and waiting.
    pub fn is_ready(&self, version: &str) -> bool {
        matches!(&self.phase, Some(Phase::Ready { version: v, .. }) if v == version)
    }

    /// The one door both the launch check and the notes' button use.
    /// Skipped and errors say nothing here.
    pub fn check_answered(&mut self, outcome: &CheckOutcome) {
        match outcome {
            CheckOutcome::Offer { version, body } => self.offer_found(Offer {
                version: version.clone(),
                body: body.clone(),
                live: true,
            }),
            CheckOutcome::Current { .. } if !self.running => {
                self.offer = None;
                self.set_phase(None);
            }
            _ => {}
        }
    }

    /// At launch, once ever: what an earlier check found, as a stub with no
    /// live handle. True the first time (the caller then runs the check).
    pub fn boot(&mut self, remembered: Option<String>) -> bool {
        if self.booted {
            return false;
        }
        self.booted = true;
        if let Some(version) = remembered {
            self.offer_found(Offer {
                version,
                body: String::new(),
                live: false,
            });
        }
        true
    }

    /// "Restart now", once.
    fn restart_now(&mut self) -> Start {
        if self.restarting {
            return Start::Nothing;
        }
        self.restarting = true;
        match self.phase.as_ref().and_then(Phase::version) {
            Some(v) => Start::RestartNow(v.to_string()),
            None => Start::Nothing,
        }
    }

    /// The one run. A second call while it is under way does nothing.
    pub fn start(&mut self) -> Start {
        if matches!(self.phase, Some(Phase::Stalled { .. })) {
            return self.restart_now();
        }
        let Some(offer) = self.offer.clone().filter(|_| !self.running) else {
            return Start::Nothing;
        };
        self.running = true;
        self.attempted = Some((self.offer_seq, offer.clone()));
        if matches!(self.phase, Some(Phase::Failed { .. })) {
            self.set_phase(Some(Phase::Offer {
                version: offer.version.clone(),
                body: offer.body.clone(),
                retrying: true,
            }));
        }
        Start::Run(offer)
    }

    /// A phase the run reported (onPhase).
    pub fn during(&mut self, phase: Phase) {
        self.set_phase(Some(phase));
    }

    /// The run failed before it could report anything: nothing was
    /// attempted, so the offer stays as it was.
    pub fn threw(&mut self, message: &str) {
        self.running = false;
        let version = self.attempted.take().map(|(_, o)| o.version);
        self.set_phase(Some(Phase::Failed {
            version,
            message: message.to_string(),
        }));
    }

    /// The run ended.
    pub fn finished(&mut self, outcome: RunOutcome, storage: &mut dyn Storage) {
        let Some((seq, attempted)) = self.attempted.take() else {
            return;
        };
        if outcome.is_final() {
            return;
        }
        self.running = false;
        let used = match &outcome {
            RunOutcome::Error { offer } => offer.clone(),
            _ => None,
        };
        let current = self.offer.clone();
        if let Some(offer) = current.filter(|_| self.offer_seq != seq) {
            // A newer offer arrived during the run. The same version as the
            // one that just failed keeps the failure on screen.
            let attempted_version = used.as_ref().unwrap_or(&attempted).version.clone();
            if matches!(outcome, RunOutcome::Error { .. }) && offer.version == attempted_version {
                return;
            }
            self.set_phase(Some(Phase::Offer {
                version: offer.version.clone(),
                body: offer.body.clone(),
                retrying: false,
            }));
            remember_found(storage, &offer.version);
            return;
        }
        if outcome == RunOutcome::Current {
            self.offer = None;
            return;
        }
        // Every attempt closes the update handle: a retry fetches afresh.
        let base = used.unwrap_or(attempted);
        self.offer = Some(Offer {
            live: false,
            ..base
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::update::{MemoryStorage, remembered_offer};

    fn offer(v: &str) -> Offer {
        Offer {
            version: v.into(),
            body: format!("notes for {v}"),
            live: true,
        }
    }

    fn found(v: &str) -> CheckOutcome {
        CheckOutcome::Offer {
            version: v.into(),
            body: format!("notes for {v}"),
        }
    }

    #[test]
    fn an_offer_found_is_the_phase_body_and_all() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        assert_eq!(
            f.phase(),
            Some(&Phase::Offer {
                version: "0.8.0".into(),
                body: "notes for 0.8.0".into(),
                retrying: false
            })
        );
    }

    #[test]
    fn a_newer_offer_during_a_run_waits_but_is_heard() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        assert_eq!(f.start(), Start::Run(offer("0.8.0")));
        f.during(Phase::Installing {
            version: "0.8.0".into(),
        });
        let before = f.heard;
        f.offer_found(offer("0.9.0"));
        assert_eq!(f.heard, before + 1, "the current phase emitted again");
        assert_eq!(
            f.phase(),
            Some(&Phase::Installing {
                version: "0.8.0".into()
            })
        );
        assert_eq!(f.start(), Start::Nothing, "one run at a time");
        let mut s = MemoryStorage::default();
        f.finished(RunOutcome::Error { offer: None }, &mut s);
        assert_eq!(
            f.phase().and_then(Phase::version),
            Some("0.9.0"),
            "what a click would install now"
        );
        assert_eq!(remembered_offer(&s, "0.7.0").as_deref(), Some("0.9.0"));
    }

    #[test]
    fn a_same_version_offer_after_a_failure_keeps_the_failure_on_screen() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        f.start();
        f.during(Phase::Failed {
            version: Some("0.8.0".into()),
            message: "the download stopped".into(),
        });
        f.offer_found(offer("0.8.0"));
        f.finished(
            RunOutcome::Error { offer: None },
            &mut MemoryStorage::default(),
        );
        assert!(matches!(f.phase(), Some(Phase::Failed { .. })));
        assert!(f.current_offer().unwrap().live, "Try again uses its handle");
    }

    #[test]
    fn a_retry_shows_what_it_attempts_and_fetches_a_fresh_handle() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        f.start();
        f.during(Phase::Failed {
            version: Some("0.8.0".into()),
            message: "no".into(),
        });
        f.finished(
            RunOutcome::Error { offer: None },
            &mut MemoryStorage::default(),
        );
        assert!(!f.current_offer().unwrap().live);
        let Start::Run(again) = f.start() else {
            panic!("a retry runs")
        };
        assert!(!again.live);
        assert_eq!(
            f.phase(),
            Some(&Phase::Offer {
                version: "0.8.0".into(),
                body: "notes for 0.8.0".into(),
                retrying: true
            })
        );
    }

    #[test]
    fn a_stalled_restart_is_restarted_once_never_reinstalled() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        f.start();
        f.during(Phase::Stalled {
            version: "0.8.0".into(),
        });
        f.finished(
            RunOutcome::Stalled { offer: None },
            &mut MemoryStorage::default(),
        );
        assert!(f.running(), "final: the run never ends");
        assert_eq!(f.start(), Start::RestartNow("0.8.0".into()));
        assert_eq!(f.start(), Start::Nothing);
    }

    #[test]
    fn a_throw_before_the_run_says_so_and_leaves_the_offer() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        f.start();
        f.threw("storage is gone");
        assert_eq!(
            f.phase(),
            Some(&Phase::Failed {
                version: Some("0.8.0".into()),
                message: "storage is gone".into()
            })
        );
        assert_eq!(f.current_offer(), Some(&offer("0.8.0")));
        assert!(!f.running());
    }

    #[test]
    fn nothing_newer_takes_the_label_down_unless_a_run_is_under_way() {
        let mut f = Flow::default();
        f.check_answered(&found("0.8.0"));
        f.check_answered(&CheckOutcome::Current {
            reason: "0.7.0".into(),
        });
        assert_eq!(f.phase(), None);
        assert_eq!(f.current_offer(), None);
        f.check_answered(&found("0.8.0"));
        f.start();
        f.check_answered(&CheckOutcome::Current {
            reason: "0.7.0".into(),
        });
        assert!(f.current_offer().is_some());
        f.check_answered(&CheckOutcome::Error {
            message: "offline".into(),
        });
        f.check_answered(&CheckOutcome::Skipped);
        assert!(
            f.current_offer().is_some(),
            "skipped and errors say nothing"
        );
    }

    #[test]
    fn boot_runs_once_and_draws_the_remembered_stub() {
        let mut f = Flow::default();
        assert!(f.boot(Some("0.8.0".into())));
        assert!(!f.current_offer().unwrap().live);
        assert_eq!(f.phase().and_then(Phase::version), Some("0.8.0"));
        f.check_answered(&found("0.8.0"));
        assert!(!f.boot(Some("0.7.5".into())), "never again");
        assert_eq!(f.current_offer(), Some(&offer("0.8.0")));
    }

    #[test]
    fn a_ready_update_is_one_click_from_its_run_and_waits_behind_a_run() {
        let mut f = Flow::default();
        f.ready_found(offer("0.8.0"));
        assert!(f.is_ready("0.8.0"));
        assert!(!f.is_ready("0.9.0"));
        assert_eq!(f.start(), Start::Run(offer("0.8.0")));
        f.during(Phase::Installing {
            version: "0.8.0".into(),
        });
        f.ready_found(offer("0.9.0"));
        assert_eq!(
            f.phase(),
            Some(&Phase::Installing {
                version: "0.8.0".into()
            }),
            "the run's phase stays"
        );
    }

    #[test]
    fn a_run_that_finds_nothing_newer_forgets_the_offer() {
        let mut f = Flow::default();
        f.offer_found(offer("0.8.0"));
        f.start();
        f.finished(RunOutcome::Current, &mut MemoryStorage::default());
        assert_eq!(f.current_offer(), None);
        assert_eq!(f.start(), Start::Nothing);
    }
}
