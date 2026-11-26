//! QuickShape as a hand meets it: small ellipses drawn by hand (a lumpy loop, the pen landing with
//! a tick, slowing into the hold, sampled as a tablet samples), and the finger that snaps a held
//! shape.

use std::f32::consts::TAU;

use procraft_engine::geom::Vec2;

use crate::tests::session;
use crate::{CanvasSession, PenSample, PointerKind, QuickShapeInfo, QuickShapeKind, TouchCommand};

/// A loop as a hand draws it, in window points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hand {
    pub(crate) centre: (f32, f32),
    /// Its half-axes, and the turn of the first from across.
    pub(crate) rx: f32,
    pub(crate) ry: f32,
    pub(crate) tilt: f32,
    /// Where on it the pen lands (a turn about the centre), which way it goes (1 or -1), and how
    /// far round (1 is once round, more runs on past its start).
    pub(crate) start: f32,
    pub(crate) way: f32,
    pub(crate) turns: f32,
    /// How long the loop takes, ms.
    pub(crate) ms: f64,
    /// How lumpy it is: the radius wanders by this share.
    pub(crate) lump: f32,
    /// How far in from its start the end comes as it runs on (window points), as a loop that
    /// crosses inside its start.
    pub(crate) end_in: f32,
    /// The tick the pen lands with (window points), outwards before it sets off round.
    pub(crate) tick: f32,
    pub(crate) seed: u32,
}

impl Hand {
    pub(crate) fn small(turns: f32) -> Hand {
        Hand {
            centre: (300.0, 300.0),
            rx: 48.0,
            ry: 34.0,
            tilt: 0.35,
            start: -1.9,
            way: -1.0,
            turns,
            ms: 620.0,
            lump: 0.05,
            end_in: 0.0,
            tick: 0.0,
            seed: 7,
        }
    }

    /// The pen's samples, 240 a second (with the hand's tremor of a third of a point), then held
    /// still for the shape to snap (`hold_ms`, trembling on the spot).
    pub(crate) fn samples(&self, hold_ms: f64) -> Vec<PenSample> {
        let mut seed = self.seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let mut tremor = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.7
        };
        let (sin, cos) = self.tilt.sin_cos();
        let at = |a: f32, inward: f32| {
            let lumps = 1.0 + self.lump * (0.6 * (2.0 * a + 1.3).sin() + 0.4 * (3.0 * a + 0.4).sin());
            let (x, y) = ((self.rx * lumps - inward) * a.cos(), (self.ry * lumps - inward) * a.sin());
            (self.centre.0 + x * cos - y * sin, self.centre.1 + x * sin + y * cos)
        };
        let step = 1000.0 / 240.0;
        let n = (self.ms / step).ceil() as usize;
        let mut out = Vec::with_capacity(n + 200);
        // the tick: the pen lands off the loop and sweeps onto it
        let first = at(self.start, 0.0);
        let out_way = (first.0 - self.centre.0, first.1 - self.centre.1);
        let len = (out_way.0 * out_way.0 + out_way.1 * out_way.1).sqrt().max(1.0);
        let ticks = if self.tick > 0.0 { 6 } else { 0 };
        for k in 0..ticks {
            let f = 1.0 - k as f32 / ticks as f32;
            let off = self.tick * f;
            out.push((first.0 + out_way.0 / len * off, first.1 + out_way.1 / len * off));
        }
        for i in 0..=n {
            let u = (i as f64 / n as f64) as f32;
            // setting off, going on, slowing into the hold
            let p = 0.25 * u + 0.75 * u * u * (3.0 - 2.0 * u);
            let a = self.start + self.way * self.turns * TAU * p;
            out.push(at(a, self.end_in * p.powi(8)));
        }
        let mut samples: Vec<PenSample> = out
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| PenSample {
                x: x + tremor(),
                y: y + tremor(),
                pressure: 0.6,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: i as f64 * step,
                roll: 0.0,
            })
            .collect();
        if let Some(&end) = samples.last() {
            let held = (hold_ms / step) as usize;
            for k in 1..=held {
                let (x, y) = (end.x + tremor() * 0.6, end.y + tremor() * 0.6);
                samples.push(PenSample { x, y, time_ms: end.time_ms + k as f64 * step, ..end });
            }
        }
        samples
    }
}

