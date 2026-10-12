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

// ------------------------------------------------- the send panel's keys
//
// send-panel.js's keepFocus, giveBackFocus and brassOwed. A flow kills
// every button in the panel while it runs, and a dead button cannot hold
// the keyboard, so it goes back to the button that started the flow, found
// by its key, once the buttons live again.

/// A panel button as drawn this frame: its key (send-panel.js's data-key),
/// its widget id, and whether it is live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyed<Id> {
    pub key: String,
    pub id: Id,
    pub enabled: bool,
}

/// Where a flow started on `key` hands the keyboard back: the live button
/// with the same key, else the live brass button, else None (the page's
/// plan, focus_plan above).
pub fn give_back<Id: Copy>(
    key: &str,
    drawn: &[Keyed<Id>],
    brass: Option<&Keyed<Id>>,
) -> Option<Id> {
    drawn
        .iter()
        .find(|k| k.key == key && k.enabled)
        .or(brass.filter(|b| b.enabled))
        .map(|k| k.id)
}

/// The brass button is owed the keyboard: on a fresh result (`Plan`: the
/// page's plan will park it on the panel's first button), after the plan
/// parked it somewhere (`At`), or about to (`Parked`: the plan runs this
/// frame and the debt sits wherever it lands).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owed<Id> {
    Plan,
    Parked,
    At(Id),
}

/// What a redraw does with that debt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pay<Id> {
    /// The brass is drawn and live: it takes the keyboard.
    Focus(Id),
    /// Still owed, from here.
    Keep(Owed<Id>),
    /// The reader moved the keyboard on: nothing is owed.
    Drop,
}

/// send-panel.js draw() and payBrassOwed(). `focused` is the keyboard's
/// place anywhere in the window, `in_panel` whether that is a panel
/// button, `first` the panel's first live button, `brass` the brass button
/// if it is drawn and live.
pub fn pay_owed<Id: Copy + PartialEq>(
    owed: Owed<Id>,
    calm: bool,
    focused: Option<Id>,
    in_panel: bool,
    first: Option<Id>,
    brass: Option<Id>,
) -> Pay<Id> {
    let owed_now = match owed {
        Owed::Plan => {
            let here = focused.filter(|_| in_panel);
            if here.is_none() {
                // The plan has not parked it in the panel yet: wait.
                return Pay::Keep(Owed::Plan);
            }
            calm && here == first
        }
        Owed::Parked => match focused {
            Some(at) if calm => return Pay::Keep(Owed::At(at)),
            _ => false,
        },
        Owed::At(at) => focused == Some(at),
    };
    if !owed_now {
        return Pay::Drop;
    }
    match (brass, focused) {
        (Some(b), _) => Pay::Focus(b),
        (None, Some(at)) if calm => Pay::Keep(Owed::At(at)),
        _ => Pay::Drop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: u32, enabled: bool, shown: bool) -> Stop<u32> {
        Stop { id, enabled, shown }
    }

    fn keyed(key: &str, id: u32, enabled: bool) -> Keyed<u32> {
        Keyed {
            key: key.into(),
            id,
            enabled,
        }
    }

    #[test]
    fn a_flow_gives_the_keyboard_back_to_its_own_key_then_the_brass_then_the_plan() {
        let brass = keyed("device:kindle", 1, true);
        let drawn = [
            brass.clone(),
            keyed("other-ways", 2, true),
            keyed("save-epub", 3, true),
        ];
        assert_eq!(give_back("save-epub", &drawn, Some(&brass)), Some(3));
        assert_eq!(give_back("gone", &drawn, Some(&brass)), Some(1));
        let dead = [keyed("save-epub", 3, false)];
        assert_eq!(give_back("save-epub", &dead, None), None);
    }

    #[test]
    fn a_fresh_result_owes_the_brass_once_the_plan_parks_on_the_panels_first_button() {
        assert_eq!(
            pay_owed(Owed::Plan, true, None, false, Some(5), None),
            Pay::Keep(Owed::Plan),
            "not parked yet"
        );
        assert_eq!(
            pay_owed(Owed::Plan, true, Some(5), true, Some(5), None),
            Pay::Keep(Owed::At(5)),
            "parked; the brass is not drawn yet"
        );
        assert_eq!(
            pay_owed(Owed::At(5), true, Some(5), true, Some(5), Some(9)),
            Pay::Focus(9)
        );
        assert_eq!(
            pay_owed(Owed::Plan, true, Some(6), true, Some(5), Some(9)),
            Pay::Drop,
            "the reader moved on"
        );
        assert_eq!(
            pay_owed(Owed::At(5), true, Some(7), false, Some(5), Some(9)),
            Pay::Drop
        );
    }

    #[test]
    fn a_card_press_owes_the_brass_wherever_the_plan_parks() {
        assert_eq!(
            pay_owed(Owed::Parked, true, Some(4), false, None, None),
            Pay::Keep(Owed::At(4))
        );
        assert_eq!(
            pay_owed(Owed::<u32>::Parked, true, None, false, None, None),
            Pay::Drop
        );
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
