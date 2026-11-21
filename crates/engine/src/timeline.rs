//! Multi-track timeline, clips, cels, non-destructive keyframe animation (`Transform2D` with
//! `anchor`, and `opacity`), easing curves, per-track onion skinning, and live Perform motion
//! filtering (with Krita's animation architecture as a reference).

use std::collections::{HashMap, HashSet};

use crate::animation::{Animation, MAX_ONION_FRAMES, Playback};
use crate::audio::{AudioAsset, AudioAssetId, stereo_pan_gains};
use crate::document::{BlendMode, Document, LayerId};
use crate::geom::{Affine, Homography, Vec2};
use crate::motion::Bezier;
use crate::warp;

/// Unique identifier of a [`Track`] in a [`Timeline`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TrackId(pub u64);

/// Unique identifier of a [`Clip`] in a [`Track`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ClipId(pub u64);

/// Unique identifier of a [`TimelineMarker`] on the timeline ruler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct MarkerId(pub u64);

fn default_marker_color() -> u32 {
    0xFFF59E0B
}

/// A named beat / section marker placed on the timeline ruler.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimelineMarker {
    pub id: MarkerId,
    pub frame: u32,
    pub label: String,
    #[serde(default = "default_marker_color")]
    pub color: u32,
}

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

    /// Moves `anchor` to `new_anchor` (normalized `[0..1]` canvas coords) while compensating
    /// `position` so the rendered pixels on the canvas do not shift at all.
    pub fn with_anchor_preserving_visual(self, new_anchor: [f32; 2], canvas_width: u32, canvas_height: u32) -> Self {
        let clean = self.repaired();
        let next_anchor = [
            if new_anchor[0].is_finite() { new_anchor[0].clamp(-4.0, 5.0) } else { clean.anchor[0] },
            if new_anchor[1].is_finite() { new_anchor[1].clamp(-4.0, 5.0) } else { clean.anchor[1] },
        ];
        let cw = canvas_width.max(1) as f32;
        let ch = canvas_height.max(1) as f32;
        let dx = (next_anchor[0] - clean.anchor[0]) * cw;
        let dy = (next_anchor[1] - clean.anchor[1]) * ch;
        let sx = dx * clean.scale[0];
        let sy = dy * clean.scale[1];
        let (sin, cos) = clean.rotation.sin_cos();
        let rx = sx * cos - sy * sin;
        let ry = sx * sin + sy * cos;
        Transform2D {
            position: [clean.position[0] + rx - dx, clean.position[1] + ry - dy],
            scale: clean.scale,
            rotation: clean.rotation,
            anchor: next_anchor,
        }
        .repaired()
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

/// Non-destructive live image filter parameters animated via keyframes or Perform mode.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LiveFilterState {
    /// Gaussian blur radius in pixels (`0.0..=128.0`).
    pub gaussian_blur: f32,
    /// Hue shift in degrees (`-180.0..=180.0`).
    pub hue_shift: f32,
    /// Saturation multiplier (`0.0..=2.0`, `1.0` = unchanged).
    pub saturation: f32,
    /// Brightness offset (`-1.0..=1.0`, `0.0` = unchanged).
    pub brightness: f32,
    /// Sharpen intensity (`0.0..=1.0`).
    pub sharpen: f32,
    /// Film grain / noise amount (`0.0..=1.0`).
    pub noise: f32,
}

impl Default for LiveFilterState {
    fn default() -> Self {
        LiveFilterState {
            gaussian_blur: 0.0,
            hue_shift: 0.0,
            saturation: 1.0,
            brightness: 0.0,
            sharpen: 0.0,
            noise: 0.0,
        }
    }
}

impl LiveFilterState {
    pub fn is_identity(&self) -> bool {
        self.gaussian_blur.abs() < 1e-3
            && self.hue_shift.abs() < 1e-3
            && (self.saturation - 1.0).abs() < 1e-3
            && self.brightness.abs() < 1e-3
            && self.sharpen.abs() < 1e-3
            && self.noise.abs() < 1e-3
    }

    pub fn has_hsb(&self) -> bool {
        self.hue_shift.abs() >= 1e-3 || (self.saturation - 1.0).abs() >= 1e-3 || self.brightness.abs() >= 1e-3
    }

    pub fn repaired(mut self) -> Self {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.gaussian_blur = f(self.gaussian_blur, 0.0, 128.0, 0.0);
        self.hue_shift = f(self.hue_shift, -180.0, 180.0, 0.0);
        self.saturation = f(self.saturation, 0.0, 2.0, 1.0);
        self.brightness = f(self.brightness, -1.0, 1.0, 0.0);
        self.sharpen = f(self.sharpen, 0.0, 1.0, 0.0);
        self.noise = f(self.noise, 0.0, 1.0, 0.0);
        self
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let mix = |a: f32, b: f32| a + (b - a) * t;
        LiveFilterState {
            gaussian_blur: mix(self.gaussian_blur, other.gaussian_blur),
            hue_shift: mix(self.hue_shift, other.hue_shift),
            saturation: mix(self.saturation, other.saturation),
            brightness: mix(self.brightness, other.brightness),
            sharpen: mix(self.sharpen, other.sharpen),
            noise: mix(self.noise, other.noise),
        }
        .repaired()
    }

    pub fn combine(self, other: Self) -> Self {
        LiveFilterState {
            gaussian_blur: self.gaussian_blur + other.gaussian_blur,
            hue_shift: self.hue_shift + other.hue_shift,
            saturation: self.saturation * other.saturation,
            brightness: self.brightness + other.brightness,
            sharpen: self.sharpen + other.sharpen,
            noise: self.noise.max(other.noise),
        }
        .repaired()
    }
}

/// Default unit quad corners `(TL, TR, BR, BL)` in normalized `[0..1]` canvas coordinates.
pub const IDENTITY_DISTORT_CORNERS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Default 4×4 bicubic warp control mesh in normalized `[0..1]` canvas coordinates.
pub fn identity_warp_mesh() -> [[f32; 2]; 16] {
    std::array::from_fn(|k| [(k % 4) as f32 / 3.0, (k / 4) as f32 / 3.0])
}

/// Non-destructive keyframeable mesh/perspective deformation (Distort 4 corners or 4×4 Warp mesh).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Deform2D {
    /// Perspective distort corners `(TL, TR, BR, BL)` in normalized canvas coordinates (`0..1`).
    pub distort_corners: Option<[[f32; 2]; 4]>,
    /// Bicubic 4×4 warp control points row-by-row in normalized canvas coordinates (`0..1`).
    pub warp_mesh: Option<[[f32; 2]; 16]>,
}

impl Deform2D {
    pub fn is_identity(&self) -> bool {
        let distort_ok = self.distort_corners.is_none_or(|c| {
            c.iter()
                .zip(IDENTITY_DISTORT_CORNERS)
                .all(|([ax, ay], [bx, by])| (ax - bx).abs() < 1e-4 && (ay - by).abs() < 1e-4)
        });
        let warp_ok = self.warp_mesh.is_none_or(|m| {
            m.iter()
                .zip(identity_warp_mesh())
                .all(|([ax, ay], [bx, by])| (ax - bx).abs() < 1e-4 && (ay - by).abs() < 1e-4)
        });
        distort_ok && warp_ok
    }

    pub fn repaired(mut self) -> Self {
        let clean = |[px, py]: [f32; 2], [fx, fy]: [f32; 2]| {
            [
                if px.is_finite() { px.clamp(-4.0, 5.0) } else { fx },
                if py.is_finite() { py.clamp(-4.0, 5.0) } else { fy },
            ]
        };
        if let Some(c) = self.distort_corners {
            self.distort_corners = Some(std::array::from_fn(|i| {
                let pt = c.get(i).copied().unwrap_or([0.0, 0.0]);
                let fb = IDENTITY_DISTORT_CORNERS.get(i).copied().unwrap_or([0.0, 0.0]);
                clean(pt, fb)
            }));
        }
        if let Some(m) = self.warp_mesh {
            let id = identity_warp_mesh();
            self.warp_mesh = Some(std::array::from_fn(|i| {
                let pt = m.get(i).copied().unwrap_or([0.0, 0.0]);
                let fb = id.get(i).copied().unwrap_or([0.0, 0.0]);
                clean(pt, fb)
            }));
        }
        if self.is_identity() {
            return Deform2D::default();
        }
        self
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let mix_pt = |[ax, ay]: [f32; 2], [bx, by]: [f32; 2]| [ax + (bx - ax) * t, ay + (by - ay) * t];
        let distort_corners = match (self.distort_corners, other.distort_corners) {
            (None, None) => None,
            (a, b) => {
                let ca = a.unwrap_or(IDENTITY_DISTORT_CORNERS);
                let cb = b.unwrap_or(IDENTITY_DISTORT_CORNERS);
                Some(std::array::from_fn(|i| {
                    mix_pt(ca.get(i).copied().unwrap_or([0.0, 0.0]), cb.get(i).copied().unwrap_or([0.0, 0.0]))
                }))
            }
        };
        let warp_mesh = match (self.warp_mesh, other.warp_mesh) {
            (None, None) => None,
            (a, b) => {
                let id = identity_warp_mesh();
                let ma = a.unwrap_or(id);
                let mb = b.unwrap_or(id);
                Some(std::array::from_fn(|i| {
                    mix_pt(ma.get(i).copied().unwrap_or([0.0, 0.0]), mb.get(i).copied().unwrap_or([0.0, 0.0]))
                }))
            }
        };
        Deform2D { distort_corners, warp_mesh }.repaired()
    }

    /// Converts this deformation into a canvas-space 4×4 [`warp::Mesh`] ready for GPU triangulation,
    /// returning `None` if no deformation is active.
    pub fn to_canvas_mesh(self, canvas_width: u32, canvas_height: u32) -> Option<warp::Mesh> {
        if self.is_identity() {
            return None;
        }
        let cw = canvas_width.max(1) as f32;
        let ch = canvas_height.max(1) as f32;
        if let Some(m) = self.warp_mesh {
            return Some(std::array::from_fn(|i| {
                let [x, y] = m.get(i).copied().unwrap_or([0.0, 0.0]);
                Vec2::new(x * cw, y * ch)
            }));
        }
        if let Some(c) = self.distort_corners {
            let quad = std::array::from_fn(|i| {
                let [x, y] = c.get(i).copied().unwrap_or([0.0, 0.0]);
                Vec2::new(x * cw, y * ch)
            });
            return Some(warp::over_quad(quad));
        }
        None
    }

    /// Computes a perspective [`Homography`] when `distort_corners` is active without a warp mesh.
    pub fn to_homography(self, canvas_width: u32, canvas_height: u32) -> Option<Homography> {
        if self.warp_mesh.is_some() {
            return None;
        }
        let c = self.distort_corners?;
        let cw = canvas_width.max(1) as f32;
        let ch = canvas_height.max(1) as f32;
        let from = [Vec2::new(0.0, 0.0), Vec2::new(cw, 0.0), Vec2::new(cw, ch), Vec2::new(0.0, ch)];
        let to = std::array::from_fn(|i| {
            let [x, y] = c.get(i).copied().unwrap_or([0.0, 0.0]);
            Vec2::new(x * cw, y * ch)
        });
        Homography::quad_to_quad(from, to)
    }
}

/// Non-destructive Layer Effects attached to a [`Track`] and rendered in real time on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TrackEffects {
    /// Automatic silhouette stroke/outline around the track's alpha contour.
    pub outline_enabled: bool,
    /// Outline thickness in canvas pixels (`0.0..=32.0`).
    pub outline_width: f32,
    /// Straight sRGB + alpha `[r, g, b, a]` of the outline.
    pub outline_color: [f32; 4],

    /// Drop shadow cast behind the track's silhouette.
    pub shadow_enabled: bool,
    /// Drop shadow pixel offset `(dx, dy)`.
    pub shadow_offset: [f32; 2],
    /// Drop shadow softness radius in pixels (`0.0..=64.0`).
    pub shadow_blur: f32,
    /// Drop shadow straight sRGB + opacity `[r, g, b, a]`.
    pub shadow_color: [f32; 4],

    /// Outer or inner luminous glow around the track's silhouette.
    pub glow_enabled: bool,
    /// Glow radius in pixels (`0.0..=64.0`).
    pub glow_radius: f32,
    /// Glow strength (`0.0..=2.0`).
    pub glow_intensity: f32,
    /// Glow straight sRGB + alpha `[r, g, b, a]`.
    pub glow_color: [f32; 4],
    /// When true, applies the glow inside the alpha edge (rim light); else outer glow.
    pub glow_inner: bool,

    /// Solid color tint/overlay over the entire track.
    pub color_overlay_enabled: bool,
    /// Straight sRGB + strength `[r, g, b, a]` (`a = 0..1` blend factor).
    pub color_overlay_color: [f32; 4],
}

impl Default for TrackEffects {
    fn default() -> Self {
        TrackEffects {
            outline_enabled: false,
            outline_width: 4.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
            shadow_enabled: false,
            shadow_offset: [8.0, 8.0],
            shadow_blur: 10.0,
            shadow_color: [0.0, 0.0, 0.0, 0.55],
            glow_enabled: false,
            glow_radius: 12.0,
            glow_intensity: 0.8,
            glow_color: [0.35, 0.78, 1.0, 1.0],
            glow_inner: false,
            color_overlay_enabled: false,
            color_overlay_color: [1.0, 1.0, 1.0, 0.5],
        }
    }
}

impl TrackEffects {
    pub fn is_active(&self) -> bool {
        let [_, _, _, oa] = self.outline_color;
        let [_, _, _, sa] = self.shadow_color;
        let [_, _, _, ca] = self.color_overlay_color;
        (self.outline_enabled && self.outline_width > 0.01 && oa > 0.01)
            || (self.shadow_enabled && sa > 0.01)
            || (self.glow_enabled && self.glow_radius > 0.01 && self.glow_intensity > 0.01)
            || (self.color_overlay_enabled && ca > 0.01)
    }

