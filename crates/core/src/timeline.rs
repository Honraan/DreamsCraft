//! Multi-track Timeline session facade for UniFFI (Android Jetpack Compose) and Desktop (`egui`):
//! atomic undoable track/clip/cel/keyframe mutations, single-call `TimelineState` snapshot,
//! live Perform motion recording with RDP curve simplification, and PNG sequence export.

use std::fs;
use std::io::BufWriter;
use std::path::Path;

use procraft_engine::animation::Playback;
use procraft_engine::audio::{decode_wav_pcm, float_pcm_to_i16};
use procraft_engine::document::{BlendMode, Document, LayerId, LayerKind};
use procraft_engine::timeline::{
    Cel, Clip, ClipId, ClipKind, Deform2D, DeformSample, Easing, Keyframe, LiveFilterState, MotionSample,
    OnionSkinSettings, Timeline, Track, TrackEffects, TrackId, Transform2D, WorkArea, simplify_deform_samples,
    simplify_motion_samples,
};

use crate::animation::AnimationPlayback;
use crate::project::{file_error, write_atomically};
use crate::{CanvasSession, MobileError};

/// Easing curve type between two keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TimelineEasingKind {
    Hold,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier,
}

/// Easing curve parameters exposed over UniFFI.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineEasingInfo {
    pub kind: TimelineEasingKind,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl From<Easing> for TimelineEasingInfo {
    fn from(e: Easing) -> Self {
        match e {
            Easing::Hold => TimelineEasingInfo { kind: TimelineEasingKind::Hold, x1: 0.0, y1: 0.0, x2: 1.0, y2: 0.0 },
            Easing::Linear => {
                TimelineEasingInfo { kind: TimelineEasingKind::Linear, x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.0 }
            }
            Easing::EaseIn => {
                TimelineEasingInfo { kind: TimelineEasingKind::EaseIn, x1: 0.42, y1: 0.0, x2: 1.0, y2: 1.0 }
            }
            Easing::EaseOut => {
                TimelineEasingInfo { kind: TimelineEasingKind::EaseOut, x1: 0.0, y1: 0.0, x2: 0.58, y2: 1.0 }
            }
            Easing::EaseInOut => {
                TimelineEasingInfo { kind: TimelineEasingKind::EaseInOut, x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 }
            }
            Easing::CubicBezier { x1, y1, x2, y2 } => {
                TimelineEasingInfo { kind: TimelineEasingKind::CubicBezier, x1, y1, x2, y2 }
            }
        }
    }
}

impl From<&TimelineEasingInfo> for Easing {
    fn from(info: &TimelineEasingInfo) -> Self {
        match info.kind {
            TimelineEasingKind::Hold => Easing::Hold,
            TimelineEasingKind::Linear => Easing::Linear,
            TimelineEasingKind::EaseIn => Easing::EaseIn,
            TimelineEasingKind::EaseOut => Easing::EaseOut,
            TimelineEasingKind::EaseInOut => Easing::EaseInOut,
            TimelineEasingKind::CubicBezier => {
                Easing::CubicBezier { x1: info.x1, y1: info.y1, x2: info.x2, y2: info.y2 }.repaired()
            }
        }
    }
}

/// Non-destructive 2D spatial transform (Move, Scale, Rotate, Anchor) exposed over UniFFI.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineTransformInfo {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

impl Default for TimelineTransformInfo {
    fn default() -> Self {
        Self::from(Transform2D::default())
    }
}

impl From<Transform2D> for TimelineTransformInfo {
    fn from(t: Transform2D) -> Self {
        TimelineTransformInfo {
            x: t.position[0],
            y: t.position[1],
            scale_x: t.scale[0],
            scale_y: t.scale[1],
            rotation: t.rotation,
            anchor_x: t.anchor[0],
            anchor_y: t.anchor[1],
        }
    }
}

impl From<&TimelineTransformInfo> for Transform2D {
    fn from(info: &TimelineTransformInfo) -> Self {
        Transform2D {
            position: [info.x, info.y],
            scale: [info.scale_x, info.scale_y],
            rotation: info.rotation,
            anchor: [info.anchor_x, info.anchor_y],
        }
        .repaired()
    }
}

/// Non-destructive live image filter parameters exposed over UniFFI.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineLiveFilterInfo {
    pub gaussian_blur: f32,
    pub hue_shift: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub sharpen: f32,
    pub noise: f32,
}

impl Default for TimelineLiveFilterInfo {
    fn default() -> Self {
        Self::from(LiveFilterState::default())
    }
}

impl From<LiveFilterState> for TimelineLiveFilterInfo {
    fn from(f: LiveFilterState) -> Self {
        TimelineLiveFilterInfo {
            gaussian_blur: f.gaussian_blur,
            hue_shift: f.hue_shift,
            saturation: f.saturation,
            brightness: f.brightness,
            sharpen: f.sharpen,
            noise: f.noise,
        }
    }
}

impl From<&TimelineLiveFilterInfo> for LiveFilterState {
    fn from(info: &TimelineLiveFilterInfo) -> Self {
        LiveFilterState {
            gaussian_blur: info.gaussian_blur,
            hue_shift: info.hue_shift,
            saturation: info.saturation,
            brightness: info.brightness,
            sharpen: info.sharpen,
            noise: info.noise,
        }
        .repaired()
    }
}

/// Non-destructive Warp (4×4 mesh = 32 floats) / Distort (4 corners = 8 floats) exposed over UniFFI.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct TimelineDeformInfo {
    /// Flattened 4 normalized `(x, y)` corners (8 floats), or empty when unused.
    pub distort_corners: Vec<f32>,
    /// Flattened 16 normalized `(x, y)` control points (32 floats), or empty when unused.
    pub warp_mesh: Vec<f32>,
}

impl From<Deform2D> for TimelineDeformInfo {
    fn from(d: Deform2D) -> Self {
        TimelineDeformInfo {
            distort_corners: d.distort_corners.map(|c| c.iter().flat_map(|p| *p).collect()).unwrap_or_default(),
            warp_mesh: d.warp_mesh.map(|m| m.iter().flat_map(|p| *p).collect()).unwrap_or_default(),
        }
    }
}

impl From<&TimelineDeformInfo> for Deform2D {
    fn from(info: &TimelineDeformInfo) -> Self {
        let distort_corners = if info.distort_corners.len() >= 8 {
            Some(std::array::from_fn(|i| {
                [
                    info.distort_corners.get(i * 2).copied().unwrap_or(0.0),
                    info.distort_corners.get(i * 2 + 1).copied().unwrap_or(0.0),
                ]
            }))
        } else {
            None
        };
        let warp_mesh = if info.warp_mesh.len() >= 32 {
            Some(std::array::from_fn(|i| {
                [
                    info.warp_mesh.get(i * 2).copied().unwrap_or(0.0),
                    info.warp_mesh.get(i * 2 + 1).copied().unwrap_or(0.0),
                ]
            }))
        } else {
            None
        };
        Deform2D { distort_corners, warp_mesh }.repaired()
    }
}

/// Non-destructive Layer Effects attached to a Track, exposed over UniFFI.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineTrackEffectsInfo {
    pub outline_enabled: bool,
    pub outline_width: f32,
    pub outline_color: Vec<f32>,
    pub shadow_enabled: bool,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_color: Vec<f32>,
    pub glow_enabled: bool,
    pub glow_radius: f32,
    pub glow_intensity: f32,
    pub glow_color: Vec<f32>,
    pub glow_inner: bool,
    pub color_overlay_enabled: bool,
    pub color_overlay_color: Vec<f32>,
}

impl From<TrackEffects> for TimelineTrackEffectsInfo {
    fn from(e: TrackEffects) -> Self {
        TimelineTrackEffectsInfo {
            outline_enabled: e.outline_enabled,
            outline_width: e.outline_width,
            outline_color: e.outline_color.to_vec(),
            shadow_enabled: e.shadow_enabled,
            shadow_offset_x: e.shadow_offset[0],
            shadow_offset_y: e.shadow_offset[1],
            shadow_blur: e.shadow_blur,
            shadow_color: e.shadow_color.to_vec(),
            glow_enabled: e.glow_enabled,
            glow_radius: e.glow_radius,
            glow_intensity: e.glow_intensity,
            glow_color: e.glow_color.to_vec(),
            glow_inner: e.glow_inner,
            color_overlay_enabled: e.color_overlay_enabled,
            color_overlay_color: e.color_overlay_color.to_vec(),
        }
    }
}

fn vec4_or(v: &[f32], fallback: [f32; 4]) -> [f32; 4] {
    match v {
        [r, g, b, a, ..] => [*r, *g, *b, *a],
        _ => fallback,
    }
}

impl From<&TimelineTrackEffectsInfo> for TrackEffects {
    fn from(info: &TimelineTrackEffectsInfo) -> Self {
        let def = TrackEffects::default();
        TrackEffects {
            outline_enabled: info.outline_enabled,
            outline_width: info.outline_width,
            outline_color: vec4_or(&info.outline_color, def.outline_color),
            shadow_enabled: info.shadow_enabled,
            shadow_offset: [info.shadow_offset_x, info.shadow_offset_y],
            shadow_blur: info.shadow_blur,
            shadow_color: vec4_or(&info.shadow_color, def.shadow_color),
            glow_enabled: info.glow_enabled,
            glow_radius: info.glow_radius,
            glow_intensity: info.glow_intensity,
            glow_color: vec4_or(&info.glow_color, def.glow_color),
            glow_inner: info.glow_inner,
            color_overlay_enabled: info.color_overlay_enabled,
            color_overlay_color: vec4_or(&info.color_overlay_color, def.color_overlay_color),
        }
        .repaired()
    }
}

/// A keyframe on a clip, track, or camera property sub-track.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineKeyframeInfo {
    pub frame: u32,
    pub easing: TimelineEasingInfo,
    pub transform: Option<TimelineTransformInfo>,
    pub opacity: Option<f32>,
    pub filter: Option<TimelineLiveFilterInfo>,
    pub deform: Option<TimelineDeformInfo>,
    pub scalar: Option<f32>,
}

/// A hand-drawn cel inside a Flipbook `Cel` clip.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TimelineCelInfo {
    pub index: u32,
    pub layer_id: u64,
    pub start_frame: u32,
    pub duration_frames: u32,
    pub hold: u32,
}

/// Type of content held by a [`TimelineClipInfo`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TimelineClipKind {
    Cel,
    Layer,
    Group,
    Audio,
    Video,
    Text,
}

/// Snapshot of a timed content block on a track.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineClipInfo {
    pub id: u64,
    pub track_id: u64,
    pub name: String,
    pub start_frame: u32,
    pub duration_frames: u32,
    pub blend: u32,
    pub opacity: f32,
    pub kind: TimelineClipKind,
    pub primary_layer_id: u64,
    pub cels: Vec<TimelineCelInfo>,
    pub transform_keyframes: Vec<TimelineKeyframeInfo>,
    pub opacity_keyframes: Vec<TimelineKeyframeInfo>,
    pub filter_keyframes: Vec<TimelineKeyframeInfo>,
    pub deform_keyframes: Vec<TimelineKeyframeInfo>,
    pub volume_keyframes: Vec<TimelineKeyframeInfo>,
    pub pan_keyframes: Vec<TimelineKeyframeInfo>,
    pub perform_onion_guide: bool,
    pub perform_guide_isolated: bool,
    pub perform_guide_opacity: f32,
    pub perform_guide_color: Vec<f32>,
    pub audio_asset_id: Option<u64>,
    /// Flattened `[min0, max0, min1, max1, ...]` peaks for AudioClip waveform drawing.
    pub waveform_peaks: Vec<f32>,
}

/// Snapshot of a horizontal track on the timeline.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineTrackInfo {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub solo: bool,
    pub opacity: f32,
    pub blend: u32,
    pub color_tag: u32,
    pub parent_group: Option<u64>,
    pub is_group: bool,
    pub expanded: bool,
    pub clip_to_below: bool,
    pub alpha_lock: bool,
    pub muted: bool,
    pub effects: TimelineTrackEffectsInfo,
    pub clips: Vec<TimelineClipInfo>,
    pub transform_keyframes: Vec<TimelineKeyframeInfo>,
    pub opacity_keyframes: Vec<TimelineKeyframeInfo>,
    pub filter_keyframes: Vec<TimelineKeyframeInfo>,
}

/// Onion skin configuration for the timeline.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineOnionInfo {
    pub enabled: bool,
    pub before_frames: u32,
    pub after_frames: u32,
    pub opacity: f32,
    pub before_color: Vec<f32>,
    pub after_color: Vec<f32>,
    pub blend_primary: bool,
    pub active_track_only: bool,
}

