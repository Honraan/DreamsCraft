//! Multi-track timeline, clips, cels, non-destructive keyframe animation (`Transform2D` with
//! `anchor`, and `opacity`), easing curves, per-track onion skinning, and live Perform motion
//! filtering (with Krita's animation architecture as a reference).

use std::collections::{HashMap, HashSet};

use crate::animation::{Animation, MAX_ONION_FRAMES, Playback};
use crate::document::{BlendMode, Document, LayerId};
use crate::geom::{Affine, Vec2};
use crate::motion::Bezier;

/// Unique identifier of a [`Track`] in a [`Timeline`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TrackId(pub u64);

/// Unique identifier of a [`Clip`] in a [`Track`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ClipId(pub u64);

/// Default frame rate for new DreamsCraft projects (cinematic 24 fps).
pub const DEFAULT_TIMELINE_FPS: u32 = 24;

/// Default timeline length in frames (5 seconds at 24 fps).
pub const DEFAULT_TIMELINE_DURATION: u32 = 120;

/// Maximum timeline duration in frames (10 minutes at 60 fps).
pub const MAX_TIMELINE_DURATION: u32 = 36_000;

/// Default track accent palette (ARGB) for visually distinguishing tracks in the UI.
pub const TRACK_COLORS: [u32; 6] = [
    0xFF3B82F6, // Blue
    0xFFEC4899, // Pink
    0xFF10B981, // Emerald
    0xFFF59E0B, // Amber
    0xFF8B5CF6, // Violet
    0xFF06B6D4, // Cyan
];

/// In/Out playback range on the timeline (inclusive frame indices `start..=end`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkArea {
    pub start: u32,
    pub end: u32,
}

impl WorkArea {
    pub fn repaired(self, duration_frames: u32) -> WorkArea {
        let last = duration_frames.max(1).saturating_sub(1);
        let start = self.start.min(last);
        let end = self.end.clamp(start, last);
        WorkArea { start, end }
    }

    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }

    pub fn is_empty(self) -> bool {
        false
    }
}

/// Onion skin settings for frame-by-frame drawing in Flipbook or Stage mode.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OnionSkinSettings {
    pub enabled: bool,
    pub before_frames: u32,
    pub after_frames: u32,
    pub opacity: f32,
    /// sRGB 0..1 tint for previous frames (default warm red/pink).
    pub before_color: [f32; 3],
    /// sRGB 0..1 tint for upcoming frames (default mint green).
    pub after_color: [f32; 3],
    pub blend_primary: bool,
    /// Only show onion skins for the currently active track/clip so static background tracks stay clean.
    pub active_track_only: bool,
}

impl Default for OnionSkinSettings {
    fn default() -> Self {
        OnionSkinSettings {
            enabled: true,
            before_frames: 3,
            after_frames: 3,
            opacity: 0.6,
            before_color: [1.0, 0.24, 0.36],
            after_color: [0.18, 0.88, 0.52],
            blend_primary: false,
            active_track_only: true,
        }
    }
}

impl OnionSkinSettings {
    pub fn repaired(mut self) -> Self {
        self.before_frames = self.before_frames.min(MAX_ONION_FRAMES);
        self.after_frames = self.after_frames.min(MAX_ONION_FRAMES);
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.0, 1.0) } else { 0.6 };
        let clean = |c: [f32; 3]| c.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 });
        self.before_color = clean(self.before_color);
        self.after_color = clean(self.after_color);
        self
    }
}

/// Non-destructive 2D spatial transform for a clip or track (Move, Scale, Rotate around an Anchor Point).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Transform2D {
    /// Translation in canvas pixels `(dx, dy)`.
    pub position: [f32; 2],
    /// Scale factors `(sx, sy)` around `anchor` (`1.0` = original size).
    pub scale: [f32; 2],
    /// Clockwise rotation in radians around `anchor`.
    pub rotation: f32,
    /// Normalized anchor point on the canvas `([0.5, 0.5]` = canvas center, `[0.0, 0.0]` = top-left).
    pub anchor: [f32; 2],
}

impl Default for Transform2D {
    fn default() -> Self {
        Transform2D { position: [0.0, 0.0], scale: [1.0, 1.0], rotation: 0.0, anchor: [0.5, 0.5] }
    }
}

impl Transform2D {
    pub fn is_identity(&self) -> bool {
        self.position[0].abs() < 1e-4
            && self.position[1].abs() < 1e-4
            && (self.scale[0] - 1.0).abs() < 1e-4
            && (self.scale[1] - 1.0).abs() < 1e-4
            && self.rotation.abs() < 1e-4
    }

    pub fn repaired(mut self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        self.position = [
            finite(self.position[0], 0.0).clamp(-65_536.0, 65_536.0),
            finite(self.position[1], 0.0).clamp(-65_536.0, 65_536.0),
        ];
        let clamp_scale = |s: f32| {
            let s = finite(s, 1.0).clamp(-64.0, 64.0);
            if s.abs() < 0.005 { if s < 0.0 { -0.005 } else { 0.005 } } else { s }
        };
        self.scale = [clamp_scale(self.scale[0]), clamp_scale(self.scale[1])];
        self.rotation = finite(self.rotation, 0.0).clamp(-1000.0, 1000.0);
        self.anchor = [finite(self.anchor[0], 0.5).clamp(-4.0, 5.0), finite(self.anchor[1], 0.5).clamp(-4.0, 5.0)];
        self
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Transform2D {
            position: [mix(self.position[0], other.position[0]), mix(self.position[1], other.position[1])],
            scale: [mix(self.scale[0], other.scale[0]), mix(self.scale[1], other.scale[1])],
            rotation: mix(self.rotation, other.rotation),
            anchor: [mix(self.anchor[0], other.anchor[0]), mix(self.anchor[1], other.anchor[1])],
        }
        .repaired()
    }

    /// Converts this 2D transform into a canvas-space [`Affine`] map for a canvas of `width × height`.
    pub fn to_affine(self, canvas_width: u32, canvas_height: u32) -> Affine {
        let clean = self.repaired();
        if clean.is_identity() {
            return Affine::IDENTITY;
        }
        let cw = canvas_width.max(1) as f32;
        let ch = canvas_height.max(1) as f32;
        let pivot = Vec2::new(clean.anchor[0] * cw, clean.anchor[1] * ch);
        let scale_affine = Affine { a: clean.scale[0], b: 0.0, c: 0.0, d: clean.scale[1], e: 0.0, f: 0.0 };
        Affine::translate(Vec2::ZERO - pivot)
            .then(scale_affine)
            .then(Affine::rotate(clean.rotation))
            .then(Affine::translate(pivot + Vec2::new(clean.position[0], clean.position[1])))
    }
}

/// Easing curve between a keyframe and the next keyframe on the same property track.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Easing {
    /// Step / Hold: keeps the value constant until the next keyframe.
    Hold,
    /// Constant speed between keyframes.
    #[default]
    Linear,
    /// Starts slow and accelerates toward the next keyframe.
    EaseIn,
    /// Starts fast and decelerates into the next keyframe.
    EaseOut,
    /// Smooth acceleration and deceleration.
    EaseInOut,
    /// Custom cubic-bezier curve with control points `(x1, y1)` and `(x2, y2)`.
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Easing {
    pub fn repaired(self) -> Self {
        let unit = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        let val = |v: f32, d: f32| if v.is_finite() { v.clamp(-2.0, 3.0) } else { d };
        match self {
            Easing::CubicBezier { x1, y1, x2, y2 } => {
                Easing::CubicBezier { x1: unit(x1, 0.42), y1: val(y1, 0.0), x2: unit(x2, 0.58), y2: val(y2, 1.0) }
            }
            other => other,
        }
    }

    /// Evaluates the normalized progress `0.0..=1.0` at fraction `t` (`0.0..=1.0`).
    pub fn sample(self, t: f32) -> f32 {
        if !t.is_finite() || t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return match self {
                Easing::Hold => 0.0,
                _ => 1.0,
            };
        }
        match self.repaired() {
            Easing::Hold => 0.0,
            Easing::Linear => t,
            Easing::EaseIn => (Bezier { seconds: 1.0, x1: 0.42, y1: 0.0, x2: 1.0, y2: 1.0 }).at(t),
            Easing::EaseOut => Bezier::ease_out(1.0).at(t),
            Easing::EaseInOut => (Bezier { seconds: 1.0, x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 }).at(t),
            Easing::CubicBezier { x1, y1, x2, y2 } => (Bezier { seconds: 1.0, x1, y1, x2, y2 }).at(t),
        }
    }
}