    pub fn repaired(mut self) -> Self {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let c4 = |c: [f32; 4]| c.map(|v| f(v, 0.0, 1.0, 0.0));
        let [sx, sy] = self.shadow_offset;
        self.outline_width = f(self.outline_width, 0.0, 32.0, 4.0);
        self.outline_color = c4(self.outline_color);
        self.shadow_offset = [f(sx, -256.0, 256.0, 8.0), f(sy, -256.0, 256.0, 8.0)];
        self.shadow_blur = f(self.shadow_blur, 0.0, 64.0, 10.0);
        self.shadow_color = c4(self.shadow_color);
        self.glow_radius = f(self.glow_radius, 0.0, 64.0, 12.0);
        self.glow_intensity = f(self.glow_intensity, 0.0, 2.0, 0.8);
        self.glow_color = c4(self.glow_color);
        self.color_overlay_color = c4(self.color_overlay_color);
        self
    }
}

/// Sub-tracks of non-destructive keyframes attached to a [`Clip`], [`Track`], or [`Timeline`] Camera.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct KeyframeTrack {
    pub transform: Vec<Keyframe<Transform2D>>,
    pub opacity: Vec<Keyframe<f32>>,
    pub filter: Vec<Keyframe<LiveFilterState>>,
    pub deform: Vec<Keyframe<Deform2D>>,
    pub volume: Vec<Keyframe<f32>>,
    pub pan: Vec<Keyframe<f32>>,
}

impl KeyframeTrack {
    pub fn is_empty(&self) -> bool {
        self.transform.is_empty()
            && self.opacity.is_empty()
            && self.filter.is_empty()
            && self.deform.is_empty()
            && self.volume.is_empty()
            && self.pan.is_empty()
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

        for kf in &mut self.filter {
            kf.value = kf.value.repaired();
            kf.easing = kf.easing.repaired();
        }
        self.filter.sort_by_key(|k| k.frame);
        self.filter.dedup_by_key(|k| k.frame);

        for kf in &mut self.deform {
            kf.value = kf.value.repaired();
            kf.easing = kf.easing.repaired();
        }
        self.deform.sort_by_key(|k| k.frame);
        self.deform.dedup_by_key(|k| k.frame);

        for kf in &mut self.volume {
            kf.value = if kf.value.is_finite() { kf.value.clamp(0.0, 2.0) } else { 1.0 };
            kf.easing = kf.easing.repaired();
        }
        self.volume.sort_by_key(|k| k.frame);
        self.volume.dedup_by_key(|k| k.frame);

        for kf in &mut self.pan {
            kf.value = if kf.value.is_finite() { kf.value.clamp(-1.0, 1.0) } else { 0.0 };
            kf.easing = kf.easing.repaired();
        }
        self.pan.sort_by_key(|k| k.frame);
        self.pan.dedup_by_key(|k| k.frame);
        self
    }

    /// Shifts all keyframes in this track by `delta` frames (clamped at 0).
    pub fn shift(&mut self, delta: i64) {
        let shift_frame = |frame: &mut u32| {
            let shifted = (i64::from(*frame) + delta).max(0);
            *frame = u32::try_from(shifted).unwrap_or(u32::MAX);
        };
        for kf in &mut self.transform {
            shift_frame(&mut kf.frame);
        }
        for kf in &mut self.opacity {
            shift_frame(&mut kf.frame);
        }
        for kf in &mut self.filter {
            shift_frame(&mut kf.frame);
        }
        for kf in &mut self.deform {
            shift_frame(&mut kf.frame);
        }
        for kf in &mut self.volume {
            shift_frame(&mut kf.frame);
        }
        for kf in &mut self.pan {
            shift_frame(&mut kf.frame);
        }
    }

    /// Preserve the two endpoints of any interpolation crossing the cut.
    fn split_at(&self, frame: u32) -> (Self, Self) {
        fn split<T: Copy>(keys: &[Keyframe<T>], frame: u32) -> (Vec<Keyframe<T>>, Vec<Keyframe<T>>) {
            let left_end = keys.partition_point(|key| key.frame < frame);
            let right_start = keys.partition_point(|key| key.frame <= frame);
            // Include one endpoint beyond each piece whenever its easing needs that endpoint.
            let left = keys.get(..(left_end + 1).min(keys.len())).unwrap_or_default();
            let right = keys.get(right_start.saturating_sub(1)..).unwrap_or_default();
            (left.to_vec(), right.to_vec())
        }
        let (lt, rt) = split(&self.transform, frame);
        let (lo, ro) = split(&self.opacity, frame);
        let (lf, rf) = split(&self.filter, frame);
        let (ld, rd) = split(&self.deform, frame);
        let (lv, rv) = split(&self.volume, frame);
        let (lp, rp) = split(&self.pan, frame);
        (
            Self { transform: lt, opacity: lo, filter: lf, deform: ld, volume: lv, pan: lp },
            Self { transform: rt, opacity: ro, filter: rf, deform: rd, volume: rv, pan: rp },
        )
    }

    /// Retains keyframes matching `pred` across all property sub-tracks.
    pub fn retain_frames(&mut self, mut pred: impl FnMut(u32) -> bool) {
        self.transform.retain(|k| pred(k.frame));
        self.opacity.retain(|k| pred(k.frame));
        self.filter.retain(|k| pred(k.frame));
        self.deform.retain(|k| pred(k.frame));
        self.volume.retain(|k| pred(k.frame));
        self.pan.retain(|k| pred(k.frame));
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

    pub fn set_filter(&mut self, frame: u32, value: LiveFilterState, easing: Option<Easing>) {
        let value = value.repaired();
        if let Some(existing) = self.filter.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::EaseInOut).repaired();
        self.filter.push(Keyframe { frame, value, easing });
        self.filter.sort_by_key(|k| k.frame);
    }

    pub fn set_deform(&mut self, frame: u32, value: Deform2D, easing: Option<Easing>) {
        let value = value.repaired();
        if let Some(existing) = self.deform.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::EaseInOut).repaired();
        self.deform.push(Keyframe { frame, value, easing });
        self.deform.sort_by_key(|k| k.frame);
    }

    pub fn set_volume(&mut self, frame: u32, volume: f32, easing: Option<Easing>) {
        let value = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { 1.0 };
        if let Some(existing) = self.volume.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::Linear).repaired();
        self.volume.push(Keyframe { frame, value, easing });
        self.volume.sort_by_key(|k| k.frame);
    }

    pub fn set_pan(&mut self, frame: u32, pan: f32, easing: Option<Easing>) {
        let value = if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 };
        if let Some(existing) = self.pan.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            if let Some(e) = easing {
                existing.easing = e.repaired();
            }
            return;
        }
        let easing = easing.unwrap_or(Easing::Linear).repaired();
        self.pan.push(Keyframe { frame, value, easing });
        self.pan.sort_by_key(|k| k.frame);
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

    pub fn remove_filter(&mut self, frame: u32) -> bool {
        let before = self.filter.len();
        self.filter.retain(|k| k.frame != frame);
        self.filter.len() != before
    }

    pub fn remove_deform(&mut self, frame: u32) -> bool {
        let before = self.deform.len();
        self.deform.retain(|k| k.frame != frame);
        self.deform.len() != before
    }

    pub fn remove_volume(&mut self, frame: u32) -> bool {
        let before = self.volume.len();
        self.volume.retain(|k| k.frame != frame);
        self.volume.len() != before
    }

    pub fn remove_pan(&mut self, frame: u32) -> bool {
        let before = self.pan.len();
        self.pan.retain(|k| k.frame != frame);
        self.pan.len() != before
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

    pub fn set_filter_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.filter.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    pub fn set_deform_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.deform.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    pub fn set_volume_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.volume.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    pub fn set_pan_easing(&mut self, frame: u32, easing: Easing) -> bool {
        if let Some(kf) = self.pan.iter_mut().find(|k| k.frame == frame) {
            kf.easing = easing.repaired();
            true
        } else {
            false
        }
    }

    /// Moves a keyframe on the specified channel from `from_frame` to `to_frame` (replacing any
    /// keyframe already at `to_frame`).
    pub fn move_transform(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.transform, from_frame, to_frame)
    }

    pub fn move_opacity(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.opacity, from_frame, to_frame)
    }

    pub fn move_filter(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.filter, from_frame, to_frame)
    }

    pub fn move_deform(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.deform, from_frame, to_frame)
    }

    pub fn move_volume(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.volume, from_frame, to_frame)
    }

    pub fn move_pan(&mut self, from_frame: u32, to_frame: u32) -> bool {
        move_kf(&mut self.pan, from_frame, to_frame)
    }

    /// Mirrors all keyframes inside `[start..end)` horizontally around the clip midpoint.
    pub fn reverse_range(&mut self, start: u32, end: u32) {
        if end <= start.saturating_add(1) {
            return;
        }
        let last = end.saturating_sub(1);
        let mirror = |f: &mut u32| {
            if *f >= start && *f <= last {
                *f = start.saturating_add(last.saturating_sub(*f));
            }
        };
        for kf in &mut self.transform {
            mirror(&mut kf.frame);
        }
        self.transform.sort_by_key(|k| k.frame);
        for kf in &mut self.opacity {
            mirror(&mut kf.frame);
        }
        self.opacity.sort_by_key(|k| k.frame);
        for kf in &mut self.filter {
            mirror(&mut kf.frame);
        }
        self.filter.sort_by_key(|k| k.frame);
        for kf in &mut self.deform {
            mirror(&mut kf.frame);
        }
        self.deform.sort_by_key(|k| k.frame);
        for kf in &mut self.volume {
            mirror(&mut kf.frame);
        }
        self.volume.sort_by_key(|k| k.frame);
        for kf in &mut self.pan {
            mirror(&mut kf.frame);
        }
        self.pan.sort_by_key(|k| k.frame);
    }

    /// Returns all distinct keyframe frames across every channel, sorted ascending.
    pub fn all_keyframe_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self
            .transform
            .iter()
            .map(|k| k.frame)
            .chain(self.opacity.iter().map(|k| k.frame))
            .chain(self.filter.iter().map(|k| k.frame))
            .chain(self.deform.iter().map(|k| k.frame))
            .chain(self.volume.iter().map(|k| k.frame))
            .chain(self.pan.iter().map(|k| k.frame))
            .collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    /// Finds the nearest keyframe before (`direction < 0`) or after (`direction > 0`) `current_frame`.
    pub fn adjacent_keyframe(&self, current_frame: u32, direction: i32) -> Option<u32> {
        let frames = self.all_keyframe_frames();
        if direction < 0 {
            frames.into_iter().rev().find(|&f| f < current_frame)
        } else if direction > 0 {
            frames.into_iter().find(|&f| f > current_frame)
        } else {
            None
        }
    }

    /// Evaluates the interpolated [`Transform2D`] at `frame`.
    pub fn evaluate_transform(&self, frame: f32) -> Transform2D {
        eval_keyframes(&self.transform, frame, Transform2D::default(), Transform2D::lerp)
    }

    /// Evaluates the interpolated opacity (`0.0..=1.0`) at `frame`.
    pub fn evaluate_opacity(&self, frame: f32) -> f32 {
        eval_keyframes(&self.opacity, frame, 1.0, |a, b, t| (a + (b - a) * t).clamp(0.0, 1.0))
    }

    /// Evaluates the interpolated [`LiveFilterState`] at `frame`.
    pub fn evaluate_filter(&self, frame: f32) -> LiveFilterState {
        eval_keyframes(&self.filter, frame, LiveFilterState::default(), LiveFilterState::lerp)
    }

    /// Evaluates the interpolated [`Deform2D`] (Warp/Distort) at `frame`.
    pub fn evaluate_deform(&self, frame: f32) -> Deform2D {
        eval_keyframes(&self.deform, frame, Deform2D::default(), Deform2D::lerp)
    }

    /// Evaluates the interpolated audio volume (`0.0..=2.0`, default `1.0`) at `frame`.
    pub fn evaluate_volume(&self, frame: f32) -> f32 {
        eval_keyframes(&self.volume, frame, 1.0, |a, b, t| (a + (b - a) * t).clamp(0.0, 2.0))
    }

    /// Evaluates the interpolated stereo pan (`-1.0..=1.0`, default `0.0`) at `frame`.
    pub fn evaluate_pan(&self, frame: f32) -> f32 {
        eval_keyframes(&self.pan, frame, 0.0, |a, b, t| (a + (b - a) * t).clamp(-1.0, 1.0))
    }
}

fn move_kf<T: Copy>(keys: &mut Vec<Keyframe<T>>, from_frame: u32, to_frame: u32) -> bool {
    if from_frame == to_frame {
        return false;
    }
    let Some(pos) = keys.iter().position(|k| k.frame == from_frame) else {
        return false;
    };
    let mut moved = keys.remove(pos);
    moved.frame = to_frame.min(MAX_TIMELINE_DURATION.saturating_sub(1));
    keys.retain(|k| k.frame != moved.frame);
    keys.push(moved);
    keys.sort_by_key(|k| k.frame);
    true
}

fn eval_keyframes<T: Copy>(keys: &[Keyframe<T>], frame: f32, default: T, lerp: impl Fn(T, T, f32) -> T) -> T {
    let frame = if frame.is_finite() { frame.max(0.0) } else { 0.0 };
    let Some(first) = keys.first() else {
        return default;
    };
    if frame <= first.frame as f32 {
        return first.value;
    }
    let Some(last) = keys.last() else {
        return first.value;
    };
    if frame >= last.frame as f32 {
        return last.value;
    }
    for pair in keys.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        let (fa, fb) = (a.frame as f32, b.frame as f32);
        if frame >= fa && frame < fb {
            let span = (fb - fa).max(1e-6);
            let raw_t = ((frame - fa) / span).clamp(0.0, 1.0);
            let eased_t = a.easing.sample(raw_t);
            return lerp(a.value, b.value, eased_t);
        }
    }
    last.value
}

/// A single hand-drawn cel inside a Flipbook [`ClipKind::Cel`] sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cel {
    /// The paint layer or layer group in [`Document`] that holds this cel's artwork.
    pub layer: LayerId,
    /// Additional frames this cel stays on screen (`0` = 1 frame total, `1` = 2 frames, etc.).
    #[serde(default)]
    pub hold: u32,
    /// Coordinate space used when drawing this cel. Missing on pre-2.4.1 projects.
    #[serde(default)]
    pub stage_space: Option<bool>,
}