impl From<&OnionSkinSettings> for TimelineOnionInfo {
    fn from(o: &OnionSkinSettings) -> Self {
        TimelineOnionInfo {
            enabled: o.enabled,
            before_frames: o.before_frames,
            after_frames: o.after_frames,
            opacity: o.opacity,
            before_color: o.before_color.to_vec(),
            after_color: o.after_color.to_vec(),
            blend_primary: o.blend_primary,
            active_track_only: o.active_track_only,
        }
    }
}

/// Work Area In/Out range on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TimelineWorkAreaInfo {
    pub start: u32,
    pub end: u32,
}

/// Complete snapshot of the multi-track timeline state for UI rendering in a single FFI call.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineState {
    pub fps: u32,
    pub duration_frames: u32,
    pub playhead: u32,
    pub playback: AnimationPlayback,
    pub work_area: Option<TimelineWorkAreaInfo>,
    pub onion: TimelineOnionInfo,
    /// Ordered bottom-to-top.
    pub tracks: Vec<TimelineTrackInfo>,
    pub active_track_id: Option<u64>,
    pub active_clip_id: Option<u64>,
    pub flipbook_open: bool,
    pub show_backstage: bool,
    pub motion_filtering: f32,
    pub auto_create_frame_on_draw: bool,
    pub auto_split_cel_on_draw: bool,
    /// Evaluated Transform2D of the active clip (or active track) at `playhead`.
    pub active_evaluated_transform: TimelineTransformInfo,
    /// Evaluated opacity of the active clip (or active track) at `playhead`.
    pub active_evaluated_opacity: f32,
    /// Evaluated LiveFilterState of the active clip at `playhead`.
    pub active_evaluated_filter: TimelineLiveFilterInfo,
    /// Evaluated Deform2D (Warp/Distort) of the active clip at `playhead`.
    pub active_evaluated_deform: TimelineDeformInfo,
    /// Global Camera Track state.
    pub camera_enabled: bool,
    pub camera_transform: TimelineTransformInfo,
    pub camera_filter: TimelineLiveFilterInfo,
    pub camera_transform_keyframes: Vec<TimelineKeyframeInfo>,
    pub camera_filter_keyframes: Vec<TimelineKeyframeInfo>,
    /// True if at least one AudioClip is present on the timeline.
    pub has_audio: bool,
    pub dialogue_markers: Vec<TimelineDialogueMarker>,
    pub flipbook_groups: Vec<u64>,
}

/// A live-recorded motion sample during Perform mode.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PerformSampleInfo {
    pub frame: u32,
    pub transform: TimelineTransformInfo,
}

/// A live-recorded Warp/Distort sample during Perform Warp mode.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PerformDeformSampleInfo {
    pub frame: u32,
    pub deform: TimelineDeformInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TimelineDialogueMarker {
    pub frame: u32,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TimelineKeyframeChannel {
    Transform,
    Opacity,
    Filter,
    Deform,
    Volume,
    Pan,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TimelineKeyframeAddress {
    pub clip_id: u64,
    pub frame: u32,
    pub channel: TimelineKeyframeChannel,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TimelineCopiedKeyframe {
    pub address: TimelineKeyframeAddress,
    pub value: TimelineKeyframeInfo,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum TimelineClipBatch {
    Move,
    TrimStart,
    TrimEnd,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum FlipbookBatch {
    Exposure,
    Reverse,
    Duplicate,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TimelineVideoRequest {
    pub clip_id: u64,
    pub file: String,
    pub time_us: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TimelineVideoPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[uniffi::export(with_foreign)]
pub trait TimelineMediaProvider: Send + Sync {
    fn video_frame(&self, request: TimelineVideoRequest) -> Option<TimelineVideoPixels>;
}

fn map_transform_kfs(kfs: &[Keyframe<Transform2D>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: Some(TimelineTransformInfo::from(k.value)),
            opacity: None,
            filter: None,
            deform: None,
            scalar: None,
        })
        .collect()
}

fn map_opacity_kfs(kfs: &[Keyframe<f32>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: None,
            opacity: Some(k.value),
            filter: None,
            deform: None,
            scalar: Some(k.value),
        })
        .collect()
}

fn map_filter_kfs(kfs: &[Keyframe<LiveFilterState>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: None,
            opacity: None,
            filter: Some(TimelineLiveFilterInfo::from(k.value)),
            deform: None,
            scalar: None,
        })
        .collect()
}

fn map_deform_kfs(kfs: &[Keyframe<Deform2D>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: None,
            opacity: None,
            filter: None,
            deform: Some(TimelineDeformInfo::from(k.value)),
            scalar: None,
        })
        .collect()
}

fn map_scalar_kfs(kfs: &[Keyframe<f32>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: None,
            opacity: None,
            filter: None,
            deform: None,
            scalar: Some(k.value),
        })
        .collect()
}

fn snapshot_clip(tl: &Timeline, track_id: TrackId, clip: &Clip, playhead: u32) -> TimelineClipInfo {
    let mut audio_asset_id = None;
    let mut waveform_peaks = Vec::new();
    let (kind, primary_layer_id, cels) = match &clip.kind {
        ClipKind::Cel { cels } => {
            let mut cursor = clip.start_frame;
            let mut out = Vec::with_capacity(cels.len());
            for (idx, cel) in cels.iter().enumerate() {
                let dur = cel.duration_frames();
                out.push(TimelineCelInfo {
                    index: u32::try_from(idx).unwrap_or(0),
                    layer_id: cel.layer.0,
                    start_frame: cursor,
                    duration_frames: dur,
                    hold: cel.hold,
                });
                cursor = cursor.saturating_add(dur);
            }
            let primary = clip
                .cel_at(playhead)
                .map(|(_, c, _)| c.layer.0)
                .or_else(|| cels.first().map(|c| c.layer.0))
                .unwrap_or(0);
            (TimelineClipKind::Cel, primary, out)
        }
        ClipKind::Layer { layer } => (TimelineClipKind::Layer, layer.0, Vec::new()),
        ClipKind::Group { members } => (TimelineClipKind::Group, members.first().map_or(0, |l| l.0), Vec::new()),
        ClipKind::Audio { asset_id, trim_start_samples, sample_rate, .. } => {
            audio_asset_id = Some(asset_id.0);
            if let Some(asset) = tl.audio_asset(*asset_id).filter(|asset| asset.duration_samples > 0) {
                let total = asset.peaks.len() as u64;
                let shown = (u64::from(clip.effective_duration()) * u64::from(*sample_rate))
                    .div_ceil(u64::from(tl.fps.clamp(1, 60)));
                let first = trim_start_samples.saturating_mul(total) / asset.duration_samples;
                let last =
                    trim_start_samples.saturating_add(shown).saturating_mul(total).div_ceil(asset.duration_samples);
                let count = (last.saturating_sub(first)).clamp(1, 128);
                for i in 0..count {
                    let low = first + (last - first) * i / count;
                    let high = first + ((last - first) * (i + 1)).div_ceil(count);
                    let buckets =
                        asset.peaks.get(low.min(total) as usize..high.min(total) as usize).unwrap_or_default();
                    waveform_peaks.push(buckets.iter().map(|p| p[0]).fold(0.0_f32, f32::min));
                    waveform_peaks.push(buckets.iter().map(|p| p[1]).fold(0.0_f32, f32::max));
                }
            }
            (TimelineClipKind::Audio, 0, Vec::new())
        }
        ClipKind::Video { layer, .. } => (TimelineClipKind::Video, layer.0, Vec::new()),
        ClipKind::Text { layer } => (TimelineClipKind::Text, layer.0, Vec::new()),
    };
    TimelineClipInfo {
        id: clip.id.0,
        track_id: track_id.0,
        name: clip.name.clone(),
        start_frame: clip.start_frame,
        duration_frames: clip.effective_duration(),
        blend: clip.blend.id(),
        opacity: clip.opacity,
        kind,
        primary_layer_id,
        cels,
        transform_keyframes: map_transform_kfs(&clip.keyframes.transform),
        opacity_keyframes: map_opacity_kfs(&clip.keyframes.opacity),
        filter_keyframes: map_filter_kfs(&clip.keyframes.filter),
        deform_keyframes: map_deform_kfs(&clip.keyframes.deform),
        volume_keyframes: map_scalar_kfs(&clip.keyframes.volume),
        pan_keyframes: map_scalar_kfs(&clip.keyframes.pan),
        perform_onion_guide: clip.perform_onion_guide,
        perform_guide_isolated: clip.perform_guide_layer.is_some(),
        perform_guide_opacity: clip.perform_guide_opacity,
        perform_guide_color: clip.perform_guide_color.to_vec(),
        audio_asset_id,
        waveform_peaks,
    }
}

fn snapshot_track(tl: &Timeline, track: &Track, playhead: u32) -> TimelineTrackInfo {
    TimelineTrackInfo {
        id: track.id.0,
        name: track.name.clone(),
        visible: track.visible,
        locked: track.locked,
        solo: track.solo,
        opacity: track.opacity,
        blend: track.blend.id(),
        color_tag: track.color_tag,
        parent_group: track.parent_group.map(|p| p.0),
        is_group: track.is_group,
        expanded: track.expanded,
        clip_to_below: track.clip_to_below,
        alpha_lock: track.alpha_lock,
        muted: track.muted,
        effects: TimelineTrackEffectsInfo::from(track.effects),
        clips: track.clips.iter().map(|c| snapshot_clip(tl, track.id, c, playhead)).collect(),
        transform_keyframes: map_transform_kfs(&track.keyframes.transform),
        opacity_keyframes: map_opacity_kfs(&track.keyframes.opacity),
        filter_keyframes: map_filter_kfs(&track.keyframes.filter),
    }
}

pub(crate) fn snapshot_timeline(tl: &Timeline) -> TimelineState {
    let playhead = tl.playhead;
    let pf = playhead as f32;
    let (active_transform, active_opacity, active_filter, active_deform) = if let Some(cid) = tl.active_clip
        && let Some(clip) = tl.clip(cid)
    {
        (
            clip.keyframes.evaluate_transform(pf),
            clip.keyframes.evaluate_opacity(pf),
            clip.keyframes.evaluate_filter(pf),
            clip.keyframes.evaluate_deform(pf),
        )
    } else if let Some(tid) = tl.active_track
        && let Some(track) = tl.track(tid)
    {
        (
            track.keyframes.evaluate_transform(pf),
            track.keyframes.evaluate_opacity(pf),
            track.keyframes.evaluate_filter(pf),
            Deform2D::default(),
        )
    } else {
        (Transform2D::default(), 1.0, LiveFilterState::default(), Deform2D::default())
    };
    let has_audio = tl.tracks.iter().flat_map(|t| &t.clips).any(|c| matches!(c.kind, ClipKind::Audio { .. }));

    TimelineState {
        fps: tl.fps,
        duration_frames: tl.duration_frames,
        playhead,
        playback: match tl.playback {
            Playback::Loop => AnimationPlayback::Loop,
            Playback::PingPong => AnimationPlayback::PingPong,
            Playback::OneShot => AnimationPlayback::OneShot,
        },
        work_area: tl.work_area.map(|w| TimelineWorkAreaInfo { start: w.start, end: w.end }),
        onion: TimelineOnionInfo::from(&tl.onion),
        tracks: tl.tracks.iter().map(|t| snapshot_track(tl, t, playhead)).collect(),
        active_track_id: tl.active_track.map(|t| t.0),
        active_clip_id: tl.active_clip.map(|c| c.0),
        flipbook_open: tl.flipbook_open,
        show_backstage: tl.show_backstage,
        motion_filtering: tl.motion_filtering,
        auto_create_frame_on_draw: tl.auto_create_frame_on_draw,
        auto_split_cel_on_draw: tl.auto_split_cel_on_draw,
        active_evaluated_transform: TimelineTransformInfo::from(active_transform),
        active_evaluated_opacity: active_opacity,
        active_evaluated_filter: TimelineLiveFilterInfo::from(active_filter),
        active_evaluated_deform: TimelineDeformInfo::from(active_deform),
        camera_enabled: tl.camera_enabled,
        camera_transform: TimelineTransformInfo::from(tl.camera.evaluate_transform(pf)),
        camera_filter: TimelineLiveFilterInfo::from(tl.camera.evaluate_filter(pf)),
        camera_transform_keyframes: map_transform_kfs(&tl.camera.transform),
        camera_filter_keyframes: map_filter_kfs(&tl.camera.filter),
        has_audio,
        dialogue_markers: tl
            .dialogue_markers
            .iter()
            .map(|(&frame, label)| TimelineDialogueMarker { frame, label: label.clone() })
            .collect(),
        flipbook_groups: tl.flipbook_groups.iter().map(|id| id.0).collect(),
    }
}

/// Synchronizes `doc.current` to the active clip's primary paint layer at `playhead` so drawing
/// strokes go to the right cel/layer at the playhead.
pub(crate) fn sync_current_layer_to_playhead(doc: &mut Document) {
    let target_layer = doc.timeline.as_ref().and_then(|tl| {
        let frame = tl.playhead;
        if let Some(cid) = tl.active_clip
            && let Some(clip) = tl.clip(cid)
            && let Some(layer) = clip.layer_at(frame)
        {
            return Some(layer);
        }
        if let Some(tid) = tl.active_track
            && let Some(track) = tl.track(tid)
        {
            for clip in &track.clips {
                if let Some(layer) = clip.layer_at(frame) {
                    return Some(layer);
                }
            }
        }
        None
    });
    if let Some(layer) = target_layer
        && let Some(meta) = doc.layer(layer)
    {
        let resolved = if meta.kind == LayerKind::Group {
            let members = doc.members(Some(layer));
            if members.contains(&doc.current()) {
                doc.current()
            } else {
                members
                    .iter()
                    .rev()
                    .copied()
                    .find(|&id| doc.layer(id).is_some_and(|l| l.kind == LayerKind::Paint))
                    .unwrap_or(layer)
            }
        } else {
            layer
        };
        doc.set_current(resolved);
    }
}

fn editable_clip(tl: &mut Timeline, id: u64) -> Option<&mut Clip> {
    let clip = ClipId(id);
    if tl.track(tl.track_of_clip(clip)?)?.locked {
        return None;
    }
    tl.clip_mut(clip)
}

fn remove_timeline_keyframe(tl: &mut Timeline, address: &TimelineKeyframeAddress) -> bool {
    let Some(clip) = editable_clip(tl, address.clip_id) else { return false };
    let keys = &mut clip.keyframes;
    let frame = address.frame;
    match address.channel {
        TimelineKeyframeChannel::Transform => keys.remove_transform(frame),
        TimelineKeyframeChannel::Opacity => keys.remove_opacity(frame),
        TimelineKeyframeChannel::Filter => keys.remove_filter(frame),
        TimelineKeyframeChannel::Deform => keys.remove_deform(frame),
        TimelineKeyframeChannel::Volume => keys.remove_volume(frame),
        TimelineKeyframeChannel::Pan => keys.remove_pan(frame),
    }
}

fn write_timeline_keyframe(
    clip: &mut Clip,
    channel: TimelineKeyframeChannel,
    frame: u32,
    value: TimelineKeyframeInfo,
) -> bool {
    let ease = Some(Easing::from(&value.easing));
    let keys = &mut clip.keyframes;
    match channel {
        TimelineKeyframeChannel::Transform => {
            value.transform.map(|v| keys.set_transform(frame, Transform2D::from(&v), ease))
        }
        TimelineKeyframeChannel::Opacity => value.opacity.map(|v| keys.set_opacity(frame, v, ease)),
        TimelineKeyframeChannel::Filter => {
            value.filter.map(|v| keys.set_filter(frame, LiveFilterState::from(&v), ease))
        }
        TimelineKeyframeChannel::Deform => value.deform.map(|v| keys.set_deform(frame, Deform2D::from(&v), ease)),
        TimelineKeyframeChannel::Volume => value.scalar.map(|v| keys.set_volume(frame, v, ease)),
        TimelineKeyframeChannel::Pan => value.scalar.map(|v| keys.set_pan(frame, v, ease)),
    }
    .is_some()
}

/// Duplicates the whole layer unit, including its masks and any sublayers.
fn copy_timeline_layer(doc: &mut Document, layer: LayerId, pairs: &mut Vec<(LayerId, LayerId)>) -> Option<LayerId> {
    let count =
        doc.layer_ids().filter(|&id| id == layer || doc.is_inside(id, layer) || doc.mask_of(layer) == Some(id)).count();
    if !doc.has_room(count) {
        return None;
    }
    let (copy, copied) = doc.duplicate(layer)?;
    pairs.extend(copied);
    Some(copy)
}

impl CanvasSession {
    fn paste_timeline_clips(&self, ids: &[u64], at: Option<u32>) -> Option<Vec<u64>> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let mut changed = st.doc.clone();
        let tl = changed.ensure_timeline();
        let ids: std::collections::BTreeSet<_> = ids.iter().copied().map(ClipId).collect();
        let sources: Vec<_> =
            ids.into_iter().map(|cid| Some((tl.track_of_clip(cid)?, tl.clip(cid)?.clone()))).collect::<Option<_>>()?;
        let origin = sources.iter().map(|(_, clip)| clip.start_frame).min()?;
        let mut pairs = Vec::new();
        let mut result = Vec::new();
        for (track, mut clip) in sources {
            let start = match at {
                Some(at) => at.checked_add(clip.start_frame - origin)?,
                None => clip.end_frame(),
            };
            let mut copied = std::collections::HashMap::new();
            let mut map = |layer: &mut LayerId| -> Option<()> {
                if let Some(&id) = copied.get(layer) {
                    *layer = id;
                } else {
                    let id = copy_timeline_layer(&mut changed, *layer, &mut pairs)?;
                    copied.insert(*layer, id);
                    *layer = id;
                }
                Some(())
            };
            match &mut clip.kind {
                ClipKind::Cel { cels } => {
                    for cel in cels {
                        map(&mut cel.layer)?;
                    }
                }
                ClipKind::Layer { layer } | ClipKind::Text { layer } | ClipKind::Video { layer, .. } => map(layer)?,
                ClipKind::Group { members } => {
                    for layer in members {
                        map(layer)?;
                    }
                }
                ClipKind::Audio { .. } => {}
            }
            if let Some(layer) = &mut clip.perform_guide_layer {
                map(layer)?;
            }
            result.push(changed.timeline.as_mut()?.place_clip_copy(track, clip, start)?.0);
        }
        sync_current_layer_to_playhead(&mut changed);
        let before = std::mem::replace(&mut st.doc, changed);
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("paste clips");
            canvas.duplicate_layers(pairs, before);
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(result)
    }

    fn edit_flipbook(&self, id: ClipId, indices: &[u32], mode: FlipbookBatch, exposure: u32) -> Option<()> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let mut changed = st.doc.clone();
        let source = editable_clip(changed.timeline.as_mut()?, id.0)?;
        let ClipKind::Cel { cels } = &source.kind else { return None };
        let indices: std::collections::BTreeSet<_> = indices.iter().map(|&i| i as usize).collect();
        if indices.is_empty() {
            return None;
        }
        let picked: Vec<_> = indices.iter().map(|&i| cels.get(i).copied()).collect::<Option<_>>()?;
        let mut cels = cels.clone();
        let mut pairs = Vec::new();
        match mode {
            FlipbookBatch::Exposure => {
                for &index in &indices {
                    cels.get_mut(index)?.hold = exposure.clamp(1, 601) - 1;
                }
            }
            FlipbookBatch::Reverse => {
                for (&index, cel) in indices.iter().zip(picked.into_iter().rev()) {
                    *cels.get_mut(index)? = cel;
                }
            }
            FlipbookBatch::Duplicate => {
                let after = indices.last().copied()? + 1;
                let copies: Vec<_> = picked
                    .into_iter()
                    .map(|cel| {
                        Some(Cel { layer: copy_timeline_layer(&mut changed, cel.layer, &mut pairs)?, hold: cel.hold })
                    })
                    .collect::<Option<_>>()?;
                cels.splice(after..after, copies);
            }
        }
        let tl = changed.timeline.as_mut()?;
        let clip = tl.clip_mut(id)?;
        clip.kind = ClipKind::Cel { cels };
        clip.sync_duration();
        let end = clip.end_frame();
        if end > procraft_engine::timeline::MAX_TIMELINE_DURATION {
            return None;
        }
        tl.duration_frames = tl.duration_frames.max(end);
        sync_current_layer_to_playhead(&mut changed);
        let before = std::mem::replace(&mut st.doc, changed);
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("edit Flipbook frames");
            canvas.duplicate_layers(pairs, before);
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(())
    }
}

