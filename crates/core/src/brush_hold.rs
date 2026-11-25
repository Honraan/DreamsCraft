//! A brush held in the library, as the original does it (measured from screen recordings at about
//! 31 frames a second): held still [`BRUSH_MENU_HOLD`], its menu shows while the finger is still
//! down (let go before, it was a tap, which chooses the brush). The finger moving on within
//! [`BRUSH_DRAG_WINDOW`] of the menu showing lifts the brush to follow it, the menu going; moving on
//! later, the hold is over and the list under the menu has the touch (it scrolls), the menu staying.
//! Let go without moving on, the menu stays and nothing is chosen. The apps follow the touch; what
//! a move does is [`brush_hold_move`]'s to say.

/// How long a brush is held still before its menu shows, seconds from the touch coming down: the
/// menu started showing 0.32 to 0.35 s into each hold measured.
pub const BRUSH_MENU_HOLD: f64 = 0.33;

/// How long after its menu shows a held brush still lifts to follow the finger moving on, seconds:
/// presses of 0.4 and 0.55 s then moved on lifted the brush, their moves taking hold 0.51 and
/// 0.67 s after the touch came down (0.18 and 0.34 s after the menu showed); presses of 0.7 s and
/// 1 s scrolled the list under the menu instead. So the window ends 0.65 to 0.75 s after the touch
/// came down: here in the middle of that.
pub const BRUSH_DRAG_WINDOW: f64 = 0.37;

/// What the finger holding a brush does as it moves (see [`brush_hold_move`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BrushHoldMove {
    /// Not yet past the slop: the hold goes on as it is.
    Stays,
    /// The brush lifts and follows the finger; its menu goes.
    Drags,
    /// The hold is over without lifting the brush: the list under the menu has the touch (it
    /// scrolls), and the menu stays.
    Leaves,
}

/// [`BRUSH_MENU_HOLD`], for the apps.
#[uniffi::export]
pub fn brush_menu_hold_seconds() -> f64 {
    BRUSH_MENU_HOLD
}

/// What the finger holding a brush does, moved `moved` from where it came down (the app's points
/// or pixels, as `slop` is), `since_menu` seconds after the brush's menu showed: within the slop,
/// nothing; past it within [`BRUSH_DRAG_WINDOW`] the brush lifts to be dragged, if it can be
/// (`movable`: the Recent set's brushes stay where they are); past it later, or for a brush that
/// stays, the hold is over and the list has the touch. A distance or a time that isn't a number is
/// taken as no move and as late.
#[uniffi::export]
pub fn brush_hold_move(moved: f32, slop: f32, since_menu: f64, movable: bool) -> BrushHoldMove {
    if moved.is_nan() || moved <= slop.max(0.0) {
        return BrushHoldMove::Stays;
    }
    // (a clock a moment behind the touch's own counts as the menu just showing)
    if movable && since_menu.is_finite() && since_menu.max(0.0) <= BRUSH_DRAG_WINDOW {
        BrushHoldMove::Drags
    } else {
        BrushHoldMove::Leaves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The presses measured on the original, and when their moves took hold (from the touch coming
    /// down: where the menu began to fade as the brush lifted), and what each move then did: the
    /// drag window has to keep each where it was seen.
    #[test]
    fn the_measured_presses_move_on_as_the_original_did() {
        // still holds: the menu showed 0.32 to 0.35 s in, and a hold of 0.3 s let go was a tap
        let hold = brush_menu_hold_seconds();
        assert!((0.32..=0.35).contains(&hold) && hold > 0.3, "{hold}");
        // a 0.4 s press (its move taking hold at 0.508 s) and a 0.55 s one (0.668 s) lifted the
        // brush; a 0.7 s press, its move no sooner than 0.75 s, and a 1 s one scrolled the list
        for (press, moved_at, lifts) in [(0.4, 0.508, true), (0.55, 0.668, true), (0.7, 0.75, false), (1.0, 1.0, false)]
        {
            let moved = brush_hold_move(30.0, 8.0, moved_at - BRUSH_MENU_HOLD, true);
            let want = if lifts { BrushHoldMove::Drags } else { BrushHoldMove::Leaves };
            assert_eq!(moved, want, "a press of {press} s, moved on at {moved_at} s");
        }
    }

    #[test]
    fn a_finger_never_quite_still_keeps_the_hold() {
        for moved in [0.0, 1.0, 3.5, 7.9, 8.0] {
            assert_eq!(brush_hold_move(moved, 8.0, 0.0, true), BrushHoldMove::Stays);
            assert_eq!(brush_hold_move(moved, 8.0, 5.0, true), BrushHoldMove::Stays);
        }
        // a move that isn't a number is no move
        assert_eq!(brush_hold_move(f32::NAN, 8.0, 0.1, true), BrushHoldMove::Stays);
    }

    #[test]
    fn moved_on_in_time_the_brush_lifts_and_later_the_list_has_it() {
        assert_eq!(brush_hold_move(8.5, 8.0, 0.0, true), BrushHoldMove::Drags);
        assert_eq!(brush_hold_move(40.0, 8.0, BRUSH_DRAG_WINDOW, true), BrushHoldMove::Drags);
        assert_eq!(brush_hold_move(40.0, 8.0, BRUSH_DRAG_WINDOW + 0.01, true), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(40.0, 8.0, 3.0, true), BrushHoldMove::Leaves);
        // a clock a moment behind: the menu just showed
        assert_eq!(brush_hold_move(40.0, 8.0, -0.004, true), BrushHoldMove::Drags);
        // a time that isn't a number is late
        assert_eq!(brush_hold_move(40.0, 8.0, f64::NAN, true), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(40.0, 8.0, f64::INFINITY, true), BrushHoldMove::Leaves);
    }

    #[test]
    fn the_recent_sets_brushes_never_lift() {
        assert_eq!(brush_hold_move(40.0, 8.0, 0.05, false), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(4.0, 8.0, 0.05, false), BrushHoldMove::Stays);
    }
}
