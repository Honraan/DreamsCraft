//! View tools: how the canvas shows on the screen, four features each with a switch of its own and
//! its controls under it. Value check: Values only shows the canvas in greys ([`values`]). Trace
//! view: Solo shows the layer you're on alone, Trace dims the rest ([`trace`]). Reference windows:
//! more than one Reference window open at once ([`references`]). Rotation reset: Reset rotation
//! turns the canvas upright, and pinching can keep its turn and its fit ([`rotation`]). What is
//! saved, exported and picked stays as it is. It's a plugin made of those features, not part of
//! the app: `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

mod references;
mod rotation;
mod trace;
mod values;

/// The controls by their place in the manifest, as they start: each feature's switch off.
const STARTS: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 0.7, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0];
static CONTROLS: kit::Controls<12> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Value check's switch, then its own: Values only, and its contrast.
pub const VALUE_CHECK: usize = 0;
pub const VALUES: usize = 1;
pub const CONTRAST: usize = 2;
/// Trace view's switch, then its own: the Layer view (Normal, Solo or Trace), and how far Trace
/// dims the others.
pub const TRACE_VIEW: usize = 3;
pub const VIEW: usize = 4;
pub const DIM: usize = 5;
/// Reference windows' switch, then its own: how many windows at once.
pub const REFERENCE_WINDOWS: usize = 6;
pub const WINDOWS: usize = 7;
/// Rotation reset's switch, then its own: where Reset rotation turns to (0 upright, 1 the nearest
/// right angle), whether pinching keeps the turn, and whether a quick pinch keeps from fitting.
pub const ROTATION_RESET: usize = 8;
pub const TO: usize = 9;
pub const LOCK: usize = 10;
pub const NO_FIT: usize = 11;

/// The actions by their place in the manifest: Trace view's row in the Layers panel and its two
/// keys, then Reset rotation.
const LAYER_VIEW: usize = 0;
const SOLO_KEY: usize = 1;
const TRACE_KEY: usize = 2;
const RESET: usize = 3;

kit::actions!(|index| match index {
    LAYER_VIEW | SOLO_KEY | TRACE_KEY => trace::act(index),
    RESET => rotation::reset_rotation(),
    _ => {}
});

// Trace view's panel is the only one shown
kit::panel!(trace::touched);

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 4] = [VALUE_CHECK, TRACE_VIEW, REFERENCE_WINDOWS, ROTATION_RESET];

/// The tests one at a time, as they share the controls.
#[cfg(test)]
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A test's turn on `feature`: its controls as they start, then its switch on. The other features'
/// controls are left as they are.
#[cfg(test)]
fn fresh(feature: usize) -> std::sync::MutexGuard<'static, ()> {
    let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let end = FEATURES.iter().copied().find(|&next| next > feature).unwrap_or(STARTS.len());
    for (at, start) in STARTS.iter().enumerate().take(end).skip(feature) {
        procraft_control(at as i32, *start);
    }
    procraft_control(feature as i32, 1.0);
    turn
}