#[uniffi::export]
impl CanvasSession {
    /// Returns the complete multi-track [`TimelineState`] snapshot if the timeline is active, or
    /// migrates/builds one on demand when Animation Assist is active.
    pub fn timeline_state(&self) -> Option<TimelineState> {
        let st = self.shared.lock();
        if let Some(tl) = st.doc.timeline.as_ref() {
            return Some(snapshot_timeline(tl));
        }
        st.doc
            .animation
            .as_ref()
            .filter(|a| !a.pages)
            .map(|a| snapshot_timeline(&Timeline::from_legacy_animation(&st.doc, a)))
    }

    /// Enables or disables the multi-track DreamsCraft timeline as an undo step.
    pub fn set_timeline_enabled(&self, on: bool) -> bool {
        let changed = self
            .change(if on { "enable Timeline" } else { "hide Timeline" }, |d| {
                let was_on = d.timeline.is_some();
                if was_on == on {
                    return None;
                }
                if on {
                    d.ensure_timeline();
                } else {
                    d.timeline = None;
                }
                Some(())
            })
            .is_some();
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if let Some(c) = st.canvas.as_mut() {
            c.set_play_frame(None);
            c.invalidate();
        }
        self.shared.frame(st);
        changed
    }

    /// Moves the timeline playhead to `frame` in real time (scrubbing), syncing the active drawing
    /// layer and invalidating the Stage composite without creating an undo step.
    pub fn timeline_set_playhead(&self, frame: u32) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if st.doc.timeline.is_none() {
            st.doc.ensure_timeline();
        }
        let Some(tl) = st.doc.timeline.as_mut() else { return };
        let clamped = frame.min(tl.duration_frames.saturating_sub(1));
        if tl.playhead != clamped {
            tl.playhead = clamped;
            // Select the clip under the playhead on the active track if the active clip doesn't cover it
            if let Some(tid) = tl.active_track
                && let Some(track) = tl.track(tid)
            {
                let covers = tl
                    .active_clip
                    .and_then(|cid| track.clips.iter().find(|c| c.id == cid))
                    .is_some_and(|c| c.contains_frame(clamped));
                if !covers && let Some(under) = track.clips.iter().find(|c| c.contains_frame(clamped)) {
                    let under_id = under.id;
                    tl.active_clip = Some(under_id);
                }
            }
            sync_current_layer_to_playhead(&mut st.doc);
            if let Some(c) = st.canvas.as_mut() {
                c.invalidate();
            }
            self.shared.frame(st);
        }
    }

    /// Steps the playhead forward or backward by `delta` frames (wrapping within Work Area or
    /// timeline duration), returning the new playhead frame.
    pub fn timeline_step_frame(&self, delta: i32) -> u32 {
        let next = {
            let mut guard = self.shared.lock();
            let tl = guard.doc.ensure_timeline();
            let (start, end) = tl.work_area.map_or((0, tl.duration_frames.saturating_sub(1)), |w| (w.start, w.end));
            let len = i64::from(end.saturating_sub(start).saturating_add(1)).max(1);
            let rel = (i64::from(tl.playhead) - i64::from(start) + i64::from(delta)).rem_euclid(len);
            start.saturating_add(u32::try_from(rel).unwrap_or(0))
        };
        self.timeline_set_playhead(next);
        next
    }

    /// Updates the timeline frame rate (`1..=60` fps) as an undo step.
    pub fn timeline_set_fps(&self, fps: u32) {
        let clamped = fps.clamp(1, 60);
        let _ = self.change("frame rate", |d| {
            let tl = d.ensure_timeline();
            if tl.fps == clamped {
                return None;
            }
            tl.fps = clamped;
            if let Some(a) = d.animation.as_mut() {
                a.fps = clamped;
            }
            Some(())
        });
    }

    /// Updates the total timeline duration in frames.
    pub fn timeline_set_duration(&self, duration_frames: u32) {
        let _ = self.change("timeline duration", |d| {
            let tl = d.ensure_timeline();
            let target = duration_frames.clamp(1, procraft_engine::timeline::MAX_TIMELINE_DURATION);
            if tl.duration_frames == target {
                return None;
            }
            tl.duration_frames = target;
            tl.playhead = tl.playhead.min(target.saturating_sub(1));
            tl.work_area = tl.work_area.map(|w| w.repaired(target));
            Some(())
        });
    }

    /// Updates the timeline playback mode (`Loop`, `PingPong`, `OneShot`) as an undo step.
    pub fn timeline_set_playback(&self, playback: AnimationPlayback) {
        let mode = match playback {
            AnimationPlayback::Loop => Playback::Loop,
            AnimationPlayback::PingPong => Playback::PingPong,
            AnimationPlayback::OneShot => Playback::OneShot,
        };
        let _ = self.change("playback mode", |d| {
            let tl = d.ensure_timeline();
            if tl.playback == mode {
                return None;
            }
            tl.playback = mode;
            if let Some(a) = d.animation.as_mut() {
                a.playback = mode;
            }
            Some(())
        });
    }

    /// Sets or clears the timeline Work Area (In/Out loop markers) as an undo step.
    pub fn timeline_set_work_area(&self, work_area: Option<TimelineWorkAreaInfo>) {
        let _ = self.change("work area", |d| {
            let tl = d.ensure_timeline();
            let dur = tl.duration_frames;
            let next = work_area.map(|w| WorkArea { start: w.start, end: w.end }.repaired(dur));
            if tl.work_area == next {
                return None;
            }
            tl.work_area = next;
            Some(())
        });
    }

    /// Updates the timeline's Onion Skin settings as an undo step.
    pub fn timeline_set_onion_skin(&self, onion: TimelineOnionInfo) {
        let color3 = |v: &[f32], fallback: [f32; 3]| match v {
            [r, g, b, ..] => [*r, *g, *b],
            _ => fallback,
        };
        let _ = self.change("onion skin", |d| {
            let tl = d.ensure_timeline();
            let prev_before = tl.onion.before_color;
            let prev_after = tl.onion.after_color;
            let next = OnionSkinSettings {
                enabled: onion.enabled,
                before_frames: onion.before_frames,
                after_frames: onion.after_frames,
                opacity: onion.opacity,
                before_color: color3(&onion.before_color, prev_before),
                after_color: color3(&onion.after_color, prev_after),
                blend_primary: onion.blend_primary,
                active_track_only: onion.active_track_only,
            }
            .repaired();
            if tl.onion == next {
                return None;
            }
            tl.onion = next;
            if let Some(a) = d.animation.as_mut() {
                a.onion_frames = if onion.enabled { onion.before_frames.max(onion.after_frames) } else { 0 };
                a.onion_opacity = onion.opacity;
                a.before = color3(&onion.before_color, a.before);
                a.after = color3(&onion.after_color, a.after);
                a.blend_primary = onion.blend_primary;
            }
            Some(())
        });
    }

    /// Toggles Flipbook drawing mode (collapsed full-stage frame strip vs full multi-track timeline).
    pub fn timeline_set_flipbook(&self, open: bool) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.doc.ensure_timeline().flipbook_open = open;
        self.shared.frame(st);
    }

    /// Toggles Backstage visibility around the Stage.
    pub fn timeline_set_backstage(&self, show: bool) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.doc.ensure_timeline().show_backstage = show;
        self.shared.frame(st);
    }

    /// Updates the default Motion Filtering factor (`0.0..=1.0`) for Perform mode.
    pub fn timeline_set_motion_filtering(&self, filtering: f32) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.doc.ensure_timeline().motion_filtering =
            if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { 0.35 };
        self.shared.frame(st);
    }

    /// Adds a new track (and optionally an initial drawing `CelClip` at the playhead) as one undo
    /// step, returning the new track's id (or 0 if no room for a layer).
    pub fn timeline_add_track(&self, name: Option<String>, with_drawing_clip: bool) -> u64 {
        self.change("new track", |d| {
            let new_layer = if with_drawing_clip {
                if !d.has_room(1) {
                    return None;
                }
                Some(d.add_layer())
            } else {
                None
            };
            let tl = d.ensure_timeline();
            let playhead = tl.playhead;
            let track_id = tl.add_track(name.as_deref(), false);
            if let Some(layer) = new_layer {
                tl.add_cel_clip(track_id, playhead, layer, 0);
            }
            sync_current_layer_to_playhead(d);
            Some(track_id.0)
        })
        .unwrap_or(0)
    }

    /// Groups `track_ids` under a new Group Track as one undo step, returning the group track id.
    pub fn timeline_add_group_track(&self, track_ids: Vec<u64>, name: Option<String>) -> u64 {
        self.change("group tracks", |d| {
            let ids: Vec<TrackId> = track_ids.into_iter().map(TrackId).collect();
            let tl = d.ensure_timeline();
            tl.group_tracks(&ids, name.as_deref()).map(|g| g.0)
        })
        .unwrap_or(0)
    }

    /// Selects `track_id` as the active track.
    pub fn timeline_select_track(&self, track_id: u64) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let tid = TrackId(track_id);
        let tl = st.doc.ensure_timeline();
        if tl.track(tid).is_none() {
            return false;
        }
        tl.active_track = Some(tid);
        let playhead = tl.playhead;
        let clip_at = tl
            .track(tid)
            .and_then(|t| t.clips.iter().find(|c| c.contains_frame(playhead)).or_else(|| t.clips.first()))
            .map(|c| c.id);
        tl.active_clip = clip_at;
        sync_current_layer_to_playhead(&mut st.doc);
        if let Some(c) = st.canvas.as_mut() {
            c.invalidate();
        }
        self.shared.frame(st);
        true
    }

    /// Renames `track_id` as an undo step.
    pub fn timeline_rename_track(&self, track_id: u64, name: String) -> bool {
        let trimmed = name.trim().to_owned();
        if trimmed.is_empty() {
            return false;
        }
        self.change("rename track", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            if track.name == trimmed {
                return None;
            }
            track.name = trimmed.chars().take(128).collect();
            Some(())
        })
        .is_some()
    }

    /// Toggles `track_id`'s visibility as an undo step.
    pub fn timeline_set_track_visible(&self, track_id: u64, visible: bool) -> bool {
        self.change(if visible { "show track" } else { "hide track" }, |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            if track.visible == visible {
                return None;
            }
            track.visible = visible;
            Some(())
        })
        .is_some()
    }

    /// Toggles `track_id`'s lock state as an undo step.
    pub fn timeline_set_track_locked(&self, track_id: u64, locked: bool) -> bool {
        self.change(if locked { "lock track" } else { "unlock track" }, |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            if track.locked == locked {
                return None;
            }
            track.locked = locked;
            Some(())
        })
        .is_some()
    }

    /// Toggles `track_id`'s solo visibility as an undo step.
    pub fn timeline_set_track_solo(&self, track_id: u64, solo: bool) -> bool {
        self.change("solo track", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            if track.solo == solo {
                return None;
            }
            track.solo = solo;
            Some(())
        })
        .is_some()
    }

    /// Expands or collapses `track_id`'s sub-tracks in the timeline UI.
    pub fn timeline_set_track_expanded(&self, track_id: u64, expanded: bool) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let Some(track) = st.doc.ensure_timeline().track_mut(TrackId(track_id)) else {
            return false;
        };
        track.expanded = expanded;
        self.shared.frame(st);
        true
    }

    /// Changes `track_id`'s base opacity (`0.0..=1.0`), coalescing drags until `done` into one undo step.
    pub fn timeline_set_track_opacity(&self, track_id: u64, opacity: f32, done: bool) -> bool {
        let clamped = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
        self.change_live("track opacity", done, |d| {
            let Some(track) = d.ensure_timeline().track_mut(TrackId(track_id)) else {
                return false;
            };
            if (track.opacity - clamped).abs() < 1e-6 {
                return false;
            }
            track.opacity = clamped;
            true
        })
    }

    /// Changes `track_id`'s blend mode as an undo step.
    pub fn timeline_set_track_blend(&self, track_id: u64, blend: u32) -> bool {
        let Some(mode) = BlendMode::from_id(blend) else { return false };
        self.change("track blend mode", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            if track.blend == mode {
                return None;
            }
            track.blend = mode;
            Some(())
        })
        .is_some()
    }

    /// Reorders `track_id` to `to_index` (0 = bottom-most track) as an undo step.
    pub fn timeline_reorder_track(&self, track_id: u64, to_index: u32) -> bool {
        self.change("reorder track", |d| {
            let tl = d.ensure_timeline();
            let from = tl.tracks.iter().position(|t| t.id == TrackId(track_id))?;
            let to = (to_index as usize).min(tl.tracks.len().saturating_sub(1));
            if from == to {
                return None;
            }
            let track = tl.tracks.remove(from);
            tl.tracks.insert(to, track);
            Some(())
        })
        .is_some()
    }

    /// Deletes `track_id` (and safely removes orphaned layers). If it was the last track, keeps one
    /// clean empty track so the canvas stays editable.
    pub fn timeline_delete_track(&self, track_id: u64) -> bool {
        self.change("delete track", |d| {
            let tl = d.ensure_timeline();
            let removed_layers = tl.delete_track(TrackId(track_id))?;
            if tl.tracks.is_empty() {
                tl.add_track(Some("Track 1"), false);
            }
            let still_managed = tl.managed_layers();
            for lid in removed_layers {
                if !still_managed.contains(&lid) && d.layers().len() > 1 {
                    let _ = d.delete_layer(lid);
                }
            }
            sync_current_layer_to_playhead(d);
            Some(())
        })
        .is_some()
    }

    /// Selects `clip_id` as the active clip and focuses its track.
    pub fn timeline_select_clip(&self, clip_id: u64) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let cid = ClipId(clip_id);
        let tl = st.doc.ensure_timeline();
        let Some(tid) = tl.track_of_clip(cid) else { return false };
        tl.active_track = Some(tid);
        tl.active_clip = Some(cid);
        sync_current_layer_to_playhead(&mut st.doc);
        if let Some(c) = st.canvas.as_mut() {
            c.invalidate();
        }
        self.shared.frame(st);
        true
    }

    /// Adds a new `CelClip` (Flipbook drawing clip) on `track_id` at `start_frame` as an undo step.
    pub fn timeline_add_cel_clip(&self, track_id: u64, start_frame: u32) -> u64 {
        self.change("new drawing clip", |d| {
            if !d.has_room(1) {
                return None;
            }
            let layer = d.add_layer();
            let tl = d.ensure_timeline();
            let cid = tl.add_cel_clip(TrackId(track_id), start_frame, layer, 0)?;
            sync_current_layer_to_playhead(d);
            Some(cid.0)
        })
        .unwrap_or(0)
    }

    /// Adds a new static `LayerClip` on `track_id` at `start_frame` for `duration_frames` as an undo step.
    pub fn timeline_add_layer_clip(&self, track_id: u64, start_frame: u32, duration_frames: u32) -> u64 {
        self.change("new content clip", |d| {
            if !d.has_room(1) {
                return None;
            }
            let layer = d.add_layer();
            let tl = d.ensure_timeline();
            let cid = tl.add_layer_clip(TrackId(track_id), start_frame, duration_frames, layer, None)?;
            sync_current_layer_to_playhead(d);
            Some(cid.0)
        })
        .unwrap_or(0)
    }

    /// Moves `clip_id` to `new_start_frame` (and optionally `target_track_id`), coalescing drags
    /// until `done` into one undo step.
    pub fn timeline_move_clip(
        &self,
        clip_id: u64,
        new_start_frame: u32,
        target_track_id: Option<u64>,
        done: bool,
    ) -> bool {
        self.change_live("move clip", done, |d| {
            d.ensure_timeline().move_clip(ClipId(clip_id), new_start_frame, target_track_id.map(TrackId))
        })
    }

    /// Resizes `clip_id` to `new_duration_frames`, coalescing drags until `done` into one undo step.
    pub fn timeline_resize_clip(&self, clip_id: u64, new_duration_frames: u32, done: bool) -> bool {
        self.change_live("resize clip", done, |d| d.ensure_timeline().resize_clip(ClipId(clip_id), new_duration_frames))
    }

    /// Splits `clip_id` at the current timeline playhead as an undo step, returning the new clip id.
    pub fn timeline_split_clip_at_playhead(&self, clip_id: u64) -> Option<u64> {
        self.change("split clip", |d| {
            let cid = ClipId(clip_id);
            let is_video = d
                .timeline
                .as_ref()
                .and_then(|tl| tl.clip(cid))
                .is_some_and(|clip| matches!(clip.kind, ClipKind::Video { .. }));
            if is_video && !d.has_room(1) {
                return None;
            }
            let tl = d.ensure_timeline();
            let right = tl.split_clip(cid, tl.playhead)?;
            if is_video {
                let new_layer = d.add_layer();
                if let Some(ClipKind::Video { layer, .. }) =
                    d.ensure_timeline().clip_mut(right).map(|clip| &mut clip.kind)
                {
                    *layer = new_layer;
                }
            }
            sync_current_layer_to_playhead(d);
            Some(right.0)
        })
    }

    /// Duplicates one clip immediately after its current end.
    pub fn timeline_duplicate_clip(&self, clip_id: u64) -> Option<u64> {
        self.paste_timeline_clips(&[clip_id], None)?.into_iter().next()
    }

    /// Copies the clips and their drawing layers in one step, preserving their relative positions.
    pub fn timeline_paste_clips(&self, ids: Vec<u64>, frame: u32) -> Vec<u64> {
        self.paste_timeline_clips(&ids, Some(frame)).unwrap_or_default()
    }

    /// Moves or trims the selected clips as one change.
    pub fn timeline_edit_clips(&self, ids: Vec<u64>, edit: TimelineClipBatch, frame: u32) -> bool {
        self.change("edit selected clips", |doc| {
            let mut changed = doc.clone();
            let tl = changed.ensure_timeline();
            let ids: std::collections::BTreeSet<_> = ids.into_iter().map(ClipId).collect();
            let starts: Vec<_> = ids
                .iter()
                .map(|id| {
                    let track = tl.track(tl.track_of_clip(*id)?)?;
                    (!track.locked).then(|| tl.clip(*id).map(|clip| (*id, clip.start_frame))).flatten()
                })
                .collect::<Option<_>>()?;
            let origin = starts.iter().map(|(_, start)| *start).min()?;
            let mut removed = Vec::new();
            for (id, start) in starts {
                match edit {
                    TimelineClipBatch::Move => {
                        let target = frame.checked_add(start - origin)?;
                        let end = target.checked_add(tl.clip(id)?.effective_duration())?;
                        if end > procraft_engine::timeline::MAX_TIMELINE_DURATION || !tl.move_clip(id, target, None) {
                            return None;
                        }
                    }
                    TimelineClipBatch::TrimStart | TimelineClipBatch::TrimEnd => {
                        removed.extend(tl.trim_clip(id, frame, matches!(edit, TimelineClipBatch::TrimStart))?);
                    }
                }
            }
            let managed = tl.managed_layers();
            for layer in removed {
                if !managed.contains(&layer) {
                    changed.delete_layer(layer);
                }
            }
            sync_current_layer_to_playhead(&mut changed);
            *doc = changed;
            Some(())
        })
        .is_some()
    }

    /// Deletes `clip_id` as an undo step.
    pub fn timeline_delete_clip(&self, clip_id: u64) -> bool {
        self.change("delete clip", |d| {
            let cid = ClipId(clip_id);
            let tl = d.ensure_timeline();
            let tid = tl.track_of_clip(cid)?;
            if tl.track(tid).is_some_and(|t| t.locked) {
                return None;
            }
            let total_clips: usize = tl.tracks.iter().map(|t| t.clips.len()).sum();
            if total_clips <= 1 {
                return None;
            }
            let track = tl.track_mut(tid)?;
            let pos = track.clips.iter().position(|c| c.id == cid)?;
            track.clips.remove(pos);
            tl.active_clip = tl
                .track(tid)
                .and_then(|t| t.clips.first())
                .map(|c| c.id)
                .or_else(|| tl.tracks.iter().flat_map(|t| &t.clips).next().map(|c| c.id));
            sync_current_layer_to_playhead(d);
            Some(())
        })
        .is_some()
    }

    /// Changes `clip_id`'s blend mode as an undo step.
    pub fn timeline_set_clip_blend(&self, clip_id: u64, blend: u32) -> bool {
        let Some(mode) = BlendMode::from_id(blend) else { return false };
        self.change("clip blend mode", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            if clip.blend == mode {
                return None;
            }
            clip.blend = mode;
            Some(())
        })
        .is_some()
    }

    /// Changes `clip_id`'s base opacity (`0.0..=1.0`), coalescing drags until `done` into one undo step.
    pub fn timeline_set_clip_opacity(&self, clip_id: u64, opacity: f32, done: bool) -> bool {
        let clamped = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
        self.change_live("clip opacity", done, |d| {
            let Some(clip) = d.ensure_timeline().clip_mut(ClipId(clip_id)) else {
                return false;
            };
            if (clip.opacity - clamped).abs() < 1e-6 {
                return false;
            }
            clip.opacity = clamped;
            true
        })
    }

    /// Adds a new empty cel after the current cel in `clip_id` (or the active `CelClip`) as an
    /// undo step, returning the new layer id.
    pub fn timeline_add_cel(&self, clip_id: u64) -> Option<u64> {
        self.change("new cel", |d| {
            if !d.has_room(1) {
                return None;
            }
            let new_layer = d.add_layer();
            let tl = d.ensure_timeline();
            let cid = if clip_id != 0 { ClipId(clip_id) } else { tl.active_clip? };
            let playhead = tl.playhead;
            let after_idx = tl.clip(cid).and_then(|c| c.cel_at(playhead)).map(|(idx, _, _)| idx);
            tl.insert_cel(cid, after_idx, new_layer, 0)?;
            let cel_count = tl.clip(cid).map_or(1, |c| match &c.kind {
                ClipKind::Cel { cels } => cels.len(),
                _ => 1,
            });
            d.rename(new_layer, &format!("Cel {cel_count}"));
            sync_current_layer_to_playhead(d);
            Some(new_layer.0)
        })
    }

    /// Duplicates the cel at `cel_index` inside `clip_id` (including its GPU pixels) as an undo
    /// step, returning the new layer id.
    pub fn timeline_duplicate_cel(&self, clip_id: u64, cel_index: u32) -> Option<u64> {
        let src_layer = {
            let mut guard = self.shared.lock();
            let tl = guard.doc.ensure_timeline();
            let clip = tl.clip(ClipId(clip_id))?;
            let ClipKind::Cel { cels } = &clip.kind else { return None };
            cels.get(cel_index as usize)?.layer.0
        };
        self.duplicated(src_layer, "duplicate cel", |doc, copy_layer| {
            let tl = doc.ensure_timeline();
            let hold = tl
                .clip(ClipId(clip_id))
                .and_then(|c| match &c.kind {
                    ClipKind::Cel { cels } => cels.get(cel_index as usize).map(|cel| cel.hold),
                    _ => None,
                })
                .unwrap_or(0);
            tl.insert_cel(ClipId(clip_id), Some(cel_index as usize), copy_layer, hold);
            sync_current_layer_to_playhead(doc);
        })
    }

    /// Deletes the cel at `cel_index` inside `clip_id` as an undo step.
    pub fn timeline_delete_cel(&self, clip_id: u64, cel_index: u32) -> bool {
        self.change("delete cel", |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            let tid = tl.track_of_clip(cid)?;
            if tl.track(tid).is_some_and(|t| t.locked) {
                return None;
            }
            let clip = tl.clip_mut(cid)?;
            let ClipKind::Cel { cels } = &mut clip.kind else { return None };
            if cels.len() <= 1 || (cel_index as usize) >= cels.len() {
                return None;
            }
            cels.remove(cel_index as usize);
            clip.sync_duration();
            sync_current_layer_to_playhead(d);
            Some(())
        })
        .is_some()
    }

    /// Sets the hold (additional exposure frames) of `cel_index` in `clip_id`, coalescing drags
    /// until `done` into one undo step.
    pub fn timeline_set_cel_hold(&self, clip_id: u64, cel_index: u32, hold: u32, done: bool) -> bool {
        let clamped = hold.min(600);
        self.change_live("cel hold duration", done, |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            let Some(tid) = tl.track_of_clip(cid) else { return false };
            if tl.track(tid).is_some_and(|t| t.locked) {
                return false;
            }
            let Some(clip) = tl.clip_mut(cid) else { return false };
            let ClipKind::Cel { cels } = &mut clip.kind else { return false };
            let Some(cel) = cels.get_mut(cel_index as usize) else { return false };
            if cel.hold == clamped {
                return false;
            }
            let layer = cel.layer;
            cel.hold = clamped;
            clip.sync_duration();
            let end = clip.end_frame();
            tl.duration_frames = tl.duration_frames.max(end);
            if let Some(a) = d.animation.as_mut() {
                a.set_hold(layer, clamped.min(60));
            }
            true
        })
    }

    pub fn timeline_edit_flipbook(&self, clip_id: u64, indices: Vec<u32>, mode: FlipbookBatch, exposure: u32) -> bool {
        self.edit_flipbook(ClipId(clip_id), &indices, mode, exposure).is_some()
    }

    pub fn timeline_add_flipbook_track(&self, clip_id: u64) -> Option<u64> {
        self.change("add Flipbook track", |doc| {
            if !doc.has_room(1) {
                return None;
            }
            let mut changed = doc.clone();
            let layer = changed.add_layer();
            let clip = changed.ensure_timeline().add_flipbook_track(ClipId(clip_id), layer)?;
            sync_current_layer_to_playhead(&mut changed);
            *doc = changed;
            Some(clip.0)
        })
    }

    pub fn timeline_set_dialogue_marker(&self, frame: u32, label: Option<String>) -> bool {
        self.change("dialogue marker", |doc| {
            let tl = doc.timeline.as_mut()?;
            if frame >= tl.duration_frames {
                return None;
            }
            match label {
                Some(label) => {
                    let name: String = label.trim().chars().take(48).collect();
                    tl.dialogue_markers.insert(frame, name);
                }
                None => {
                    tl.dialogue_markers.remove(&frame)?;
                }
            }
            Some(())
        })
        .is_some()
    }

    pub fn timeline_copy_keyframes(&self, addresses: Vec<TimelineKeyframeAddress>) -> Vec<TimelineCopiedKeyframe> {
        let st = self.shared.lock();
        let Some(tl) = st.doc.timeline.as_ref() else { return Vec::new() };
        addresses
            .into_iter()
            .filter_map(|address| {
                let clip = tl.clip(ClipId(address.clip_id))?;
                if !clip.contains_frame(address.frame) {
                    return None;
                }
                let keys = &clip.keyframes;
                let values = match address.channel {
                    TimelineKeyframeChannel::Transform => map_transform_kfs(&keys.transform),
                    TimelineKeyframeChannel::Opacity => map_opacity_kfs(&keys.opacity),
                    TimelineKeyframeChannel::Filter => map_filter_kfs(&keys.filter),
                    TimelineKeyframeChannel::Deform => map_deform_kfs(&keys.deform),
                    TimelineKeyframeChannel::Volume => map_scalar_kfs(&keys.volume),
                    TimelineKeyframeChannel::Pan => map_scalar_kfs(&keys.pan),
                };
                let value = values.into_iter().find(|value| value.frame == address.frame)?;
                Some(TimelineCopiedKeyframe { address, value })
            })
            .collect()
    }

    pub fn timeline_paste_keyframes(
        &self,
        keys: Vec<TimelineCopiedKeyframe>,
        frame: u32,
        target: Option<u64>,
        move_original: bool,
    ) -> bool {
        self.change("edit selected keyframes", |doc| {
            let mut changed = doc.clone();
            let tl = changed.timeline.as_mut()?;
            let origin = keys.iter().map(|key| key.address.frame).min()?;
            if move_original {
                for key in &keys {
                    if !remove_timeline_keyframe(tl, &key.address) {
                        return None;
                    }
                }
            }
            for key in keys {
                let mut address = key.address;
                address.clip_id = target.unwrap_or(address.clip_id);
                address.frame = frame.checked_add(address.frame - origin)?;
                let clip = editable_clip(tl, address.clip_id)?;
                if !clip.contains_frame(address.frame)
                    || !write_timeline_keyframe(clip, address.channel, address.frame, key.value)
                {
                    return None;
                }
            }
            *doc = changed;
            Some(())
        })
        .is_some()
    }

    pub fn timeline_set_keyframe_easing(&self, address: TimelineKeyframeAddress, easing: TimelineEasingInfo) -> bool {
        self.change("keyframe easing", |doc| {
            let keys = &mut editable_clip(doc.timeline.as_mut()?, address.clip_id)?.keyframes;
            let e = Easing::from(&easing);
            let f = address.frame;
            match address.channel {
                TimelineKeyframeChannel::Transform => keys.set_transform_easing(f, e),
                TimelineKeyframeChannel::Opacity => keys.set_opacity_easing(f, e),
                TimelineKeyframeChannel::Filter => keys.set_filter_easing(f, e),
                TimelineKeyframeChannel::Deform => keys.set_deform_easing(f, e),
                TimelineKeyframeChannel::Volume => keys.set_volume_easing(f, e),
                TimelineKeyframeChannel::Pan => keys.set_pan_easing(f, e),
            }
            .then_some(())
        })
        .is_some()
    }

    pub fn timeline_remove_keyframe(&self, address: TimelineKeyframeAddress) -> bool {
        self.change("delete keyframe", |doc| remove_timeline_keyframe(doc.timeline.as_mut()?, &address).then_some(()))
            .is_some()
    }

    /// Sets or updates a non-destructive `Move` (`Transform2D`) keyframe on `clip_id` at `frame`,
    /// coalescing interactive Stage drags until `done` into one undo step.
    pub fn timeline_set_transform_keyframe(
        &self,
        clip_id: u64,
        frame: u32,
        transform: TimelineTransformInfo,
        done: bool,
    ) -> bool {
        let t2d = Transform2D::from(&transform);
        self.change_live("move keyframe", done, |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            let Some(tid) = tl.track_of_clip(cid) else { return false };
            if tl.track(tid).is_some_and(|t| t.locked) {
                return false;
            }
            let Some(clip) = tl.clip_mut(cid) else { return false };
            clip.keyframes.set_transform(frame, t2d, None);
            true
        })
    }

    /// Sets or updates a non-destructive `Move` (`Transform2D`) keyframe on `track_id` (e.g. a
    /// Group Track) at `frame`, coalescing drags until `done` into one undo step.
    pub fn timeline_set_track_transform_keyframe(
        &self,
        track_id: u64,
        frame: u32,
        transform: TimelineTransformInfo,
        done: bool,
    ) -> bool {
        let t2d = Transform2D::from(&transform);
        self.change_live("track move keyframe", done, |d| {
            let tl = d.ensure_timeline();
            let Some(track) = tl.track_mut(TrackId(track_id)) else { return false };
            if track.locked {
                return false;
            }
            track.keyframes.set_transform(frame, t2d, None);
            true
        })
    }

    /// Sets or updates an `Opacity` keyframe on `clip_id` at `frame`, coalescing drags until
    /// `done` into one undo step.
    pub fn timeline_set_opacity_keyframe(&self, clip_id: u64, frame: u32, opacity: f32, done: bool) -> bool {
        self.change_live("opacity keyframe", done, |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            let Some(tid) = tl.track_of_clip(cid) else { return false };
            if tl.track(tid).is_some_and(|t| t.locked) {
                return false;
            }
            let Some(clip) = tl.clip_mut(cid) else { return false };
            clip.keyframes.set_opacity(frame, opacity, None);
            true
        })
    }

    /// Deletes a `Move` (`Transform2D`) keyframe on `clip_id` at `frame` as an undo step.
    pub fn timeline_delete_transform_keyframe(&self, clip_id: u64, frame: u32) -> bool {
        self.change("delete keyframe", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.remove_transform(frame).then_some(())
        })
        .is_some()
    }

    /// Deletes an `Opacity` keyframe on `clip_id` at `frame` as an undo step.
    pub fn timeline_delete_opacity_keyframe(&self, clip_id: u64, frame: u32) -> bool {
        self.change("delete keyframe", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.remove_opacity(frame).then_some(())
        })
        .is_some()
    }

    /// Changes the outgoing easing curve of the `Move` keyframe at `frame` on `clip_id` as an undo step.
    pub fn timeline_set_transform_easing(&self, clip_id: u64, frame: u32, easing: TimelineEasingInfo) -> bool {
        let e = Easing::from(&easing);
        self.change("keyframe easing", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.set_transform_easing(frame, e).then_some(())
        })
        .is_some()
    }

    /// Changes the outgoing easing curve of the `Opacity` keyframe at `frame` on `clip_id` as an undo step.
    pub fn timeline_set_opacity_easing(&self, clip_id: u64, frame: u32, easing: TimelineEasingInfo) -> bool {
        let e = Easing::from(&easing);
        self.change("keyframe easing", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.set_opacity_easing(frame, e).then_some(())
        })
        .is_some()
    }

    /// Deletes a batch of clips (e.g. from Stylus Scratch-to-Delete or Lasso multi-selection) as
    /// one atomic undo step.
    pub fn timeline_delete_clips(&self, clip_ids: Vec<u64>) -> bool {
        if clip_ids.is_empty() {
            return false;
        }
        self.change("delete clips", |d| {
            let mut any = false;
            let mut removed_layers = Vec::new();
            {
                let tl = d.ensure_timeline();
                for raw in &clip_ids {
                    let cid = ClipId(*raw);
                    let Some(tid) = tl.track_of_clip(cid) else { continue };
                    if tl.track(tid).is_some_and(|t| t.locked) {
                        continue;
                    }
                    let total_clips: usize = tl.tracks.iter().map(|t| t.clips.len()).sum();
                    if total_clips <= 1 {
                        break;
                    }
                    if let Some(track) = tl.track_mut(tid)
                        && let Some(pos) = track.clips.iter().position(|c| c.id == cid)
                    {
                        let removed = track.clips.remove(pos);
                        removed_layers.extend(removed.referenced_layers());
                        any = true;
                    }
                }
                if any {
                    tl.active_clip = tl
                        .active_track
                        .and_then(|t| tl.track(t))
                        .and_then(|t| t.clips.first())
                        .map(|c| c.id)
                        .or_else(|| tl.tracks.iter().flat_map(|t| &t.clips).next().map(|c| c.id));
                }
            }
            if !any {
                return None;
            }
            let still_managed = d.ensure_timeline().managed_layers();
            for lid in removed_layers {
                if !still_managed.contains(&lid) && d.layers().len() > 1 {
                    let _ = d.delete_layer(lid);
                }
            }
            sync_current_layer_to_playhead(d);
            Some(())
        })
        .is_some()
    }

    /// Starts a live Perform recording session: flushes any pending edit and captures a single
    /// pre-Perform document snapshot so all live preview updates collapse into 1 atomic undo step.
    pub fn timeline_begin_perform(&self) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let before = st.doc.clone();
        st.pending_doc = Some(before);
        st.pending_name = Some("perform motion".to_owned());
    }

    /// Updates `clip_id`'s live transform during Perform playback without creating extra undo steps.
    pub fn timeline_preview_perform_transform(
        &self,
        clip_id: u64,
        frame: u32,
        transform: TimelineTransformInfo,
    ) -> bool {
        let t2d = Transform2D::from(&transform);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        if st.pending_doc.is_none() {
            st.pending_doc = Some(st.doc.clone());
            st.pending_name = Some("perform motion".to_owned());
        }
        let tl = st.doc.ensure_timeline();
        let cid = ClipId(clip_id);
        if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
            return false;
        }
        let Some(clip) = tl.clip_mut(cid) else { return false };
        clip.keyframes.set_transform(frame, t2d, None);
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        true
    }

    /// Updates `clip_id`'s live Warp/Distort deformation during Perform playback without creating
    /// extra undo steps.
    pub fn timeline_preview_perform_deform(&self, clip_id: u64, frame: u32, deform: TimelineDeformInfo) -> bool {
        let def = Deform2D::from(&deform);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        if st.pending_doc.is_none() {
            st.pending_doc = Some(st.doc.clone());
            st.pending_name = Some("perform warp".to_owned());
        }
        let tl = st.doc.ensure_timeline();
        let cid = ClipId(clip_id);
        if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
            return false;
        }
        let Some(clip) = tl.clip_mut(cid) else { return false };
        clip.keyframes.set_deform(frame, def, None);
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        true
    }

    /// Cancels an active Perform recording, restoring the exact pre-Perform document cleanly.
    pub fn timeline_cancel_perform(&self) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.pending_name = None;
        let Some(before) = st.pending_doc.take() else {
            return false;
        };
        st.doc = before;
        sync_current_layer_to_playhead(&mut st.doc);
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        true
    }

    pub fn timeline_perform_curve(&self, samples: Vec<PerformSampleInfo>, filtering: f32) -> Vec<PerformSampleInfo> {
        let samples: Vec<_> = samples
            .into_iter()
            .map(|s| MotionSample { frame: s.frame, transform: Transform2D::from(&s.transform) })
            .collect();
        simplify_motion_samples(&samples, filtering)
            .into_iter()
            .map(|k| PerformSampleInfo { frame: k.frame, transform: TimelineTransformInfo::from(k.value) })
            .collect()
    }

    /// Commits a live Perform motion recording on `clip_id`: restores pre-Perform keyframes from
    /// the session snapshot, simplifies `samples` with `filtering` (`0.0..=1.0`, RDP), and pushes
    /// **exactly one** atomic undo step (`"perform motion"`).
    pub fn timeline_commit_perform(&self, clip_id: u64, samples: Vec<PerformSampleInfo>, filtering: f32) -> u32 {
        if samples.is_empty() {
            let _ = self.timeline_cancel_perform();
            return 0;
        }
        let engine_samples: Vec<MotionSample> = samples
            .iter()
            .map(|s| MotionSample { frame: s.frame, transform: Transform2D::from(&s.transform) })
            .collect();
        let simplified = simplify_motion_samples(&engine_samples, filtering);
        if simplified.is_empty() {
            let _ = self.timeline_cancel_perform();
            return 0;
        }
        let first_frame = simplified.first().map_or(0, |k| k.frame);
        let last_frame = simplified.last().map_or(first_frame, |k| k.frame);
        let count = u32::try_from(simplified.len()).unwrap_or(0);

        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        // Consume the pre-Perform snapshot directly (avoiding a separate intermediate settle step!)
        let before = st.pending_doc.take().unwrap_or_else(|| st.doc.clone());
        st.pending_name = None;

        // Start from the clean pre-Perform clip keyframes so raw preview frames don't linger
        let cid = ClipId(clip_id);
        let orig_transform_kfs =
            before.timeline.as_ref().and_then(|t| t.clip(cid)).map(|c| c.keyframes.transform.clone());

        let tl = st.doc.ensure_timeline();
        tl.motion_filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { tl.motion_filtering };
        if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
            st.doc = before;
            return 0;
        }
        let Some(clip) = tl.clip_mut(cid) else {
            st.doc = before;
            return 0;
        };
        if let Some(orig) = orig_transform_kfs {
            clip.keyframes.transform = orig;
        }
        // Replace any existing transform keyframes within the recorded range [first_frame..=last_frame]
        clip.keyframes.transform.retain(|k| k.frame < first_frame || k.frame > last_frame);
        for kf in &simplified {
            clip.keyframes.set_transform(kf.frame, kf.value, Some(kf.easing));
        }

        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("perform motion");
            canvas.record_document(before);
        }
        st.revision += 1;
        self.shared.frame(st);
        count
    }

    /// Exports the animation (either the full timeline or the active `WorkArea` when
    /// `work_area_only` is true) as numbered PNG files (`frame_0001.png`, ...) inside
    /// `folder_path`, returning the number of frames written.
    pub fn export_png_sequence(
        &self,
        folder_path: String,
        max_side: u32,
        transparent: bool,
        work_area_only: bool,
    ) -> Result<u32, MobileError> {
        let dir = Path::new(&folder_path);
        fs::create_dir_all(dir).map_err(|e| file_error("saving", e))?;
        let total = self.animation_frame_count();
        if total == 0 {
            return Err(file_error("saving", "there are no frames"));
        }
        let (start, end) = {
            let st = self.shared.lock();
            if work_area_only
                && let Some(tl) = st.doc.timeline.as_ref()
                && let Some(w) = tl.work_area
            {
                (w.start.min(total.saturating_sub(1)), w.end.min(total.saturating_sub(1)))
            } else {
                (0, total.saturating_sub(1))
            }
        };
        let count = end.saturating_sub(start).saturating_add(1);
        self.exported(0.0);
        for (idx, frame_num) in (start..=end).enumerate() {
            let Some(img) = self.animation_frame(frame_num, transparent, max_side) else {
                return Err(file_error("saving", "a frame could not be drawn"));
            };
            let file_path = dir.join(format!("frame_{:04}.png", idx + 1));
            write_atomically(&file_path, |f| {
                let out = fs::File::create(f).map_err(|e| file_error("saving", e))?;
                let mut encoder = png::Encoder::new(BufWriter::new(out), img.width, img.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(|e| file_error("saving", e))?;
                writer.write_image_data(&img.rgba).map_err(|e| file_error("saving", e))?;
                writer.finish().map_err(|e| file_error("saving", e))
            })?;
            self.exported((idx + 1) as f32 / count.max(1) as f32);
        }
        self.exported(1.0);
        Ok(count)
    }

    // ---- DreamsCraft v2.0: Auto-Draw, Perform Motion Ghost, Track Options & Effects, Audio,
    // ---- Live Filters, Warp/Distort, Video/Text Clips, Multi-Layer Cels, and Camera Track ----

    /// Configures automatic frame creation when drawing on empty timeline spots and automatic
    /// hold splitting when drawing inside an extended cel hold.
    pub fn timeline_set_auto_draw_options(&self, auto_create_frame: bool, auto_split_cel: bool) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let tl = st.doc.ensure_timeline();
        tl.auto_create_frame_on_draw = auto_create_frame;
        tl.auto_split_cel_on_draw = auto_split_cel;
        self.shared.frame(st);
    }

    /// Configures **Perform Motion Onion Skin (Ghost 1st Frame)** on `clip_id`: when `enabled`,
    /// renders the first frame (or an isolated guide layer when `isolate_first_frame` is true) as a
    /// tinted onion skin moving along the Perform motion trajectory so the user can animate
    /// frame-by-frame right on top of it.
    pub fn timeline_set_perform_onion_guide(
        &self,
        clip_id: u64,
        enabled: bool,
        isolate_first_frame: bool,
        opacity: f32,
        color_rgb: Vec<f32>,
    ) -> bool {
        self.change("perform motion ghost", |d| {
            let cid = ClipId(clip_id);
            let tl = d.ensure_timeline();
            let clip = tl.clip_mut(cid)?;
            clip.perform_onion_guide = enabled;
            clip.perform_guide_opacity = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 0.45 };
            if let [r, g, b, ..] = color_rgb.as_slice() {
                clip.perform_guide_color = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)];
            }
            if isolate_first_frame {
                if clip.perform_guide_layer.is_none() {
                    clip.perform_guide_layer = clip.first_layer();
                }
            } else {
                clip.perform_guide_layer = None;
            }
            Some(())
        })
        .is_some()
    }

    /// Duplicates `track_id` (including all its clips, keyframes, effects, and underlying GPU
    /// layer pixels) right above `track_id` as a single undo step.
    pub fn timeline_duplicate_track(&self, track_id: u64) -> Option<u64> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let tid = TrackId(track_id);
        let (pos, src_track) = {
            let tl = st.doc.ensure_timeline();
            let idx = tl.tracks.iter().position(|t| t.id == tid)?;
            (idx, tl.tracks.get(idx)?.clone())
        };
        let layers_needed: usize = src_track.clips.iter().map(|c| c.referenced_layers().len()).sum();
        if !st.doc.has_room(layers_needed) {
            return None;
        }
        let before = st.doc.clone();
        let mut pairs = Vec::new();
        let mut map_layer = |doc: &mut Document, old: LayerId| -> LayerId {
            if let Some((new_id, copy_pairs)) = doc.duplicate(old) {
                pairs.extend(copy_pairs);
                new_id
            } else {
                old
            }
        };

        let mut next_id = {
            let tl = st.doc.ensure_timeline();
            tl.tracks
                .iter()
                .map(|t| t.id.0)
                .chain(tl.tracks.iter().flat_map(|t| &t.clips).map(|c| c.id.0))
                .max()
                .unwrap_or(0)
                .saturating_add(100)
        };
        let new_track_id = TrackId(next_id);
        next_id = next_id.saturating_add(1);

        let mut new_clips = Vec::with_capacity(src_track.clips.len());
        for clip in &src_track.clips {
            let new_clip_id = ClipId(next_id);
            next_id = next_id.saturating_add(1);
            let new_kind = match &clip.kind {
                ClipKind::Cel { cels } => ClipKind::Cel {
                    cels: cels.iter().map(|c| Cel { layer: map_layer(&mut st.doc, c.layer), hold: c.hold }).collect(),
                },
                ClipKind::Layer { layer } => ClipKind::Layer { layer: map_layer(&mut st.doc, *layer) },
                ClipKind::Text { layer } => ClipKind::Text { layer: map_layer(&mut st.doc, *layer) },
                ClipKind::Video { video_id, layer, source_fps, trim_start_us, frame_count } => ClipKind::Video {
                    video_id: *video_id,
                    layer: map_layer(&mut st.doc, *layer),
                    source_fps: *source_fps,
                    trim_start_us: *trim_start_us,
                    frame_count: *frame_count,
                },
                ClipKind::Group { members } => {
                    ClipKind::Group { members: members.iter().map(|&m| map_layer(&mut st.doc, m)).collect() }
                }
                ClipKind::Audio { asset_id, trim_start_samples, sample_rate, channels } => ClipKind::Audio {
                    asset_id: *asset_id,
                    trim_start_samples: *trim_start_samples,
                    sample_rate: *sample_rate,
                    channels: *channels,
                },
            };
            new_clips.push(Clip {
                id: new_clip_id,
                name: clip.name.clone(),
                start_frame: clip.start_frame,
                duration_frames: clip.duration_frames,
                blend: clip.blend,
                opacity: clip.opacity,
                kind: new_kind,
                keyframes: clip.keyframes.clone(),
                perform_onion_guide: clip.perform_onion_guide,
                perform_guide_layer: clip.perform_guide_layer.map(|g| map_layer(&mut st.doc, g)),
                perform_guide_opacity: clip.perform_guide_opacity,
                perform_guide_color: clip.perform_guide_color,
            });
        }
        let mut copy_track = src_track;
        copy_track.id = new_track_id;
        copy_track.name = format!("{} Copy", copy_track.name);
        copy_track.clips = new_clips;

        let tl = st.doc.ensure_timeline();
        let insert_at = (pos + 1).min(tl.tracks.len());
        tl.tracks.insert(insert_at, copy_track);
        tl.active_track = Some(new_track_id);
        tl.active_clip = tl.track(new_track_id).and_then(|t| t.clips.first()).map(|c| c.id);
        st.doc.repair_timeline();
        sync_current_layer_to_playhead(&mut st.doc);

        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("duplicate track");
            canvas.duplicate_layers(pairs, before);
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(new_track_id.0)
    }

    /// Reorders `track_id` up (`delta > 0`) or down (`delta < 0`) in the track stack as an undo step.
    pub fn timeline_move_track(&self, track_id: u64, delta: i32) -> bool {
        self.change("reorder track", |d| d.ensure_timeline().move_track(TrackId(track_id), delta).then_some(()))
            .is_some()
    }

    /// Removes all clips from `track_id` while keeping the track and its effects as an undo step.
    pub fn timeline_clear_track_clips(&self, track_id: u64) -> bool {
        self.change("clear track", |d| {
            let tl = d.ensure_timeline();
            let removed = tl.clear_track_clips(TrackId(track_id))?;
            let still_managed = tl.managed_layers();
            for lid in removed {
                if !still_managed.contains(&lid) && d.layers().len() > 1 {
                    let _ = d.delete_layer(lid);
                }
            }
            sync_current_layer_to_playhead(d);
            Some(())
        })
        .is_some()
    }

    /// Merges `track_id` into the track immediately below it as an undo step.
    pub fn timeline_merge_track_down(&self, track_id: u64) -> bool {
        self.change("merge track down", |d| {
            let ok = d.ensure_timeline().merge_track_down(TrackId(track_id));
            if ok {
                sync_current_layer_to_playhead(d);
            }
            ok.then_some(())
        })
        .is_some()
    }

    /// Sets the accent color tag (ARGB) of `track_id` as an undo step.
    pub fn timeline_set_track_color(&self, track_id: u64, color_argb: u32) -> bool {
        self.change("track color", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            track.color_tag = color_argb;
            Some(())
        })
        .is_some()
    }

    /// Toggles track-level Clipping Mask (`Clip to Track Below`) on `track_id` as an undo step.
    pub fn timeline_set_track_clip_to_below(&self, track_id: u64, enabled: bool) -> bool {
        self.change("clip to track below", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            track.clip_to_below = enabled;
            Some(())
        })
        .is_some()
    }

    /// Toggles Alpha Lock on `track_id` (and synchronizes alpha lock on its cel layers).
    pub fn timeline_set_track_alpha_lock(&self, track_id: u64, enabled: bool) -> bool {
        self.change("track alpha lock", |d| {
            let layers = {
                let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
                track.alpha_lock = enabled;
                track.clips.iter().flat_map(Clip::referenced_layers).collect::<Vec<_>>()
            };
            for lid in layers {
                d.set_alpha_lock(lid, enabled);
            }
            Some(())
        })
        .is_some()
    }

    /// Toggles Mute for audio clips on `track_id`.
    pub fn timeline_set_track_muted(&self, track_id: u64, muted: bool) -> bool {
        self.change("mute track", |d| {
            let track = d.ensure_timeline().track_mut(TrackId(track_id))?;
            track.muted = muted;
            Some(())
        })
        .is_some()
    }

    /// Updates non-destructive GPU Layer Effects (Outline, Drop Shadow, Outer/Inner Glow, Color
    /// Overlay) on `track_id`, coalescing slider drags until `done`.
    pub fn timeline_set_track_effects(&self, track_id: u64, effects: TimelineTrackEffectsInfo, done: bool) -> bool {
        let fx = TrackEffects::from(&effects);
        self.change_live("track layer effects", done, |d| {
            let Some(track) = d.ensure_timeline().track_mut(TrackId(track_id)) else { return false };
            track.effects = fx;
            true
        })
    }

    /// Sets or updates a non-destructive Live Filter keyframe (`Gaussian Blur`, `HSB`, `Sharpen`,
    /// `Noise`) on `clip_id` at `frame`, coalescing drags until `done`.
    pub fn timeline_set_filter_keyframe(
        &self,
        clip_id: u64,
        frame: u32,
        filter: TimelineLiveFilterInfo,
        done: bool,
    ) -> bool {
        let f = LiveFilterState::from(&filter);
        self.change_live("filter keyframe", done, |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
                return false;
            }
            let Some(clip) = tl.clip_mut(cid) else { return false };
            clip.keyframes.set_filter(frame, f, None);
            true
        })
    }

    /// Deletes a Live Filter keyframe on `clip_id` at `frame` as an undo step.
    pub fn timeline_delete_filter_keyframe(&self, clip_id: u64, frame: u32) -> bool {
        self.change("delete filter keyframe", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.remove_filter(frame).then_some(())
        })
        .is_some()
    }

    /// Changes the outgoing easing curve of the Live Filter keyframe at `frame` on `clip_id`.
    pub fn timeline_set_filter_easing(&self, clip_id: u64, frame: u32, easing: TimelineEasingInfo) -> bool {
        let e = Easing::from(&easing);
        self.change("filter keyframe easing", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.set_filter_easing(frame, e).then_some(())
        })
        .is_some()
    }

    /// Sets or updates a non-destructive `Warp` (4×4 mesh) / `Distort` (4 corners) keyframe on
    /// `clip_id` at `frame`, coalescing drags until `done`.
    pub fn timeline_set_deform_keyframe(
        &self,
        clip_id: u64,
        frame: u32,
        deform: TimelineDeformInfo,
        done: bool,
    ) -> bool {
        let def = Deform2D::from(&deform);
        self.change_live("warp keyframe", done, |d| {
            let tl = d.ensure_timeline();
            let cid = ClipId(clip_id);
            if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
                return false;
            }
            let Some(clip) = tl.clip_mut(cid) else { return false };
            clip.keyframes.set_deform(frame, def, None);
            true
        })
    }

    /// Deletes a Warp/Distort keyframe on `clip_id` at `frame` as an undo step.
    pub fn timeline_delete_deform_keyframe(&self, clip_id: u64, frame: u32) -> bool {
        self.change("delete warp keyframe", |d| {
            let clip = d.ensure_timeline().clip_mut(ClipId(clip_id))?;
            clip.keyframes.remove_deform(frame).then_some(())
        })
        .is_some()
    }

    /// Commits a live Perform Warp/Distort recording on `clip_id` with Ramer–Douglas–Peucker
    /// simplification as **one atomic undo step** (`"perform warp"`).
    pub fn timeline_commit_perform_deform(
        &self,
        clip_id: u64,
        samples: Vec<PerformDeformSampleInfo>,
        filtering: f32,
    ) -> u32 {
        if samples.is_empty() {
            let _ = self.timeline_cancel_perform();
            return 0;
        }
        let engine_samples: Vec<DeformSample> =
            samples.iter().map(|s| DeformSample { frame: s.frame, deform: Deform2D::from(&s.deform) }).collect();
        let simplified = simplify_deform_samples(&engine_samples, filtering);
        if simplified.is_empty() {
            let _ = self.timeline_cancel_perform();
            return 0;
        }
        let first_frame = simplified.first().map_or(0, |k| k.frame);
        let last_frame = simplified.last().map_or(first_frame, |k| k.frame);
        let count = u32::try_from(simplified.len()).unwrap_or(0);

        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        let before = st.pending_doc.take().unwrap_or_else(|| st.doc.clone());
        st.pending_name = None;

        let cid = ClipId(clip_id);
        let orig_deform_kfs = before.timeline.as_ref().and_then(|t| t.clip(cid)).map(|c| c.keyframes.deform.clone());

        let tl = st.doc.ensure_timeline();
        tl.motion_filtering = if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { tl.motion_filtering };
        if tl.track_of_clip(cid).and_then(|t| tl.track(t)).is_some_and(|t| t.locked) {
            st.doc = before;
            return 0;
        }
        let Some(clip) = tl.clip_mut(cid) else {
            st.doc = before;
            return 0;
        };
        if let Some(orig) = orig_deform_kfs {
            clip.keyframes.deform = orig;
        }
        clip.keyframes.deform.retain(|k| k.frame < first_frame || k.frame > last_frame);
        for kf in &simplified {
            clip.keyframes.set_deform(kf.frame, kf.value, Some(kf.easing));
        }

        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("perform warp");
            canvas.record_document(before);
        }
        st.revision += 1;
        self.shared.frame(st);
        count
    }

    /// Imports WAV audio bytes (8/16/24-bit PCM or 32-bit float WAV, e.g. from `SoundDecode.kt`)
    /// onto a new Audio track at `start_frame` as an undo step, returning the new `ClipId`.
    pub fn timeline_import_audio_wav(&self, name: String, wav_bytes: Vec<u8>, start_frame: u32) -> Option<u64> {
        let decoded = decode_wav_pcm(&wav_bytes)?;
        self.change("import audio", |d| {
            let (_, cid, _) = d.ensure_timeline().add_audio_clip(
                None,
                name.clone(),
                decoded.sample_rate,
                decoded.channels,
                decoded.samples.clone(),
                start_frame,
            )?;
            Some(cid.0)
        })
    }

    /// Imports interleaved 16-bit PCM audio onto a new Audio track at `start_frame` as an undo step.
    pub fn timeline_import_audio_pcm(
        &self,
        name: String,
        sample_rate: u32,
        channels: u8,
        pcm_i16: Vec<i16>,
        start_frame: u32,
    ) -> Option<u64> {
        let floats: Vec<f32> = pcm_i16.iter().map(|&s| f32::from(s) / 32_768.0).collect();
        self.change("import audio", |d| {
            let (_, cid, _) = d.ensure_timeline().add_audio_clip(
                None,
                name.clone(),
                sample_rate,
                channels,
                floats.clone(),
                start_frame,
            )?;
            Some(cid.0)
        })
    }

    /// Sets an audio volume keyframe (`0.0..=2.0`) on `clip_id` at `frame`.
    pub fn timeline_set_audio_volume_keyframe(&self, clip_id: u64, frame: u32, volume: f32, done: bool) -> bool {
        self.change_live("audio volume keyframe", done, |d| {
            let Some(clip) = d.ensure_timeline().clip_mut(ClipId(clip_id)) else { return false };
            clip.keyframes.set_volume(frame, volume, None);
            true
        })
    }

    /// Sets an audio stereo pan keyframe (`-1.0..=1.0`) on `clip_id` at `frame`.
    pub fn timeline_set_audio_pan_keyframe(&self, clip_id: u64, frame: u32, pan: f32, done: bool) -> bool {
        self.change_live("audio pan keyframe", done, |d| {
            let Some(clip) = d.ensure_timeline().clip_mut(ClipId(clip_id)) else { return false };
            clip.keyframes.set_pan(frame, pan, None);
            true
        })
    }

    /// Mixes all visible, unmuted timeline audio tracks over `start_frame..=end_frame` (inclusive)
    /// into interleaved 16-bit stereo PCM at `sample_rate` Hz (for Android `AudioTrack` playback,
    /// lip-sync Audio Scrubbing, and `VideoWriter` AAC MP4 muxing).
    pub fn timeline_mix_audio_pcm(&self, start_frame: u32, end_frame: u32, sample_rate: u32) -> Vec<i16> {
        let st = self.shared.lock();
        let Some(tl) = st.doc.timeline.as_ref() else {
            return Vec::new();
        };
        let floats = tl.mix_audio_range(start_frame, end_frame, sample_rate);
        float_pcm_to_i16(&floats)
    }

    /// Creates the track, clip, and source reference as one undo step.
    pub fn timeline_import_video(
        &self,
        name: String,
        start_frame: u32,
        duration_frames: u32,
        source_fps: f32,
        frame_count: u32,
        file: String,
    ) -> Option<u64> {
        if !Timeline::valid_video_filename(&file) {
            return None;
        }
        self.change("import video", |doc| {
            if !doc.has_room(1) {
                return None;
            }
            let layer = doc.add_layer();
            let tl = doc.ensure_timeline();
            let track = tl.add_track(Some(&name), false);
            let clip =
                tl.add_video_clip(track, start_frame, duration_frames, layer, source_fps, frame_count, Some(&name))?;
            tl.video_files.insert(clip.0, file);
            Some(clip.0)
        })
    }

    /// Video input for an absolute timeline frame, in source microseconds.
    pub fn timeline_video_requests(&self, frame: u32) -> Vec<TimelineVideoRequest> {
        let st = self.shared.lock();
        let Some(tl) = st.doc.timeline.as_ref() else { return Vec::new() };
        if tl.video_files.is_empty() {
            return Vec::new();
        }
        let scene = tl.evaluate(&st.doc, frame as f32, true);
        scene
            .items
            .iter()
            .filter_map(|item| {
                let clip = tl.clip(item.clip)?;
                let ClipKind::Video { video_id, trim_start_us, .. } = clip.kind else { return None };
                let file = tl.video_files.get(&video_id)?.clone();
                let time_us = trim_start_us
                    .saturating_add(u64::from(frame - clip.start_frame) * 1_000_000 / u64::from(tl.fps.clamp(1, 60)));
                Some(TimelineVideoRequest { clip_id: clip.id.0, file, time_us })
            })
            .collect()
    }

    /// Adds a `VideoClip` (for rotoscoping or video compositing) on `track_id` as an undo step.
    pub fn timeline_add_video_clip(
        &self,
        track_id: u64,
        start_frame: u32,
        duration_frames: u32,
        source_fps: f32,
        frame_count: u32,
        name: Option<String>,
    ) -> u64 {
        self.change("import video clip", |d| {
            if !d.has_room(1) {
                return None;
            }
            let layer = d.add_layer();
            let tl = d.ensure_timeline();
            let cid = tl.add_video_clip(
                TrackId(track_id),
                start_frame,
                duration_frames,
                layer,
                source_fps,
                frame_count,
                name.as_deref(),
            )?;
            Some(cid.0)
        })
        .unwrap_or(0)
    }

    /// Uploads a decoded RGBA frame (`width × height`) into the managed layer of `VideoClip`
    /// `clip_id` without pushing an undo step.
    pub fn timeline_supply_video_frame(&self, clip_id: u64, width: u32, height: u32, rgba: Vec<u8>) -> bool {
        let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if width == 0 || height == 0 || rgba.len() < expected {
            return false;
        }
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let target_layer = st.doc.timeline.as_ref().and_then(|tl| {
            let clip = tl.clip(ClipId(clip_id))?;
            match clip.kind {
                ClipKind::Video { layer, .. } => Some(layer),
                _ => None,
            }
        });
        let Some(layer) = target_layer else { return false };
        let (cw, ch) = (st.doc.width(), st.doc.height());
        if let Some(canvas) = st.canvas.as_mut() {
            if width == cw && height == ch {
                canvas.put_layer(layer, rgba);
            } else {
                let w = width.min(cw);
                let h = height.min(ch);
                let x = (cw.saturating_sub(w)) / 2;
                let y = (ch.saturating_sub(h)) / 2;
                let mut cropped = vec![0u8; (w as usize) * (h as usize) * 4];
                for row in 0..h as usize {
                    let src_start = row * (width as usize) * 4;
                    let dst_start = row * (w as usize) * 4;
                    let row_bytes = (w as usize) * 4;
                    if let (Some(src), Some(dst)) =
                        (rgba.get(src_start..src_start + row_bytes), cropped.get_mut(dst_start..dst_start + row_bytes))
                    {
                        dst.copy_from_slice(src);
                    }
                }
                canvas.put_layer_region(layer, x, y, w, h, cropped);
            }
            canvas.invalidate();
            self.shared.frame(st);
            return true;
        }
        false
    }

    /// Adds an animated `TextClip` on `track_id` at `start_frame` for `duration_frames` as an undo step.
    pub fn timeline_add_text_clip(
        &self,
        track_id: u64,
        start_frame: u32,
        duration_frames: u32,
        name: Option<String>,
    ) -> u64 {
        self.change("new text clip", |d| {
            if !d.has_room(1) {
                return None;
            }
            let layer = d.add_layer();
            let tl = d.ensure_timeline();
            let cid = tl.add_text_clip(TrackId(track_id), start_frame, duration_frames, layer, name.as_deref())?;
            sync_current_layer_to_playhead(d);
            Some(cid.0)
        })
        .unwrap_or(0)
    }

    /// Returns the sub-layer IDs inside a Flipbook `Cel` (if the cel is a `LayerKind::Group`,
    /// returns all its child paint layers; otherwise returns the single paint layer ID).
    pub fn timeline_cel_sublayer_ids(&self, clip_id: u64, cel_index: u32) -> Vec<u64> {
        let st = self.shared.lock();
        let Some(tl) = st.doc.timeline.as_ref() else {
            return Vec::new();
        };
        let Some(clip) = tl.clip(ClipId(clip_id)) else {
            return Vec::new();
        };
        let ClipKind::Cel { cels } = &clip.kind else {
            return Vec::new();
        };
        let Some(cel) = cels.get(cel_index as usize) else {
            return Vec::new();
        };
        if st.doc.layer(cel.layer).is_some_and(|l| l.kind == LayerKind::Group) {
            st.doc.members(Some(cel.layer)).into_iter().map(|id| id.0).collect()
        } else {
            vec![cel.layer.0]
        }
    }

    /// Adds a new sub-layer inside a Flipbook `Cel` (converting a single-layer cel transparently
    /// into a `LayerKind::Group` holding both the existing drawing and the new sub-layer) so
    /// artists can separate Lineart, Color, and Shading inside a single animation frame.
    pub fn timeline_add_cel_sublayer(&self, clip_id: u64, cel_index: u32) -> Option<u64> {
        self.change("add cel sub-layer", |d| {
            if !d.has_room(2) {
                return None;
            }
            let cel_layer = {
                let tl = d.ensure_timeline();
                let clip = tl.clip(ClipId(clip_id))?;
                let ClipKind::Cel { cels } = &clip.kind else { return None };
                cels.get(cel_index as usize)?.layer
            };
            let is_group = d.layer(cel_layer).is_some_and(|l| l.kind == LayerKind::Group);
            let group_id = if is_group {
                cel_layer
            } else {
                d.clear_picks();
                d.set_current(cel_layer);
                let new_group = d.group_chosen()?;
                let tl = d.ensure_timeline();
                if let Some(clip) = tl.clip_mut(ClipId(clip_id))
                    && let ClipKind::Cel { cels } = &mut clip.kind
                    && let Some(slot) = cels.get_mut(cel_index as usize)
                {
                    slot.layer = new_group;
                }
                new_group
            };
            let top_child = d.members(Some(group_id)).last().copied().unwrap_or(group_id);
            d.set_current(top_child);
            let sub_layer = d.add_layer();
            if d.group_of(sub_layer) != Some(group_id) {
                d.move_to(sub_layer, procraft_engine::document::Place::Onto(group_id));
            }
            d.set_current(sub_layer);
            Some(sub_layer.0)
        })
    }

    /// Enables or disables the global Camera Track transform on Stage preview and Export as an undo step.
    pub fn timeline_set_camera_enabled(&self, enabled: bool) {
        let _ = self.change("camera track", |d| {
            let tl = d.ensure_timeline();
            if tl.camera_enabled == enabled {
                return None;
            }
            tl.camera_enabled = enabled;
            Some(())
        });
    }

    /// Sets a global Camera Track `Transform2D` keyframe (Pan/Zoom/Rotate) at `frame`,
    /// automatically enabling the Camera Track.
    pub fn timeline_set_camera_transform_keyframe(
        &self,
        frame: u32,
        transform: TimelineTransformInfo,
        done: bool,
    ) -> bool {
        let t2d = Transform2D::from(&transform);
        self.change_live("camera keyframe", done, |d| {
            let tl = d.ensure_timeline();
            tl.camera_enabled = true;
            tl.camera.set_transform(frame, t2d, None);
            true
        })
    }

    /// Sets a global Camera Track `LiveFilterState` keyframe (e.g. Depth-of-Field lens blur or
    /// fade-to-black) at `frame`.
    pub fn timeline_set_camera_filter_keyframe(&self, frame: u32, filter: TimelineLiveFilterInfo, done: bool) -> bool {
        let f = LiveFilterState::from(&filter);
        self.change_live("camera filter keyframe", done, |d| {
            let tl = d.ensure_timeline();
            tl.camera_enabled = true;
            tl.camera.set_filter(frame, f, None);
            true
        })
    }

    /// Deletes the global Camera Track keyframes at `frame` as an undo step.
    pub fn timeline_delete_camera_keyframe(&self, frame: u32) -> bool {
        self.change("delete camera keyframe", |d| {
            let tl = d.ensure_timeline();
            let a = tl.camera.remove_transform(frame);
            let b = tl.camera.remove_filter(frame);
            (a || b).then_some(())
        })
        .is_some()
    }
}
