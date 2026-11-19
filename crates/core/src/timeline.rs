//! Multi-track Timeline session facade for UniFFI (Android Jetpack Compose) and Desktop (`egui`):
//! atomic undoable track/clip/cel/keyframe mutations, single-call `TimelineState` snapshot,
//! live Perform motion recording with RDP curve simplification, and PNG sequence export.

use std::fs;
use std::io::BufWriter;
use std::path::Path;

use procraft_engine::animation::Playback;
use procraft_engine::document::{BlendMode, Document, LayerId};
use procraft_engine::timeline::{
    Cel, Clip, ClipId, ClipKind, Easing, Keyframe, MotionSample, OnionSkinSettings, Timeline, Track, TrackId,
    Transform2D, WorkArea, simplify_motion_samples,
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

/// A keyframe on a clip or track property sub-track.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineKeyframeInfo {
    pub frame: u32,
    pub easing: TimelineEasingInfo,
    pub transform: Option<TimelineTransformInfo>,
    pub opacity: Option<f32>,
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
    pub clips: Vec<TimelineClipInfo>,
    pub transform_keyframes: Vec<TimelineKeyframeInfo>,
    pub opacity_keyframes: Vec<TimelineKeyframeInfo>,
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
    /// Evaluated Transform2D of the active clip (or active track) at `playhead`.
    pub active_evaluated_transform: TimelineTransformInfo,
    /// Evaluated opacity of the active clip (or active track) at `playhead`.
    pub active_evaluated_opacity: f32,
}

/// A live-recorded motion sample during Perform mode.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PerformSampleInfo {
    pub frame: u32,
    pub transform: TimelineTransformInfo,
}

fn map_transform_kfs(kfs: &[Keyframe<Transform2D>]) -> Vec<TimelineKeyframeInfo> {
    kfs.iter()
        .map(|k| TimelineKeyframeInfo {
            frame: k.frame,
            easing: TimelineEasingInfo::from(k.easing),
            transform: Some(TimelineTransformInfo::from(k.value)),
            opacity: None,
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
        })
        .collect()
}

fn snapshot_clip(track_id: TrackId, clip: &Clip, playhead: u32) -> TimelineClipInfo {
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
    }
}

fn snapshot_track(track: &Track, playhead: u32) -> TimelineTrackInfo {
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
        clips: track.clips.iter().map(|c| snapshot_clip(track.id, c, playhead)).collect(),
        transform_keyframes: map_transform_kfs(&track.keyframes.transform),
        opacity_keyframes: map_opacity_kfs(&track.keyframes.opacity),
    }
}