/// A single keyframe at timeline `frame` with value `T` and outgoing [`Easing`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Keyframe<T> {
    pub frame: u32,
    pub value: T,
    #[serde(default)]
    pub easing: Easing,
}

/// Sub-tracks of non-destructive keyframes attached to a [`Clip`] or [`Track`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct KeyframeTrack {
    pub transform: Vec<Keyframe<Transform2D>>,
    pub opacity: Vec<Keyframe<f32>>,
}

impl KeyframeTrack {
    pub fn is_empty(&self) -> bool {
        self.transform.is_empty() && self.opacity.is_empty()
    }

    pub fn repaired(mut self) -> Self {
        for kf in &mut self.transform {
            kf.value = kf.value.repaired();
            kf.easing = kf.easing.repaired();
        }
        self.transform.sort_by_key(|k| k.frame);
        self.transform.dedup_by_key(|k| k.frame);

        for kf in &mut self.opacity {
            kf.value = if kf.value.is_finite() { kf.value.clamp(0.0, 1.0) } else { 1.0 };
            kf.easing = kf.easing.repaired();
        }
        self.opacity.sort_by_key(|k| k.frame);
        self.opacity.dedup_by_key(|k| k.frame);
        self
    }

    /// Shifts all keyframes in this track by `delta` frames (clamped at 0).
    pub fn shift(&mut self, delta: i64) {
        for kf in &mut self.transform {
            let shifted = (i64::from(kf.frame) + delta).max(0);
            kf.frame = u32::try_from(shifted).unwrap_or(u32::MAX);
        }
        for kf in &mut self.opacity {
            let shifted = (i64::from(kf.frame) + delta).max(0);
            kf.frame = u32::try_from(shifted).unwrap_or(u32::MAX);
        }
    }

    pub fn set_transform(&mut self, frame: u32, value: Transform2D, easing: Option<Easing>) {
        let value = value.repaired();
        if let Some(existing) = self.transform.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::EaseInOut).repaired();
        self.transform.push(Keyframe { frame, value, easing });
        self.transform.sort_by_key(|k| k.frame);
    }

    pub fn set_opacity(&mut self, frame: u32, opacity: f32, easing: Option<Easing>) {
        let value = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
        if let Some(existing) = self.opacity.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::EaseInOut).repaired();
        self.opacity.push(Keyframe { frame, value, easing });
        self.opacity.sort_by_key(|k| k.frame);
    }

    pub fn remove_transform(&mut self, frame: u32) -> bool {
        let before = self.transform.len();
        self.transform.retain(|k| k.frame != frame);
        self.transform.len() != before
    }

    pub fn remove_opacity(&mut self, frame: u32) -> bool {
        let before = self.opacity.len();
        self.opacity.retain(|k| k.frame != frame);
        self.opacity.len() != before
    }

    pub fn set_transform_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.transform.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    pub fn set_opacity_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.opacity.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    /// Evaluates the interpolated [`Transform2D`] at `frame`.
    pub fn evaluate_transform(&self, frame: f32) -> Transform2D {
        let frame = if frame.is_finite() { frame.max(0.0) } else { 0.0 };
        let Some(first) = self.transform.first() else {
            return Transform2D::default();
        };
        if frame <= first.frame as f32 {
            return first.value;
        }
        let Some(last) = self.transform.last() else {
            return first.value;
        };
        if frame >= last.frame as f32 {
            return last.value;
        }
        for pair in self.transform.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
            let (fa, fb) = (a.frame as f32, b.frame as f32);
            if frame >= fa && frame <= fb {
                let span = (fb - fa).max(1e-6);
                let raw_t = ((frame - fa) / span).clamp(0.0, 1.0);
                let eased_t = a.easing.sample(raw_t);
                return a.value.lerp(b.value, eased_t);
            }
        }
        last.value
    }

    /// Evaluates the interpolated opacity (`0.0..=1.0`) at `frame`.
    pub fn evaluate_opacity(&self, frame: f32) -> f32 {
        let frame = if frame.is_finite() { frame.max(0.0) } else { 0.0 };
        let Some(first) = self.opacity.first() else {
            return 1.0;
        };
        if frame <= first.frame as f32 {
            return first.value;
        }
        let Some(last) = self.opacity.last() else {
            return first.value;
        };
        if frame >= last.frame as f32 {
            return last.value;
        }
        for pair in self.opacity.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
            let (fa, fb) = (a.frame as f32, b.frame as f32);
            if frame >= fa && frame <= fb {
                let span = (fb - fa).max(1e-6);
                let raw_t = ((frame - fa) / span).clamp(0.0, 1.0);
                let eased_t = a.easing.sample(raw_t);
                return (a.value + (b.value - a.value) * eased_t).clamp(0.0, 1.0);
            }
        }
        last.value
    }
}

/// A single hand-drawn cel inside a Flipbook [`ClipKind::Cel`] sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cel {
    /// The paint layer or layer group in [`Document`] that holds this cel's artwork.
    pub layer: LayerId,
    /// Additional frames this cel stays on screen (`0` = 1 frame total, `1` = 2 frames, etc.).
    #[serde(default)]
    pub hold: u32,
}

impl Cel {
    /// Total number of timeline frames this cel occupies (`1 + hold`).
    pub fn duration_frames(self) -> u32 {
        1u32.saturating_add(self.hold.min(600))
    }
}

/// What a [`Clip`] on a track holds.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ClipKind {
    /// Frame-by-frame Flipbook drawing sequence: consecutive cels starting at `clip.start_frame`.
    Cel { cels: Vec<Cel> },
    /// Static content/layer shown across `clip.start_frame..clip.start_frame + clip.duration_frames`.
    Layer { layer: LayerId },
    /// Group clip referencing a group of layers or acting as a motion container.
    Group { members: Vec<LayerId> },
}

/// A timed content block on a [`Track`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    pub start_frame: u32,
    pub duration_frames: u32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default = "unit_one")]
    pub opacity: f32,
    pub kind: ClipKind,
    #[serde(default)]
    pub keyframes: KeyframeTrack,
}

fn unit_one() -> f32 {
    1.0
}

impl Clip {
    pub fn end_frame(&self) -> u32 {
        self.start_frame.saturating_add(self.effective_duration())
    }

    /// For a `Cel` clip, its duration is the sum of its cels' durations (at least 1); for other
    /// clips, `self.duration_frames.max(1)`.
    pub fn effective_duration(&self) -> u32 {
        match &self.kind {
            ClipKind::Cel { cels } if !cels.is_empty() => {
                cels.iter().map(|c| c.duration_frames()).fold(0u32, u32::saturating_add).max(1)
            }
            _ => self.duration_frames.max(1),
        }
    }

    pub fn sync_duration(&mut self) {
        self.duration_frames = self.effective_duration();
    }

    pub fn contains_frame(&self, frame: u32) -> bool {
        frame >= self.start_frame && frame < self.end_frame()
    }

    /// For a `Cel` clip, returns `(cel_index, cel, offset_into_cel)` at timeline `frame`.
    pub fn cel_at(&self, frame: u32) -> Option<(usize, Cel, u32)> {
        if !self.contains_frame(frame) {
            return None;
        }
        let ClipKind::Cel { cels } = &self.kind else {
            return None;
        };
        let local = frame.saturating_sub(self.start_frame);
        let mut cursor = 0u32;
        for (idx, &cel) in cels.iter().enumerate() {
            let len = cel.duration_frames();
            if local >= cursor && local < cursor.saturating_add(len) {
                return Some((idx, cel, local.saturating_sub(cursor)));
            }
            cursor = cursor.saturating_add(len);
        }
        None
    }