/// The widest gap, in turns, between the turns about `centre` of the points of `outline`, the
/// ellipse with half-axes `a` and `b` (from its centre) taken as round.
fn widest_gap(outline: &[Vec2], centre: Vec2, a: Vec2, b: Vec2) -> f32 {
    let det = a.x * b.y - a.y * b.x;
    if det.abs() < 1e-6 || outline.is_empty() {
        return 1.0;
    }
    let mut turns: Vec<f32> = outline
        .iter()
        .map(|p| {
            let d = *p - centre;
            let (u, v) = ((d.x * b.y - d.y * b.x) / det, (a.x * d.y - a.y * d.x) / det);
            v.atan2(u).rem_euclid(TAU) / TAU
        })
        .collect();
    turns.sort_by(f32::total_cmp);
    let wrap = turns.first().copied().unwrap_or(0.0) + 1.0 - turns.last().copied().unwrap_or(0.0);
    turns.windows(2).map(|w| w[1] - w[0]).fold(wrap, f32::max)
}

/// What a held stroke of `samples` snapped to, and the widest gap (in turns) its outline leaves.
fn snap(s: &CanvasSession, samples: &[PenSample], kind: PointerKind) -> (Option<QuickShapeKind>, f32) {
    s.stroke_begin(kind, samples[..1].to_vec());
    s.stroke_add(samples[1..].to_vec());
    let shape = s.quickshape_hold().map(|i| i.kind);
    let gap = {
        let st = s.shared.lock();
        st.shape.as_ref().map_or(1.0, |edit| {
            let (_, nodes) = edit.geometry();
            let outline: Vec<Vec2> = edit.samples().iter().map(|s| s.pos).collect();
            match nodes.as_slice() {
                [c, a, b, ..] if matches!(shape, Some(QuickShapeKind::Ellipse | QuickShapeKind::Circle)) => {
                    widest_gap(&outline, *c, *a - *c, *b - *c)
                }
                _ => 1.0,
            }
        })
    };
    s.stroke_end(vec![]);
    s.quickshape_done();
    s.undo();
    (shape, gap)
}

/// An ellipse's or circle's centre and mean radius (window points), from its handles (the ends of
/// its axes).
fn round_of(info: &QuickShapeInfo) -> (Vec2, f32) {
    let h = |i: usize| Vec2::new(info.handles[2 * i], info.handles[2 * i + 1]);
    let c = (h(0) + h(2)) * 0.5;
    (c, (h(0).distance(c) + h(1).distance(c)) * 0.5)
}

fn touch_at(x: f32, y: f32, time_ms: f64) -> Vec<PenSample> {
    vec![PenSample { x, y, pressure: 0.6, tilt: 0.0, orientation: 0.0, time_ms, roll: 0.0 }]
}

/// A loop drawn by `kind` (pointer 1) and held until it snaps, through the touches as the app
/// hands them over. Its last sample.
fn draw_and_hold(s: &CanvasSession, kind: PointerKind, hand: &Hand) -> PenSample {
    let samples = hand.samples(700.0);
    s.touch_down(1, kind, samples[..1].to_vec());
    for chunk in samples[1..].chunks(4) {
        s.touch_move(1, chunk.to_vec());
    }
    let last = *samples.last().unwrap();
    // (the hand's tremor restarts the wait, so it runs out after the last sample)
    let out = s.touch_tick(last.time_ms + 700.0);
    assert!(out.contains(&TouchCommand::ShapeSnapped), "{kind:?}: no snap ({out:?})");
    last
}