pub(crate) fn snapshot_timeline(tl: &Timeline) -> TimelineState {
    let playhead = tl.playhead;
    let (active_transform, active_opacity) = if let Some(cid) = tl.active_clip
        && let Some(clip) = tl.clip(cid)
    {
        (clip.keyframes.evaluate_transform(playhead as f32), clip.keyframes.evaluate_opacity(playhead as f32))
    } else if let Some(tid) = tl.active_track
        && let Some(track) = tl.track(tid)
    {
        (track.keyframes.evaluate_transform(playhead as f32), track.keyframes.evaluate_opacity(playhead as f32))
    } else {
        (Transform2D::default(), 1.0)
    };

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
        tracks: tl.tracks.iter().map(|t| snapshot_track(t, playhead)).collect(),
        active_track_id: tl.active_track.map(|t| t.0),
        active_clip_id: tl.active_clip.map(|c| c.0),
        flipbook_open: tl.flipbook_open,
        show_backstage: tl.show_backstage,
        motion_filtering: tl.motion_filtering,
        active_evaluated_transform: TimelineTransformInfo::from(active_transform),
        active_evaluated_opacity: active_opacity,
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
        && doc.layer(layer).is_some()
    {
        doc.set_current(layer);
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
        let tl = st.doc.ensure_timeline();
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

    /// Updates the timeline frame rate (`1..=60` fps).
    pub fn timeline_set_fps(&self, fps: u32) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let clamped = fps.clamp(1, 60);
        st.doc.ensure_timeline().fps = clamped;
        if let Some(a) = st.doc.animation.as_mut() {
            a.fps = clamped;
        }
        self.shared.frame(st);
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

    /// Updates the timeline playback mode (`Loop`, `PingPong`, `OneShot`).
    pub fn timeline_set_playback(&self, playback: AnimationPlayback) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let mode = match playback {
            AnimationPlayback::Loop => Playback::Loop,
            AnimationPlayback::PingPong => Playback::PingPong,
            AnimationPlayback::OneShot => Playback::OneShot,
        };
        st.doc.ensure_timeline().playback = mode;
        if let Some(a) = st.doc.animation.as_mut() {
            a.playback = mode;
        }
        self.shared.frame(st);
    }

    /// Sets or clears the timeline Work Area (In/Out loop markers).
    pub fn timeline_set_work_area(&self, work_area: Option<TimelineWorkAreaInfo>) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let tl = st.doc.ensure_timeline();
        let dur = tl.duration_frames;
        tl.work_area = work_area.map(|w| WorkArea { start: w.start, end: w.end }.repaired(dur));
        self.shared.frame(st);
    }

    /// Updates the timeline's Onion Skin settings live.
    pub fn timeline_set_onion_skin(&self, onion: TimelineOnionInfo) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let color3 = |v: &[f32], fallback: [f32; 3]| match v {
            [r, g, b, ..] => [*r, *g, *b],
            _ => fallback,
        };
        let tl = st.doc.ensure_timeline();
        let prev_before = tl.onion.before_color;
        let prev_after = tl.onion.after_color;
        tl.onion = OnionSkinSettings {
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
        if let Some(a) = st.doc.animation.as_mut() {
            a.onion_frames = if onion.enabled { onion.before_frames.max(onion.after_frames) } else { 0 };
            a.onion_opacity = onion.opacity;
            a.before = color3(&onion.before_color, a.before);
            a.after = color3(&onion.after_color, a.after);
            a.blend_primary = onion.blend_primary;
        }
        if let Some(c) = st.canvas.as_mut() {
            c.invalidate();
        }
        self.shared.frame(st);
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

    /// Deletes `track_id` (so long as at least one track remains) as an undo step.
    pub fn timeline_delete_track(&self, track_id: u64) -> bool {
        self.change("delete track", |d| {
            let tl = d.ensure_timeline();
            if tl.tracks.len() <= 1 {
                return None;
            }
            let tid = TrackId(track_id);
            if tl.track(tid).is_some_and(|t| t.locked) {
                return None;
            }
            let pos = tl.tracks.iter().position(|t| t.id == tid)?;
            tl.tracks.remove(pos);
            for t in &mut tl.tracks {
                if t.parent_group == Some(tid) {
                    t.parent_group = None;
                }
            }
            tl.active_track = tl.tracks.last().map(|t| t.id);
            tl.active_clip = tl.active_track.and_then(|a| tl.track(a)).and_then(|t| t.clips.first()).map(|c| c.id);
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
            let tl = d.ensure_timeline();
            let at = tl.playhead;
            tl.split_clip(ClipId(clip_id), at).map(|c| c.0)
        })
    }

    /// Duplicates `clip_id` (and its underlying layer(s) on the GPU) right after `clip_id` as an
    /// undo step, returning the new clip id.
    pub fn timeline_duplicate_clip(&self, clip_id: u64) -> Option<u64> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let cid = ClipId(clip_id);
        let (track_id, clip) = {
            let tl = st.doc.ensure_timeline();
            let tid = tl.track_of_clip(cid)?;
            if tl.track(tid).is_some_and(|t| t.locked) {
                return None;
            }
            (tid, tl.clip(cid)?.clone())
        };
        let layers_needed = clip.referenced_layers().len();
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
        let new_kind = match &clip.kind {
            ClipKind::Cel { cels } => {
                let new_cels: Vec<Cel> =
                    cels.iter().map(|c| Cel { layer: map_layer(&mut st.doc, c.layer), hold: c.hold }).collect();
                ClipKind::Cel { cels: new_cels }
            }
            ClipKind::Layer { layer } => ClipKind::Layer { layer: map_layer(&mut st.doc, *layer) },
            ClipKind::Group { members } => {
                let new_members: Vec<LayerId> = members.iter().map(|&m| map_layer(&mut st.doc, m)).collect();
                ClipKind::Group { members: new_members }
            }
        };
        let new_start = clip.end_frame();
        let delta = i64::from(new_start) - i64::from(clip.start_frame);
        let mut new_kf = clip.keyframes.clone();
        new_kf.shift(delta);

        let tl = st.doc.ensure_timeline();
        let new_clip_id = ClipId(tl.tracks.iter().flat_map(|t| &t.clips).map(|c| c.id.0).max().unwrap_or(0) + 100);
        let mut new_clip = Clip {
            id: new_clip_id,
            name: format!("{} Copy", clip.name),
            start_frame: new_start,
            duration_frames: clip.effective_duration(),
            blend: clip.blend,
            opacity: clip.opacity,
            kind: new_kind,
            keyframes: new_kf,
        };
        new_clip.sync_duration();
        let end = new_clip.end_frame();
        if let Some(track) = tl.track_mut(track_id) {
            track.clips.push(new_clip);
            track.clips.sort_by_key(|c| c.start_frame);
        }
        tl.duration_frames = tl.duration_frames.max(end);
        tl.active_clip = Some(new_clip_id);
        tl.playhead = new_start;
        st.doc.repair_timeline();
        sync_current_layer_to_playhead(&mut st.doc);

        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step("duplicate clip");
            canvas.duplicate_layers(pairs, before);
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(new_clip_id.0)
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

    /// Commits a live Perform motion recording on `clip_id`: simplifies `samples` with
    /// `filtering` (`0.0..=1.0`, Ramer–Douglas–Peucker) and writes the resulting `Move` keyframes
    /// as a single atomic undo step. Returns the number of simplified keyframes written.
    pub fn timeline_commit_perform(&self, clip_id: u64, samples: Vec<PerformSampleInfo>, filtering: f32) -> u32 {
        if samples.is_empty() {
            return 0;
        }
        let engine_samples: Vec<MotionSample> = samples
            .iter()
            .map(|s| MotionSample { frame: s.frame, transform: Transform2D::from(&s.transform) })
            .collect();
        let simplified = simplify_motion_samples(&engine_samples, filtering);
        if simplified.is_empty() {
            return 0;
        }
        let first_frame = simplified.first().map_or(0, |k| k.frame);
        let last_frame = simplified.last().map_or(first_frame, |k| k.frame);
        let count = u32::try_from(simplified.len()).unwrap_or(0);
        let ok = self
            .change("perform motion", |d| {
                let tl = d.ensure_timeline();
                tl.motion_filtering =
                    if filtering.is_finite() { filtering.clamp(0.0, 1.0) } else { tl.motion_filtering };
                let cid = ClipId(clip_id);
                let tid = tl.track_of_clip(cid)?;
                if tl.track(tid).is_some_and(|t| t.locked) {
                    return None;
                }
                let clip = tl.clip_mut(cid)?;
                // Replace any existing transform keyframes within the recorded range [first_frame..=last_frame]
                clip.keyframes.transform.retain(|k| k.frame < first_frame || k.frame > last_frame);
                for kf in &simplified {
                    clip.keyframes.set_transform(kf.frame, kf.value, Some(kf.easing));
                }
                Some(())
            })
            .is_some();
        if ok { count } else { 0 }
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
}