impl Cel {
    /// Total number of timeline frames this cel occupies (`1 + hold`).
    pub fn duration_frames(self) -> u32 {
        1u32.saturating_add(self.hold.min(600))
    }
}

/// How a [`Clip`] repeats its cels when stretched past its natural cel sequence length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClipLoopMode {
    /// Plays once; stretching a `Cel` clip extends its last cel's hold duration.
    #[default]
    None,
    /// Loops the cel sequence cyclically across `clip.duration_frames`.
    Loop,
    /// Alternates forward and backward across `clip.duration_frames`.
    PingPong,
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
    /// Audio clip referencing an [`AudioAsset`] with trim offset, volume, and pan keyframes.
    Audio {
        asset_id: AudioAssetId,
        #[serde(default)]
        trim_start_samples: u64,
        #[serde(default = "default_sample_rate")]
        sample_rate: u32,
        #[serde(default = "default_channels")]
        channels: u8,
    },
    /// Imported video clip decoded on demand into a managed paint layer for rotoscoping/compositing.
    Video {
        video_id: u64,
        layer: LayerId,
        source_fps: f32,
        #[serde(default)]
        trim_in_frames: u32,
        frame_count: u32,
    },
    /// Animated text clip referencing a `LayerKind::Paint` text layer in [`Document`].
    Text { layer: LayerId },
}

fn default_sample_rate() -> u32 {
    44_100
}

fn default_channels() -> u8 {
    2
}

fn default_perform_guide_opacity() -> f32 {
    0.45
}

fn default_perform_guide_color() -> [f32; 3] {
    [0.2, 0.78, 1.0]
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
    /// When enabled, renders the first frame (or `perform_guide_layer`) as an animated onion-skin
    /// ghost following the Perform motion curve so the artist can draw frame-by-frame over it.
    #[serde(default)]
    pub perform_onion_guide: bool,
    /// Optional dedicated guide layer (kept out of playback/export, shown only as motion ghost).
    #[serde(default)]
    pub perform_guide_layer: Option<LayerId>,
    /// Opacity (`0.0..=1.0`) of the Perform motion onion-skin ghost.
    #[serde(default = "default_perform_guide_opacity")]
    pub perform_guide_opacity: f32,
    /// Tint color `[r, g, b]` (`0..1`) of the Perform motion onion-skin ghost.
    #[serde(default = "default_perform_guide_color")]
    pub perform_guide_color: [f32; 3],
    /// Loop / Ping-Pong repetition mode for `Cel` clips stretched past their natural cel span.
    #[serde(default)]
    pub loop_mode: ClipLoopMode,
    /// Phase into the source sequence at the left edge of a repeated clip.
    #[serde(default)]
    pub loop_offset: u32,
}

fn unit_one() -> f32 {
    1.0
}

impl Clip {
    pub fn end_frame(&self) -> u32 {
        self.start_frame.saturating_add(self.effective_duration())
    }

    /// Natural single-cycle length of this clip's cels (or `duration_frames` for non-cel clips).
    pub fn cels_duration(&self) -> u32 {
        match &self.kind {
            ClipKind::Cel { cels } if !cels.is_empty() => {
                cels.iter().map(|c| c.duration_frames()).fold(0u32, u32::saturating_add).max(1)
            }
            _ => self.duration_frames.max(1),
        }
    }

    /// For a non-looping `Cel` clip, its duration is the sum of its cels' durations (at least 1);
    /// for looping/ping-pong `Cel` clips or other clips, the trimmed duration.
    pub fn effective_duration(&self) -> u32 {
        if self.loop_mode == ClipLoopMode::None { self.cels_duration() } else { self.duration_frames.max(1) }
    }

    pub fn sync_duration(&mut self) {
        self.duration_frames = self.effective_duration();
    }

    pub fn contains_frame(&self, frame: u32) -> bool {
        frame >= self.start_frame && frame < self.end_frame()
    }

    pub fn loop_period(&self) -> u32 {
        let length = self.cels_duration();
        if self.loop_mode == ClipLoopMode::PingPong {
            length.saturating_sub(1).saturating_mul(2).max(1)
        } else {
            length
        }
    }

    fn phase_at(&self, frame: u32) -> u32 {
        if self.loop_mode == ClipLoopMode::None {
            return 0;
        }
        (i64::from(self.loop_offset) + i64::from(frame) - i64::from(self.start_frame))
            .rem_euclid(i64::from(self.loop_period())) as u32
    }

    /// First visible occurrence of a source exposure after a cut or trim.
    pub fn first_exposure(&self, start: u32, duration: u32, period: u32) -> Option<u32> {
        let period = i64::from(period.max(1));
        let phase = i64::from(self.loop_offset) % period;
        let distance = |lo: i64, hi: i64| {
            if (lo..=hi).contains(&phase) { 0 } else { (lo - phase).rem_euclid(period) as u32 }
        };
        let lo = i64::from(start);
        let hi = lo + i64::from(duration) - 1;
        let offset = match self.loop_mode {
            ClipLoopMode::None => start,
            ClipLoopMode::Loop => distance(lo, hi),
            ClipLoopMode::PingPong => distance(lo, hi).min(distance(period - hi, period - lo)),
        };
        (offset < self.duration_frames).then(|| self.start_frame.saturating_add(offset))
    }

    /// For a `Cel` clip, returns `(cel_index, cel, offset_into_cel)` at timeline `frame`,
    /// honoring [`ClipLoopMode::Loop`] and [`ClipLoopMode::PingPong`].
    pub fn cel_at(&self, frame: u32) -> Option<(usize, Cel, u32)> {
        if !self.contains_frame(frame) {
            return None;
        }
        let ClipKind::Cel { cels } = &self.kind else {
            return None;
        };
        let cels_len = self.cels_duration();
        let raw_local = frame - self.start_frame;
        let local = match self.loop_mode {
            ClipLoopMode::None => raw_local,
            ClipLoopMode::Loop => {
                (u64::from(raw_local) + u64::from(self.loop_offset)).rem_euclid(u64::from(cels_len)) as u32
            }
            ClipLoopMode::PingPong => {
                if cels_len <= 1 {
                    0
                } else {
                    let period = cels_len.saturating_sub(1).saturating_mul(2).max(1);
                    let m = ((u64::from(raw_local) + u64::from(self.loop_offset)) % u64::from(period)) as u32;
                    if m < cels_len { m } else { period.saturating_sub(m) }
                }
            }
        };
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
        self.first_exposure(offset, cels.get(cel_index)?.duration_frames(), self.loop_period())
    }

    /// First visual layer of this clip (used as the default Perform motion ghost reference).
    pub fn first_layer(&self) -> Option<LayerId> {
        match &self.kind {
            ClipKind::Cel { cels } => cels.first().map(|c| c.layer),
            ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => Some(*layer),
            ClipKind::Group { members } => members.first().copied(),
            ClipKind::Audio { .. } => None,
        }
    }

    /// Primary layer represented by this clip at `frame`.
    pub fn layer_at(&self, frame: u32) -> Option<LayerId> {
        if !self.contains_frame(frame) {
            return None;
        }
        match &self.kind {
            ClipKind::Cel { .. } => self.cel_at(frame).map(|(_, cel, _)| cel.layer),
            ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => Some(*layer),
            ClipKind::Group { members } => members.first().copied(),
            ClipKind::Audio { .. } => None,
        }
    }

    /// All layers referenced anywhere inside this clip.
    pub fn referenced_layers(&self) -> Vec<LayerId> {
        let mut out = match &self.kind {
            ClipKind::Cel { cels } => cels.iter().map(|c| c.layer).collect(),
            ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => vec![*layer],
            ClipKind::Group { members } => members.clone(),
            ClipKind::Audio { .. } => Vec::new(),
        };
        if let Some(guide) = self.perform_guide_layer
            && !out.contains(&guide)
        {
            out.push(guide);
        }
        out
    }

    fn new_visual(id: ClipId, name: String, start_frame: u32, duration_frames: u32, kind: ClipKind) -> Clip {
        Clip {
            id,
            name,
            start_frame,
            duration_frames,
            blend: BlendMode::Normal,
            opacity: 1.0,
            kind,
            keyframes: KeyframeTrack::default(),
            perform_onion_guide: false,
            perform_guide_layer: None,
            perform_guide_opacity: default_perform_guide_opacity(),
            perform_guide_color: default_perform_guide_color(),
            loop_mode: ClipLoopMode::None,
            loop_offset: 0,
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
    /// Clips the entire track's output to the alpha silhouette of the visual track immediately below.
    #[serde(default)]
    pub clip_to_below: bool,
    /// Locks alpha transparency when painting on cels belonging to this track.
    #[serde(default)]
    pub alpha_lock: bool,
    /// Mutes audio playback/mixing for audio clips on this track.
    #[serde(default)]
    pub muted: bool,
    /// Non-destructive GPU Layer Effects (Outline, Drop Shadow, Glow, Color Overlay).
    #[serde(default)]
    pub effects: TrackEffects,
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
    /// Combined canvas-space affine transform (camera^-1 * parent group track * track * clip).
    pub transform: Affine,
    /// Combined effective opacity (`0.0..=1.0`).
    pub opacity: f32,
    /// Effective blend mode.
    pub blend: BlendMode,
    /// Screen tint `[r, g, b, a]` (`a = 1.0` for onion skin, `0.0` for normal).
    pub tint: [f32; 4],
    pub is_onion: bool,
    /// True when this onion-skin item is a Perform Motion Ghost (first frame moving along the
    /// Perform trajectory so the user can animate over it).
    pub is_perform_guide: bool,
    /// Non-destructive live image filter state (Gaussian Blur, HSB, Sharpen, Noise).
    pub filter: LiveFilterState,
    /// Non-destructive Warp 4×4 or Distort 4-corner deformation.
    pub deform: Deform2D,
    /// Non-destructive Track Layer Effects (Outline, Drop Shadow, Glow, Color Overlay).
    pub effects: TrackEffects,
    /// When true, clips this layer to the silhouette of the track rendered immediately beneath it.
    pub clip_to_below: bool,
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
    /// Evaluated global camera transform at `frame`.
    pub camera_transform: Transform2D,
    /// Evaluated global camera filter (e.g. lens depth-of-field blur or fade).
    pub camera_filter: LiveFilterState,
}

/// A recorded transform sample during live Perform mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionSample {
    pub frame: u32,
    pub transform: Transform2D,
}

/// A recorded mesh/distort deformation sample during live Perform Warp mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeformSample {
    pub frame: u32,
    pub deform: Deform2D,
}

/// A recorded scalar sample (e.g. opacity, audio volume, or pan) during live Perform mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarSample {
    pub frame: u32,
    pub value: f32,
}