/// #411: a held ellipse snapped to a circle by a second finger, sized by the pen still holding it
/// and dropped, keeps the size it was dragged to, wherever the snapping finger was: the pen's lift
/// lets the shape go as it shows, and the finger lifting after it does nothing more. (The finger's
/// lift ended the stroke, and the shape grew or shrank to where the finger was, here more than
/// four times as big.) A finger snaps beside a pen as beside a finger: it was taken for a hand.
#[test]
fn a_circle_snapped_by_a_finger_keeps_the_size_it_was_dragged_to_when_dropped() {
    for kind in [PointerKind::Finger, PointerKind::Stylus] {
        let Some(s) = session(1024, 1024) else { return };
        s.set_display_scale(2.0);
        s.set_brush_size(0.1);
        let hand = Hand::small(1.04);
        let last = draw_and_hold(&s, kind, &hand);
        let snapped = s.quickshape_info().expect("a shape");
        assert_eq!(snapped.kind, QuickShapeKind::Ellipse, "{kind:?}");
        // a finger down far off, in a corner: a circle
        let t = last.time_ms;
        let out = s.touch_down(2, PointerKind::Finger, touch_at(60.0, 980.0, t + 20.0));
        assert!(out.contains(&TouchCommand::ShapeChanged), "{kind:?}: {out:?}");
        let circle = s.quickshape_info().unwrap();
        assert_eq!(circle.kind, QuickShapeKind::Circle, "{kind:?}");
        let (centre, r0) = round_of(&circle);
        // the pen drags it out half as big again, straight away from its middle
        let from = Vec2::new(last.x, last.y);
        let away = (from - centre).normalized_or(Vec2::new(1.0, 0.0));
        let reach = from.distance(centre);
        for k in 1..=10 {
            let at = from + away * (reach * 0.5 * k as f32 / 10.0);
            s.touch_move(1, touch_at(at.x, at.y, t + 40.0 + f64::from(k) * 8.0));
        }
        let (c_drag, r_drag) = round_of(&s.quickshape_info().unwrap());
        assert!((r_drag / r0 - 1.5).abs() < 0.02, "{kind:?}: dragged from {r0} to {r_drag}");
        assert!(c_drag.distance(centre) < 0.5);
        // the pen lifts where it is; the finger then wanders off and lifts
        let end = from + away * (reach * 0.5);
        let out = s.touch_up(1, touch_at(end.x, end.y, t + 140.0));
        assert!(out.contains(&TouchCommand::StrokeEnded), "{kind:?}: {out:?}");
        s.touch_move(2, touch_at(40.0, 1000.0, t + 160.0));
        s.touch_up(2, touch_at(30.0, 1010.0, t + 180.0));
        let dropped = s.quickshape_info().expect("still open to edit");
        let (c_end, r_end) = round_of(&dropped);
        assert_eq!(dropped.kind, QuickShapeKind::Circle, "{kind:?}");
        assert!((r_end - r_drag).abs() < 0.5, "{kind:?}: dragged to {r_drag}, dropped at {r_end}");
        assert!(c_end.distance(c_drag) < 0.5, "{kind:?}: moved from {c_drag:?} to {c_end:?}");
        println!("{kind:?}: snapped r {r0:.1}, dragged to {r_drag:.1}, dropped at {r_end:.1}");
        s.quickshape_done();
    }
}

/// The finger that snapped a held shape lifting first shows it unsnapped again, as the pen holds
/// it; the pen's lift then keeps that.
#[test]
fn a_snapping_finger_lifted_first_leaves_the_shape_to_the_pen() {
    let Some(s) = session(1024, 1024) else { return };
    s.set_display_scale(2.0);
    s.set_brush_size(0.1);
    let last = draw_and_hold(&s, PointerKind::Finger, &Hand::small(1.04));
    let t = last.time_ms;
    s.touch_down(2, PointerKind::Finger, touch_at(60.0, 980.0, t + 20.0));
    assert_eq!(s.quickshape_info().unwrap().kind, QuickShapeKind::Circle);
    let out = s.touch_up(2, touch_at(60.0, 980.0, t + 60.0));
    assert!(out.contains(&TouchCommand::ShapeChanged), "{out:?}");
    let ellipse = s.quickshape_info().unwrap();
    assert_eq!(ellipse.kind, QuickShapeKind::Ellipse);
    s.touch_up(1, touch_at(last.x, last.y, t + 100.0));
    assert_eq!(s.quickshape_info().unwrap().handles, ellipse.handles);
    s.quickshape_done();
}