    /// Timeline start frame of cel at `cel_index`.
    pub fn cel_start_frame(&self, cel_index: usize) -> Option<u32> {
        let ClipKind::Cel { cels } = &self.kind else {
            return None;
        };
        if cel_index >= cels.len() {
            return None;
        }
        let mut offset = 0u32;
        for cel in cels.iter().take(cel_index) {
            offset = offset.saturating_add(cel.duration_frames());
        }
        Some(self.start_frame.saturating_add(offset))
    }

    /// Primary layer represented by this clip at `frame`.
    pub fn layer_at(&self, frame: u32) -> Option<LayerId> {
        if !self.contains_frame(frame) {
            return None;
        }
        match &self.kind {
            ClipKind::Cel { .. } => self.cel_at(frame).map(|(_, cel, _)| cel.layer),
            ClipKind::Layer { layer } => Some(*layer),
            ClipKind::Group { members } => members.first().copied(),
        }
    }

    /// All layers referenced anywhere inside this clip.
    pub fn referenced_layers(&self) -> Vec<LayerId> {
        match &self.kind {
            ClipKind::Cel { cels } => cels.iter().map(|c| c.layer).collect(),
            ClipKind::Layer { layer } => vec![*layer],
            ClipKind::Group { members } => members.clone(),
        }
    }
}

/// A horizontal track on the [`Timeline`] (stacked bottom-to-top).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub solo: bool,
    #[serde(default = "unit_one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default = "default_track_color")]
    pub color_tag: u32,
    /// Optional parent group track (hierarchical Transform2D and opacity inheritance).
    #[serde(default)]
    pub parent_group: Option<TrackId>,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default = "bool_true")]
    pub expanded: bool,
    #[serde(default)]
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub keyframes: KeyframeTrack,
}

fn default_track_color() -> u32 {
    TRACK_COLORS[0]
}

fn bool_true() -> bool {
    true
}

/// One composited layer item in an [`EvaluatedScene`] at a specific frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvaluatedLayer {
    pub layer: LayerId,
    pub track: TrackId,
    pub clip: ClipId,
    /// Combined canvas-space affine transform (parent group track * track * clip).
    pub transform: Affine,
    /// Combined effective opacity (`0.0..=1.0`).
    pub opacity: f32,
    /// Effective blend mode.
    pub blend: BlendMode,
    /// Screen tint `[r, g, b, a]` (`a = 1.0` for onion skin, `0.0` for normal).
    pub tint: [f32; 4],
    pub is_onion: bool,
}

/// The complete evaluated scene of a [`Timeline`] at a given frame, ready for GPU compositing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluatedScene {
    pub frame: f32,
    pub playing: bool,
    /// Ordered bottom-to-top for compositing.
    pub items: Vec<EvaluatedLayer>,
    /// Set of all layers managed by any track in the timeline (so the renderer knows which document
    /// layers are driven by the timeline).
    pub managed_layers: HashSet<LayerId>,
}

/// A recorded sample during live Perform mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionSample {
    pub frame: u32,
    pub transform: Transform2D,
}

/// Multi-track timeline and animation state for a DreamsCraft [`Document`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Timeline {
    pub fps: u32,
    pub duration_frames: u32,
    pub playhead: u32,
    pub playback: Playback,
    pub work_area: Option<WorkArea>,
    pub onion: OnionSkinSettings,
    /// Tracks ordered bottom-to-top.
    pub tracks: Vec<Track>,
    pub active_track: Option<TrackId>,
    pub active_clip: Option<ClipId>,
    /// Whether the workspace is currently in focused Flipbook drawing mode.
    pub flipbook_open: bool,
    /// Whether the stage shows the darkened Backstage outside the canvas bounds while editing.
    pub show_backstage: bool,
    /// Live Perform curve simplification amount (`0.0..=1.0`).
    pub motion_filtering: f32,
    next_id: u64,
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline {
            fps: DEFAULT_TIMELINE_FPS,
            duration_frames: DEFAULT_TIMELINE_DURATION,
            playhead: 0,
            playback: Playback::Loop,
            work_area: None,
            onion: OnionSkinSettings::default(),
            tracks: Vec::new(),
            active_track: None,
            active_clip: None,
            flipbook_open: false,
            show_backstage: true,
            motion_filtering: 0.35,
            next_id: 1,
        }
    }
}

impl Timeline {
    /// Creates a new timeline initialized with one Drawing track containing a `Cel` clip for `first_layer`.
    pub fn new_with_layer(first_layer: LayerId) -> Timeline {
        let mut tl = Timeline::default();
        let track_id = tl.alloc_track_id();
        let clip_id = tl.alloc_clip_id();
        let clip = Clip {
            id: clip_id,
            name: "Drawing 1".to_owned(),
            start_frame: 0,
            duration_frames: 1,
            blend: BlendMode::Normal,
            opacity: 1.0,
            kind: ClipKind::Cel { cels: vec![Cel { layer: first_layer, hold: 0 }] },
            keyframes: KeyframeTrack::default(),
        };
        let track = Track {
            id: track_id,
            name: "Track 1".to_owned(),
            visible: true,
            locked: false,
            solo: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            color_tag: TRACK_COLORS[0],
            parent_group: None,
            is_group: false,
            expanded: true,
            clips: vec![clip],
            keyframes: KeyframeTrack::default(),
        };
        tl.tracks.push(track);
        tl.active_track = Some(track_id);
        tl.active_clip = Some(clip_id);
        tl
    }

    /// Migrates a legacy ProCraft [`Animation`] and [`Document`] layer stack into a multi-track [`Timeline`].
    pub fn from_legacy_animation(doc: &Document, anim: &Animation) -> Timeline {
        let all_frames = crate::animation::frames(doc);
        let played_range = crate::animation::played(Some(anim), all_frames.len());
        let mut tl = Timeline {
            fps: anim.fps.clamp(1, 60),
            duration_frames: DEFAULT_TIMELINE_DURATION,
            playhead: 0,
            playback: anim.playback,
            work_area: None,
            onion: OnionSkinSettings {
                enabled: !anim.pages && anim.onion_frames > 0,
                before_frames: anim.onion_frames.min(MAX_ONION_FRAMES),
                after_frames: anim.onion_frames.min(MAX_ONION_FRAMES),
                opacity: anim.onion_opacity,
                before_color: if anim.before == [0.0; 3] { [1.0, 0.24, 0.36] } else { anim.before },
                after_color: if anim.after == [0.0; 3] { [0.18, 0.88, 0.52] } else { anim.after },
                blend_primary: anim.blend_primary,
                active_track_only: true,
            }
            .repaired(),
            tracks: Vec::new(),
            active_track: None,
            active_clip: None,
            flipbook_open: false,
            show_backstage: true,
            motion_filtering: 0.35,
            next_id: 1,
        };

        // Calculate total played cels duration first
        let played_layers: Vec<LayerId> =
            all_frames.get(played_range.clone()).map(<[LayerId]>::to_vec).unwrap_or_default();
        let cels: Vec<Cel> = played_layers.iter().map(|&layer| Cel { layer, hold: anim.hold(layer) }).collect();
        let cels_duration: u32 = cels.iter().map(|c| c.duration_frames()).fold(0u32, u32::saturating_add).max(1);
        tl.duration_frames = tl.duration_frames.max(cels_duration);

        // 1. Optional Background track
        if anim.background
            && let Some(&bg_layer) = all_frames.first()
        {
            let bg_track_id = tl.alloc_track_id();
            let bg_clip_id = tl.alloc_clip_id();
            tl.tracks.push(Track {
                id: bg_track_id,
                name: "Background".to_owned(),
                visible: true,
                locked: false,
                solo: false,
                opacity: 1.0,
                blend: BlendMode::Normal,
                color_tag: TRACK_COLORS[2],
                parent_group: None,
                is_group: false,
                expanded: false,
                clips: vec![Clip {
                    id: bg_clip_id,
                    name: "Background".to_owned(),
                    start_frame: 0,
                    duration_frames: tl.duration_frames,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    kind: ClipKind::Layer { layer: bg_layer },
                    keyframes: KeyframeTrack::default(),
                }],
                keyframes: KeyframeTrack::default(),
            });
        }

        // 2. Main Drawing track
        let main_track_id = tl.alloc_track_id();
        let main_clip_id = tl.alloc_clip_id();
        let fallback_cels = if cels.is_empty() {
            all_frames.first().copied().into_iter().map(|layer| Cel { layer, hold: 0 }).collect()
        } else {
            cels
        };
        let mut main_clip = Clip {
            id: main_clip_id,
            name: "Animation".to_owned(),
            start_frame: 0,
            duration_frames: cels_duration,
            blend: BlendMode::Normal,
            opacity: 1.0,
            kind: ClipKind::Cel { cels: fallback_cels },
            keyframes: KeyframeTrack::default(),
        };
        main_clip.sync_duration();
        tl.tracks.push(Track {
            id: main_track_id,
            name: "Track 1".to_owned(),
            visible: true,
            locked: false,
            solo: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            color_tag: TRACK_COLORS[0],
            parent_group: None,
            is_group: false,
            expanded: true,
            clips: vec![main_clip],
            keyframes: KeyframeTrack::default(),
        });
        tl.active_track = Some(main_track_id);
        tl.active_clip = Some(main_clip_id);

        // 3. Optional Foreground track
        if anim.foreground
            && !anim.pages
            && all_frames.len() > 1
            && let Some(&fg_layer) = all_frames.last()
        {
            let fg_track_id = tl.alloc_track_id();
            let fg_clip_id = tl.alloc_clip_id();
            tl.tracks.push(Track {
                id: fg_track_id,
                name: "Foreground".to_owned(),
                visible: true,
                locked: false,
                solo: false,
                opacity: 1.0,
                blend: BlendMode::Normal,
                color_tag: TRACK_COLORS[1],
                parent_group: None,
                is_group: false,
                expanded: false,
                clips: vec![Clip {
                    id: fg_clip_id,
                    name: "Foreground".to_owned(),
                    start_frame: 0,
                    duration_frames: tl.duration_frames,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    kind: ClipKind::Layer { layer: fg_layer },
                    keyframes: KeyframeTrack::default(),
                }],
                keyframes: KeyframeTrack::default(),
            });
        }

        tl
    }

