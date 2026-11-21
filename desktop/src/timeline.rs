//! Native Desktop **DreamsCraft Multi-Track Timeline & Flipbook Dock** integrated directly into
//! ProCraft's egui workspace using the same neutral dark/light frosted palette (`crate::look`).

use std::time::{Duration, Instant};

use egui::{Align2, Color32, Id, Order, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use procraft_core::{
    CanvasSession, TimelineClipInfo, TimelineClipKind, TimelineClipLoopMode, TimelineState, TimelineTrackInfo,
    animation_next_step,
};

use crate::look::{self, Shade, Themed, Weight, argb, text};

const SURFACE: Themed = Themed(0xF51C_1C1C, 0xF5E6_E6E6);
const BAR_FILL: Themed = Themed(0xFF22_2222, 0xFFEA_EAEA);
const TRACK_BG: Themed = Themed(0xFF16_1616, 0xFFDF_DFDF);
const HEADER_BG: Themed = Themed(0xFF20_2020, 0xFFE3_E3E3);
const CLIP_FILL: Themed = Themed(0xFF2E_2E2E, 0xFFFF_FFFF);
const PILL_BG: Themed = Themed(0xFF30_3030, 0xFFD5_D5D5);
const INK: Themed = Themed(0xFFF2_F2F2, 0xFF22_2222);
const DIM_INK: Themed = Themed(0xFF9A_9A9A, 0xFF5E_5E5E);
const EDGE: Themed = Themed(0xFF3A_3A3A, 0xFFC8_C8C8);

const ACCENT: u32 = 0xFF00_76FD;
const PERFORM_RED: u32 = 0xFFFF_2D55;
const KEY_GOLD: u32 = 0xFFFF_B800;
const OPACITY_CYAN: u32 = 0xFF06_D6F2;
const FILTER_PURPLE: u32 = 0xFFA8_55F7;
const WARP_PINK: u32 = 0xFFEC_4899;
const AUDIO_GREEN: u32 = 0xFF22_C55E;
const CAMERA_SKY: u32 = 0xFF38_BDF8;

const BAR_H: f32 = 36.0;
const RULER_H: f32 = 24.0;
const HEADER_W: f32 = 152.0;
const TRACK_H: f32 = 34.0;

struct Playing {
    at: u32,
    step: i32,
    next_at: Instant,
}

/// Interactive DreamsCraft multi-track timeline and compact Flipbook dock for desktop ProCraft.
pub struct DreamsTimelineDock {
    playing: Option<Playing>,
    px_per_frame: f32,
    scroll_x: f32,
    dock_height: f32,
}

impl Default for DreamsTimelineDock {
    fn default() -> Self {
        Self { playing: None, px_per_frame: 14.0, scroll_x: 0.0, dock_height: 220.0 }
    }
}

impl DreamsTimelineDock {
    /// Stops any active playback cleanly (when leaving the canvas or opening a full-screen dialog).
    pub fn reset(&mut self, session: &CanvasSession) {
        if self.playing.take().is_some() {
            session.set_play_frame(None);
        }
    }

    fn toggle_play(&mut self, session: &CanvasSession, tl: &TimelineState) {
        if self.playing.take().is_some() {
            session.set_play_frame(None);
            return;
        }
        let fps = tl.fps.max(1);
        self.playing = Some(Playing {
            at: tl.playhead,
            step: 1,
            next_at: Instant::now() + Duration::from_secs_f64(1.0 / f64::from(fps)),
        });
        session.set_play_frame(Some(tl.playhead));
    }

    fn tick_playback(&mut self, ctx: &egui::Context, session: &CanvasSession, tl: &TimelineState) {
        let Some(p) = self.playing.as_mut() else {
            return;
        };
        let total = session.animation_frame_count().max(1);
        let now = Instant::now();
        if now >= p.next_at {
            let fps = tl.fps.max(1);
            if let Some(next) = animation_next_step(tl.playback, total, p.at, p.step) {
                p.at = next.at;
                p.step = next.step;
                p.next_at = now + Duration::from_secs_f64(1.0 / f64::from(fps));
                let frame = tl.work_area.as_ref().map_or(p.at, |wa| (wa.start + p.at).min(wa.end));
                session.set_play_frame(Some(frame));
                session.timeline_set_playhead(frame);
            } else {
                self.playing = None;
                session.set_play_frame(None);
                return;
            }
        }
        if let Some(p) = self.playing.as_ref() {
            ctx.request_repaint_after(p.next_at.saturating_duration_since(now));
        }
    }

    /// Draws the DreamsCraft timeline (or compact Flipbook bar when `tl.flipbook_open` is true).
    /// Returns `true` when the multi-track timeline is active on `session` (so legacy `AnimationBar`
    /// is skipped).
    pub fn show(&mut self, ctx: &egui::Context, session: &CanvasSession) -> bool {
        let Some(tl) = session.timeline_state() else {
            self.reset(session);
            return false;
        };
        self.tick_playback(ctx, session, &tl);

        let screen = ctx.content_rect();
        if tl.flipbook_open {
            self.show_flipbook(ctx, session, &tl, screen);
            return true;
        }

        let height = self.dock_height.clamp(160.0, (screen.height() * 0.55).max(160.0));
        let dock = Rect::from_min_max(pos2(screen.left(), screen.bottom() - height), screen.max);

        egui::Area::new(Id::unique("dreams_timeline_dock")).fixed_pos(dock.min).order(Order::Middle).show(ctx, |ui| {
            // Absorb pointer events so painting strokes never leak behind the dock
            ui.allocate_rect(dock, Sense::click_and_drag());
            ui.painter().rect_filled(dock, 0.0, argb(SURFACE));
            ui.painter().line_segment(
                [pos2(dock.left(), dock.top()), pos2(dock.right(), dock.top())],
                Stroke::new(1.0, argb(EDGE)),
            );

            // 1. Center Action / Transport Bar
            let bar = Rect::from_min_size(dock.min, vec2(dock.width(), BAR_H));
            self.draw_action_bar(ui, session, &tl, bar);

            // 2. Time Ruler & Corner Frame Controls
            let ruler_top = bar.bottom();
            let corner = Rect::from_min_size(pos2(dock.left(), ruler_top), vec2(HEADER_W, RULER_H));
            let ruler = Rect::from_min_max(pos2(corner.right(), ruler_top), pos2(dock.right(), ruler_top + RULER_H));
            self.draw_ruler(ui, session, &tl, corner, ruler);

            // 3. Multi-Track Dopesheet
            let body = Rect::from_min_max(pos2(dock.left(), ruler.bottom()), dock.max);
            self.draw_tracks(ui, session, &tl, body);
        });

        true
    }

    fn draw_action_bar(&mut self, ui: &mut Ui, session: &CanvasSession, tl: &TimelineState, bar: Rect) {
        ui.painter().rect_filled(bar, 0.0, argb(BAR_FILL));
        ui.painter().line_segment(
            [pos2(bar.left(), bar.bottom()), pos2(bar.right(), bar.bottom())],
            Stroke::new(1.0, argb(EDGE)),
        );

        let mut x = bar.left() + 10.0;
        let cy = bar.center().y;
        let is_playing = self.playing.is_some();

        // Play / Pause pill
        if pill(
            ui,
            &mut x,
            cy,
            if is_playing { "Pause" } else { "Play" },
            if is_playing { PERFORM_RED } else { ACCENT },
            0xFFFF_FFFF,
            "dt_play",
        ) {
            self.toggle_play(session, tl);
        }
        // Step frame & keyframe
        if pill(ui, &mut x, cy, "<◆", PILL_BG.value(), KEY_GOLD, "dt_prev_kf") {
            self.reset(session);
            let cid = tl.active_clip_id.unwrap_or(0);
            let _ = session.timeline_jump_keyframe(cid, -1);
        }
        if pill(ui, &mut x, cy, "‹", PILL_BG.value(), INK.value(), "dt_prev_f") {
            self.reset(session);
            let _ = session.timeline_step_frame(-1);
        }
        let tc = format!("F{}/{} · {}fps", tl.playhead + 1, tl.duration_frames, tl.fps);
        let tc_rect =
            text(ui.painter(), pos2(x + 2.0, cy), Align2::LEFT_CENTER, &tc, 12.0, Weight::SemiBold, argb(INK));
        x = tc_rect.right() + 8.0;
        if pill(ui, &mut x, cy, "›", PILL_BG.value(), INK.value(), "dt_next_f") {
            self.reset(session);
            let _ = session.timeline_step_frame(1);
        }
        if pill(ui, &mut x, cy, "◆>", PILL_BG.value(), KEY_GOLD, "dt_next_kf") {
            self.reset(session);
            let cid = tl.active_clip_id.unwrap_or(0);
            let _ = session.timeline_jump_keyframe(cid, 1);
        }

        // FPS Cycle
        let fps_label = format!("{}fps", tl.fps);
        if pill(ui, &mut x, cy, &fps_label, PILL_BG.value(), DIM_INK.value(), "dt_fps") {
            let next = match tl.fps {
                0..=12 => 24,
                13..=24 => 30,
                25..=30 => 60,
                _ => 12,
            };
            session.timeline_set_fps(next);
        }
        // Onion Skin toggle
        let onion_bg = if tl.onion.enabled { KEY_GOLD } else { PILL_BG.value() };
        let onion_fg = if tl.onion.enabled { 0xFF00_0000 } else { DIM_INK.value() };
        if pill(ui, &mut x, cy, "Onion", onion_bg, onion_fg, "dt_onion") {
            let mut next = tl.onion.clone();
            next.enabled = !next.enabled;
            session.timeline_set_onion_skin(next);
        }
        // Backstage toggle
        let bs_bg = if tl.show_backstage { ACCENT } else { PILL_BG.value() };
        let bs_fg = if tl.show_backstage { 0xFFFF_FFFF } else { DIM_INK.value() };
        if pill(ui, &mut x, cy, "Backstage", bs_bg, bs_fg, "dt_backstage") {
            session.timeline_set_backstage(!tl.show_backstage);
        }
        // Camera toggle
        let cam_bg = if tl.camera_enabled { CAMERA_SKY } else { PILL_BG.value() };
        let cam_fg = if tl.camera_enabled { 0xFF00_0000 } else { DIM_INK.value() };
        if pill(ui, &mut x, cy, "Camera", cam_bg, cam_fg, "dt_cam") {
            session.timeline_set_camera_enabled(!tl.camera_enabled);
        }
        // Flipbook toggle
        if pill(ui, &mut x, cy, "Flipbook", PILL_BG.value(), OPACITY_CYAN, "dt_flipbook") {
            self.reset(session);
            session.timeline_set_flipbook(true);
        }

        // Right-aligned clip / track editing actions
        if pill(ui, &mut x, cy, "+Marker", PILL_BG.value(), KEY_GOLD, "dt_marker") {
            let _ = session.timeline_add_marker(tl.playhead, None, None);
        }
        if pill(ui, &mut x, cy, "+Cel", ACCENT, 0xFFFF_FFFF, "dt_add_cel") {
            self.reset(session);
            let _ = session.timeline_add_cel(0);
        }
        if pill(ui, &mut x, cy, "+Track", PILL_BG.value(), INK.value(), "dt_add_track") {
            self.reset(session);
            let _ = session.timeline_add_track(None, true);
        }
        if let Some(cid) = tl.active_clip_id {
            if pill(ui, &mut x, cy, "Split", PILL_BG.value(), PERFORM_RED, "dt_split") {
                let _ = session.timeline_split_clip_at_playhead(cid);
            }
            if pill(ui, &mut x, cy, "Fill", PILL_BG.value(), INK.value(), "dt_fill") {
                let _ = session.timeline_fill_clip_duration(cid);
            }
            let active_loop = active_clip(tl).map_or(TimelineClipLoopMode::None, |c| c.loop_mode);
            let loop_label = match active_loop {
                TimelineClipLoopMode::None => "Once",
                TimelineClipLoopMode::Loop => "Loop ⟳",
                TimelineClipLoopMode::PingPong => "PingPong ⇄",
            };
            if pill(ui, &mut x, cy, loop_label, PILL_BG.value(), KEY_GOLD, "dt_loop") {
                let next = match active_loop {
                    TimelineClipLoopMode::None => TimelineClipLoopMode::Loop,
                    TimelineClipLoopMode::Loop => TimelineClipLoopMode::PingPong,
                    TimelineClipLoopMode::PingPong => TimelineClipLoopMode::None,
                };
                let _ = session.timeline_set_clip_loop_mode(cid, next);
            }
            if pill(ui, &mut x, cy, "Rev", PILL_BG.value(), INK.value(), "dt_rev") {
                let _ = session.timeline_reverse_clip(cid);
            }
        }
    }

    fn draw_ruler(&mut self, ui: &mut Ui, session: &CanvasSession, tl: &TimelineState, corner: Rect, ruler: Rect) {
        ui.painter().rect_filled(corner, 0.0, argb(HEADER_BG));
        ui.painter().rect_filled(ruler, 0.0, argb(HEADER_BG));
        ui.painter().line_segment(
            [pos2(corner.right(), corner.top()), pos2(corner.right(), corner.bottom())],
            Stroke::new(1.0, argb(EDGE)),
        );
        ui.painter().line_segment(
            [pos2(corner.left(), corner.bottom()), pos2(ruler.right(), ruler.bottom())],
            Stroke::new(1.0, argb(EDGE)),
        );

        let zoom_label = format!("Tracks ({}) · Zoom", tl.tracks.len());
        text(
            ui.painter(),
            pos2(corner.left() + 8.0, corner.center().y),
            Align2::LEFT_CENTER,
            &zoom_label,
            11.0,
            Weight::SemiBold,
            argb(DIM_INK),
        );

        // Scrubbing & zoom on ruler
        let resp = ui.interact(ruler, ui.scope_id().with("dt_ruler_scrub"), Sense::click_and_drag());
        if (resp.clicked() || resp.dragged())
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.reset(session);
            let rel_x = (pos.x - ruler.left() + self.scroll_x).max(0.0);
            let max_frame = tl.duration_frames.saturating_sub(1);
            let frame = ((rel_x / self.px_per_frame).floor().max(0.0) as u32).min(max_frame);
            if frame != tl.playhead {
                session.timeline_set_playhead(frame);
            }
        }
        if resp.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            if delta.x.abs() > 0.1 {
                self.scroll_x = (self.scroll_x - delta.x).max(0.0);
            }
            if delta.y.abs() > 0.1 {
                self.px_per_frame = (self.px_per_frame * (1.0 + delta.y * 0.002)).clamp(5.0, 48.0);
            }
        }

        let painter = ui.painter().with_clip_rect(ruler);
        let fps = tl.fps.max(1);
        let total = tl.duration_frames.max(1);

        if let Some(wa) = tl.work_area.as_ref() {
            let x0 = ruler.left() + (wa.start as f32) * self.px_per_frame - self.scroll_x;
            let x1 = ruler.left() + ((wa.end + 1) as f32) * self.px_per_frame - self.scroll_x;
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, ruler.top()), pos2(x1.max(x0 + 3.0), ruler.bottom())),
                0.0,
                Color32::from_rgba_unmultiplied(14, 218, 245, 42),
            );
        }

        let step = if self.px_per_frame >= 18.0 {
            1
        } else if self.px_per_frame >= 9.0 {
            2
        } else {
            fps
        };
        let mut f = 0u32;
        while f < total {
            let x = ruler.left() + (f as f32) * self.px_per_frame - self.scroll_x;
            if x > ruler.right() + 24.0 {
                break;
            }
            if x >= ruler.left() - 24.0 {
                let is_sec = f.is_multiple_of(fps);
                let tick_h = if is_sec { 11.0 } else { 5.0 };
                painter.line_segment(
                    [pos2(x, ruler.bottom() - tick_h), pos2(x, ruler.bottom())],
                    Stroke::new(if is_sec { 1.4 } else { 0.8 }, argb(DIM_INK)),
                );
                if is_sec {
                    let s = format!("{}s", f / fps);
                    text(
                        &painter,
                        pos2(x + 3.0, ruler.top() + 7.0),
                        Align2::LEFT_CENTER,
                        &s,
                        9.5,
                        Weight::Medium,
                        argb(DIM_INK),
                    );
                }
            }
            f = f.saturating_add(step);
        }

        // Ruler Markers (click to jump, right-click to delete)
        for m in &tl.markers {
            let mx = ruler.left() + (m.frame as f32 + 0.5) * self.px_per_frame - self.scroll_x;
            if mx < ruler.left() - 16.0 || mx > ruler.right() + 16.0 {
                continue;
            }
            let mr = Rect::from_min_size(pos2(mx - 8.0, ruler.top() + 2.0), vec2(16.0, 14.0));
            painter.rect_filled(mr, 3.0, argb(0xFF00_0000 | (m.color & 0x00FF_FFFF)));
            let lbl = if m.label.is_empty() { "M" } else { &m.label };
            text(&painter, mr.center(), Align2::CENTER_CENTER, lbl, 8.5, Weight::Bold, Color32::BLACK);
            let mresp = ui.interact(mr, ui.scope_id().with(("dt_marker", m.id)), Sense::click());
            if mresp.clicked() {
                self.reset(session);
                session.timeline_set_playhead(m.frame);
            } else if mresp.secondary_clicked() {
                let _ = session.timeline_delete_marker(m.id);
            }
        }

        // Playhead indicator
        let px = ruler.left() + (tl.playhead as f32 + 0.5) * self.px_per_frame - self.scroll_x;
        painter.line_segment([pos2(px, ruler.top()), pos2(px, ruler.bottom())], Stroke::new(2.0, argb(PERFORM_RED)));
    }

    fn draw_tracks(&mut self, ui: &mut Ui, session: &CanvasSession, tl: &TimelineState, body: Rect) {
        let header_col = Rect::from_min_max(body.min, pos2(body.left() + HEADER_W, body.bottom()));
        let lane_col = Rect::from_min_max(pos2(header_col.right(), body.top()), body.max);
        ui.painter().rect_filled(header_col, 0.0, argb(HEADER_BG));
        ui.painter().rect_filled(lane_col, 0.0, argb(TRACK_BG));
        ui.painter().line_segment(
            [pos2(header_col.right(), body.top()), pos2(header_col.right(), body.bottom())],
            Stroke::new(1.0, argb(EDGE)),
        );

        let lane_clip = ui.painter().with_clip_rect(lane_col);
        let head_clip = ui.painter().with_clip_rect(header_col);

        for (row_idx, track) in tl.tracks.iter().rev().enumerate() {
            let y0 = body.top() + (row_idx as f32) * TRACK_H;
            if y0 > body.bottom() {
                break;
            }
            let h_rect = Rect::from_min_size(pos2(header_col.left(), y0), vec2(HEADER_W, TRACK_H));
            let l_rect = Rect::from_min_size(pos2(lane_col.left(), y0), vec2(lane_col.width(), TRACK_H));
            self.draw_track_row(ui, (&head_clip, &lane_clip), session, tl, track, (h_rect, l_rect));
        }

        // Continuous playhead line across all tracks
        let px = lane_col.left() + (tl.playhead as f32 + 0.5) * self.px_per_frame - self.scroll_x;
        if px >= lane_col.left() && px <= lane_col.right() {
            lane_clip.line_segment(
                [pos2(px, lane_col.top()), pos2(px, lane_col.bottom())],
                Stroke::new(2.0, argb(PERFORM_RED)),
            );
        }
    }

    fn draw_track_row(
        &mut self,
        ui: &mut Ui,
        painters: (&egui::Painter, &egui::Painter),
        session: &CanvasSession,
        tl: &TimelineState,
        track: &TimelineTrackInfo,
        rects: (Rect, Rect),
    ) {
        let (head_clip, lane_clip) = painters;
        let (h_rect, l_rect) = rects;
        let selected = tl.active_track_id == Some(track.id);
        let tag_rgb = 0xFF00_0000 | (track.color_tag & 0x00FF_FFFF);
        if selected {
            head_clip.rect_filled(h_rect, 0.0, Color32::from_rgba_unmultiplied(0, 118, 253, 36));
        }
        head_clip.rect_filled(
            Rect::from_min_size(pos2(h_rect.left() + 4.0, h_rect.top() + 5.0), vec2(3.5, h_rect.height() - 10.0)),
            2.0,
            argb(tag_rgb),
        );
        head_clip.line_segment(
            [pos2(h_rect.left(), h_rect.bottom()), pos2(h_rect.right(), h_rect.bottom())],
            Stroke::new(0.75, argb(EDGE)),
        );
        lane_clip.line_segment(
            [pos2(l_rect.left(), l_rect.bottom()), pos2(l_rect.right(), l_rect.bottom())],
            Stroke::new(0.75, argb(EDGE)),
        );

        let name_rect =
            Rect::from_min_max(pos2(h_rect.left() + 12.0, h_rect.top()), pos2(h_rect.right() - 56.0, h_rect.bottom()));
        text(
            head_clip,
            pos2(name_rect.left(), h_rect.center().y),
            Align2::LEFT_CENTER,
            &track.name,
            11.5,
            if selected { Weight::Bold } else { Weight::Medium },
            argb(if track.visible { INK } else { DIM_INK }),
        );
        if ui.interact(name_rect, ui.scope_id().with(("dt_sel_track", track.id)), Sense::click()).clicked() {
            let _ = session.timeline_select_track(track.id);
        }

        // Compact Solo (S) & Eye (V) buttons
        let solo_r = Rect::from_min_size(pos2(h_rect.right() - 48.0, h_rect.center().y - 9.0), vec2(18.0, 18.0));
        head_clip.rect_filled(solo_r, 4.0, argb(if track.solo { KEY_GOLD } else { PILL_BG.value() }));
        text(
            head_clip,
            solo_r.center(),
            Align2::CENTER_CENTER,
            "S",
            10.0,
            Weight::Bold,
            argb(if track.solo { 0xFF00_0000 } else { DIM_INK.value() }),
        );
        if ui.interact(solo_r, ui.scope_id().with(("dt_solo", track.id)), Sense::click()).clicked() {
            let _ = session.timeline_set_track_solo(track.id, !track.solo);
        }

        let vis_r = Rect::from_min_size(pos2(h_rect.right() - 26.0, h_rect.center().y - 9.0), vec2(20.0, 18.0));
        head_clip.rect_filled(vis_r, 4.0, argb(PILL_BG));
        text(
            head_clip,
            vis_r.center(),
            Align2::CENTER_CENTER,
            if track.visible { "●" } else { "○" },
            10.0,
            Weight::Bold,
            argb(if track.visible { INK } else { DIM_INK }),
        );
        if ui.interact(vis_r, ui.scope_id().with(("dt_vis", track.id)), Sense::click()).clicked() {
            let _ = session.timeline_set_track_visible(track.id, !track.visible);
        }

        // Clips in this track lane
        for clip in &track.clips {
            let x0 = l_rect.left() + (clip.start_frame as f32) * self.px_per_frame - self.scroll_x;
            let w = ((clip.duration_frames as f32) * self.px_per_frame).max(16.0);
            let cr = Rect::from_min_size(pos2(x0, l_rect.top() + 3.0), vec2(w, l_rect.height() - 6.0));
            if cr.right() < l_rect.left() || cr.left() > l_rect.right() {
                continue;
            }
            let clip_sel = tl.active_clip_id == Some(clip.id);
            let accent = match clip.kind {
                TimelineClipKind::Audio => AUDIO_GREEN,
                TimelineClipKind::Video => CAMERA_SKY,
                TimelineClipKind::Text => FILTER_PURPLE,
                _ => tag_rgb,
            };
            lane_clip.rect_filled(cr, 5.0, argb(CLIP_FILL));
            lane_clip.rect_stroke(
                cr,
                5.0,
                Stroke::new(if clip_sel { 1.6 } else { 1.0 }, argb(if clip_sel { accent } else { EDGE.value() })),
                StrokeKind::Inside,
            );
            // Top colored stripe
            lane_clip.rect_filled(Rect::from_min_size(cr.min, vec2(cr.width(), 2.5)), 2.0, argb(accent));

            let loop_suffix = match clip.loop_mode {
                TimelineClipLoopMode::None => "",
                TimelineClipLoopMode::Loop => " ⟳",
                TimelineClipLoopMode::PingPong => " ⇄",
            };
            let title = format!("{}{}", clip.name, loop_suffix);
            text(
                lane_clip,
                pos2(cr.left() + 6.0, cr.center().y - 1.0),
                Align2::LEFT_CENTER,
                &title,
                10.5,
                Weight::Medium,
                argb(INK),
            );

            // Draw keyframe diamonds along the clip's bottom strip
            draw_kf_dots(
                lane_clip,
                l_rect.left(),
                self.scroll_x,
                self.px_per_frame,
                cr.bottom() - 4.5,
                &clip.transform_keyframes,
                KEY_GOLD,
            );
            draw_kf_dots(
                lane_clip,
                l_rect.left(),
                self.scroll_x,
                self.px_per_frame,
                cr.bottom() - 4.5,
                &clip.opacity_keyframes,
                OPACITY_CYAN,
            );
            draw_kf_dots(
                lane_clip,
                l_rect.left(),
                self.scroll_x,
                self.px_per_frame,
                cr.bottom() - 4.5,
                &clip.filter_keyframes,
                FILTER_PURPLE,
            );
            draw_kf_dots(
                lane_clip,
                l_rect.left(),
                self.scroll_x,
                self.px_per_frame,
                cr.bottom() - 4.5,
                &clip.deform_keyframes,
                WARP_PINK,
            );

            let cresp =
                ui.interact(cr.intersect(l_rect), ui.scope_id().with(("dt_clip", clip.id)), Sense::click_and_drag());
            if cresp.clicked() {
                let _ = session.timeline_select_clip(clip.id);
            } else if cresp.dragged() {
                let dx_frames = (cresp.drag_delta().x / self.px_per_frame).round() as i32;
                if dx_frames != 0 {
                    let next_start = (clip.start_frame as i32 + dx_frames).max(0) as u32;
                    let _ = session.timeline_move_clip(clip.id, next_start, None, false);
                }
            } else if cresp.drag_stopped() {
                let _ = session.timeline_move_clip(clip.id, clip.start_frame, None, true);
            }
        }
    }

    fn show_flipbook(&mut self, ctx: &egui::Context, session: &CanvasSession, tl: &TimelineState, screen: Rect) {
        let w = screen.width().min(680.0);
        let h = 88.0;
        let bar = Rect::from_min_size(pos2(screen.center().x - w * 0.5, screen.bottom() - h - 14.0), vec2(w, h));
        egui::Area::new(Id::unique("dreams_flipbook_dock")).fixed_pos(bar.min).order(Order::Middle).show(ctx, |ui| {
            ui.allocate_rect(bar, Sense::click_and_drag());
            ui.painter().rect_filled(bar, 18.0, argb(SURFACE));
            ui.painter().rect_stroke(bar, 18.0, Stroke::new(1.0, argb(EDGE)), StrokeKind::Inside);

            let mut x = bar.left() + 12.0;
            let top_y = bar.top() + 18.0;
            if pill(ui, &mut x, top_y, "Timeline", ACCENT, 0xFFFF_FFFF, "fb_expand") {
                session.timeline_set_flipbook(false);
            }
            let playing = self.playing.is_some();
            if pill(
                ui,
                &mut x,
                top_y,
                if playing { "Pause" } else { "Play" },
                if playing { PERFORM_RED } else { ACCENT },
                0xFFFF_FFFF,
                "fb_play",
            ) {
                self.toggle_play(session, tl);
            }
            let tc = format!("F{}/{}", tl.playhead + 1, tl.duration_frames);
            let tr =
                text(ui.painter(), pos2(x + 2.0, top_y), Align2::LEFT_CENTER, &tc, 11.5, Weight::SemiBold, argb(INK));
            x = tr.right() + 8.0;
            let onion_bg = if tl.onion.enabled { KEY_GOLD } else { PILL_BG.value() };
            let onion_fg = if tl.onion.enabled { 0xFF00_0000 } else { DIM_INK.value() };
            if pill(ui, &mut x, top_y, "Onion", onion_bg, onion_fg, "fb_onion") {
                let mut next = tl.onion.clone();
                next.enabled = !next.enabled;
                session.timeline_set_onion_skin(next);
            }

            // Active cel strip
            let strip_y = bar.top() + 38.0;
            if let Some(clip) = active_clip(tl).filter(|c| c.kind == TimelineClipKind::Cel) {
                let mut cx = bar.left() + 12.0;
                let mut f_cursor = clip.start_frame;
                let mut active_idx = 0usize;
                for (i, cel) in clip.cels.iter().enumerate() {
                    let dur = cel.duration_frames.max(1);
                    let is_curr = tl.playhead >= f_cursor && tl.playhead < f_cursor + dur;
                    if is_curr {
                        active_idx = i;
                    }
                    let cel_r = Rect::from_min_size(pos2(cx, strip_y), vec2(44.0, 38.0));
                    if cel_r.right() < bar.right() - 160.0 {
                        ui.painter().rect_filled(cel_r, 8.0, argb(CLIP_FILL));
                        ui.painter().rect_stroke(
                            cel_r,
                            8.0,
                            Stroke::new(
                                if is_curr { 2.0 } else { 1.0 },
                                argb(if is_curr { ACCENT } else { EDGE.value() }),
                            ),
                            StrokeKind::Inside,
                        );
                        let lbl = if cel.hold > 0 { format!("{} +{}", i + 1, cel.hold) } else { format!("{}", i + 1) };
                        text(
                            ui.painter(),
                            cel_r.center(),
                            Align2::CENTER_CENTER,
                            &lbl,
                            11.0,
                            Weight::SemiBold,
                            argb(INK),
                        );
                        if ui.interact(cel_r, ui.scope_id().with(("fb_cel", clip.id, i)), Sense::click()).clicked() {
                            self.reset(session);
                            session.timeline_set_playhead(f_cursor);
                        }
                        cx += 50.0;
                    }
                    f_cursor = f_cursor.saturating_add(dur);
                }

                let mut rx = bar.right() - 150.0;
                let ry = strip_y + 19.0;
                if pill(ui, &mut rx, ry, "+Cel", ACCENT, 0xFFFF_FFFF, "fb_add_cel") {
                    let _ = session.timeline_add_cel(clip.id);
                }
                if pill(ui, &mut rx, ry, "Dup", PILL_BG.value(), INK.value(), "fb_dup_cel") {
                    let _ = session.timeline_duplicate_cel(clip.id, active_idx as u32);
                }
                if clip.cels.len() > 1 && pill(ui, &mut rx, ry, "Del", PILL_BG.value(), PERFORM_RED, "fb_del_cel") {
                    let _ = session.timeline_delete_cel(clip.id, active_idx as u32);
                }
            }
        });
    }
}

