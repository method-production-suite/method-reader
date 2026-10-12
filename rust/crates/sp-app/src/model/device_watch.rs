//! What is plugged in, asked every two seconds while a book is open:
//! a state machine the runtime ticks (was `desktop/ui/device-watch.js`).

use serde_json::Value;

use super::device::name_of;
use super::json::ok;
use super::send_routes::{Device, with_transport};

pub const POLL_MS: f64 = 2000.0;

/// `devices --json`'s list, rebuilt clean, or None. An empty name is named
/// for its kind.
pub fn devices_from(answer: &Value) -> Option<Vec<Device>> {
    if !ok(answer) {
        return None;
    }
    answer
        .get("devices")?
        .as_array()?
        .iter()
        .map(|d| {
            let text = |k: &str| {
                d.get(k)
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
            };
            let id = text("id")?;
            let kind = text("kind")?;
            let name = d.get("name")?.as_str()?;
            let volume = match d.get("volume")? {
                Value::String(s) => Some(s.clone()),
                Value::Null => None,
                _ => return None,
            };
            Some(with_transport(
                Device {
                    id: id.into(),
                    kind: kind.into(),
                    name: if name.trim().is_empty() {
                        name_of(kind).into()
                    } else {
                        name.into()
                    },
                    volume,
                    ..Default::default()
                },
                d,
            ))
        })
        .collect()
}

/// The poll's state: running or not, one ask out at a time, the last list.
#[derive(Debug, Default)]
pub struct Watcher {
    running: bool,
    era: u64,
    out_for: Option<u64>,
    next_at: f64,
    list: Option<Vec<Device>>,
}

impl Watcher {
    pub fn start(&mut self, now: f64) {
        if !self.running {
            self.running = true;
            self.next_at = now;
        }
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.era += 1;
        self.list = None;
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// Whether to ask now; if so, the era the answer belongs to.
    pub fn due(&mut self, now: f64) -> Option<u64> {
        if !self.running || now < self.next_at || self.out_for == Some(self.era) {
            return None;
        }
        self.next_at = now + POLL_MS;
        self.out_for = Some(self.era);
        Some(self.era)
    }

    /// When the next ask falls due, for the window to wake then: None while
    /// it is stopped or an ask is still out (its answer wakes the window).
    pub fn next_due(&self) -> Option<f64> {
        (self.running && self.out_for != Some(self.era)).then_some(self.next_at)
    }

    /// An answer arrived. Returns the new list when it changed (what the
    /// listeners hear), None otherwise.
    pub fn answered(&mut self, era: u64, answer: Option<&Value>) -> Option<Vec<Device>> {
        if self.out_for == Some(era) {
            self.out_for = None;
        }
        if era != self.era {
            return None;
        }
        let next = devices_from(answer?)?;
        if self.list.as_ref() == Some(&next) {
            return None;
        }
        self.list = Some(next.clone());
        Some(next)
    }

    pub fn current(&self) -> Option<&[Device]> {
        self.list.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listing(devices: Value) -> Value {
        json!({"ok": true, "devices": devices})
    }

    #[test]
    fn a_listing_is_taken_whole_and_an_empty_name_is_named_for_its_kind() {
        let list = devices_from(&listing(
            json!([{"id": "k", "kind": "kindle", "name": "", "volume": "/V"}]),
        ))
        .unwrap();
        assert_eq!(list[0].name, "Kindle");
        assert!(
            devices_from(&listing(
                json!([{"id": "k", "kind": "kindle", "name": "K"}])
            ))
            .is_none()
        );
        assert!(devices_from(&json!({"ok": false})).is_none());
    }

    #[test]
    fn it_asks_every_two_seconds_one_ask_at_a_time() {
        let mut w = Watcher::default();
        assert_eq!(w.due(0.0), None, "not running");
        w.start(0.0);
        let era = w.due(0.0).unwrap();
        assert_eq!(w.due(5000.0), None, "one ask out");
        let heard = w.answered(era, Some(&listing(json!([])))).unwrap();
        assert!(heard.is_empty());
        assert_eq!(w.due(1000.0), None, "two seconds apart");
        let era = w.due(2000.0).unwrap();
        assert_eq!(
            w.answered(era, Some(&listing(json!([])))),
            None,
            "unchanged says nothing"
        );
    }

    #[test]
    fn the_window_is_woken_for_the_next_ask_and_only_then() {
        let mut w = Watcher::default();
        assert_eq!(w.next_due(), None, "not running: nothing to wake for");
        w.start(100.0);
        assert_eq!(w.next_due(), Some(100.0));
        let era = w.due(100.0).unwrap();
        assert_eq!(w.next_due(), None, "the answer wakes the window");
        w.answered(era, Some(&listing(json!([]))));
        assert_eq!(w.next_due(), Some(2100.0));
        w.stop();
        assert_eq!(w.next_due(), None);
    }

    #[test]
    fn an_answer_from_before_a_stop_is_dropped() {
        let mut w = Watcher::default();
        w.start(0.0);
        let old = w.due(0.0).unwrap();
        w.stop();
        w.start(10.0);
        let fresh = w.due(10.0).expect("an old ask does not block the new era");
        assert_eq!(w.answered(old, Some(&listing(json!([])))), None);
        assert!(w.answered(fresh, Some(&listing(json!([])))).is_some());
        assert_eq!(
            w.answered(fresh, None),
            None,
            "a failed listing changes nothing"
        );
    }
}