    fn alloc_track_id(&mut self) -> TrackId {
        let id = TrackId(self.next_id.max(1));
        self.next_id = id.0.saturating_add(1);
        id
    }

    fn alloc_clip_id(&mut self) -> ClipId {
        let id = ClipId(self.next_id.max(1));
        self.next_id = id.0.saturating_add(1);
        id
    }

    /// Sanitizes and repairs the timeline against `valid_layers` in the document.
    pub fn repaired(mut self, valid_layers: &HashSet<LayerId>) -> Timeline {
        self.fps = self.fps.clamp(1, 60);
        self.duration_frames = self.duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        self.onion = self.onion.repaired();
        self.motion_filtering =
            if self.motion_filtering.is_finite() { self.motion_filtering.clamp(0.0, 1.0) } else { 0.35 };

        let mut max_seen_id = 0u64;
        let mut seen_tracks = HashSet::new();
        self.tracks.retain(|t| seen_tracks.insert(t.id));

        let mut seen_clips = HashSet::new();
        for track in &mut self.tracks {
            max_seen_id = max_seen_id.max(track.id.0);
            if track.name.trim().is_empty() {
                track.name = "Track".to_owned();
            }
            track.opacity = if track.opacity.is_finite() { track.opacity.clamp(0.0, 1.0) } else { 1.0 };
            track.keyframes = std::mem::take(&mut track.keyframes).repaired();
            track.clips.retain(|c| seen_clips.insert(c.id));

            for clip in &mut track.clips {
                max_seen_id = max_seen_id.max(clip.id.0);
                if clip.name.trim().is_empty() {
                    clip.name = "Clip".to_owned();
                }
                clip.opacity = if clip.opacity.is_finite() { clip.opacity.clamp(0.0, 1.0) } else { 1.0 };
                clip.keyframes = std::mem::take(&mut clip.keyframes).repaired();
                match &mut clip.kind {
                    ClipKind::Cel { cels } => {
                        if !valid_layers.is_empty() {
                            cels.retain(|c| valid_layers.contains(&c.layer));
                        }
                        for cel in cels.iter_mut() {
                            cel.hold = cel.hold.min(600);
                        }
                    }
                    ClipKind::Layer { .. } => {}
                    ClipKind::Group { members } => {
                        if !valid_layers.is_empty() {
                            members.retain(|m| valid_layers.contains(m));
                        }
                    }
                }
                clip.sync_duration();
            }
            if !valid_layers.is_empty() {
                track.clips.retain(|clip| match &clip.kind {
                    ClipKind::Cel { cels } => !cels.is_empty(),
                    ClipKind::Layer { layer } => valid_layers.contains(layer),
                    ClipKind::Group { members } => track.is_group || !members.is_empty(),
                });
            }
            track.clips.sort_by_key(|c| c.start_frame);
        }

        // Validate parent_group links and prevent cycles
        let group_ids: HashSet<TrackId> = self.tracks.iter().filter(|t| t.is_group).map(|t| t.id).collect();
        for track in &mut self.tracks {
            if let Some(p) = track.parent_group
                && (p == track.id || !group_ids.contains(&p))
            {
                track.parent_group = None;
            }
        }
        let parent_map: HashMap<TrackId, Option<TrackId>> =
            self.tracks.iter().map(|t| (t.id, t.parent_group)).collect();
        for track in &mut self.tracks {
            let mut visited = HashSet::new();
            let mut cursor = track.parent_group;
            while let Some(curr) = cursor {
                if !visited.insert(curr) {
                    track.parent_group = None;
                    break;
                }
                cursor = parent_map.get(&curr).copied().flatten();
            }
        }

        // Ensure duration_frames encompasses all clips
        let max_end = self.tracks.iter().flat_map(|t| &t.clips).map(Clip::end_frame).max().unwrap_or(1);
        self.duration_frames = self.duration_frames.max(max_end).clamp(1, MAX_TIMELINE_DURATION);
        self.playhead = self.playhead.min(self.duration_frames.saturating_sub(1));
        self.work_area = self.work_area.map(|w| w.repaired(self.duration_frames));
        self.next_id = self.next_id.max(max_seen_id.saturating_add(1));

        // Ensure active_track and active_clip point to existing items
        if self.active_track.is_none_or(|id| !self.tracks.iter().any(|t| t.id == id)) {
            self.active_track = self.tracks.last().map(|t| t.id);
        }
        if self.active_clip.is_none_or(|cid| !self.tracks.iter().flat_map(|t| &t.clips).any(|c| c.id == cid)) {
            self.active_clip = self
                .active_track
                .and_then(|tid| self.track(tid))
                .and_then(|t| t.clips.first())
                .map(|c| c.id)
                .or_else(|| self.tracks.iter().flat_map(|t| &t.clips).next().map(|c| c.id));
        }
        self
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.tracks.iter().flat_map(|t| &t.clips).find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.tracks.iter_mut().flat_map(|t| &mut t.clips).find(|c| c.id == id)
    }

    pub fn track_of_clip(&self, clip_id: ClipId) -> Option<TrackId> {
        self.tracks.iter().find(|t| t.clips.iter().any(|c| c.id == clip_id)).map(|t| t.id)
    }

    /// All layers referenced by any clip in the timeline.
    pub fn managed_layers(&self) -> HashSet<LayerId> {
        self.tracks.iter().flat_map(|t| &t.clips).flat_map(Clip::referenced_layers).collect()
    }

    /// Adds a new empty track at the top of the timeline and makes it active.
    pub fn add_track(&mut self, name: Option<&str>, is_group: bool) -> TrackId {
        let id = self.alloc_track_id();
        let count = self.tracks.len() + 1;
        let color = TRACK_COLORS.get((count - 1) % TRACK_COLORS.len()).copied().unwrap_or(TRACK_COLORS[0]);
        let default_name = if is_group { format!("Group {count}") } else { format!("Track {count}") };
        let name = name.map(str::trim).filter(|s| !s.is_empty()).map_or(default_name, ToOwned::to_owned);
        self.tracks.push(Track {
            id,
            name,
            visible: true,
            locked: false,
            solo: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            color_tag: color,
            parent_group: None,
            is_group,
            expanded: true,
            clips: Vec::new(),
            keyframes: KeyframeTrack::default(),
        });
        self.active_track = Some(id);
        id
    }

