//! Floating panels: the Layers panel and the brush library each torn off into a window of its own,
//! dragged out by its grabber, while its switch is on (under the feature's). The window stays open
//! while one draws, is moved by its grabber, sized from its corner and closed by its Close, and the
//! app keeps where it was left.
//!
//! The app asks whether each panel may float, by its number, and does the rest: the windows, what
//! is in them (the panels as they hang from their buttons) and keeping them.

use crate::{CONTROLS, FLOAT_BRUSHES as BRUSHES, FLOAT_LAYERS as LAYERS, FLOATING_PANELS as FEATURE};

/// The panels the app asks about, by their number: the Layers panel, then the brush library.
const PANELS: [usize; 2] = [LAYERS, BRUSHES];

/// Whether panel `panel` may be torn off into a window of its own: as its switch says; a panel it
/// doesn't know stays where it hangs, as every panel does with the feature's switch off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_floating_panel(panel: i32) -> i32 {
    let switch = usize::try_from(panel).ok().and_then(|at| PANELS.get(at).copied());
    i32::from(CONTROLS.on(FEATURE) && switch.is_some_and(|switch| CONTROLS.on(switch)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn each_panel_floats_while_its_switch_is_on() {
        let _turn = crate::fresh(FEATURE);
        let said = || (procraft_floating_panel(0), procraft_floating_panel(1));
        // both start on
        assert_eq!(said(), (1, 1));
        procraft_control(LAYERS as i32, 0.0);
        assert_eq!(said(), (0, 1));
        procraft_control(BRUSHES as i32, 0.0);
        assert_eq!(said(), (0, 0));
        procraft_control(LAYERS as i32, 1.0);
        assert_eq!(said(), (1, 0));
        // a panel it doesn't know, and a control the plugin doesn't have
        procraft_control(BRUSHES as i32, 1.0);
        assert_eq!(
            (procraft_floating_panel(-1), procraft_floating_panel(2), procraft_floating_panel(i32::MAX)),
            (0, 0, 0)
        );
        procraft_control(crate::STARTS.len() as i32, 0.0);
        assert_eq!(said(), (1, 1));
    }

    #[test]
    fn with_its_switch_off_no_panel_floats() {
        let _turn = crate::fresh(FEATURE);
        let said = || (procraft_floating_panel(0), procraft_floating_panel(1));
        assert_eq!(said(), (1, 1));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(said(), (0, 0));
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(said(), (1, 1));
    }
}