fn active_clip(tl: &TimelineState) -> Option<&TimelineClipInfo> {
    let cid = tl.active_clip_id?;
    tl.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == cid)
}

fn draw_kf_dots(
    painter: &egui::Painter,
    lane_left: f32,
    scroll_x: f32,
    px_per_frame: f32,
    y: f32,
    kfs: &[procraft_core::TimelineKeyframeInfo],
    color: u32,
) {
    let c = argb(color);
    for kf in kfs {
        let x = lane_left + (kf.frame as f32 + 0.5) * px_per_frame - scroll_x;
        let pts = vec![pos2(x, y - 3.5), pos2(x + 3.5, y), pos2(x, y + 3.5), pos2(x - 3.5, y)];
        painter.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
    }
}

fn pill(ui: &mut Ui, x: &mut f32, cy: f32, label: &str, bg: u32, fg: u32, id: &str) -> bool {
    let pad_x = 8.0;
    let h = 22.0;
    let w = (label.chars().count() as f32 * 6.5 + pad_x * 2.0).max(24.0);
    let r = Rect::from_min_size(pos2(*x, cy - h * 0.5), vec2(w, h));
    *x = r.right() + 5.0;
    let bg_c = if look::light() && bg == PILL_BG.0 { argb(PILL_BG.1) } else { argb(bg) };
    ui.painter().rect_filled(r, 7.0, bg_c);
    text(ui.painter(), r.center(), Align2::CENTER_CENTER, label, 11.0, Weight::SemiBold, argb(fg));
    ui.interact(r, ui.scope_id().with(id), Sense::click()).clicked()
}
