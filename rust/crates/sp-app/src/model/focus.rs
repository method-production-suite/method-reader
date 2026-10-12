//! Where the keyboard stands when something takes the focus away:
//! `desktop/ui/focus.js`. In egui a control is a widget id; the screen
//! records each focusable control as it draws it, in order, and this
//! decides which one the keyboard goes back to.

/// One control a keyboard could stand on, as the screen drew it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop<Id> {
    pub id: Id,
    pub enabled: bool,
    pub shown: bool,
}

/// The candidates in the order focus.js tries them: the first control in
/// the showing pane, the pane itself, the surface's own tab, the Convert
/// tab (always there).
pub fn focus_plan<Id: Copy>(
    pane_controls: &[Stop<Id>],
    pane: Stop<Id>,
    tab: Stop<Id>,
    convert_tab: Stop<Id>,
) -> Vec<Stop<Id>> {
    let mut plan: Vec<Stop<Id>> = pane_controls.to_vec();
    plan.extend([pane, tab, convert_tab]);
    plan
}

/// Whether a control can actually take the focus now.
pub fn can_focus<Id>(stop: &Stop<Id>) -> bool {
    stop.enabled && stop.shown
}

/// The first candidate that can take the focus, or None: leave it alone.
pub fn first_stop<Id: Copy>(candidates: &[Stop<Id>]) -> Option<Id> {
    candidates.iter().find(|s| can_focus(s)).map(|s| s.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: u32, enabled: bool, shown: bool) -> Stop<u32> {
        Stop { id, enabled, shown }
    }

    #[test]
    fn the_first_live_control_in_the_pane_wins() {
        let plan = focus_plan(
            &[
                stop(1, false, true),
                stop(2, true, false),
                stop(3, true, true),
            ],
            stop(10, true, true),
            stop(20, true, true),
            stop(30, true, true),
        );
        assert_eq!(first_stop(&plan), Some(3));
    }

    #[test]
    fn a_pane_with_no_control_takes_the_keyboard_itself() {
        let plan = focus_plan(
            &[],
            stop(10, true, true),
            stop(20, true, true),
            stop(30, true, true),
        );
        assert_eq!(first_stop(&plan), Some(10));
    }

    #[test]
    fn then_its_tab_then_convert() {
        let gone = stop(10, true, false);
        assert_eq!(
            first_stop(&focus_plan(
                &[],
                gone,
                stop(20, true, true),
                stop(30, true, true)
            )),
            Some(20)
        );
        assert_eq!(
            first_stop(&focus_plan(
                &[],
                gone,
                stop(20, true, false),
                stop(30, true, true)
            )),
            Some(30)
        );
        assert_eq!(first_stop::<u32>(&[]), None);
    }
}