    /// Adds a `CelClip` on `track_id` starting at `start_frame` with `initial_layer`.
    pub fn add_cel_clip(
        &mut self,
        track_id: TrackId,
        start_frame: u32,
        initial_layer: LayerId,
        hold: u32,
    ) -> Option<ClipId> {
        let clip_id = self.alloc_clip_id();
        let track = self.track_mut(track_id)?;
        let idx = track.clips.len() + 1;
        let cel = Cel { layer: initial_layer, hold: hold.min(600) };
        let dur = cel.duration_frames();
        let clip = Clip {
            id: clip_id,
            name: format!("Drawing {idx}"),
            start_frame,
            duration_frames: dur,
            blend: BlendMode::Normal,
            opacity: 1.0,
            kind: ClipKind::Cel { cels: vec![cel] },
            keyframes: KeyframeTrack::default(),
        };
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(track_id);
        self.active_clip = Some(clip_id);
        Some(clip_id)
    }

    /// Adds a static `LayerClip` on `track_id` starting at `start_frame` for `duration_frames`.
    pub fn add_layer_clip(
        &mut self,
        track_id: TrackId,
        start_frame: u32,
        duration_frames: u32,
        layer: LayerId,
        name: Option<&str>,
    ) -> Option<ClipId> {
        let clip_id = self.alloc_clip_id();
        let track = self.track_mut(track_id)?;
        let dur = duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        let default_name = format!("Content {}", track.clips.len() + 1);
        let name = name.map(str::trim).filter(|s| !s.is_empty()).map_or(default_name, ToOwned::to_owned);
        let clip = Clip {
            id: clip_id,
            name,
            start_frame,
            duration_frames: dur,
            blend: BlendMode::Normal,
            opacity: 1.0,
            kind: ClipKind::Layer { layer },
            keyframes: KeyframeTrack::default(),
        };
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(track_id);
        self.active_clip = Some(clip_id);
        Some(clip_id)
    }