/// A recorded Live Filter sample (Gaussian Blur, HSB, Sharpen, Noise) during live Perform mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterSample {
    pub frame: u32,
    pub filter: LiveFilterState,
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
    /// Automatically create a new frame/cel when drawing on an empty spot in the timeline.
    #[serde(default = "bool_true")]
    pub auto_create_frame_on_draw: bool,
    /// Automatically split a held cel when drawing on an extended hold frame (`offset > 0`).
    #[serde(default = "bool_true")]
    pub auto_split_cel_on_draw: bool,
    /// Global Camera Track keyframes (Pan/Zoom/Rotate + Lens Blur/HSB).
    #[serde(default)]
    pub camera: KeyframeTrack,
    /// Whether the Camera Track transform is currently enabled on the Stage/Export.
    #[serde(default)]
    pub camera_enabled: bool,
    /// Imported audio assets referenced by [`ClipKind::Audio`] clips.
    #[serde(default)]
    pub audio_assets: Vec<AudioAsset>,
    /// Project-relative video source names, shared by copies and cuts of the same clip.
    #[serde(default)]
    pub video_files: std::collections::BTreeMap<u64, String>,
    /// Embedded sources from an opened package, available before its first directory save.
    #[serde(skip)]
    pub packaged_video: std::collections::BTreeMap<String, std::sync::Arc<Vec<u8>>>,
    /// Named beat / bookmark markers on the timeline ruler.
    #[serde(default)]
    pub markers: Vec<TimelineMarker>,
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
            auto_create_frame_on_draw: true,
            auto_split_cel_on_draw: true,
            camera: KeyframeTrack::default(),
            camera_enabled: false,
            audio_assets: Vec::new(),
            video_files: Default::default(),
            packaged_video: Default::default(),
            markers: Vec::new(),
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
        let clip = Clip::new_visual(
            clip_id,
            "Drawing 1".to_owned(),
            0,
            1,
            ClipKind::Cel { cels: vec![Cel { stage_space: Some(false), layer: first_layer, hold: 0 }] },
        );
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
            clip_to_below: false,
            alpha_lock: false,
            muted: false,
            effects: TrackEffects::default(),
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
            auto_create_frame_on_draw: true,
            auto_split_cel_on_draw: true,
            camera: KeyframeTrack::default(),
            camera_enabled: false,
            audio_assets: Vec::new(),
            video_files: Default::default(),
            packaged_video: Default::default(),
            markers: Vec::new(),
            next_id: 1,
        };

        // Calculate total played cels duration first
        let played_layers: Vec<LayerId> =
            all_frames.get(played_range.clone()).map(<[LayerId]>::to_vec).unwrap_or_default();
        let cels: Vec<Cel> = played_layers
            .iter()
            .map(|&layer| Cel { stage_space: Some(false), layer, hold: anim.hold(layer) })
            .collect();
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
                clip_to_below: false,
                alpha_lock: false,
                muted: false,
                effects: TrackEffects::default(),
                clips: vec![Clip::new_visual(
                    bg_clip_id,
                    "Background".to_owned(),
                    0,
                    tl.duration_frames,
                    ClipKind::Layer { layer: bg_layer },
                )],
                keyframes: KeyframeTrack::default(),
            });
        }

        // 2. Main Drawing track
        let main_track_id = tl.alloc_track_id();
        let main_clip_id = tl.alloc_clip_id();
        let fallback_cels = if cels.is_empty() {
            all_frames
                .first()
                .copied()
                .into_iter()
                .map(|layer| Cel { stage_space: Some(false), layer, hold: 0 })
                .collect()
        } else {
            cels
        };
        let mut main_clip = Clip::new_visual(
            main_clip_id,
            "Animation".to_owned(),
            0,
            cels_duration,
            ClipKind::Cel { cels: fallback_cels },
        );
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
            clip_to_below: false,
            alpha_lock: false,
            muted: false,
            effects: TrackEffects::default(),
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
                clip_to_below: false,
                alpha_lock: false,
                muted: false,
                effects: TrackEffects::default(),
                clips: vec![Clip::new_visual(
                    fg_clip_id,
                    "Foreground".to_owned(),
                    0,
                    tl.duration_frames,
                    ClipKind::Layer { layer: fg_layer },
                )],
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

    fn alloc_audio_asset_id(&mut self) -> AudioAssetId {
        let id = AudioAssetId(self.next_id.max(1));
        self.next_id = id.0.saturating_add(1);
        id
    }

    fn alloc_marker_id(&mut self) -> MarkerId {
        let id = MarkerId(self.next_id.max(1));
        self.next_id = id.0.saturating_add(1);
        id
    }

    /// Adds a named beat/section marker at `frame` on the timeline ruler.
    pub fn add_marker(&mut self, frame: u32, label: Option<&str>, color: Option<u32>) -> MarkerId {
        let id = self.alloc_marker_id();
        let clamped_frame = frame.min(self.duration_frames.saturating_sub(1));
        let default_label = format!("M{}", self.markers.len() + 1);
        let text =
            label.map(str::trim).filter(|s| !s.is_empty()).map_or(default_label, |s| s.chars().take(64).collect());
        let color = color.unwrap_or_else(default_marker_color);
        self.markers.push(TimelineMarker { id, frame: clamped_frame, label: text, color });
        self.markers.sort_by_key(|m| (m.frame, m.id));
        id
    }

    /// Removes `marker_id` from the timeline ruler.
    pub fn delete_marker(&mut self, marker_id: MarkerId) -> bool {
        let before = self.markers.len();
        self.markers.retain(|m| m.id != marker_id);
        self.markers.len() != before
    }

    /// Moves `marker_id` to `new_frame` on the timeline ruler.
    pub fn move_marker(&mut self, marker_id: MarkerId, new_frame: u32) -> bool {
        let clamped = new_frame.min(self.duration_frames.saturating_sub(1));
        let Some(marker) = self.markers.iter_mut().find(|m| m.id == marker_id) else {
            return false;
        };
        if marker.frame == clamped {
            return false;
        }
        marker.frame = clamped;
        self.markers.sort_by_key(|m| (m.frame, m.id));
        true
    }

    /// Sanitizes and repairs the timeline against `valid_layers` in the document.
    pub fn repaired(mut self, valid_layers: &HashSet<LayerId>) -> Timeline {
        self.video_files.retain(|_, name| Self::valid_video_filename(name));
        self.fps = self.fps.clamp(1, 60);
        self.duration_frames = self.duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        self.onion = self.onion.repaired();
        self.camera = std::mem::take(&mut self.camera).repaired();
        self.motion_filtering =
            if self.motion_filtering.is_finite() { self.motion_filtering.clamp(0.0, 1.0) } else { 0.35 };

        let mut max_seen_id = 0u64;
        for asset in &self.audio_assets {
            max_seen_id = max_seen_id.max(asset.id.0);
        }
        let valid_audio: HashSet<AudioAssetId> = self.audio_assets.iter().map(|a| a.id).collect();

        let mut seen_tracks = HashSet::new();
        self.tracks.retain(|t| seen_tracks.insert(t.id));

        let mut seen_clips = HashSet::new();
        for track in &mut self.tracks {
            max_seen_id = max_seen_id.max(track.id.0);
            if track.name.trim().is_empty() {
                track.name = "Track".to_owned();
            }
            track.opacity = if track.opacity.is_finite() { track.opacity.clamp(0.0, 1.0) } else { 1.0 };
            track.effects = track.effects.repaired();
            track.keyframes = std::mem::take(&mut track.keyframes).repaired();
            track.clips.retain(|c| seen_clips.insert(c.id));

            for clip in &mut track.clips {
                max_seen_id = max_seen_id.max(clip.id.0);
                if clip.name.trim().is_empty() {
                    clip.name = "Clip".to_owned();
                }
                clip.opacity = if clip.opacity.is_finite() { clip.opacity.clamp(0.0, 1.0) } else { 1.0 };
                clip.perform_guide_opacity = if clip.perform_guide_opacity.is_finite() {
                    clip.perform_guide_opacity.clamp(0.0, 1.0)
                } else {
                    default_perform_guide_opacity()
                };
                clip.perform_guide_color =
                    clip.perform_guide_color.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 });
                if let Some(guide) = clip.perform_guide_layer
                    && !valid_layers.is_empty()
                    && !valid_layers.contains(&guide)
                {
                    clip.perform_guide_layer = None;
                }
                clip.keyframes = std::mem::take(&mut clip.keyframes).repaired();
                match &mut clip.kind {
                    ClipKind::Cel { cels } => {
                        if !valid_layers.is_empty() {
                            cels.retain(|c| valid_layers.contains(&c.layer));
                        }
                        for (idx, cel) in cels.iter_mut().enumerate() {
                            cel.stage_space.get_or_insert(clip.perform_onion_guide && idx > 0);
                            cel.hold = cel.hold.min(600);
                        }
                    }
                    ClipKind::Layer { .. }
                    | ClipKind::Video { .. }
                    | ClipKind::Text { .. }
                    | ClipKind::Audio { .. } => {}
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
                    ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => {
                        valid_layers.contains(layer)
                    }
                    ClipKind::Group { members } => track.is_group || !members.is_empty(),
                    ClipKind::Audio { asset_id, .. } => valid_audio.is_empty() || valid_audio.contains(asset_id),
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

        let mut seen_markers = HashSet::new();
        let last_frame = self.duration_frames.saturating_sub(1);
        self.markers.retain(|m| seen_markers.insert(m.id));
        for marker in &mut self.markers {
            max_seen_id = max_seen_id.max(marker.id.0);
            marker.frame = marker.frame.min(last_frame);
            if marker.label.trim().is_empty() {
                marker.label = "Marker".to_owned();
            }
        }
        self.markers.sort_by_key(|m| (m.frame, m.id));
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

    pub fn audio_asset(&self, id: AudioAssetId) -> Option<&AudioAsset> {
        self.audio_assets.iter().find(|a| a.id == id)
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
            clip_to_below: false,
            alpha_lock: false,
            muted: false,
            effects: TrackEffects::default(),
            clips: Vec::new(),
            keyframes: KeyframeTrack::default(),
        });
        self.active_track = Some(id);
        id
    }

    /// Deletes `track_id` from the timeline, unparenting any child tracks and returning all
    /// [`LayerId`]s referenced by its clips so the caller can clean up orphaned layers.
    pub fn delete_track(&mut self, track_id: TrackId) -> Option<Vec<LayerId>> {
        let idx = self.tracks.iter().position(|t| t.id == track_id)?;
        let removed = self.tracks.remove(idx);
        for t in &mut self.tracks {
            if t.parent_group == Some(track_id) {
                t.parent_group = None;
            }
        }
        let layers: Vec<LayerId> = removed.clips.iter().flat_map(Clip::referenced_layers).collect();
        if self.active_track == Some(track_id) {
            self.active_track = self.tracks.last().map(|t| t.id);
        }
        if self.active_clip.is_some_and(|cid| removed.clips.iter().any(|c| c.id == cid)) {
            self.active_clip = self
                .active_track
                .and_then(|tid| self.track(tid))
                .and_then(|t| t.clips.first())
                .map(|c| c.id)
                .or_else(|| self.tracks.iter().flat_map(|t| &t.clips).next().map(|c| c.id));
        }
        Some(layers)
    }

    /// Moves `track_id` up (`delta > 0`, higher z-order) or down (`delta < 0`, lower z-order).
    pub fn move_track(&mut self, track_id: TrackId, delta: i32) -> bool {
        let Some(pos) = self.tracks.iter().position(|t| t.id == track_id) else {
            return false;
        };
        let target = (pos as i64 + i64::from(delta)).clamp(0, self.tracks.len().saturating_sub(1) as i64) as usize;
        if target == pos {
            return false;
        }
        let track = self.tracks.remove(pos);
        self.tracks.insert(target, track);
        true
    }

    /// Clears all clips on `track_id`, returning their referenced [`LayerId`]s.
    pub fn clear_track_clips(&mut self, track_id: TrackId) -> Option<Vec<LayerId>> {
        let track = self.track_mut(track_id)?;
        if track.locked {
            return None;
        }
        let layers: Vec<LayerId> = track.clips.iter().flat_map(Clip::referenced_layers).collect();
        let removed_ids: HashSet<ClipId> = track.clips.iter().map(|c| c.id).collect();
        track.clips.clear();
        if self.active_clip.is_some_and(|cid| removed_ids.contains(&cid)) {
            self.active_clip = self.tracks.iter().flat_map(|t| &t.clips).next().map(|c| c.id);
        }
        Some(layers)
    }

    /// Merges `track_id`'s clips into the track immediately below it (`index - 1`) and removes `track_id`.
    pub fn merge_track_down(&mut self, track_id: TrackId) -> bool {
        let Some(pos) = self.tracks.iter().position(|t| t.id == track_id) else {
            return false;
        };
        if pos == 0 {
            return false;
        }
        if self.tracks.get(pos).is_some_and(|t| t.locked) || self.tracks.get(pos - 1).is_some_and(|t| t.locked) {
            return false;
        }
        let upper = self.tracks.remove(pos);
        let Some(lower) = self.tracks.get_mut(pos - 1) else {
            return false;
        };
        let lower_id = lower.id;
        lower.clips.extend(upper.clips);
        lower.clips.sort_by_key(|c| c.start_frame);
        for t in &mut self.tracks {
            if t.parent_group == Some(track_id) {
                t.parent_group = Some(lower_id);
            }
        }
        self.active_track = Some(lower_id);
        true
    }

    /// Registers an [`AudioAsset`] from interleaved float PCM and places an [`ClipKind::Audio`]
    /// clip on `track_id` (or creates a new Audio track if `track_id` is `None`).
    pub fn add_audio_clip(
        &mut self,
        track_id: Option<TrackId>,
        name: String,
        sample_rate: u32,
        channels: u8,
        pcm: Vec<f32>,
        start_frame: u32,
    ) -> Option<(TrackId, ClipId, AudioAssetId)> {
        let asset_id = self.alloc_audio_asset_id();
        let asset = AudioAsset::from_pcm(asset_id, name.clone(), sample_rate, channels, pcm);
        let dur_frames = asset.duration_in_timeline_frames(self.fps).clamp(1, MAX_TIMELINE_DURATION);
        let sr = asset.sample_rate;
        let ch = asset.channels;
        self.audio_assets.push(asset);

        let tid = match track_id {
            Some(id) if self.track(id).is_some() => id,
            _ => {
                let id = self.add_track(Some(&format!("Audio {}", self.audio_assets.len())), false);
                if let Some(t) = self.track_mut(id) {
                    t.color_tag = TRACK_COLORS[2]; // Emerald audio accent
                }
                id
            }
        };
        let clip_id = self.alloc_clip_id();
        let track = self.track_mut(tid)?;
        let clip = Clip::new_visual(
            clip_id,
            if name.trim().is_empty() { "Audio".to_owned() } else { name },
            start_frame,
            dur_frames,
            ClipKind::Audio { asset_id, trim_start_samples: 0, sample_rate: sr, channels: ch },
        );
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(tid);
        self.active_clip = Some(clip_id);
        Some((tid, clip_id, asset_id))
    }

    pub fn valid_video_filename(name: &str) -> bool {
        name.strip_prefix("video-")
            .and_then(|s| s.strip_suffix(".media"))
            .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    }

    pub fn video_sources(&self) -> std::collections::BTreeSet<&str> {
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .filter_map(|clip| {
                let ClipKind::Video { video_id, .. } = clip.kind else { return None };
                self.video_files.get(&video_id).map(String::as_str).filter(|name| Self::valid_video_filename(name))
            })
            .collect()
    }

    /// Adds a [`ClipKind::Video`] clip on `track_id` backed by managed `layer`.
    pub fn add_video_clip(
        &mut self,
        track_id: TrackId,
        start_frame: u32,
        duration_frames: u32,
        layer: LayerId,
        source_fps: f32,
        frame_count: u32,
        name: Option<&str>,
    ) -> Option<ClipId> {
        let clip_id = self.alloc_clip_id();
        let video_id = clip_id.0;
        let track = self.track_mut(track_id)?;
        let dur = duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        let default_name = format!("Video {}", track.clips.len() + 1);
        let name = name.map(str::trim).filter(|s| !s.is_empty()).map_or(default_name, ToOwned::to_owned);
        let clip = Clip::new_visual(
            clip_id,
            name,
            start_frame,
            dur,
            ClipKind::Video {
                video_id,
                layer,
                source_fps: if source_fps.is_finite() { source_fps.clamp(1.0, 120.0) } else { 24.0 },
                trim_in_frames: 0,
                frame_count: frame_count.max(1),
            },
        );
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(track_id);
        self.active_clip = Some(clip_id);
        Some(clip_id)
    }

    /// Adds a [`ClipKind::Text`] clip on `track_id` backed by text layer `layer`.
    pub fn add_text_clip(
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
        let default_name = format!("Text {}", track.clips.len() + 1);
        let name = name.map(str::trim).filter(|s| !s.is_empty()).map_or(default_name, ToOwned::to_owned);
        let clip = Clip::new_visual(clip_id, name, start_frame, dur, ClipKind::Text { layer });
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(track_id);
        self.active_clip = Some(clip_id);
        Some(clip_id)
    }

    /// Deterministic stereo audio mixer for timeline frames `start_frame..=end_frame` (inclusive)
    /// at `target_sample_rate` Hz. Combines all visible, unmuted audio tracks honoring solo,
    /// trim offsets, and animated `volume` + `pan` keyframes with easing.
    pub fn mix_audio_range(&self, start_frame: u32, end_frame: u32, target_sample_rate: u32) -> Vec<f32> {
        let sr = target_sample_rate.clamp(8_000, 192_000);
        let fps = f64::from(self.fps.clamp(1, 60));
        let end = end_frame.max(start_frame);
        let frame_span = u64::from(end.saturating_sub(start_frame).saturating_add(1));
        let total_samples = ((frame_span as f64 / fps) * f64::from(sr)).round() as usize;
        if total_samples == 0 || self.audio_assets.is_empty() {
            return Vec::new();
        }
        // Cap to 10 minutes of stereo samples to avoid accidental huge allocations
        let capped_samples = total_samples.min((sr as usize).saturating_mul(600));
        let mut out = vec![0.0_f32; capped_samples.saturating_mul(2)];
        let any_solo = self.tracks.iter().any(|t| t.solo && self.audio_track_is_audible(t.id));

        for track in &self.tracks {
            if track.is_group || !self.audio_track_is_audible(track.id) {
                continue;
            }
            if any_solo && !self.track_or_ancestor_is_solo(track.id) {
                continue;
            }
            let lineage: Vec<&Track> =
                std::iter::successors(Some(track), |t| t.parent_group.and_then(|id| self.track(id)))
                    .take(self.tracks.len())
                    .collect();
            for clip in &track.clips {
                let ClipKind::Audio { asset_id, trim_start_samples, .. } = clip.kind else {
                    continue;
                };
                if clip.end_frame() <= start_frame || clip.start_frame > end {
                    continue;
                }
                let Some(asset) = self.audio_asset(asset_id) else {
                    continue;
                };
                if asset.pcm.is_empty() {
                    continue;
                }
                let src_sr = f64::from(asset.sample_rate.max(1));
                let dst_sr = f64::from(sr);
                let clip_start_sec = f64::from(clip.start_frame) / fps;
                let clip_end_sec = f64::from(clip.end_frame()) / fps;
                let range_start_sec = f64::from(start_frame) / fps;

                let to_sample = |frame: u32| {
                    (u64::from(frame.saturating_sub(start_frame)) * u64::from(sr))
                        .div_ceil(u64::from(self.fps.clamp(1, 60))) as usize
                };
                let first = to_sample(clip.start_frame);
                let last = to_sample(clip.end_frame()).min(capped_samples);
                for i in first..last {
                    let t_sec = range_start_sec + (i as f64 / dst_sr);
                    if t_sec < clip_start_sec || t_sec >= clip_end_sec {
                        continue;
                    }
                    let local_sec = t_sec - clip_start_sec;
                    let src_pos = (trim_start_samples as f64) + local_sec * src_sr;
                    let (sl, sr_sample) = asset.sample_stereo(src_pos);
                    let timeline_frame = (t_sec * fps) as f32;
                    let mut gain = clip.keyframes.evaluate_volume(timeline_frame) * clip.opacity;
                    let mut pan = clip.keyframes.evaluate_pan(timeline_frame);
                    for parent in &lineage {
                        gain *= parent.opacity
                            * parent.keyframes.evaluate_opacity(timeline_frame)
                            * parent.keyframes.evaluate_volume(timeline_frame);
                        pan += parent.keyframes.evaluate_pan(timeline_frame);
                    }
                    let gain = gain.clamp(0.0, 2.0);
                    let pan = pan.clamp(-1.0, 1.0);
                    let (gl, gr) = stereo_pan_gains(pan);
                    let idx = i.saturating_mul(2);
                    if let Some(slot_l) = out.get_mut(idx) {
                        *slot_l += sl * gain * gl;
                    }
                    if let Some(slot_r) = out.get_mut(idx + 1) {
                        *slot_r += sr_sample * gain * gr;
                    }
                }
            }
        }
        for sample in &mut out {
            *sample = sample.clamp(-1.0, 1.0);
        }
        out
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
        let cel = Cel { stage_space: Some(false), layer: initial_layer, hold: hold.min(600) };
        let dur = cel.duration_frames();
        let clip =
            Clip::new_visual(clip_id, format!("Drawing {idx}"), start_frame, dur, ClipKind::Cel { cels: vec![cel] });
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
        let clip = Clip::new_visual(clip_id, name, start_frame, dur, ClipKind::Layer { layer });
        let end = clip.end_frame();
        track.clips.push(clip);
        track.clips.sort_by_key(|c| c.start_frame);
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        self.active_track = Some(track_id);
        self.active_clip = Some(clip_id);
        Some(clip_id)
    }

    /// Ensures there is an editable drawing frame/cel at the current `playhead` when the user
    /// begins a brush stroke on an empty region of the timeline (or on a held cel with
    /// `auto_split_cel_on_draw` enabled).
    ///
    /// Returns `Some((layer_id, created_new_frame))`, or `None` if no unlocked track is available
    /// or `alloc_layer` could not allocate a new layer.
    pub fn ensure_frame_for_drawing(
        &mut self,
        mut alloc_layer: impl FnMut() -> Option<LayerId>,
    ) -> Option<(LayerId, bool)> {
        let frame = self.playhead;

        // Resolve a drawable (non-group, unlocked) target track
        let target_track_id = self
            .active_track
            .filter(|&id| self.track(id).is_some_and(|t| !t.is_group && !t.locked))
            .or_else(|| self.tracks.iter().rev().find(|t| !t.is_group && !t.locked).map(|t| t.id));

        let tid = match target_track_id {
            Some(id) => id,
            None => {
                if !self.auto_create_frame_on_draw {
                    return None;
                }
                self.add_track(None, false)
            }
        };
        self.active_track = Some(tid);

        // Check if there is already a clip covering `frame` on `tid`
        let existing_hit = self.track(tid).and_then(|t| {
            t.clips.iter().find(|c| c.contains_frame(frame)).map(|c| {
                let cel_hit = c.cel_at(frame);
                (c.id, c.layer_at(frame), cel_hit, matches!(c.kind, ClipKind::Audio { .. }))
            })
        });

        if let Some((cid, layer_opt, cel_hit, is_audio)) = existing_hit {
            if is_audio {
                // Don't paint onto an audio track; create a fresh drawing track above it
                if !self.auto_create_frame_on_draw {
                    return None;
                }
                let new_layer = alloc_layer()?;
                let draw_tid = self.add_track(None, false);
                let new_cid = self.add_cel_clip(draw_tid, frame, new_layer, 0)?;
                self.active_track = Some(draw_tid);
                self.active_clip = Some(new_cid);
                return Some((new_layer, true));
            }
            self.active_clip = Some(cid);
            // If we landed on an extended hold frame (`offset > 0`) inside a CelClip and auto-split
            // is enabled, split the hold at `frame` and insert a fresh blank cel right at `frame`.
            if self.auto_split_cel_on_draw
                && let Some((cel_idx, cel, offset)) = cel_hit
                && offset > 0
            {
                let new_layer = alloc_layer()?;
                if let Some(clip) = self.clip_mut(cid)
                    && let ClipKind::Cel { cels } = &mut clip.kind
                    && let Some(slot) = cels.get_mut(cel_idx)
                {
                    let remaining_hold = cel.hold.saturating_sub(offset);
                    slot.hold = offset.saturating_sub(1);
                    cels.insert(
                        cel_idx + 1,
                        Cel { stage_space: Some(clip.perform_onion_guide), layer: new_layer, hold: remaining_hold },
                    );
                    clip.sync_duration();
                    return Some((new_layer, true));
                }
            }
            return layer_opt.map(|l| (l, false));
        }

        // Empty spot on the timeline! Automatically create a cel/clip right at `frame`.
        if !self.auto_create_frame_on_draw {
            return None;
        }
        let new_layer = alloc_layer()?;

        // If a CelClip ends right at `frame` (contiguous frame-by-frame drawing), append a new cel
        // to that CelClip so Flipbook sequencing stays seamless.
        let contiguous_cel_clip = self.track(tid).and_then(|t| {
            t.clips
                .iter()
                .find(|c| {
                    matches!(c.kind, ClipKind::Cel { .. })
                        && c.loop_mode == ClipLoopMode::None
                        && c.end_frame() == frame
                })
                .map(|c| c.id)
        });

        if let Some(cid) = contiguous_cel_clip
            && let Some(clip) = self.clip_mut(cid)
            && let ClipKind::Cel { cels } = &mut clip.kind
        {
            cels.push(Cel { stage_space: Some(clip.perform_onion_guide), layer: new_layer, hold: 0 });
            clip.sync_duration();
            let end = clip.end_frame();
            self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
            self.active_clip = Some(cid);
            return Some((new_layer, true));
        }

        // Otherwise create a brand new CelClip positioned exactly at `frame`!
        let new_cid = self.add_cel_clip(tid, frame, new_layer, 0)?;
        self.active_clip = Some(new_cid);
        Some((new_layer, true))
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

    /// Resizes `clip_id`'s duration (stretching the last cel's hold for non-looping `CelClip`s,
    /// or adjusting the total repeated duration for `Loop` / `PingPong` clips and other clip kinds).
    pub fn resize_clip(&mut self, clip_id: ClipId, new_duration_frames: u32) -> bool {
        let Some(track_id) = self.track_of_clip(clip_id) else { return false };
        if self.track(track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let target_dur = new_duration_frames.clamp(1, MAX_TIMELINE_DURATION);
        let Some(clip) = self.clip_mut(clip_id) else { return false };
        let old_dur = clip.effective_duration();
        let loop_mode = clip.loop_mode;
        match &mut clip.kind {
            ClipKind::Cel { cels } => {
                if cels.is_empty() {
                    return false;
                }
                if loop_mode == ClipLoopMode::None {
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
                } else {
                    clip.duration_frames = target_dur;
                    clip.sync_duration();
                }
            }
            ClipKind::Layer { .. }
            | ClipKind::Group { .. }
            | ClipKind::Audio { .. }
            | ClipKind::Video { .. }
            | ClipKind::Text { .. } => {
                clip.duration_frames = target_dur;
            }
        }
        if clip.effective_duration() == old_dur {
            return false;
        }
        let end = clip.end_frame();
        self.duration_frames = self.duration_frames.max(end).min(MAX_TIMELINE_DURATION);
        true
    }

    /// Trims the left edge of `clip_id` to `new_start_frame` while keeping its end frame fixed
    /// (left-edge trim handle). For `Audio` / `Video` clips, advances source trim offset
    /// accordingly; for `Cel` clips, shifts/adjusts the first cel hold safely.
    pub fn trim_clip_left(&mut self, clip_id: ClipId, new_start_frame: u32) -> bool {
        let Some(track_id) = self.track_of_clip(clip_id) else { return false };
        if self.track(track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let fps = self.fps.clamp(1, 60);
        let Some(clip) = self.clip_mut(clip_id) else { return false };
        let old_start = clip.start_frame;
        let old_end = clip.end_frame();
        let clamped_start = new_start_frame.min(old_end.saturating_sub(1));
        if clamped_start == old_start {
            return false;
        }
        let new_dur = old_end.saturating_sub(clamped_start).max(1);
        let loop_mode = clip.loop_mode;
        let phase = clip.phase_at(clamped_start);
        match &mut clip.kind {
            ClipKind::Cel { cels } => {
                if cels.is_empty() {
                    return false;
                }
                if loop_mode != ClipLoopMode::None {
                    clip.loop_offset = phase;
                    clip.start_frame = clamped_start;
                    clip.duration_frames = new_dur;
                } else if clamped_start < old_start {
                    // Expanding left: extend first cel hold
                    let grow = old_start.saturating_sub(clamped_start);
                    if let Some(first) = cels.first_mut() {
                        let next_hold = first.hold.saturating_add(grow).min(600);
                        let applied = next_hold.saturating_sub(first.hold);
                        if applied == 0 {
                            return false;
                        }
                        first.hold = next_hold;
                        clip.start_frame = old_start.saturating_sub(applied);
                    }
                } else {
                    // Shrinking from left: consume holds on the first cel without deleting cels
                    let shrink = clamped_start.saturating_sub(old_start);
                    if let Some(first) = cels.first_mut() {
                        let can_take = shrink.min(first.hold);
                        if can_take == 0 {
                            return false;
                        }
                        first.hold = first.hold.saturating_sub(can_take);
                        clip.start_frame = old_start.saturating_add(can_take);
                    }
                }
                clip.sync_duration();
            }
            ClipKind::Audio { trim_start_samples, sample_rate, .. } => {
                let delta_frames = i64::from(clamped_start) - i64::from(old_start);
                let delta_samples = (delta_frames * i64::from(*sample_rate)) / i64::from(fps);
                let next_trim = (i128::from(*trim_start_samples) + i128::from(delta_samples)).max(0);
                *trim_start_samples = u64::try_from(next_trim).unwrap_or(0);
                clip.start_frame = clamped_start;
                clip.duration_frames = new_dur;
            }
            ClipKind::Video { trim_in_frames, .. } => {
                let delta_frames = i64::from(clamped_start) - i64::from(old_start);
                let next_trim = (i64::from(*trim_in_frames) + delta_frames).max(0);
                *trim_in_frames = u32::try_from(next_trim).unwrap_or(0);
                clip.start_frame = clamped_start;
                clip.duration_frames = new_dur;
            }
            ClipKind::Layer { .. } | ClipKind::Group { .. } | ClipKind::Text { .. } => {
                clip.start_frame = clamped_start;
                clip.duration_frames = new_dur;
            }
        }
        if let Some(track) = self.track_mut(track_id) {
            track.clips.sort_by_key(|c| c.start_frame);
        }
        true
    }

    /// Expands `clip_id` to fill until the next clip on the same track (or the end of the timeline
    /// if it is the last clip on that track).
    pub fn fill_clip_duration(&mut self, clip_id: ClipId) -> bool {
        let Some(track_id) = self.track_of_clip(clip_id) else { return false };
        if self.track(track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let Some(clip) = self.clip(clip_id) else { return false };
        let start = clip.start_frame;
        let current_end = clip.end_frame();
        let next_clip_start = self
            .track(track_id)
            .into_iter()
            .flat_map(|t| &t.clips)
            .filter(|c| c.id != clip_id && c.start_frame >= current_end)
            .map(|c| c.start_frame)
            .min();
        let target_end = next_clip_start.unwrap_or(self.duration_frames).max(current_end);
        let new_dur = target_end.saturating_sub(start).max(1);
        if new_dur <= clip.effective_duration() {
            return false;
        }
        self.resize_clip(clip_id, new_dur)
    }

    /// Reverses the cel playback order and mirrors keyframes inside `clip_id`.
    pub fn reverse_clip(&mut self, clip_id: ClipId) -> bool {
        let Some(track_id) = self.track_of_clip(clip_id) else { return false };
        if self.track(track_id).is_some_and(|t| t.locked) {
            return false;
        }
        let Some(clip) = self.clip_mut(clip_id) else { return false };
        let start = clip.start_frame;
        let end = clip.end_frame();
        let mut changed = false;
        if let ClipKind::Cel { cels } = &mut clip.kind
            && cels.len() > 1
        {
            cels.reverse();
            clip.loop_offset = 0;
            changed = true;
        }
        if !clip.keyframes.is_empty() {
            clip.keyframes.reverse_range(start, end);
            changed = true;
        }
        changed
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
        let fps = self.fps.clamp(1, 60);

        let (left_kf, right_kf) = clip.keyframes.split_at(at_frame);

        let (left_kind, right_kind) = match &clip.kind {
            ClipKind::Layer { layer } => (ClipKind::Layer { layer: *layer }, ClipKind::Layer { layer: *layer }),
            ClipKind::Text { layer } => (ClipKind::Text { layer: *layer }, ClipKind::Text { layer: *layer }),
            ClipKind::Group { members } => {
                (ClipKind::Group { members: members.clone() }, ClipKind::Group { members: members.clone() })
            }
            ClipKind::Audio { asset_id, trim_start_samples, sample_rate, channels } => {
                let offset_samples =
                    ((u64::from(left_dur).saturating_mul(u64::from(*sample_rate))) / u64::from(fps)).max(1);
                (
                    ClipKind::Audio {
                        asset_id: *asset_id,
                        trim_start_samples: *trim_start_samples,
                        sample_rate: *sample_rate,
                        channels: *channels,
                    },
                    ClipKind::Audio {
                        asset_id: *asset_id,
                        trim_start_samples: trim_start_samples.saturating_add(offset_samples),
                        sample_rate: *sample_rate,
                        channels: *channels,
                    },
                )
            }
            ClipKind::Video { video_id, layer, source_fps, trim_in_frames, frame_count } => (
                ClipKind::Video {
                    video_id: *video_id,
                    layer: *layer,
                    source_fps: *source_fps,
                    trim_in_frames: *trim_in_frames,
                    frame_count: *frame_count,
                },
                ClipKind::Video {
                    video_id: *video_id,
                    layer: *layer,
                    source_fps: *source_fps,
                    trim_in_frames: trim_in_frames.saturating_add(left_dur),
                    frame_count: *frame_count,
                },
            ),
            ClipKind::Cel { cels } => {
                if clip.loop_mode != ClipLoopMode::None {
                    (ClipKind::Cel { cels: cels.clone() }, ClipKind::Cel { cels: cels.clone() })
                } else {
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
                        left_cels.push(Cel {
                            stage_space: cel.stage_space,
                            layer: cel.layer,
                            hold: offset.saturating_sub(1),
                        });
                        let mut right_cels = vec![Cel {
                            stage_space: cel.stage_space,
                            layer: cel.layer,
                            hold: total_hold.saturating_sub(offset),
                        }];
                        if let Some(rest) = cels.get(cel_idx + 1..) {
                            right_cels.extend_from_slice(rest);
                        }
                        (ClipKind::Cel { cels: left_cels }, ClipKind::Cel { cels: right_cels })
                    }
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
            perform_onion_guide: clip.perform_onion_guide,
            perform_guide_layer: clip.perform_guide_layer,
            perform_guide_opacity: clip.perform_guide_opacity,
            perform_guide_color: clip.perform_guide_color,
            loop_mode: clip.loop_mode,
            loop_offset: clip.phase_at(at_frame),
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

    /// Ungroups `group_id`: unparents all its direct children (moving them to `group_id`'s parent
    /// group, if any) and removes the group track.
    pub fn ungroup_track(&mut self, group_id: TrackId) -> bool {
        let Some(pos) = self.tracks.iter().position(|t| t.id == group_id && t.is_group) else {
            return false;
        };
        let parent_of_group = self.tracks.get(pos).and_then(|t| t.parent_group);
        for t in &mut self.tracks {
            if t.parent_group == Some(group_id) {
                t.parent_group = parent_of_group;
            }
        }
        self.tracks.remove(pos);
        if self.active_track == Some(group_id) {
            self.active_track = self.tracks.last().map(|t| t.id);
        }
        true
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
        cels.insert(
            insert_at,
            Cel { stage_space: Some(clip.perform_onion_guide && insert_at > 0), layer, hold: hold.min(600) },
        );
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

    pub fn clip_stage_affine(&self, track_id: TrackId, frame: f32, width: u32, height: u32) -> Affine {
        let track = self.evaluate_track_affine(track_id, frame, width, height);
        if !self.camera_enabled {
            return track;
        }
        let camera = self.camera.evaluate_transform(frame).to_affine(width, height);
        track.then(camera.inverse().unwrap_or(Affine::IDENTITY))
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

    /// Evaluates the combined hierarchical [`LiveFilterState`] of `track_id` at `frame`.
    pub fn evaluate_track_filter(&self, track_id: TrackId, frame: f32) -> LiveFilterState {
        let mut filter = LiveFilterState::default();
        let mut seen = HashSet::new();
        let mut cursor = Some(track_id);
        while let Some(tid) = cursor {
            if !seen.insert(tid) {
                break;
            }
            if let Some(track) = self.track(tid) {
                filter = filter.combine(track.keyframes.evaluate_filter(frame));
                cursor = track.parent_group;
            } else {
                break;
            }
        }
        filter
    }

    /// Deterministic evaluation of the timeline at `frame` (supports integer or sub-frame),
    /// producing the bottom-to-top compositing list, Perform Motion Ghosts, Track Effects,
    /// Live Filters, Warp/Distort deformations, and Camera Track framing.
    pub fn evaluate(&self, doc: &Document, frame: f32, playing: bool) -> EvaluatedScene {
        let frame = if frame.is_finite() { frame.max(0.0) } else { 0.0 };
        let frame_u32 = frame.floor() as u32;
        let (cw, ch) = (doc.width(), doc.height());
        let any_solo = self.tracks.iter().any(|t| t.solo && t.visible);
        let managed = self.managed_layers();
        let mut items = Vec::new();

        // Evaluate global Camera Track
        let camera_transform = self.camera.evaluate_transform(frame);
        let camera_filter = self.camera.evaluate_filter(frame);
        let apply_camera = self.camera_enabled || (playing && !self.camera.is_empty());
        let camera_inv = if apply_camera && !camera_transform.is_identity() {
            camera_transform.to_affine(cw, ch).inverse().unwrap_or(Affine::IDENTITY)
        } else {
            Affine::IDENTITY
        };
        let scene_cam_filter = if apply_camera { camera_filter } else { LiveFilterState::default() };

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
            let track_filter = self.evaluate_track_filter(track.id, frame);
            let track_effects = track.effects.repaired();

            for clip in &track.clips {
                if matches!(clip.kind, ClipKind::Audio { .. }) {
                    continue;
                }
                let clip_base_opacity = track_opacity * clip.opacity.clamp(0.0, 1.0);
                if clip_base_opacity <= 0.0 {
                    continue;
                }
                let is_active_focus = !self.onion.active_track_only
                    || self.active_clip == Some(clip.id)
                    || (self.active_clip.is_none() && self.active_track == Some(track.id));

                let clip_motion_affine =
                    clip.keyframes.evaluate_transform(frame).to_affine(cw, ch).then(track_affine).then(camera_inv);
                let clip_stage_affine = track_affine.then(camera_inv);
                let clip_filter = track_filter.combine(clip.keyframes.evaluate_filter(frame)).combine(scene_cam_filter);
                let clip_deform = clip.keyframes.evaluate_deform(frame);

                // The motion guide belongs to this clip's editing exposure.
                if !playing
                    && clip.perform_onion_guide
                    && clip.contains_frame(frame_u32)
                    && let Some(guide_layer) = clip.perform_guide_layer.or_else(|| clip.first_layer())
                {
                    let guide_alpha = (clip.perform_guide_opacity.clamp(0.0, 1.0) * clip_base_opacity).clamp(0.0, 1.0);
                    if guide_alpha > 1e-4 {
                        let [r, g, b] = clip.perform_guide_color;
                        items.push(EvaluatedLayer {
                            layer: guide_layer,
                            track: track.id,
                            clip: clip.id,
                            transform: clip_motion_affine,
                            opacity: guide_alpha,
                            blend: BlendMode::Normal,
                            tint: [r, g, b, 1.0],
                            is_onion: true,
                            is_perform_guide: true,
                            filter: LiveFilterState::default(),
                            deform: clip_deform,
                            effects: TrackEffects::default(),
                            clip_to_below: false,
                        });
                    }
                }

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
                                    // When tracing over Perform Motion Ghost, drawn cels sit in stage space
                                    let onion_xform = if cel.stage_space == Some(true) {
                                        clip_stage_affine
                                    } else {
                                        clip_motion_affine
                                    };
                                    let [r, g, b] = self.onion.before_color;
                                    items.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: onion_xform,
                                        opacity: onion_alpha,
                                        blend: BlendMode::Normal,
                                        tint: [r, g, b, 1.0],
                                        is_onion: true,
                                        is_perform_guide: false,
                                        filter: LiveFilterState::default(),
                                        deform: Deform2D::default(),
                                        effects: TrackEffects::default(),
                                        clip_to_below: false,
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
                                    let onion_xform = if cel.stage_space == Some(true) {
                                        clip_stage_affine
                                    } else {
                                        clip_motion_affine
                                    };
                                    let [r, g, b] = self.onion.after_color;
                                    onion_after.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: onion_xform,
                                        opacity: onion_alpha,
                                        blend: BlendMode::Normal,
                                        tint: [r, g, b, 1.0],
                                        is_onion: true,
                                        is_perform_guide: false,
                                        filter: LiveFilterState::default(),
                                        deform: Deform2D::default(),
                                        effects: TrackEffects::default(),
                                        clip_to_below: false,
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
                        let effective_blend = if clip.blend != BlendMode::Normal { clip.blend } else { track.blend };
                        match &clip.kind {
                            ClipKind::Cel { .. } => {
                                if let Some((_, cel, _)) = clip.cel_at(frame_u32) {
                                    // Tracing cels were painted in stage coordinates.
                                    let use_stage_space = cel.stage_space == Some(true);
                                    let item_xform =
                                        if use_stage_space { clip_stage_affine } else { clip_motion_affine };
                                    let item_deform = if use_stage_space { Deform2D::default() } else { clip_deform };
                                    items.push(EvaluatedLayer {
                                        layer: cel.layer,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: item_xform,
                                        opacity: final_opacity,
                                        blend: effective_blend,
                                        tint: [0.0; 4],
                                        is_onion: false,
                                        is_perform_guide: false,
                                        filter: clip_filter,
                                        deform: item_deform,
                                        effects: track_effects,
                                        clip_to_below: track.clip_to_below,
                                    });
                                }
                            }
                            ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => {
                                items.push(EvaluatedLayer {
                                    layer: *layer,
                                    track: track.id,
                                    clip: clip.id,
                                    transform: clip_motion_affine,
                                    opacity: final_opacity,
                                    blend: effective_blend,
                                    tint: [0.0; 4],
                                    is_onion: false,
                                    is_perform_guide: false,
                                    filter: clip_filter,
                                    deform: clip_deform,
                                    effects: track_effects,
                                    clip_to_below: track.clip_to_below,
                                });
                            }
                            ClipKind::Group { members } => {
                                for &member in members {
                                    items.push(EvaluatedLayer {
                                        layer: member,
                                        track: track.id,
                                        clip: clip.id,
                                        transform: clip_motion_affine,
                                        opacity: final_opacity,
                                        blend: effective_blend,
                                        tint: [0.0; 4],
                                        is_onion: false,
                                        is_perform_guide: false,
                                        filter: clip_filter,
                                        deform: clip_deform,
                                        effects: track_effects,
                                        clip_to_below: track.clip_to_below,
                                    });
                                }
                            }
                            ClipKind::Audio { .. } => {}
                        }
                    }
                }

                if !self.onion.blend_primary {
                    items.extend(onion_after);
                }
            }
        }

        EvaluatedScene {
            frame,
            playing,
            items,
            managed_layers: managed,
            camera_transform,
            camera_filter: scene_cam_filter,
        }
    }

    /// Returns the active display transform (`LayerSpace -> CanvasSpace`) for `layer` at the
    /// current playhead (not playing), combining clip keyframes, parent track/group transforms,
    /// and the active Camera Track. Walks up masks and parent groups so painting on a mask or
    /// sub-layer uses its exact visual transform.
    pub fn layer_transform(&self, doc: &Document, layer: LayerId) -> Affine {
        let scene = self.evaluate(doc, self.playhead as f32, false);
        let mut cursor = Some(layer);
        let mut guard = 0usize;
        while let Some(lid) = cursor {
            if let Some(item) = scene.items.iter().rev().find(|item| !item.is_onion && item.layer == lid) {
                return item.transform;
            }
            cursor = doc.layer(lid).and_then(|l| l.parent);
            guard += 1;
            if guard > 32 {
                break;
            }
        }
        Affine::IDENTITY
    }

    /// Returns `(transform, spatial_fx_pad_px, needs_full_dirty)` for `layer` at the current
    /// playhead so GPU compositing during painting only redraws the transformed + FX-padded dirty
    /// region instead of recompositing the entire canvas on every brush stamp.
    pub fn layer_dirty_info(&self, doc: &Document, layer: LayerId) -> (Affine, i32, bool) {
        let scene = self.evaluate(doc, self.playhead as f32, false);
        // If this layer also appears as an onion/Perform ghost (potentially at a different
        // transform), fall back to full dirty so both copies stay in sync.
        if scene.items.iter().any(|item| item.is_onion && item.layer == layer) {
            return (Affine::IDENTITY, 0, true);
        }
        let fx_pad = |filter: LiveFilterState, effects: TrackEffects| -> i32 {
            let f = filter.repaired();
            let e = effects.repaired();
            let mut pad = f.gaussian_blur.ceil() as i32;
            if f.sharpen > 0.01 {
                pad = pad.max(2);
            }
            if e.outline_enabled {
                pad = pad.max(e.outline_width.ceil() as i32);
            }
            if e.glow_enabled {
                pad = pad.max(e.glow_radius.ceil() as i32);
            }
            if e.shadow_enabled {
                let sreach = e.shadow_blur.ceil() as i32
                    + e.shadow_offset[0].abs().ceil().max(e.shadow_offset[1].abs().ceil()) as i32;
                pad = pad.max(sreach);
            }
            pad
        };
        let mut cursor = Some(layer);
        let mut guard = 0usize;
        while let Some(lid) = cursor {
            if let Some((idx, item)) =
                scene.items.iter().enumerate().rev().find(|(_, item)| !item.is_onion && item.layer == lid)
            {
                if !item.deform.is_identity() {
                    return (item.transform, 0, true);
                }
                let mut pad = fx_pad(item.filter, item.effects);
                // Any non-onion item above clipped to this track depends on its alpha silhouette.
                for above in scene.items.get(idx + 1..).unwrap_or_default() {
                    if above.is_onion {
                        continue;
                    }
                    if above.clip_to_below {
                        if !above.deform.is_identity() {
                            return (item.transform, 0, true);
                        }
                        pad = pad.max(fx_pad(above.filter, above.effects));
                    } else if above.track != item.track {
                        break;
                    }
                }
                // Scale bilinear sampling + AA margin with the camera/clip magnification so zoomed-in
                // camera strokes never leave stale dirty-rect fringes.
                let sf = item.transform.scale_factor();
                let scale_pad = if sf.is_finite() { (sf.ceil() as i32).clamp(1, 32).saturating_mul(2) } else { 4 };
                return (item.transform, pad.saturating_add(scale_pad).saturating_add(2), false);
            }
            cursor = doc.layer(lid).and_then(|l| l.parent);
            guard += 1;
            if guard > 32 {
                break;
            }
        }
        (Affine::IDENTITY, 4, false)
    }

    fn audio_track_is_audible(&self, track_id: TrackId) -> bool {
        let mut cursor = Some(track_id);
        for _ in 0..self.tracks.len() {
            let Some(id) = cursor else { return true };
            let Some(track) = self.track(id) else { return false };
            if !track.visible || track.muted {
                return false;
            }
            cursor = track.parent_group;
        }
        cursor.is_none()
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

/// Simplifies live-recorded Perform Warp / Distort [`DeformSample`]s using Ramer–Douglas–Peucker
/// over the 16-point mesh / 4-corner quad normalized coordinates.
pub fn simplify_deform_samples(samples: &[DeformSample], filtering: f32) -> Vec<Keyframe<Deform2D>> {
    if samples.is_empty() {
        return Vec::new();
    }
    let filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { 0.35 };
    let mut sorted: Vec<DeformSample> =
        samples.iter().map(|s| DeformSample { frame: s.frame, deform: s.deform.repaired() }).collect();
    sorted.sort_by_key(|s| s.frame);
    let mut unique: Vec<DeformSample> = Vec::with_capacity(sorted.len());
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
            .map(|s| Keyframe { frame: s.frame, value: s.deform, easing: Easing::Linear })
            .collect();
    }
    let tolerance = filtering * filtering * 0.08 + filtering * 0.004;
    let mut keep = vec![false; unique.len()];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(last) = keep.last_mut() {
        *last = true;
    }
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
            let expected = start.deform.lerp(end.deform, t);
            let err = deform_distance(sample.deform, expected);
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
        .filter_map(|(s, k)| k.then_some(Keyframe { frame: s.frame, value: s.deform, easing }))
        .collect()
}

/// Simplifies live-recorded scalar Perform samples (Opacity, Audio Volume, Pan) with RDP.
pub fn simplify_scalar_samples(samples: &[ScalarSample], filtering: f32, lo: f32, hi: f32) -> Vec<Keyframe<f32>> {
    if samples.is_empty() {
        return Vec::new();
    }
    let filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { 0.35 };
    let clean = |v: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
    let mut sorted: Vec<ScalarSample> =
        samples.iter().map(|s| ScalarSample { frame: s.frame, value: clean(s.value) }).collect();
    sorted.sort_by_key(|s| s.frame);
    let mut unique: Vec<ScalarSample> = Vec::with_capacity(sorted.len());
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
            .map(|s| Keyframe { frame: s.frame, value: s.value, easing: Easing::Linear })
            .collect();
    }
    let span_val = (hi - lo).abs().max(1e-3);
    let tolerance = span_val * (filtering * filtering * 0.08 + filtering * 0.005);
    let mut keep = vec![false; unique.len()];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(last) = keep.last_mut() {
        *last = true;
    }
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
            let expected = start.value + (end.value - start.value) * t;
            let err = (sample.value - expected).abs();
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
        .filter_map(|(s, k)| k.then_some(Keyframe { frame: s.frame, value: s.value, easing }))
        .collect()
}

/// Simplifies live-recorded Perform Live Filter samples (Gaussian Blur, HSB, Sharpen, Noise) with RDP.
pub fn simplify_filter_samples(samples: &[FilterSample], filtering: f32) -> Vec<Keyframe<LiveFilterState>> {
    if samples.is_empty() {
        return Vec::new();
    }
    let filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { 0.35 };
    let mut sorted: Vec<FilterSample> =
        samples.iter().map(|s| FilterSample { frame: s.frame, filter: s.filter.repaired() }).collect();
    sorted.sort_by_key(|s| s.frame);
    let mut unique: Vec<FilterSample> = Vec::with_capacity(sorted.len());
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
            .map(|s| Keyframe { frame: s.frame, value: s.filter, easing: Easing::Linear })
            .collect();
    }
    let tolerance = filtering * filtering * 0.10 + filtering * 0.006;
    let mut keep = vec![false; unique.len()];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(last) = keep.last_mut() {
        *last = true;
    }
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
            let expected = start.filter.lerp(end.filter, t);
            let err = filter_distance(sample.filter, expected);
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
        .filter_map(|(s, k)| k.then_some(Keyframe { frame: s.frame, value: s.filter, easing }))
        .collect()
}

/// Normalized perceptual distance (`0..~1`) between two [`LiveFilterState`] values.
fn filter_distance(a: LiveFilterState, b: LiveFilterState) -> f32 {
    let db = (a.gaussian_blur - b.gaussian_blur).abs() / 64.0;
    let dh = (a.hue_shift - b.hue_shift).abs() / 180.0;
    let ds = (a.saturation - b.saturation).abs();
    let dl = (a.brightness - b.brightness).abs();
    let dsh = (a.sharpen - b.sharpen).abs();
    let dn = (a.noise - b.noise).abs();
    db.max(dh).max(ds).max(dl).max(dsh).max(dn)
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

/// Maximum normalized control-point displacement between two [`Deform2D`] states.
fn deform_distance(a: Deform2D, b: Deform2D) -> f32 {
    let mut worst = 0.0_f32;
    let ca = a.distort_corners.unwrap_or(IDENTITY_DISTORT_CORNERS);
    let cb = b.distort_corners.unwrap_or(IDENTITY_DISTORT_CORNERS);
    for ([ax, ay], [bx, by]) in ca.iter().zip(cb.iter()) {
        worst = worst.max((ax - bx).hypot(ay - by));
    }
    let id = identity_warp_mesh();
    let ma = a.warp_mesh.unwrap_or(id);
    let mb = b.warp_mesh.unwrap_or(id);
    for ([ax, ay], [bx, by]) in ma.iter().zip(mb.iter()) {
        worst = worst.max((ax - bx).hypot(ay - by));
    }
    worst
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
    fn audio_mix_is_order_independent_and_respects_group_visibility() {
        let mut tl = Timeline::default();
        let mut tracks = Vec::new();
        for sample in [0.8, 0.8, -0.8] {
            let (track, clip, _) = tl.add_audio_clip(None, "Tone".into(), 8_000, 1, vec![sample; 8_000], 0).unwrap();
            tl.clip_mut(clip).unwrap().keyframes.set_pan(0, -1.0, None);
            tracks.push(track);
        }
        let mix = tl.mix_audio_range(0, 0, 8_000);
        assert!((mix[0] - 0.8).abs() < 1e-5);
        assert!(tl.move_track(tracks[2], -2));
        assert_eq!(mix, tl.mix_audio_range(0, 0, 8_000));
        let group = tl.group_tracks(&tracks, None).unwrap();
        tl.track_mut(group).unwrap().visible = false;
        assert!(tl.mix_audio_range(0, 0, 8_000).iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn splitting_and_moving_clips_preserve_duration_and_easing() {
        let l1 = LayerId(1);
        let mut tl = Timeline::new_with_layer(l1);
        let track = tl.active_track.unwrap();
        let clip = tl.add_layer_clip(track, 10, 20, l1, Some("Block")).unwrap();
        let keys = &mut tl.clip_mut(clip).unwrap().keyframes;
        keys.set_transform(10, Transform2D::default(), Some(Easing::EaseInOut));
        keys.set_transform(24, Transform2D { position: [100.0, 0.0], ..Transform2D::default() }, None);
        keys.set_opacity(14, 0.2, Some(Easing::Hold));
        keys.set_opacity(22, 0.8, None);
        keys.set_opacity(28, 1.0, None);

        assert!(tl.move_clip(clip, 30, None));
        assert_eq!(tl.clip(clip).unwrap().keyframes.transform[0].frame, 30);
        let before = tl.clip(clip).unwrap().keyframes.clone();
        let right = tl.split_clip(clip, 40).unwrap();
        assert_eq!(tl.clip(clip).unwrap().duration_frames, 10);
        assert_eq!(tl.clip(right).unwrap().start_frame, 40);
        assert_eq!(tl.clip(right).unwrap().duration_frames, 10);
        for half_frame in 60..100 {
            let frame = half_frame as f32 / 2.0;
            let after = &tl.clip(if frame < 40.0 { clip } else { right }).unwrap().keyframes;
            assert_eq!(before.evaluate_transform(frame), after.evaluate_transform(frame));
            assert_eq!(before.evaluate_opacity(frame), after.evaluate_opacity(frame));
        }
        assert_eq!(before.evaluate_opacity(42.0), 0.8);
    }

    #[test]
    fn auto_draw_creates_frames_and_splits_holds() {
        let mut doc = Document::new(100, 100);
        let l0 = doc.current();
        let mut tl = Timeline::new_with_layer(l0);
        let cid = tl.active_clip.unwrap();

        // 1. Drawing on frame 1 (immediately after cel 0 at frame 0) extends the existing CelClip
        tl.playhead = 1;
        let (l1, created) = tl.ensure_frame_for_drawing(|| Some(doc.add_layer())).unwrap();
        assert!(created);
        assert_ne!(l1, l0);
        assert_eq!(tl.clip(cid).unwrap().duration_frames, 2);

        // 2. Drawing on an empty gap at frame 10 creates a brand new CelClip starting at frame 10
        tl.playhead = 10;
        let (l10, created10) = tl.ensure_frame_for_drawing(|| Some(doc.add_layer())).unwrap();
        assert!(created10);
        let new_cid = tl.active_clip.unwrap();
        assert_ne!(new_cid, cid);
        assert_eq!(tl.clip(new_cid).unwrap().start_frame, 10);
        assert_eq!(tl.clip(new_cid).unwrap().layer_at(10), Some(l10));

        // 3. Extend cel at frame 10 to hold for 4 extra frames (10..15) and draw on frame 12 -> auto-splits hold!
        tl.resize_clip(new_cid, 5);
        tl.playhead = 12;
        let (l12, created12) = tl.ensure_frame_for_drawing(|| Some(doc.add_layer())).unwrap();
        assert!(created12);
        assert_eq!(tl.clip(new_cid).unwrap().layer_at(10), Some(l10));
        assert_eq!(tl.clip(new_cid).unwrap().layer_at(11), Some(l10));
        assert_eq!(tl.clip(new_cid).unwrap().layer_at(12), Some(l12));
        assert_eq!(tl.clip(new_cid).unwrap().layer_at(14), Some(l12));
    }

    #[test]
    fn perform_onion_guide_moves_first_frame_as_onion_skin_under_hand_drawn_cels() {
        let mut doc = Document::new(200, 100);
        let guide_cel = doc.current();
        let drawn_cel = doc.add_layer();
        let mut tl = Timeline::new_with_layer(guide_cel);
        let cid = tl.active_clip.unwrap();
        tl.clip_mut(cid).unwrap().perform_onion_guide = true;
        tl.insert_cel(cid, Some(0), drawn_cel, 10);

        // Record Perform motion from x=0 at frame 0 to x=100 at frame 10 and enable Perform Motion Ghost
        {
            let clip = tl.clip_mut(cid).unwrap();
            clip.perform_onion_guide = true;
            clip.perform_guide_opacity = 0.5;
            clip.keyframes.set_transform(0, Transform2D::default(), Some(Easing::Linear));
            clip.keyframes.set_transform(
                10,
                Transform2D { position: [100.0, 0.0], ..Transform2D::default() },
                Some(Easing::Linear),
            );
        }

        // At frame 5 while editing (playing = false):
        // - The Perform Motion Ghost renders `guide_cel` shifted by x=+50 as an onion guide
        // - The hand-drawn `drawn_cel` (`cel_idx == 1`) sits in unshifted stage space!
        let scene = tl.evaluate(&doc, 5.0, false);
        let ghost = scene.items.iter().find(|i| i.is_perform_guide).expect("perform motion ghost present");
        assert_eq!(ghost.layer, guide_cel);
        assert!(ghost.is_onion);
        assert!((ghost.transform.e - 50.0).abs() < 1e-3);

        let primary = scene.items.iter().find(|i| !i.is_onion && i.layer == drawn_cel).unwrap();
        assert!(primary.transform.e.abs() < 1e-3);

        assert!(tl.evaluate(&doc, 20.0, false).items.iter().all(|item| !item.is_perform_guide));
        tl.clip_mut(cid).unwrap().perform_onion_guide = false;
        let hidden = tl.evaluate(&doc, 5.0, true);
        assert_eq!(
            hidden.items.iter().find(|i| !i.is_onion && i.layer == drawn_cel).unwrap().transform,
            primary.transform
        );
        tl.clip_mut(cid).unwrap().perform_onion_guide = true;

        // During playback (playing = true), the onion ghost turns off cleanly
        let play_scene = tl.evaluate(&doc, 5.0, true);
        assert!(play_scene.items.iter().all(|i| !i.is_perform_guide));
    }

    #[test]
    fn loop_cuts_and_trims_keep_cel_phase_and_visible_exposure() {
        for mode in [ClipLoopMode::Loop, ClipLoopMode::PingPong] {
            let mut tl = Timeline::new_with_layer(LayerId(1));
            let id = tl.active_clip.unwrap();
            tl.insert_cel(id, Some(0), LayerId(2), 1);
            tl.insert_cel(id, Some(1), LayerId(3), 0);
            tl.clip_mut(id).unwrap().loop_mode = mode;
            tl.resize_clip(id, 13);
            let before = tl.clip(id).unwrap().clone();
            let right = tl.split_clip(id, 1).unwrap();
            assert_eq!(tl.clip(id).unwrap().end_frame(), 1);
            assert!(tl.trim_clip_left(right, 5));
            let after = tl.clip(right).unwrap();
            assert_eq!(after.end_frame(), before.end_frame());
            for frame in 5..after.end_frame() {
                assert_eq!(after.cel_at(frame), before.cel_at(frame));
            }
            for index in 0..3 {
                assert_eq!(after.cel_start_frame(index), (5..13).find(|&f| before.cel_at(f).unwrap().0 == index));
            }
        }
    }

    #[test]
    fn track_management_effects_and_clip_to_below() {
        let mut doc = Document::new(100, 100);
        let l1 = doc.current();
        let l2 = doc.add_layer();
        let mut tl = Timeline::new_with_layer(l1);
        let t1 = tl.active_track.unwrap();
        let t2 = tl.add_track(Some("Shading"), false);
        tl.add_layer_clip(t2, 0, 10, l2, Some("Shadow")).unwrap();

        {
            let tr = tl.track_mut(t2).unwrap();
            tr.clip_to_below = true;
            tr.effects.outline_enabled = true;
            tr.effects.outline_width = 6.0;
            tr.effects.shadow_enabled = true;
        }
        let scene = tl.evaluate(&doc, 0.0, true);
        let top = scene.items.iter().find(|i| i.track == t2).unwrap();
        assert!(top.clip_to_below);
        assert!(top.effects.outline_enabled);
        assert_eq!(top.effects.outline_width, 6.0);

        // Reorder and merge down
        assert!(tl.move_track(t2, -1));
        assert_eq!(tl.tracks[0].id, t2);
        assert!(tl.move_track(t2, 1));
        assert!(tl.merge_track_down(t2));
        assert_eq!(tl.tracks.len(), 1);
        assert_eq!(tl.tracks[0].id, t1);
        assert_eq!(tl.tracks[0].clips.len(), 2);

        // Delete track returns referenced layers
        let removed_layers = tl.delete_track(t1).unwrap();
        assert!(removed_layers.contains(&l1));
        assert!(removed_layers.contains(&l2));
    }

    #[test]
    fn audio_clip_mixing_respects_volume_pan_and_mute() {
        let mut tl = Timeline { fps: 24, ..Default::default() };
        // 0.5 seconds of constant 0.5 mono signal at 24000 Hz
        let pcm = vec![0.5_f32; 12_000];
        let (tid, cid, asset_id) = tl.add_audio_clip(None, "Voice".to_owned(), 24_000, 1, pcm, 0).unwrap();
        assert!(tl.audio_asset(asset_id).is_some());
        assert!(!tl.audio_asset(asset_id).unwrap().peaks.is_empty());

        // Fade volume from 0.0 at frame 0 to 1.0 at frame 12, panned hard right (+1.0)
        {
            let clip = tl.clip_mut(cid).unwrap();
            clip.keyframes.set_volume(0, 0.0, Some(Easing::Linear));
            clip.keyframes.set_volume(12, 1.0, Some(Easing::Linear));
            clip.keyframes.set_pan(0, 1.0, Some(Easing::Linear));
        }

        let mixed = tl.mix_audio_range(0, 11, 24_000);
        assert!(!mixed.is_empty());
        // Hard right pan -> left channel near 0, right channel rises toward 0.5
        let last_l = mixed[mixed.len() - 2];
        let last_r = mixed[mixed.len() - 1];
        assert!(last_l.abs() < 1e-3);
        assert!(last_r > 0.35);

        // Muting the track silences the mix
        tl.track_mut(tid).unwrap().muted = true;
        let muted = tl.mix_audio_range(0, 11, 24_000);
        assert!(muted.iter().all(|&s| s.abs() < 1e-6));
    }

    #[test]
    fn live_filters_warp_deform_and_camera_track_evaluation() {
        let doc = Document::new(200, 100);
        let l1 = doc.current();
        let mut tl = Timeline::new_with_layer(l1);
        let cid = tl.active_clip.unwrap();
        tl.resize_clip(cid, 20);

        {
            let clip = tl.clip_mut(cid).unwrap();
            clip.keyframes.set_filter(
                0,
                LiveFilterState { gaussian_blur: 0.0, ..Default::default() },
                Some(Easing::Linear),
            );
            clip.keyframes.set_filter(
                10,
                LiveFilterState { gaussian_blur: 20.0, hue_shift: 90.0, ..Default::default() },
                Some(Easing::Linear),
            );
            let mut warped = identity_warp_mesh();
            warped[0] = [0.2, 0.1];
            clip.keyframes.set_deform(0, Deform2D::default(), Some(Easing::Linear));
            clip.keyframes.set_deform(
                10,
                Deform2D { distort_corners: None, warp_mesh: Some(warped) },
                Some(Easing::Linear),
            );
        }
        // Enable Camera Track shifted +40px in X -> scene items get camera^-1 (-40px in X)
        tl.camera_enabled = true;
        tl.camera.set_transform(0, Transform2D { position: [40.0, 0.0], ..Default::default() }, Some(Easing::Linear));

        let scene = tl.evaluate(&doc, 5.0, true);
        let item = scene.items.first().unwrap();
        assert!((item.filter.gaussian_blur - 10.0).abs() < 1e-3);
        assert!((item.filter.hue_shift - 45.0).abs() < 1e-3);
        assert!((item.transform.e - (-40.0)).abs() < 1e-3);
        assert!(!item.deform.is_identity());
        let mesh = item.deform.to_canvas_mesh(200, 100).unwrap();
        assert!((mesh[0].x - 20.0).abs() < 1e-3);

        // Perform Warp RDP simplifies linear deformation trajectory to 2 endpoints
        let samples: Vec<DeformSample> = (0..=10)
            .map(|f| {
                let mut m = identity_warp_mesh();
                m[0] = [f as f32 * 0.02, 0.0];
                DeformSample { frame: f, deform: Deform2D { distort_corners: None, warp_mesh: Some(m) } }
            })
            .collect();
        let simplified = simplify_deform_samples(&samples, 0.4);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn v24_anchor_keep_position_clip_loop_trim_fill_reverse_ungroup_markers_and_perform_rdp() {
        let mut doc = Document::new(200, 100);
        let l1 = doc.current();
        let mut tl = Timeline::new_with_layer(l1);
        let tid = tl.active_track.unwrap();
        let cid = tl.active_clip.unwrap();

        // 1. Anchor reposition preserves visual pixels under scale + rotation + translation
        let orig = Transform2D { position: [25.0, -15.0], scale: [1.5, 0.8], rotation: 0.75, anchor: [0.5, 0.5] };
        let shifted = orig.with_anchor_preserving_visual([0.15, 0.85], 200, 100);
        let m_orig = orig.to_affine(200, 100);
        let m_shifted = shifted.to_affine(200, 100);
        for (px, py) in [(0.0, 0.0), (200.0, 0.0), (200.0, 100.0), (0.0, 100.0), (77.0, 33.0)] {
            let (ox, oy) = (m_orig.a * px + m_orig.c * py + m_orig.e, m_orig.b * px + m_orig.d * py + m_orig.f);
            let (sx, sy) =
                (m_shifted.a * px + m_shifted.c * py + m_shifted.e, m_shifted.b * px + m_shifted.d * py + m_shifted.f);
            assert!(
                (ox - sx).abs() < 1e-3 && (oy - sy).abs() < 1e-3,
                "pixel ({px},{py}) moved: ({ox},{oy}) vs ({sx},{sy})"
            );
        }

        // 2. ClipLoopMode::Loop & PingPong cycle cels over extended clip duration
        let l2 = doc.add_layer();
        let l3 = doc.add_layer();
        {
            let clip = tl.clip_mut(cid).unwrap();
            clip.kind = ClipKind::Cel {
                cels: vec![
                    Cel { stage_space: Some(false), layer: l1, hold: 0 },
                    Cel { stage_space: Some(false), layer: l2, hold: 0 },
                    Cel { stage_space: Some(false), layer: l3, hold: 0 },
                ],
            };
            clip.loop_mode = ClipLoopMode::Loop;
        }
        assert!(tl.resize_clip(cid, 9));
        let clip = tl.clip(cid).unwrap();
        assert_eq!(clip.cel_at(0).map(|c| c.1.layer), Some(l1));
        assert_eq!(clip.cel_at(3).map(|c| c.1.layer), Some(l1));
        assert_eq!(clip.cel_at(4).map(|c| c.1.layer), Some(l2));
        assert_eq!(clip.cel_at(5).map(|c| c.1.layer), Some(l3));

        tl.clip_mut(cid).unwrap().loop_mode = ClipLoopMode::PingPong;
        let clip = tl.clip(cid).unwrap();
        // Ping-pong of 3 frames (0,1,2): 0,1,2,1,0,1,2
        assert_eq!(clip.cel_at(0).map(|c| c.1.layer), Some(l1));
        assert_eq!(clip.cel_at(1).map(|c| c.1.layer), Some(l2));
        assert_eq!(clip.cel_at(2).map(|c| c.1.layer), Some(l3));
        assert_eq!(clip.cel_at(3).map(|c| c.1.layer), Some(l2));
        assert_eq!(clip.cel_at(4).map(|c| c.1.layer), Some(l1));

        // 3. Reverse clip reverses cels and mirrors keyframes
        tl.clip_mut(cid).unwrap().keyframes.set_transform(0, orig, None);
        tl.clip_mut(cid).unwrap().keyframes.set_transform(8, Transform2D::default(), None);
        assert!(tl.reverse_clip(cid));
        let clip = tl.clip(cid).unwrap();
        if let ClipKind::Cel { cels } = &clip.kind {
            assert_eq!(cels[0].layer, l3);
            assert_eq!(cels[2].layer, l1);
        }
        assert!((clip.keyframes.evaluate_transform(8.0).position[0] - 25.0).abs() < 1e-3);

        // 4. Trim left keeps end frame fixed; Fill duration expands to timeline end
        assert!(tl.trim_clip_left(cid, 3));
        let clip = tl.clip(cid).unwrap();
        assert_eq!(clip.start_frame, 3);
        assert_eq!(clip.duration_frames, 6);
        assert!(tl.fill_clip_duration(cid));
        assert_eq!(tl.clip(cid).unwrap().end_frame(), tl.duration_frames);

        // 5. Keyframe move & adjacent jump
        assert!(tl.clip_mut(cid).unwrap().keyframes.move_transform(8, 12));
        assert_eq!(tl.clip(cid).unwrap().keyframes.adjacent_keyframe(0, 1), Some(12));
        assert_eq!(tl.clip(cid).unwrap().keyframes.adjacent_keyframe(20, -1), Some(12));

        // 6. Track grouping & ungrouping
        let t2 = tl.add_track(Some("Track 2"), false);
        let gid = tl.group_tracks(&[tid, t2], Some("Group A")).unwrap();
        assert_eq!(tl.track(tid).unwrap().parent_group, Some(gid));
        assert!(tl.ungroup_track(gid));
        assert!(tl.track(gid).is_none());
        assert_eq!(tl.track(tid).unwrap().parent_group, None);

        // 7. Timeline Markers CRUD & repair
        let m1 = tl.add_marker(15, Some("Beat 1"), None);
        let m2 = tl.add_marker(5, Some("Intro"), Some(0xFF00_FF00));
        assert_eq!(tl.markers.len(), 2);
        assert_eq!(tl.markers[0].id, m2); // sorted by frame
        assert!(tl.move_marker(m1, 25));
        assert_eq!(tl.markers[1].frame, 25);
        assert!(tl.delete_marker(m2));
        assert_eq!(tl.markers.len(), 1);

        // 8. Scalar & Filter Perform RDP simplification
        let op_samples: Vec<ScalarSample> =
            (0..=10).map(|f| ScalarSample { frame: f, value: f as f32 * 0.1 }).collect();
        let simp_op = simplify_scalar_samples(&op_samples, 0.5, 0.0, 1.0);
        assert_eq!(simp_op.len(), 2);
        let flt_samples: Vec<FilterSample> = (0..=10)
            .map(|f| FilterSample {
                frame: f,
                filter: LiveFilterState { gaussian_blur: f as f32 * 2.0, ..Default::default() },
            })
            .collect();
        let simp_flt = simplify_filter_samples(&flt_samples, 0.5);
        assert_eq!(simp_flt.len(), 2);
    }
}