/// #411: small loops drawn by hand (lumpy, landing with a tick or not, slowing into the hold)
/// snap to ellipses: one run on past its start, crossing inside it or not, closes with no piece
/// missing; one stopped short half way round or more is the ellipse left open by the gap the
/// stroke left, as the reference has it (it goes by how far round the ellipse a stroke goes); one
/// that goes less far is an arc of it.
#[test]
fn small_hand_drawn_loops_snap_to_ellipses_closed_or_open_as_drawn() {
    let Some(s) = session(1024, 1024) else { return };
    s.set_display_scale(2.0);
    s.set_brush_size(0.1);
    let mut wrong = Vec::new();
    let mut loops = 0;
    for (rx, ry) in [(30.0, 22.0), (48.0, 34.0), (80.0, 58.0)] {
        for (turns, tick, end_in) in [
            (1.03, 0.0, 0.0),
            (1.08, 4.0, 0.0),
            (1.12, 0.0, 6.0),
            (1.18, 5.0, 4.0),
            (0.62, 0.0, 0.0),
            (0.75, 4.0, 0.0),
            (0.85, 0.0, 0.0),
            (0.92, 5.0, 0.0),
            (0.4, 0.0, 0.0),
        ] {
            for seed in [3, 11] {
                loops += 1;
                let hand = Hand { rx, ry, turns, tick, end_in, seed, ..Hand::small(turns) };
                let (kind, gap) = snap(&s, &hand.samples(700.0), PointerKind::Finger);
                let what =
                    format!("{rx}x{ry} {turns} round, tick {tick}, {end_in} in, seed {seed}: {kind:?}, gap {gap:.3}");
                let right = if turns < 0.5 {
                    kind == Some(QuickShapeKind::Arc)
                } else if turns >= 1.0 {
                    kind == Some(QuickShapeKind::Ellipse) && gap < 0.02
                } else {
                    kind == Some(QuickShapeKind::Ellipse) && (gap - (1.0 - turns)).abs() < 0.05
                };
                if !right {
                    wrong.push(what);
                }
            }
        }
    }
    println!("{} of {loops} hand-drawn loops snapped wrong", wrong.len());
    assert!(wrong.is_empty(), "{} of {loops} snapped wrong: {wrong:#?}", wrong.len());
}

#[test]
#[ignore]
fn survey_hand_drawn_small_ellipses() {
    let Some(s) = session(1024, 1024) else { return };
    s.set_display_scale(2.0);
    s.set_brush_size(0.1);
    for (rx, ry) in [(30.0, 22.0), (48.0, 34.0), (80.0, 58.0), (140.0, 100.0)] {
        for turns in [0.62, 0.75, 0.85, 0.92, 0.97, 1.0, 1.04, 1.1, 1.18] {
            for (lump, tick, end_in) in [(0.03, 0.0, 0.0), (0.06, 4.0, 0.0), (0.05, 0.0, 6.0), (0.08, 5.0, 4.0)] {
                let hand = Hand { rx, ry, turns, lump, tick, end_in, ..Hand::small(turns) };
                let (kind, gap) = snap(&s, &hand.samples(700.0), PointerKind::Finger);
                let want = (1.0 - turns).max(0.0);
                println!(
                    "r {rx:>3}x{ry:<3} turns {turns:.2} lump {lump:.2} tick {tick} in {end_in}: {kind:?}, gap {gap:.3} (drawn {want:.3})"
                );
            }
        }
    }
}