    /// Moves `clip_id` to `new_start_frame` (and optionally to `target_track`), shifting its
    /// keyframes by the same frame delta.
    pub fn move_clip(&mut self, clip_id: ClipId, new_start_frame: u32, target_track: Option<TrackId>) -> bool {
        let Some(src_track_id) = self.track_of_clip(clip_id) else {
            return false;
        };
        if self.track(src_track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let dst_track_id = target_track.unwrap_or(src_track_id);
        if self.track(dst_track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let new_start = new_start_frame.min(MAX_TIMELINE_DURATION.saturating_sub(1));
        if src_track_id == dst_track_id {
            let Some(clip) = self.clip_mut(clip_id) else { return false };
            let delta = i64::from(new_start) - i64::from(clip.start_frame);
            clip.start_frame = new_start;
            clip.keyframes.shift(delta);
            let end = clip.end_frame();
            if let Some(track) = self.track_mut(src_track_id) {
                track.clips.sort_by_key(|c| c.start_frame);
            }
            self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
            return true;
        }
        let Some(src_track) = self.track_mut(src_track_id) else { return false };
        let Some(pos) = src_track.clips.iter().position(|c| c.id == clip_id) else { return false };
        let mut clip = src_track.clips.remove(pos);
        let delta = i64::from(new_start) - i64::from(clip.start_frame);
        clip.start_frame = new_start;
        clip.keyframes.shift(delta);
        let end = clip.end_frame();
        let Some(dst_track) = self.track_mut(dst_track_id) else { return false };
        dst_track.clips.push(clip);
        dst_track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(dst_track_id);
        self.active_clip = Some(clip_id);
        true
    }

    /// Resizes `clip_id`'s duration (or its last cel's hold if it is a `CelClip`).
    pub fn resize_clip(&mut self, clip_id: ClipId, new_duration_frames: u32) -> bool {
        let Some(track_id) = self.track_of_clip(clip_id) else { return false };
        if self.track(track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let target_dur = new_duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        let Some(clip) = self.clip_mut(clip_id) else { return false };
        match &mut clip.kind {
            ClipKind::Cel { cels } => {
                if cels.is_empty() {
                    return false;
                }
                let others: u32 = cels
                    .iter()
                    .take(cels.len().saturating_sub(1))
                    .map(|c| c.duration_frames())
                    .fold(0u32, u32::saturating_add);
                if let Some(last) = cels.last_mut() {
                    let rem = target_dur.saturating_sub(others).max(1);
                    last.hold = rem.saturating_sub(1).min(600);
                }
                clip.sync_duration();
            }
            ClipKind::Layer { .. } | ClipKind::Group { .. } => {
                clip.duration_frames = target_dur;
            }
        }
        let end = clip.end_frame();
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        true
    }

    /// Splits `clip_id` at `at_frame` into two consecutive clips on the same track, returning the
    /// newly created right-hand clip's [`ClipId`].
    pub fn split_clip(&mut self, clip_id: ClipId, at_frame: u32) -> Option<ClipId> {
        let track_id = self.track_of_clip(clip_id)?;
        if self.track(track_id).is_some_and(|t| t.locked) {
            return None;
        }
        let clip = self.clip(clip_id)?.clone();
        if at_frame <= clip.start_frame || at_frame >= clip.end_frame() {
            return None;
        }
        let new_clip_id = self.alloc_clip_id();
        let left_dur = at_frame.saturating_sub(clip.start_frame);
        let right_dur = clip.end_frame().saturating_sub(at_frame);

        let mut left_kf = clip.keyframes.clone();
        left_kf.transform.retain(|k| k.frame < at_frame);
        left_kf.opacity.retain(|k| k.frame < at_frame);

        let mut right_kf = clip.keyframes.clone();
        right_kf.transform.retain(|k| k.frame >= at_frame);
        right_kf.opacity.retain(|k| k.frame >= at_frame);

        let (left_kind, right_kind) = match &clip.kind {
            ClipKind::Layer { layer } => (ClipKind::Layer { layer: *layer }, ClipKind::Layer { layer: *layer }),
            ClipKind::Group { members } => {
                (ClipKind::Group { members: members.clone() }, ClipKind::Group { members: members.clone() })
            }
            ClipKind::Cel { cels } => {
                let (cel_idx, cel, offset) = clip.cel_at(at_frame)?;
                if offset == 0 && cel_idx > 0 {
                    let left_cels = cels.get(..cel_idx)?.to_vec();
                    let right_cels = cels.get(cel_idx..)?.to_vec();
                    (ClipKind::Cel { cels: left_cels }, ClipKind::Cel { cels: right_cels })
                } else {
                    // Split across a held cel
                    let total_hold = cel.hold;
                    if offset == 0 || offset > total_hold {
                        return None;
                    }
                    let mut left_cels = cels.get(..cel_idx)?.to_vec();
                    left_cels.push(Cel { layer: cel.layer, hold: offset.saturating_sub(1) });
                    let mut right_cels = vec![Cel { layer: cel.layer, hold: total_hold.saturating_sub(offset) }];
                    if let Some(rest) = cels.get(cel_idx + 1..) {
                        right_cels.extend_from_slice(rest);
                    }
                    (ClipKind::Cel { cels: left_cels }, ClipKind::Cel { cels: right_cels })
                }
            }
        };

        let track = self.track_mut(track_id)?;
        let left_clip = track.clips.iter_mut().find(|c| c.id == clip_id)?;
        left_clip.duration_frames = left_dur;
        left_clip.kind = left_kind;
        left_clip.keyframes = left_kf;
        left_clip.sync_duration();

        let mut right_clip = Clip {
            id: new_clip_id,
            name: format!("{} (2)", clip.name),
            start_frame: at_frame,
            duration_frames: right_dur,
            blend: clip.blend,
            opacity: clip.opacity,
            kind: right_kind,
            keyframes: right_kf,
        };
        right_clip.sync_duration();
        track.clips.push(right_clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.active_clip = Some(new_clip_id);
        Some(new_clip_id)
    }

    /// Groups the given `track_ids` under a new Group [`Track`], returning the group track's [`TrackId`].
    pub fn group_tracks(&mut self, track_ids: &[TrackId], name: Option<&str>) -> Option<TrackId> {
        let valid: Vec<TrackId> =
            track_ids.iter().copied().filter(|id| self.tracks.iter().any(|t| t.id == *id)).collect();
        if valid.is_empty() {
            return None;
        }
        let group_id = self.add_track(name.or(Some("Group")), true);
        for tid in valid {
            if tid != group_id
                && let Some(t) = self.track_mut(tid)
            {
                t.parent_group = Some(group_id);
            }
        }
        Some(group_id)
    }

    /// Appends or inserts a new cel with `layer` into the active `CelClip` (or creates a `CelClip`
    /// on the active track if none exists), advancing the playhead to the new cel.
    pub fn insert_cel(
        &mut self,
        clip_id: ClipId,
        after_cel_index: Option<usize>,
        layer: LayerId,
        hold: u32,
    ) -> Option<usize> {
        let track_id = self.track_of_clip(clip_id)?;
        if self.track(track_id).is_some_and(|t| t.locked) {
            return None;
        }
        let clip = self.clip_mut(clip_id)?;
        let ClipKind::Cel { cels } = &mut clip.kind else {
            return None;
        };
        let insert_at = after_cel_index.map(|i| (i + 1).min(cels.len())).unwrap_or(cels.len());
        cels.insert(insert_at, Cel { layer, hold: hold.min(600) });
        clip.sync_duration();
        let new_cel_frame = clip.cel_start_frame(insert_at);
        let end = clip.end_frame();
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        if let Some(f) = new_cel_frame {
            self.playhead = f.min(self.duration_frames.saturating_sub(1));
        }
        Some(insert_at)
    }

    /// Evaluates the combined hierarchical [`Transform2D`] of `track_id` at `frame` as an [`Affine`] matrix.
    pub fn evaluate_track_affine(&self, track_id: TrackId, frame: f32, cw: u32, ch: u32) -> Affine {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut cursor = Some(track_id);
        while let Some(tid) = cursor {
            if !seen.insert(tid) {
                break;
            }
            if let Some(track) = self.track(tid) {
                chain.push(track.keyframes.evaluate_transform(frame).to_affine(cw, ch));
                cursor = track.parent_group;
            } else {
                break;
            }
        }
        // Apply from innermost track outward to root group track
        let mut acc = Affine::IDENTITY;
        for m in chain {
            acc = acc.then(m);
        }
        acc
    }

    /// Evaluates the combined hierarchical opacity of `track_id` at `frame`.
    pub fn evaluate_track_opacity(&self, track_id: TrackId, frame: f32) -> f32 {
        let mut opacity = 1.0_f32;
        let mut seen = HashSet::new();
        let mut cursor = Some(track_id);
        while let Some(tid) = cursor {
            if !seen.insert(tid) {
                break;
            }
            if let Some(track) = self.track(tid) {
                if !track.visible {
                    return 0.0;
                }
                opacity *= track.opacity.clamp(0.0, 1.0) * track.keyframes.evaluate_opacity(frame);
                cursor = track.parent_group;
            } else {
                break;
            }
        }
        opacity.clamp(0.0, 1.0)
    }

    /// Deterministic evaluation of the timeline at `frame` (supports integer or sub-frame),
    /// producing the bottom-to-top compositing list and active onion skins.
    pub fn evaluate(&self, doc: &Document, frame: f32, playing: bool) -> EvaluatedScene {
        let frame = if frame.is_finite() { frame.max(0.0) } else { 0.0 };
        let frame_u32 = frame.floor() as u32;
        let (cw, ch) = (doc.width(), doc.height());
        let any_solo = self.tracks.iter().any(|t| t.solo && t.visible);
        let managed = self.managed_layers();
        let mut items = Vec::new();

        for track in &self.tracks {
            if !track.visible || track.is_group {
                continue;
            }
            if any_solo && !self.track_or_ancestor_is_solo(track.id) {
                continue;
            }
            let track_opacity = self.evaluate_track_opacity(track.id, frame);
            if track_opacity <= 0.0 {
                continue;
            }
            let track_affine = self.evaluate_track_affine(track.id, frame, cw, ch);

            for clip in &track.clips {
                let clip_base_opacity = track_opacity * clip.opacity.clamp(0.0, 1.0);
                if clip_base_opacity <= 0.0 {
                    continue;
                }
                let is_active_focus = !self.onion.active_track_only
                    || self.active_clip == Some(clip.id)
                    || (self.active_clip.is_none() && self.active_track == Some(track.id));

                // 1. Onion skin cels BEFORE the current frame (for CelClips when !playing && onion.enabled)
                let mut onion_after = Vec::new();
                if !playing
                    && self.onion.enabled
                    && is_active_focus
                    && let ClipKind::Cel { cels } = &clip.kind
                    && !cels.is_empty()
                {
                    let current_cel_idx = self.nearest_cel_index(clip, frame_u32);
                    if let Some(curr_idx) = current_cel_idx {
                        let clip_kf_opacity = clip.keyframes.evaluate_opacity(frame);
                        let clip_affine = clip.keyframes.evaluate_transform(frame).to_affine(cw, ch).then(track_affine);
                        let n_before = self.onion.before_frames as usize;
                        let n_after = self.onion.after_frames as usize;
                        let base_onion = self.onion.opacity.clamp(0.0, 1.0);
                        let base_sq = base_onion * base_onion;

                        if n_before > 0 {
                            let start_idx = curr_idx.saturating_sub(n_before);
                            for idx in start_idx..curr_idx {
                                let distance = curr_idx - idx;
                                let fade = 1.0 - (distance.saturating_sub(1) as f32) / (n_before as f32);
                                let onion_alpha =
                                    (base_sq * fade * clip_base_opacity * clip_kf_opacity).clamp(0.0, 1.0);
                                if onion_alpha > 1e-4
                                    && let Some(cel) = cels.get(idx)
                                {
                                    let [r, g, b] = self.onion.before_color;
                                    items.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: clip_affine,
                                        opacity: onion_alpha,
                                        blend: BlendMode::Normal,
                                        tint: [r, g, b, 1.0],
                                        is_onion: true,
                                    });
                                }
                            }
                        }

                        if n_after > 0 {
                            let end_idx = (curr_idx + n_after).min(cels.len().saturating_sub(1));
                            for idx in (curr_idx + 1)..=end_idx {
                                let distance = idx - curr_idx;
                                let fade = 1.0 - (distance.saturating_sub(1) as f32) / (n_after as f32);
                                let onion_alpha =
                                    (base_sq * fade * clip_base_opacity * clip_kf_opacity).clamp(0.0, 1.0);
                                if onion_alpha > 1e-4
                                    && let Some(cel) = cels.get(idx)
                                {
                                    let [r, g, b] = self.onion.after_color;
                                    onion_after.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: clip_affine,
                                        opacity: onion_alpha,
                                        blend: BlendMode::Normal,
                                        tint: [r, g, b, 1.0],
                                        is_onion: true,
                                    });
                                }
                            }
                        }
                    }
                }

                if self.onion.blend_primary {
                    items.append(&mut onion_after);
                }

                // 2. Active primary content of the clip at `frame_u32`
                if clip.contains_frame(frame_u32) {
                    let kf_opacity = clip.keyframes.evaluate_opacity(frame);
                    let final_opacity = (clip_base_opacity * kf_opacity).clamp(0.0, 1.0);
                    if final_opacity > 1e-4 {
                        let clip_affine = clip.keyframes.evaluate_transform(frame).to_affine(cw, ch).then(track_affine);
                        let effective_blend = if clip.blend != BlendMode::Normal { clip.blend } else { track.blend };
                        match &clip.kind {
                            ClipKind::Cel { .. } => {
                                if let Some((_, cel, _)) = clip.cel_at(frame_u32) {
                                    items.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: clip_affine,
                                        opacity: final_opacity,
                                        blend: effective_blend,
                                        tint: [0.0; 4],
                                        is_onion: false,
                                    });
                                }
                            }
                            ClipKind::Layer { layer } => {
                                items.push(EvaluatedLayer {
                                    layer: *layer,
                                    track: track.id,
                                    clip: clip.id,
                                    transform: clip_affine,
                                    opacity: final_opacity,
                                    blend: effective_blend,
                                    tint: [0.0; 4],
                                    is_onion: false,
                                });
                            }
                            ClipKind::Group { members } => {
                                for &member in members {
                                    items.push(EvaluatedLayer {
                                        layer: member,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: clip_affine,
                                        opacity: final_opacity,
                                        blend: effective_blend,
                                        tint: [0.0; 4],
                                        is_onion: false,
                                    });
                                }
                            }
                        }
                    }
                }

                if !self.onion.blend_primary {
                    items.extend(onion_after);
                }
            }
        }

        EvaluatedScene { frame, playing, items, managed_layers: managed }
    }

    fn track_or_ancestor_is_solo(&self, track_id: TrackId) -> bool {
        let mut seen = HashSet::new();
        let mut cursor = Some(track_id);
        while let Some(tid) = cursor {
            if !seen.insert(tid) {
                break;
            }
            if let Some(track) = self.track(tid) {
                if track.solo {
                    return true;
                }
                cursor = track.parent_group;
            } else {
                break;
            }
        }
        false
    }

    fn nearest_cel_index(&self, clip: &Clip, frame: u32) -> Option<usize> {
        let ClipKind::Cel { cels } = &clip.kind else {
            return None;
        };
        if cels.is_empty() {
            return None;
        }
        if let Some((idx, _, _)) = clip.cel_at(frame) {
            return Some(idx);
        }
        if frame < clip.start_frame { Some(0) } else { Some(cels.len().saturating_sub(1)) }
    }
}

/// Simplifies raw live-recorded Perform [`MotionSample`]s into smooth keyframes using
/// Ramer–Douglas–Peucker (RDP) curve simplification controlled by `filtering` (`0.0..=1.0`).
///
/// - `filtering == 0.0`: preserves every distinct frame sample (no smoothing).
/// - `filtering > 0.0`: progressively filters out hand jitter and intermediate points along
///   near-linear segments, producing clean editable keyframes with [`Easing::EaseInOut`] / [`Easing::Linear`].
pub fn simplify_motion_samples(samples: &[MotionSample], filtering: f32) -> Vec<Keyframe<Transform2D>> {
    if samples.is_empty() {
        return Vec::new();
    }
    let filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { 0.35 };

    // Deduplicate by frame (keep latest sample for each frame) and sort by frame
    let mut sorted: Vec<MotionSample> =
        samples.iter().map(|s| MotionSample { frame: s.frame, transform: s.transform.repaired() }).collect();
    sorted.sort_by_key(|s| s.frame);
    let mut unique: Vec<MotionSample> = Vec::with_capacity(sorted.len());
    for s in sorted {
        if let Some(last) = unique.last_mut()
            && last.frame == s.frame
        {
            *last = s;
        } else {
            unique.push(s);
        }
    }

    if unique.len() <= 2 || filtering <= 1e-4 {
        return unique
            .into_iter()
            .map(|s| Keyframe { frame: s.frame, value: s.transform, easing: Easing::Linear })
            .collect();
    }

    // Tolerance scales quadratically with filtering:
    // At filtering = 0.35 -> ~6 px position tolerance; at 1.0 -> ~48 px position tolerance.
    let tolerance = filtering * filtering * 48.0 + filtering * 2.0;
    let mut keep = vec![false; unique.len()];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(last) = keep.last_mut() {
        *last = true;
    }

    // Iterative RDP stack to avoid deep recursion
    let mut stack = vec![(0usize, unique.len().saturating_sub(1))];
    while let Some((start_idx, end_idx)) = stack.pop() {
        if end_idx <= start_idx + 1 {
            continue;
        }
        let (Some(start), Some(end)) = (unique.get(start_idx), unique.get(end_idx)) else {
            continue;
        };
        let span = (end.frame.saturating_sub(start.frame)).max(1) as f32;
        let mut max_err = 0.0_f32;
        let mut split_idx = None;

        for idx in (start_idx + 1)..end_idx {
            let Some(sample) = unique.get(idx) else { continue };
            let t = (sample.frame.saturating_sub(start.frame) as f32 / span).clamp(0.0, 1.0);
            let expected = start.transform.lerp(end.transform, t);
            let err = transform_distance(sample.transform, expected);
            if err > max_err {
                max_err = err;
                split_idx = Some(idx);
            }
        }

        if max_err > tolerance
            && let Some(mid) = split_idx
        {
            if let Some(slot) = keep.get_mut(mid) {
                *slot = true;
            }
            stack.push((start_idx, mid));
            stack.push((mid, end_idx));
        }
    }

    let easing = if filtering >= 0.25 { Easing::EaseInOut } else { Easing::Linear };
    unique
        .into_iter()
        .zip(keep)
        .filter_map(|(s, k)| k.then_some(Keyframe { frame: s.frame, value: s.transform, easing }))
        .collect()
}

/// Perceptual distance between two [`Transform2D`] states in equivalent pixel units.
fn transform_distance(a: Transform2D, b: Transform2D) -> f32 {
    let dx = a.position[0] - b.position[0];
    let dy = a.position[1] - b.position[1];
    // 100% scale change or 1 radian rotation weights roughly like 200 pixels of motion
    let dsx = (a.scale[0] - b.scale[0]) * 200.0;
    let dsy = (a.scale[1] - b.scale[1]) * 200.0;
    let drot = (a.rotation - b.rotation) * 200.0;
    let dax = (a.anchor[0] - b.anchor[0]) * 200.0;
    let day = (a.anchor[1] - b.anchor[1]) * 200.0;
    (dx * dx + dy * dy + dsx * dsx + dsy * dsy + drot * drot + dax * dax + day * day).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_curves_interpolate_monotonically_and_hold_steps() {
        assert_eq!(Easing::Hold.sample(0.0), 0.0);
        assert_eq!(Easing::Hold.sample(0.5), 0.0);
        assert_eq!(Easing::Hold.sample(0.999), 0.0);

        assert!((Easing::Linear.sample(0.25) - 0.25).abs() < 1e-6);
        assert!((Easing::Linear.sample(0.75) - 0.75).abs() < 1e-6);

        // EaseIn starts slower than linear; EaseOut starts faster than linear
        assert!(Easing::EaseIn.sample(0.25) < 0.25);
        assert!(Easing::EaseOut.sample(0.25) > 0.25);
        assert!((Easing::EaseInOut.sample(0.5) - 0.5).abs() < 0.02);

        // NaN and out-of-range inputs are handled safely
        assert_eq!(Easing::EaseInOut.sample(f32::NAN), 0.0);
        assert_eq!(Easing::EaseInOut.sample(-5.0), 0.0);
        assert_eq!(Easing::EaseInOut.sample(5.0), 1.0);
    }

    #[test]
    fn keyframe_track_evaluates_transform_and_opacity_across_keyframes() {
        let mut kt = KeyframeTrack::default();
        kt.set_transform(
            0,
            Transform2D { position: [0.0, 0.0], scale: [1.0, 1.0], rotation: 0.0, anchor: [0.5, 0.5] },
            Some(Easing::Linear),
        );
        kt.set_transform(
            10,
            Transform2D { position: [100.0, -50.0], scale: [2.0, 2.0], rotation: 1.0, anchor: [0.5, 0.5] },
            Some(Easing::Hold),
        );
        kt.set_transform(
            20,
            Transform2D { position: [300.0, 50.0], scale: [1.0, 1.0], rotation: 0.0, anchor: [0.5, 0.5] },
            Some(Easing::Linear),
        );

        let mid = kt.evaluate_transform(5.0);
        assert!((mid.position[0] - 50.0).abs() < 1e-4);
        assert!((mid.position[1] - (-25.0)).abs() < 1e-4);
        assert!((mid.scale[0] - 1.5).abs() < 1e-4);
        assert!((mid.rotation - 0.5).abs() < 1e-4);

        // Between frame 10 and 20 is Hold easing, so frame 15 stays at frame 10's value
        let held = kt.evaluate_transform(15.0);
        assert!((held.position[0] - 100.0).abs() < 1e-4);

        let end = kt.evaluate_transform(20.0);
        assert!((end.position[0] - 300.0).abs() < 1e-4);

        kt.set_opacity(0, 0.2, Some(Easing::Linear));
        kt.set_opacity(10, 0.8, Some(Easing::Linear));
        assert!((kt.evaluate_opacity(5.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn transform2d_respects_custom_anchor_pivot() {
        // Pure scale by 2x around anchor (0.25, 0.25) on a 100x100 canvas (pivot at (25, 25)):
        // The pivot point (25, 25) must remain fixed at (25, 25)!
        let t = Transform2D { position: [0.0, 0.0], scale: [2.0, 2.0], rotation: 0.0, anchor: [0.25, 0.25] };
        let affine = t.to_affine(100, 100);
        let mapped_pivot = affine.apply(Vec2::new(25.0, 25.0));
        assert!((mapped_pivot.x - 25.0).abs() < 1e-4 && (mapped_pivot.y - 25.0).abs() < 1e-4);

        // And (50, 50) moves twice as far from (25, 25) -> (75, 75)
        let mapped_center = affine.apply(Vec2::new(50.0, 50.0));
        assert!((mapped_center.x - 75.0).abs() < 1e-4 && (mapped_center.y - 75.0).abs() < 1e-4);
    }

    #[test]
    fn timeline_evaluates_multitrack_hierarchy_cels_and_onion_skins() {
        let mut doc = Document::new(200, 100);
        let bg_layer = doc.current();
        let cel1 = doc.add_layer();
        let cel2 = doc.add_layer();

        let mut tl = Timeline::new_with_layer(cel1);
        let main_track_id = tl.active_track.unwrap();
        let main_clip_id = tl.active_clip.unwrap();

        // Set cel1 hold to 1 (2 frames: 0 and 1), and add cel2 (frame 2)
        if let Some(Clip { kind: ClipKind::Cel { cels }, .. }) = tl.clip_mut(main_clip_id) {
            cels[0].hold = 1;
        }
        tl.clip_mut(main_clip_id).unwrap().sync_duration();
        tl.insert_cel(main_clip_id, Some(0), cel2, 0);

        // Add a background track at the bottom
        let bg_track_id = tl.add_track(Some("BG"), false);
        let bg_clip_id = tl.add_layer_clip(bg_track_id, 0, 10, bg_layer, Some("BG Clip")).unwrap();
        // Put BG track at index 0 (bottom)
        tl.tracks.swap(0, 1);
        tl.active_track = Some(main_track_id);
        tl.active_clip = Some(main_clip_id);

        // Evaluate at frame 2 while drawing (playing = false):
        // Should composite BG layer (no onion skin), cel1 (before onion skin), and cel2 (primary)
        let scene = tl.evaluate(&doc, 2.0, false);
        assert_eq!(scene.items.len(), 3);
        assert_eq!(scene.items[0].layer, bg_layer);
        assert!(!scene.items[0].is_onion);
        assert_eq!(scene.items[0].clip, bg_clip_id);

        assert_eq!(scene.items[1].layer, cel1);
        assert!(scene.items[1].is_onion);
        assert_eq!([scene.items[1].tint[0], scene.items[1].tint[1], scene.items[1].tint[2]], tl.onion.before_color);

        assert_eq!(scene.items[2].layer, cel2);
        assert!(!scene.items[2].is_onion);

        // When playing = true, onion skins are hidden automatically
        let playing_scene = tl.evaluate(&doc, 2.0, true);
        assert_eq!(playing_scene.items.len(), 2);
        assert!(playing_scene.items.iter().all(|i| !i.is_onion));
    }

    #[test]
    fn legacy_animation_migrates_background_holds_and_foreground_tracks() {
        let mut doc = Document::new(100, 100);
        let bg = doc.current();
        let f1 = doc.add_layer();
        let f2 = doc.add_layer();
        let fg = doc.add_layer();

        let mut anim = Animation { fps: 12, background: true, foreground: true, ..Animation::default() };
        anim.set_hold(f1, 2);

        let tl = Timeline::from_legacy_animation(&doc, &anim);
        assert_eq!(tl.fps, 12);
        assert_eq!(tl.tracks.len(), 3);
        assert_eq!(tl.tracks[0].name, "Background");
        assert_eq!(tl.tracks[2].name, "Foreground");

        // At frame 0: bg, f1, fg
        let s0 = tl.evaluate(&doc, 0.0, true);
        let layers0: Vec<LayerId> = s0.items.iter().map(|i| i.layer).collect();
        assert_eq!(layers0, vec![bg, f1, fg]);

        // At frame 2 (still on f1 because hold = 2 -> frames 0, 1, 2): bg, f1, fg
        let s2 = tl.evaluate(&doc, 2.0, true);
        let layers2: Vec<LayerId> = s2.items.iter().map(|i| i.layer).collect();
        assert_eq!(layers2, vec![bg, f1, fg]);

        // At frame 3 (now on f2): bg, f2, fg
        let s3 = tl.evaluate(&doc, 3.0, true);
        let layers3: Vec<LayerId> = s3.items.iter().map(|i| i.layer).collect();
        assert_eq!(layers3, vec![bg, f2, fg]);
    }

    #[test]
    fn perform_motion_filtering_simplifies_collinear_samples_and_preserves_corners() {
        // Straight motion from frame 0 (x=0) to frame 10 (x=100), then sharp turn to frame 20 (x=100, y=100)
        let mut samples = Vec::new();
        for f in 0..=10 {
            samples.push(MotionSample {
                frame: f,
                transform: Transform2D { position: [f as f32 * 10.0, 0.0], ..Transform2D::default() },
            });
        }
        for f in 11..=20 {
            samples.push(MotionSample {
                frame: f,
                transform: Transform2D { position: [100.0, (f - 10) as f32 * 10.0], ..Transform2D::default() },
            });
        }

        // With filtering = 0.0, all 21 frames are kept
        let raw = simplify_motion_samples(&samples, 0.0);
        assert_eq!(raw.len(), 21);

        // With filtering = 0.5, only the 3 key corners (frame 0, frame 10, frame 20) are kept!
        let simplified = simplify_motion_samples(&samples, 0.5);
        let frames: Vec<u32> = simplified.iter().map(|k| k.frame).collect();
        assert_eq!(frames, vec![0, 10, 20]);
    }

    #[test]
    fn splitting_and_moving_clips_preserve_duration_and_shift_keyframes() {
        let l1 = LayerId(1);
        let mut tl = Timeline::new_with_layer(l1);
        let track_id = tl.active_track.unwrap();
        let clip_id = tl.add_layer_clip(track_id, 10, 20, l1, Some("Block")).unwrap();
        tl.clip_mut(clip_id).unwrap().keyframes.set_transform(
            15,
            Transform2D { position: [40.0, 20.0], ..Transform2D::default() },
            Some(Easing::Linear),
        );

        // Move clip from 10 to 30 (+20 frames) -> keyframe shifts from 15 to 35
        assert!(tl.move_clip(clip_id, 30, None));
        assert_eq!(tl.clip(clip_id).unwrap().keyframes.transform[0].frame, 35);

        // Split clip (30..50) at frame 40 -> left (30..40) keeps keyframe at 35, right (40..50) has none
        let right_id = tl.split_clip(clip_id, 40).unwrap();
        assert_eq!(tl.clip(clip_id).unwrap().duration_frames, 10);
        assert_eq!(tl.clip(clip_id).unwrap().keyframes.transform.len(), 1);
        assert_eq!(tl.clip(right_id).unwrap().start_frame, 40);
        assert_eq!(tl.clip(right_id).unwrap().duration_frames, 10);
        assert!(tl.clip(right_id).unwrap().keyframes.transform.is_empty());
    }
}
