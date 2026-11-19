package io.github.artcraftandroid.procraft.ui

import android.view.HapticFeedbackConstants
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.DreamsWorkMode
import io.github.artcraftandroid.procraft.core.AnimationPlayback
import io.github.artcraftandroid.procraft.core.TimelineClipInfo
import io.github.artcraftandroid.procraft.core.TimelineClipKind
import io.github.artcraftandroid.procraft.core.TimelineEasingKind
import io.github.artcraftandroid.procraft.core.TimelineKeyframeInfo
import io.github.artcraftandroid.procraft.core.TimelineState
import io.github.artcraftandroid.procraft.core.TimelineTrackInfo
import io.github.artcraftandroid.procraft.core.TimelineTransformInfo
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.roundToInt
import kotlin.math.sin

// ---- Pure Layout, Timecode, Pinch-Zoom, and Snap Math (Unit-Tested) ----

internal const val MIN_TIME_ZOOM_DP = 8f
internal const val MAX_TIME_ZOOM_DP = 96f
internal const val MIN_TRACK_HEIGHT_DP = 38f
internal const val MAX_TRACK_HEIGHT_DP = 110f
internal const val MIN_SPLIT_RATIO = 0.22f
internal const val MAX_SPLIT_RATIO = 0.68f
internal const val FLIPBOOK_COLLAPSE_THRESHOLD = 0.16f

/** Outcome of dragging the split handle on the center action bar. */
internal data class SplitDragOutcome(
    val ratio: Float,
    val enterFlipbook: Boolean,
)

/** Formats a 0-based [frame] index at [fps] into `MM:SS:FF` timecode. */
internal fun formatTimecode(frame: Int, fps: Int): String {
    val safeFps = fps.coerceIn(1, 60)
    val safeFrame = frame.coerceAtLeast(0)
    val ff = safeFrame % safeFps
    val totalSeconds = safeFrame / safeFps
    val ss = totalSeconds % 60
    val mm = totalSeconds / 60
    return "%02d:%02d:%02d".format(mm, ss, ff)
}

/** Converts a [frame] number to horizontal pixel offset given [zoomPxPerFrame] and [scrollPx]. */
internal fun frameToPixel(frame: Float, zoomPxPerFrame: Float, scrollPx: Float): Float {
    val z = if (zoomPxPerFrame.isFinite()) zoomPxPerFrame.coerceAtLeast(1f) else 24f
    val s = if (scrollPx.isFinite()) scrollPx else 0f
    return frame * z - s
}

/** Converts a horizontal touch position [xPx] to a clamped frame index in `0 until maxFrames`. */
internal fun pixelToFrame(xPx: Float, zoomPxPerFrame: Float, scrollPx: Float, maxFrames: Int): Int {
    val limit = maxFrames.coerceAtLeast(1)
    if (!xPx.isFinite()) return 0
    val z = if (zoomPxPerFrame.isFinite()) zoomPxPerFrame.coerceAtLeast(1f) else 24f
    val s = if (scrollPx.isFinite()) scrollPx else 0f
    val raw = ((xPx + s) / z).toInt()
    return raw.coerceIn(0, limit - 1)
}

/** Horizontal 2-finger pinch zoom update for the timeline time axis (`dp per frame`). */
internal fun pinchTimeZoom(
    currentZoomDp: Float,
    scaleFactor: Float,
    minZoomDp: Float = MIN_TIME_ZOOM_DP,
    maxZoomDp: Float = MAX_TIME_ZOOM_DP,
): Float {
    val base = if (currentZoomDp.isFinite()) currentZoomDp.coerceIn(minZoomDp, maxZoomDp) else 24f
    if (!scaleFactor.isFinite() || scaleFactor <= 0f) return base
    return (base * scaleFactor).coerceIn(minZoomDp, maxZoomDp)
}

/** Vertical 2-finger pinch zoom update for track lane height (`dp`). */
internal fun pinchTrackHeight(
    currentHeightDp: Float,
    scaleFactor: Float,
    minHeightDp: Float = MIN_TRACK_HEIGHT_DP,
    maxHeightDp: Float = MAX_TRACK_HEIGHT_DP,
): Float {
    val base = if (currentHeightDp.isFinite()) currentHeightDp.coerceIn(minHeightDp, maxHeightDp) else 54f
    if (!scaleFactor.isFinite() || scaleFactor <= 0f) return base
    return (base * scaleFactor).coerceIn(minHeightDp, maxHeightDp)
}

/**
 * Snaps [rawFrame] to the nearest boundary in [candidates] (e.g. playhead, adjacent clip start/end,
 * or work area markers) when within [snapThresholdFrames].
 */
internal fun snapClipFrame(rawFrame: Int, candidates: List<Int>, snapThresholdFrames: Int = 2): Int {
    val clamped = rawFrame.coerceAtLeast(0)
    if (snapThresholdFrames <= 0 || candidates.isEmpty()) return clamped
    var best = clamped
    var bestDist = snapThresholdFrames + 1
    for (c in candidates) {
        val d = abs(clamped - c)
        if (d <= snapThresholdFrames && d < bestDist) {
            bestDist = d
            best = c.coerceAtLeast(0)
        }
    }
    return best
}

/**
 * Computes the new split ratio when dragging the Center Action Bar by [deltaY] (positive = down)
 * inside a container of [containerHeight] px. Pulling down below [FLIPBOOK_COLLAPSE_THRESHOLD]
 * automatically collapses the multi-track timeline into **Flipbook Mode**.
 */
internal fun splitViewHandleDrag(
    currentRatio: Float,
    deltaY: Float,
    containerHeight: Float,
): SplitDragOutcome {
    if (!containerHeight.isFinite() || containerHeight <= 1f || !deltaY.isFinite()) {
        return SplitDragOutcome(currentRatio.coerceIn(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO), false)
    }
    val next = currentRatio - (deltaY / containerHeight)
    if (next < FLIPBOOK_COLLAPSE_THRESHOLD) {
        return SplitDragOutcome(
            ratio = currentRatio.coerceIn(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO),
            enterFlipbook = true,
        )
    }
    return SplitDragOutcome(
        ratio = next.coerceIn(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO),
        enterFlipbook = false,
    )
}

// ---- Theme Tokens for DreamsCraft Workspace ----

private val CenterBarHeight = 46.dp
private val RulerHeight = 30.dp
private val TrackHeaderWidth = 148.dp
private val KeySubTrackHeight = 24.dp

private val DreamsSurface get() = if (Pc.light) Color(0xFFEAEAEA) else Color(0xFF1A1A1D)
private val DreamsBarFill get() = if (Pc.light) Color(0xFFDCDCE0) else Color(0xFF242429)
private val DreamsTrackBg get() = if (Pc.light) Color(0xFFE2E2E6) else Color(0xFF141417)
private val DreamsHeaderBg get() = if (Pc.light) Color(0xFFD5D5DA) else Color(0xFF202025)
private val DreamsClipFill get() = if (Pc.light) Color(0xFFF7F7FA) else Color(0xFF2E2E36)
private val DreamsInk get() = if (Pc.light) Color(0xFF222225) else Color(0xFFEAEAF0)
private val DreamsDimInk get() = if (Pc.light) Color(0xFF686870) else Color(0xFF9696A0)
private val DreamsEdge get() = if (Pc.light) Color(0xFFBDBDC4) else Color(0xFF383842)
private val PerformRed = Color(0xFFFF2D55)
private val KeyframeGold = Color(0xFFFFB800)
private val OpacityCyan = Color(0xFF06B6D4)

/**
 * Root DreamsCraft Animation Workspace overlay:
 * - When `timeline == null`, falls back to legacy `AnimationBar` (for Page Assist).
 * - When `timeline.flipbookOpen == true`, maximizes the Stage and shows the floating Flipbook dock
 *   with a 1-tap expand button back to the Multi-Track Timeline.
 * - Otherwise, shows the Stage above the timeline:
 *   1. Stage Backstage & Non-Destructive Keyframe/Perform Gizmo on top
 *   2. Draggable **Center Action Bar** in the middle
 *   3. Touch-first **Multi-Track Dopesheet & KeyTracks** on the bottom
 */
@Composable
fun BoxScope.DreamsWorkspaceOverlay(
    vm: CanvasViewModel,
    windowWidth: Dp,
    windowHeight: Dp,
) {
    val tl = vm.timeline
    if (tl == null) {
        if (vm.animation != null) {
            LaunchedEffect(Unit) { vm.updateStageBottomInset(0f) }
            AnimationBar(vm)
        } else {
            LaunchedEffect(Unit) { vm.updateStageBottomInset(0f) }
        }
        return
    }

    val density = LocalDensity.current
    val totalHeightPx = with(density) { windowHeight.toPx() }
    val timelineHeightDp = (windowHeight * vm.timelineSplitRatio).coerceIn(140.dp, windowHeight * 0.70f)
    val dockBottomPx = with(density) {
        if (tl.flipbookOpen) 126.dp.toPx() else (timelineHeightDp + CenterBarHeight).toPx()
    }

    LaunchedEffect(dockBottomPx, tl.flipbookOpen) {
        vm.updateStageBottomInset(dockBottomPx)
    }

    // 1. Stage Backstage Frame & Interactive Keyframe / Live Perform Gizmo in the upper Stage area
    val stageHeightDp = if (tl.flipbookOpen) windowHeight else (windowHeight - timelineHeightDp - CenterBarHeight)
    if (vm.dreamsMode == DreamsWorkMode.PERFORM || vm.stageGizmoActive) {
        StageKeyTransformOverlay(
            vm = vm,
            tl = tl,
            stageWidth = windowWidth,
            stageHeight = stageHeightDp.coerceAtLeast(120.dp),
        )
    }

    // 2. Flipbook Mode vs Full Multi-Track Split View
    if (tl.flipbookOpen) {
        Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) {
            AnimationBar(vm)
            // Floating Pill above the Flipbook bar to return to the Multi-Track Timeline or toggle Onion Skin
            Row(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 126.dp)
                    .clip(RoundedCornerShape(20.dp))
                    .background(DreamsBarFill.copy(alpha = 0.95f))
                    .border(1.dp, DreamsEdge, RoundedCornerShape(20.dp))
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = "Timeline",
                    color = Pc.Accent,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.clickable { vm.setFlipbookMode(false) },
                )
                Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))
                Text(
                    text = if (tl.onion.enabled) "Onion Skin: ON" else "Onion Skin: OFF",
                    color = if (tl.onion.enabled) KeyframeGold else DreamsDimInk,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.clickable {
                        vm.updateTimelineOnion { it.copy(enabled = !it.enabled) }
                    },
                )
                Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))
                Text(
                    text = formatTimecode(tl.playhead.toInt(), tl.fps.toInt()),
                    color = DreamsInk,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
        return
    }

    // 3. Stage and multitrack timeline with the center action bar
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .fillMaxWidth()
            .height(timelineHeightDp + CenterBarHeight)
            .background(DreamsSurface)
            // Consume all touches so no accidental strokes reach the Stage underneath
            .pointerInput(Unit) { detectTapGestures { } },
    ) {
        CenterActionBar(
            vm = vm,
            tl = tl,
            totalHeightPx = totalHeightPx,
        )
        MultiTrackDopesheet(
            vm = vm,
            tl = tl,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
        )
    }

    // Popovers standing above the Center Action Bar
    if (vm.movieSettingsOpen) {
        MovieSettingsPopover(
            vm = vm,
            tl = tl,
            bottomOffset = timelineHeightDp + CenterBarHeight + 6.dp,
        )
    }
    if (vm.addTrackMenuOpen) {
        TimelineAddMenuPopover(
            vm = vm,
            tl = tl,
            bottomOffset = timelineHeightDp + CenterBarHeight + 6.dp,
        )
    }
    if (vm.playheadActionMenuOpen) {
        PlayheadActionPopover(
            vm = vm,
            tl = tl,
            bottomOffset = timelineHeightDp + CenterBarHeight + 6.dp,
        )
    }
    vm.editingKeyframe?.let { (clipId, frame) ->
        KeyframeEasingPopover(
            vm = vm,
            tl = tl,
            clipId = clipId,
            frame = frame.toInt(),
            bottomOffset = timelineHeightDp + CenterBarHeight + 6.dp,
        )
    }
}

/**
 * The center action bar between the Stage and the timeline:
 * - Left: Play/Pause + Timecode (`MM:SS:FF` & FPS) + Onion Skin badge
 * - Center: Draggable Split Handle (pull down to collapse into Flipbook!) + Flipbook button
 * - Right: Mode switches (`Draw & Paint`, `Perform (Rec)`, `Timeline Edit`, `+ Add`)
 */
@Composable
private fun CenterActionBar(
    vm: CanvasViewModel,
    tl: TimelineState,
    totalHeightPx: Float,
) {
    val view = LocalView.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(CenterBarHeight)
            .background(DreamsBarFill)
            .border(0.5.dp, DreamsEdge)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Left: Play/Pause + Timecode + Quick Onion Toggle
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val isPlaying = vm.playingFrame != null
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (isPlaying) Pc.Accent else DreamsClipFill)
                    .clickable {
                        view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                        vm.togglePlaying()
                    }
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            ) {
                Text(
                    text = if (isPlaying) "Pause" else "Play",
                    color = if (isPlaying) Color.White else DreamsInk,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Timecode Pill -> opens Movie & Onion Skin Settings
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (vm.movieSettingsOpen) Pc.Accent.copy(alpha = 0.22f) else DreamsClipFill)
                    .border(
                        1.dp,
                        if (vm.movieSettingsOpen) Pc.Accent else DreamsEdge,
                        RoundedCornerShape(10.dp),
                    )
                    .clickable {
                        vm.movieSettingsOpen = !vm.movieSettingsOpen
                        vm.addTrackMenuOpen = false
                        vm.playheadActionMenuOpen = false
                    }
                    .padding(horizontal = 10.dp, vertical = 5.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    text = formatTimecode(tl.playhead.toInt(), tl.fps.toInt()),
                    color = DreamsInk,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "${tl.fps}fps",
                    color = DreamsDimInk,
                    fontSize = 11.sp,
                )
            }

            // Quick Onion Skin pill
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (tl.onion.enabled) KeyframeGold.copy(alpha = 0.2f) else Color.Transparent)
                    .border(1.dp, if (tl.onion.enabled) KeyframeGold else DreamsEdge, RoundedCornerShape(8.dp))
                    .clickable {
                        vm.updateTimelineOnion { it.copy(enabled = !it.enabled) }
                    }
                    .padding(horizontal = 8.dp, vertical = 5.dp),
            ) {
                Text(
                    text = "Onion",
                    color = if (tl.onion.enabled) KeyframeGold else DreamsDimInk,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }

        // Center: Draggable Split Handle (resizes Stage/Timeline; pull down to enter Flipbook!)
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight()
                .draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { deltaY ->
                        val outcome = splitViewHandleDrag(vm.timelineSplitRatio, deltaY, totalHeightPx)
                        vm.timelineSplitRatio = outcome.ratio
                        if (outcome.enterFlipbook) {
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                            vm.setFlipbookMode(true)
                        }
                    },
                ),
            contentAlignment = Alignment.Center,
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box(
                    modifier = Modifier
                        .width(42.dp)
                        .height(5.dp)
                        .clip(CircleShape)
                        .background(DreamsDimInk.copy(alpha = 0.6f)),
                )
                Text(
                    text = "Flipbook",
                    color = DreamsDimInk,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { vm.setFlipbookMode(true) }
                        .padding(horizontal = 6.dp, vertical = 3.dp),
                )
            }
        }

        // Right: Workspace mode controls (Draw & Paint, Perform, Timeline Edit, + Add)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            ModeChip(
                label = "Draw",
                selected = vm.dreamsMode == DreamsWorkMode.DRAW_PAINT && !vm.stageGizmoActive,
                activeColor = Pc.Accent,
                onClick = {
                    vm.dreamsMode = DreamsWorkMode.DRAW_PAINT
                    vm.stageGizmoActive = false
                    if (vm.performRecording) vm.finishPerformRecording()
                },
            )
            ModeChip(
                label = if (vm.performRecording) "● REC" else "Perform",
                selected = vm.dreamsMode == DreamsWorkMode.PERFORM,
                activeColor = PerformRed,
                onClick = {
                    if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                        if (vm.performRecording) vm.finishPerformRecording()
                        vm.dreamsMode = DreamsWorkMode.DRAW_PAINT
                        vm.stageGizmoActive = false
                    } else {
                        vm.dreamsMode = DreamsWorkMode.PERFORM
                        vm.stageGizmoActive = true
                    }
                },
            )
            ModeChip(
                label = "Move",
                selected = vm.stageGizmoActive && vm.dreamsMode != DreamsWorkMode.PERFORM,
                activeColor = KeyframeGold,
                onClick = {
                    if (vm.performRecording) vm.finishPerformRecording()
                    vm.dreamsMode = DreamsWorkMode.TIMELINE
                    vm.stageGizmoActive = !vm.stageGizmoActive
                },
            )
            ModeChip(
                label = "Edit",
                selected = vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT,
                activeColor = Pc.Accent,
                onClick = {
                    if (vm.performRecording) vm.finishPerformRecording()
                    vm.stageGizmoActive = false
                    vm.dreamsMode = if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT) {
                        DreamsWorkMode.DRAW_PAINT
                    } else {
                        DreamsWorkMode.TIMELINE_EDIT
                    }
                },
            )
            if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT && vm.selectedTrackIds.size >= 2) {
                ModeChip(
                    label = "Group (${vm.selectedTrackIds.size})",
                    selected = true,
                    activeColor = Color(0xFF8B5CF6),
                    onClick = { vm.groupSelectedTracks() },
                )
            }
            // + Add Button
            Box(
                modifier = Modifier
                    .size(32.dp)
                    .clip(CircleShape)
                    .background(if (vm.addTrackMenuOpen) Pc.Accent else DreamsClipFill)
                    .border(1.dp, DreamsEdge, CircleShape)
                    .clickable {
                        vm.addTrackMenuOpen = !vm.addTrackMenuOpen
                        vm.movieSettingsOpen = false
                        vm.playheadActionMenuOpen = false
                    },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "+",
                    color = if (vm.addTrackMenuOpen) Color.White else DreamsInk,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
}

@Composable
private fun ModeChip(
    label: String,
    selected: Boolean,
    activeColor: Color,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) activeColor else DreamsClipFill)
            .border(1.dp, if (selected) activeColor else DreamsEdge, RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) {
        Text(
            text = label,
            color = if (selected) Color.White else DreamsInk,
            fontSize = 13.sp,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
        )
    }
}

/**
 * Multi-track Dopesheet with:
 * - 2-finger horizontal pinch (time zoom) & 2-finger vertical pinch (track height zoom)
 * - Scrubbable Time Ruler with draggable Work Area In/Out markers
 * - Track Headers on the left + Clips, Cels, and KeyTracks (`Move` and `Opacity`) on the right
 * - Playhead vertical line with interactive Clapperboard **Action Button**
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MultiTrackDopesheet(
    vm: CanvasViewModel,
    tl: TimelineState,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val view = LocalView.current
    val zoomDp = vm.timelineZoomPxPerFrame.dp
    val zoomPx = with(density) { zoomDp.toPx().coerceAtLeast(2f) }
    val trackHeight = vm.timelineTrackHeightDp.dp
    val totalFrames = tl.durationFrames.toInt().coerceAtLeast(1)
    val contentWidthDp = (zoomDp * totalFrames).coerceAtLeast(600.dp) + 120.dp
    val horizontalScroll = rememberScrollState()
    val verticalScroll = rememberScrollState()

    Column(
        modifier = modifier
            .background(DreamsTrackBg)
            // 2-finger pinch gesture: horizontal spread zooms time, vertical spread zooms track height
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                        val pressed = event.changes.filter { it.pressed }
                        if (pressed.size >= 2) {
                            val p0 = pressed[0]
                            val p1 = pressed[1]
                            val prevDx = abs(p0.previousPosition.x - p1.previousPosition.x)
                            val currDx = abs(p0.position.x - p1.position.x)
                            val prevDy = abs(p0.previousPosition.y - p1.previousPosition.y)
                            val currDy = abs(p0.position.y - p1.position.y)
                            if (prevDx > 24f && currDx > 24f && abs(currDx - prevDx) >= abs(currDy - prevDy)) {
                                vm.timelineZoomPxPerFrame = pinchTimeZoom(vm.timelineZoomPxPerFrame, currDx / prevDx)
                                p0.consume()
                                p1.consume()
                            } else if (prevDy > 24f && currDy > 24f) {
                                vm.timelineTrackHeightDp = pinchTrackHeight(vm.timelineTrackHeightDp, currDy / prevDy)
                                p0.consume()
                                p1.consume()
                            }
                        }
                    } while (event.changes.any { it.pressed })
                }
            },
    ) {
        // Top Row: Track Column Header + Scrubbable Time Ruler
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(RulerHeight)
                .background(DreamsHeaderBg)
                .border(0.5.dp, DreamsEdge),
        ) {
            // Left Header Corner: Step Prev / Next Frame + Action Button trigger
            Row(
                modifier = Modifier
                    .width(TrackHeaderWidth)
                    .fillMaxHeight()
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    text = "◀",
                    color = DreamsInk,
                    fontSize = 13.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { vm.stepTimelineFrame(-1) }
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
                Text(
                    text = "F ${tl.playhead + 1u}/${tl.durationFrames}",
                    color = DreamsDimInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "▶",
                    color = DreamsInk,
                    fontSize = 13.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { vm.stepTimelineFrame(1) }
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }

            Box(Modifier.width(1.dp).fillMaxHeight().background(DreamsEdge))

            // Right: Time Ruler with 1-finger scrubbing & Work Area highlight
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .horizontalScroll(horizontalScroll)
                    .pointerInput(totalFrames, zoomPx) {
                        detectTapGestures { offset ->
                            val frame = pixelToFrame(offset.x, zoomPx, 0f, totalFrames)
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.setTimelinePlayhead(frame)
                        }
                    }
                    .pointerInput(totalFrames, zoomPx) {
                        detectHorizontalDragGestures(
                            onDragStart = { offset ->
                                vm.stopPlaying()
                                val frame = pixelToFrame(offset.x, zoomPx, 0f, totalFrames)
                                vm.setTimelinePlayhead(frame)
                            },
                            onHorizontalDrag = { change, _ ->
                                change.consume()
                                val frame = pixelToFrame(change.position.x, zoomPx, 0f, totalFrames)
                                if (frame != vm.timeline?.playhead?.toInt()) {
                                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                    vm.setTimelinePlayhead(frame)
                                }
                            },
                        )
                    },
            ) {
                val fps = tl.fps.toInt().coerceAtLeast(1)
                val playheadFrame = tl.playhead.toInt()
                val workArea = tl.workArea
                Canvas(
                    modifier = Modifier
                        .width(contentWidthDp)
                        .fillMaxHeight(),
                ) {
                    // Draw Work Area In/Out range highlight if active
                    if (workArea != null) {
                        val wx0 = workArea.start.toFloat() * zoomPx
                        val wx1 = (workArea.end + 1u).toFloat() * zoomPx
                        drawRect(
                            color = Pc.Accent.copy(alpha = 0.20f),
                            topLeft = Offset(wx0, 0f),
                            size = Size((wx1 - wx0).coerceAtLeast(4f), size.height),
                        )
                    }

                    // Frame ticks & second markers
                    val step = when {
                        zoomPx >= 28f -> 1
                        zoomPx >= 14f -> 2
                        zoomPx >= 7f -> 5
                        else -> fps
                    }
                    var f = 0
                    while (f < totalFrames) {
                        val x = f * zoomPx
                        val isSecond = f % fps == 0
                        val tickH = if (isSecond) size.height * 0.55f else size.height * 0.28f
                        drawLine(
                            color = if (isSecond) Color(0xFFA0A0AC) else Color(0xFF555560),
                            start = Offset(x, size.height - tickH),
                            end = Offset(x, size.height),
                            strokeWidth = if (isSecond) 2f else 1f,
                        )
                        f += step
                    }

                    // Playhead marker on the ruler
                    val px = playheadFrame * zoomPx + zoomPx * 0.5f
                    drawCircle(
                        color = PerformRed,
                        radius = 7f,
                        center = Offset(px, size.height * 0.45f),
                    )
                    drawLine(
                        color = PerformRed,
                        start = Offset(px, size.height * 0.45f),
                        end = Offset(px, size.height),
                        strokeWidth = 3f,
                    )
                }

                // Button on the playhead that opens its actions menu
                val playheadDp = zoomDp * tl.playhead.toInt()
                Box(
                    modifier = Modifier
                        .offset(x = (playheadDp - 24.dp).coerceAtLeast(0.dp), y = 2.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(PerformRed)
                        .clickable {
                            vm.playheadActionMenuOpen = !vm.playheadActionMenuOpen
                            vm.movieSettingsOpen = false
                            vm.addTrackMenuOpen = false
                        }
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                ) {
                    Text(
                        text = "Action ▾",
                        color = Color.White,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }

        // Tracks Body (Displayed Top-to-Bottom, i.e., reversed from bottom-to-top compositing order)
        val tracksTopDown = remember(tl.tracks) { tl.tracks.asReversed() }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(verticalScroll),
        ) {
            // Left Column: Track Headers
            Column(
                modifier = Modifier
                    .width(TrackHeaderWidth)
                    .background(DreamsHeaderBg),
            ) {
                for (track in tracksTopDown) {
                    TrackHeaderRow(
                        vm = vm,
                        tl = tl,
                        track = track,
                        trackHeight = trackHeight,
                    )
                }
                Spacer(Modifier.height(36.dp))
            }

            Box(Modifier.width(1.dp).fillMaxHeight().background(DreamsEdge))

            // Right Column: Scrollable Track Lanes, Clips, Cels, KeyTracks & Playhead Line
            Box(
                modifier = Modifier
                    .weight(1f)
                    .horizontalScroll(horizontalScroll),
            ) {
                Column(modifier = Modifier.width(contentWidthDp)) {
                    for (track in tracksTopDown) {
                        TrackLaneRow(
                            vm = vm,
                            tl = tl,
                            track = track,
                            trackHeight = trackHeight,
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                        )
                    }
                    Spacer(Modifier.height(36.dp))
                }

                // Continuous Playhead Line across all tracks
                val playheadX = zoomDp * tl.playhead.toInt() + zoomDp * 0.5f
                Box(
                    modifier = Modifier
                        .offset(x = playheadX)
                        .width(2.dp)
                        .height((trackHeight + KeySubTrackHeight * 2) * tracksTopDown.size.coerceAtLeast(1) + 36.dp)
                        .background(PerformRed),
                )
            }
        }
    }
}

@Composable
private fun TrackHeaderRow(
    vm: CanvasViewModel,
    tl: TimelineState,
    track: TimelineTrackInfo,
    trackHeight: Dp,
) {
    val isSelected = tl.activeTrackId == track.id || vm.selectedTrackIds.contains(track.id)
    val hasKeySubTracks = track.expanded && (
        track.transformKeyframes.isNotEmpty() ||
            track.opacityKeyframes.isNotEmpty() ||
            track.clips.any { it.transformKeyframes.isNotEmpty() || it.opacityKeyframes.isNotEmpty() }
        )
    val totalRowHeight = trackHeight + if (hasKeySubTracks) KeySubTrackHeight * 2 else 0.dp
    val tagColor = Color(0xFF000000L or (track.colorTag.toLong() and 0xFFFFFFL))

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(totalRowHeight)
            .background(if (isSelected) Pc.Accent.copy(alpha = 0.16f) else DreamsHeaderBg)
            .border(0.5.dp, DreamsEdge)
            .clickable {
                if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT) {
                    vm.toggleTrackMultiSelection(track.id)
                } else {
                    vm.selectTimelineTrack(track.id)
                }
            },
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(trackHeight)
                .padding(start = if (track.parentGroup != null) 12.dp else 4.dp, end = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Color accent bar
            Box(
                modifier = Modifier
                    .width(4.dp)
                    .height(trackHeight - 10.dp)
                    .clip(CircleShape)
                    .background(tagColor),
            )
            Spacer(Modifier.width(6.dp))

            // Expand/Collapse KeyTracks or Group chevron
            Text(
                text = if (track.expanded) "▾" else "▸",
                color = DreamsDimInk,
                fontSize = 12.sp,
                modifier = Modifier
                    .clickable { vm.toggleTrackExpanded(track.id, !track.expanded) }
                    .padding(2.dp),
            )

            // Track Name
            Column(modifier = Modifier.weight(1f).padding(horizontal = 4.dp)) {
                Text(
                    text = if (track.isGroup) "📁 ${track.name}" else track.name,
                    color = if (track.visible) DreamsInk else DreamsDimInk,
                    fontSize = 12.sp,
                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = "${(track.opacity * 100).roundToInt()}%",
                    color = DreamsDimInk,
                    fontSize = 10.sp,
                )
            }

            // Solo toggle ("S")
            Box(
                modifier = Modifier
                    .size(20.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (track.solo) KeyframeGold else Color.Transparent)
                    .clickable { vm.toggleTrackSolo(track.id, !track.solo) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "S",
                    color = if (track.solo) Color.Black else DreamsDimInk,
                    fontSize = 10.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Lock toggle
            Box(
                modifier = Modifier
                    .size(20.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .clickable { vm.toggleTrackLocked(track.id, !track.locked) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = if (track.locked) "🔒" else "🔓",
                    fontSize = 10.sp,
                )
            }

            // Visibility toggle
            Box(
                modifier = Modifier
                    .size(22.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .clickable { vm.toggleTrackVisible(track.id, !track.visible) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = if (track.visible) "👁" else "—",
                    color = if (track.visible) DreamsInk else DreamsDimInk,
                    fontSize = 11.sp,
                )
            }
        }

        if (hasKeySubTracks) {
            // Sub-track labels for Move & Opacity keyframes
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(KeySubTrackHeight)
                    .background(DreamsTrackBg.copy(alpha = 0.5f))
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("◆ Move & Scale", color = KeyframeGold, fontSize = 10.sp, fontWeight = FontWeight.Medium)
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(KeySubTrackHeight)
                    .background(DreamsTrackBg.copy(alpha = 0.7f))
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("◆ Opacity", color = OpacityCyan, fontSize = 10.sp, fontWeight = FontWeight.Medium)
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TrackLaneRow(
    vm: CanvasViewModel,
    tl: TimelineState,
    track: TimelineTrackInfo,
    trackHeight: Dp,
    zoomDp: Dp,
    zoomPx: Float,
) {
    val hasKeySubTracks = track.expanded && (
        track.transformKeyframes.isNotEmpty() ||
            track.opacityKeyframes.isNotEmpty() ||
            track.clips.any { it.transformKeyframes.isNotEmpty() || it.opacityKeyframes.isNotEmpty() }
        )
    val totalRowHeight = trackHeight + if (hasKeySubTracks) KeySubTrackHeight * 2 else 0.dp
    val tagColor = Color(0xFF000000L or (track.colorTag.toLong() and 0xFFFFFFL))
    var contextMenuClip by remember { mutableStateOf<TimelineClipInfo?>(null) }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(totalRowHeight)
            .border(0.5.dp, DreamsEdge.copy(alpha = 0.5f)),
    ) {
        // Main Clip Lane
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(trackHeight),
        ) {
            // Collect neighbor boundaries for magnetic snapping when dragging clips
            val snapCandidates = remember(track.clips, tl.playhead, tl.workArea) {
                buildList {
                    add(0)
                    add(tl.playhead.toInt())
                    tl.workArea?.let {
                        add(it.start.toInt())
                        add(it.end.toInt() + 1)
                    }
                    for (c in track.clips) {
                        add(c.startFrame.toInt())
                        add((c.startFrame + c.durationFrames).toInt())
                    }
                }
            }

            for (clip in track.clips) {
                val isClipSelected = tl.activeClipId == clip.id || vm.selectedClipIds.contains(clip.id)
                val clipStartDp = zoomDp * clip.startFrame.toInt()
                val clipWidthDp = (zoomDp * clip.durationFrames.toInt()).coerceAtLeast(18.dp)
                var dragAccumPx by remember(clip.id, clip.startFrame) { mutableFloatStateOf(0f) }
                var resizeAccumPx by remember(clip.id, clip.durationFrames) { mutableFloatStateOf(0f) }

                Box(
                    modifier = Modifier
                        .offset(x = clipStartDp, y = 3.dp)
                        .width(clipWidthDp)
                        .height(trackHeight - 6.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (isClipSelected) tagColor.copy(alpha = 0.28f) else DreamsClipFill)
                        .border(
                            width = if (isClipSelected) 2.dp else 1.dp,
                            color = if (isClipSelected) tagColor else DreamsEdge,
                            shape = RoundedCornerShape(8.dp),
                        )
                        .combinedClickable(
                            onClick = {
                                if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT) {
                                    vm.toggleClipMultiSelection(clip.id)
                                } else {
                                    vm.selectTimelineClip(clip.id)
                                }
                            },
                            onDoubleClick = {
                                vm.selectTimelineClip(clip.id)
                                if (clip.kind == TimelineClipKind.CEL) {
                                    vm.setFlipbookMode(true)
                                }
                            },
                            onLongClick = {
                                vm.selectTimelineClip(clip.id)
                                contextMenuClip = clip
                            },
                        )
                        // Dragging the clip body moves its start frame along the track
                        .pointerInput(clip.id, clip.startFrame, zoomPx) {
                            detectHorizontalDragGestures(
                                onDragStart = { dragAccumPx = 0f },
                                onHorizontalDrag = { change, dragAmount ->
                                    change.consume()
                                    dragAccumPx += dragAmount
                                    val deltaFrames = (dragAccumPx / zoomPx).roundToInt()
                                    val rawStart = (clip.startFrame.toInt() + deltaFrames).coerceAtLeast(0)
                                    val snapped = snapClipFrame(rawStart, snapCandidates)
                                    vm.moveTimelineClip(clip.id, snapped, null, done = false)
                                },
                                onDragEnd = {
                                    val deltaFrames = (dragAccumPx / zoomPx).roundToInt()
                                    val rawStart = (clip.startFrame.toInt() + deltaFrames).coerceAtLeast(0)
                                    val snapped = snapClipFrame(rawStart, snapCandidates)
                                    vm.moveTimelineClip(clip.id, snapped, null, done = true)
                                },
                            )
                        },
                ) {
                    // Render Cels inside a CelClip with thumbnails and hold bars, or static LayerClip preview
                    if (clip.kind == TimelineClipKind.CEL && clip.cels.isNotEmpty()) {
                        Row(modifier = Modifier.fillMaxSize()) {
                            for (cel in clip.cels) {
                                val celWidthDp = (zoomDp * cel.durationFrames.toInt()).coerceAtLeast(10.dp)
                                Box(
                                    modifier = Modifier
                                        .width(celWidthDp)
                                        .fillMaxHeight()
                                        .border(0.5.dp, DreamsEdge.copy(alpha = 0.6f)),
                                ) {
                                    vm.frameThumbnails[cel.layerId]?.let { thumb ->
                                        Image(
                                            bitmap = thumb,
                                            contentDescription = null,
                                            contentScale = ContentScale.Fit,
                                            modifier = Modifier
                                                .padding(2.dp)
                                                .width((zoomDp * 2).coerceAtMost(celWidthDp))
                                                .fillMaxHeight(),
                                        )
                                    }
                                    if (cel.hold > 0u) {
                                        Text(
                                            text = "+${cel.hold}",
                                            color = DreamsDimInk,
                                            fontSize = 9.sp,
                                            modifier = Modifier
                                                .align(Alignment.BottomEnd)
                                                .padding(end = 3.dp, bottom = 2.dp),
                                        )
                                    }
                                }
                            }
                        }
                    } else {
                        Row(
                            modifier = Modifier
                                .fillMaxSize()
                                .padding(horizontal = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            vm.frameThumbnails[clip.primaryLayerId]?.let { thumb ->
                                Image(
                                    bitmap = thumb,
                                    contentDescription = null,
                                    contentScale = ContentScale.Fit,
                                    modifier = Modifier.size(trackHeight - 14.dp),
                                )
                            }
                            Text(
                                text = clip.name,
                                color = DreamsInk,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Medium,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }

                    // Right-edge Drag Handle to resize duration / hold
                    Box(
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .width(12.dp)
                            .fillMaxHeight()
                            .background(tagColor.copy(alpha = 0.65f))
                            .pointerInput(clip.id, clip.durationFrames, zoomPx) {
                                detectHorizontalDragGestures(
                                    onDragStart = { resizeAccumPx = 0f },
                                    onHorizontalDrag = { change, dragAmount ->
                                        change.consume()
                                        resizeAccumPx += dragAmount
                                        val deltaFrames = (resizeAccumPx / zoomPx).roundToInt()
                                        val nextDur = (clip.durationFrames.toInt() + deltaFrames).coerceAtLeast(1)
                                        vm.resizeTimelineClip(clip.id, nextDur, done = false)
                                    },
                                    onDragEnd = {
                                        val deltaFrames = (resizeAccumPx / zoomPx).roundToInt()
                                        val nextDur = (clip.durationFrames.toInt() + deltaFrames).coerceAtLeast(1)
                                        vm.resizeTimelineClip(clip.id, nextDur, done = true)
                                    },
                                )
                            },
                    )
                }
            }
        }

        // Sub-tracks for Move & Opacity Keyframes
        if (hasKeySubTracks) {
            // 1. Move & Scale Keyframe Row
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(KeySubTrackHeight)
                    .background(DreamsTrackBg.copy(alpha = 0.45f)),
            ) {
                for (clip in track.clips) {
                    for (kf in clip.transformKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            color = KeyframeGold,
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }

            // 2. Opacity Keyframe Row
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(KeySubTrackHeight)
                    .background(DreamsTrackBg.copy(alpha = 0.65f)),
            ) {
                for (clip in track.clips) {
                    for (kf in clip.opacityKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            color = OpacityCyan,
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
    }

    // Long-press Clip Context Menu Dialog/Card
    contextMenuClip?.let { clip ->
        ClipContextMenuCard(
            vm = vm,
            clip = clip,
            onDismiss = { contextMenuClip = null },
        )
    }
}

@Composable
private fun KeyframeDiamond(
    frame: Int,
    zoomDp: Dp,
    color: Color,
    onClick: () -> Unit,
) {
    val xDp = zoomDp * frame + zoomDp * 0.5f - 8.dp
    Box(
        modifier = Modifier
            .offset(x = xDp.coerceAtLeast(0.dp), y = 3.dp)
            .size(18.dp)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(modifier = Modifier.size(12.dp)) {
            val path = Path().apply {
                moveTo(size.width / 2f, 0f)
                lineTo(size.width, size.height / 2f)
                lineTo(size.width / 2f, size.height)
                lineTo(0f, size.height / 2f)
                close()
            }
            drawPath(path, color = color)
        }
    }
}

/**
 * Interactive **Stage Keyframe & Live Perform Gizmo** rendered over the upper Stage area:
 * - Shows a bounding frame with corner handles and a draggable **Anchor Point** (crosshair pivot).
 * - 1-finger drag moves `(x, y)`; 2-finger pinch/rotate moves, scales, and rotates around the Anchor Point.
 * - In **Perform** mode (`DreamsWorkMode.PERFORM`), touching the Stage automatically starts playback
 *   and records live motion samples in real time; releasing commits the RDP-filtered keyframes and
 *   displays the **Motion Filtering** slider!
 */
@Composable
private fun BoxScope.StageKeyTransformOverlay(
    vm: CanvasViewModel,
    tl: TimelineState,
    stageWidth: Dp,
    stageHeight: Dp,
) {
    val activeClipId = tl.activeClipId ?: return
    val currentTransform = tl.activeEvaluatedTransform
    var liveTransform by remember(activeClipId, tl.playhead, currentTransform) {
        mutableStateOf(currentTransform)
    }
    var draggingAnchor by remember { mutableStateOf(false) }

    Box(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .width(stageWidth)
            .height(stageHeight)
            // Multi-touch transform gesture (1 finger = move, 2 fingers = move + scale + rotate)
            .pointerInput(activeClipId, vm.dreamsMode) {
                detectTransformGestures { _, pan, zoom, rotationDeg ->
                    if (draggingAnchor) return@detectTransformGestures
                    if (vm.dreamsMode == DreamsWorkMode.PERFORM && !vm.performRecording) {
                        vm.beginPerformRecording()
                    }
                    val rotRad = (rotationDeg * Math.PI / 180.0).toFloat()
                    val updated = TimelineTransformInfo(
                        x = liveTransform.x + pan.x,
                        y = liveTransform.y + pan.y,
                        scaleX = (liveTransform.scaleX * zoom).coerceIn(0.05f, 16f),
                        scaleY = (liveTransform.scaleY * zoom).coerceIn(0.05f, 16f),
                        rotation = liveTransform.rotation + rotRad,
                        anchorX = liveTransform.anchorX,
                        anchorY = liveTransform.anchorY,
                    )
                    liveTransform = updated
                    if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                        vm.recordPerformTransform(updated)
                    } else {
                        vm.setClipTransformKeyframe(activeClipId, tl.playhead.toInt(), updated, done = false)
                    }
                }
            }
            .pointerInput(activeClipId, vm.dreamsMode) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                    } while (event.changes.any { it.pressed })
                    draggingAnchor = false
                    if (vm.dreamsMode == DreamsWorkMode.PERFORM && vm.performRecording) {
                        vm.finishPerformRecording()
                    } else if (vm.stageGizmoActive) {
                        vm.setClipTransformKeyframe(activeClipId, tl.playhead.toInt(), liveTransform, done = true)
                    }
                }
            },
    ) {
        val gizmoColor = if (vm.dreamsMode == DreamsWorkMode.PERFORM) PerformRed else KeyframeGold
        // Visual Stage Gizmo Frame & Anchor Point Crosshair
        Canvas(modifier = Modifier.fillMaxSize()) {
            val cx = size.width * 0.5f + liveTransform.x * 0.25f
            val cy = size.height * 0.5f + liveTransform.y * 0.25f
            val boxW = (size.width * 0.42f * liveTransform.scaleX).coerceIn(40f, size.width * 0.9f)
            val boxH = (size.height * 0.42f * liveTransform.scaleY).coerceIn(40f, size.height * 0.9f)

            // Bounding Box
            drawRoundRect(
                color = gizmoColor,
                topLeft = Offset(cx - boxW / 2f, cy - boxH / 2f),
                size = Size(boxW, boxH),
                cornerRadius = CornerRadius(8f, 8f),
                style = Stroke(width = 3f),
            )

            // Corner Handles
            val corners = listOf(
                Offset(cx - boxW / 2f, cy - boxH / 2f),
                Offset(cx + boxW / 2f, cy - boxH / 2f),
                Offset(cx + boxW / 2f, cy + boxH / 2f),
                Offset(cx - boxW / 2f, cy + boxH / 2f),
            )
            for (c in corners) {
                drawCircle(color = Color.White, radius = 8f, center = c)
                drawCircle(color = gizmoColor, radius = 6f, center = c)
            }

            // Anchor Point Pivot Crosshair
            val ax = (cx - boxW / 2f) + boxW * liveTransform.anchorX
            val ay = (cy - boxH / 2f) + boxH * liveTransform.anchorY
            drawCircle(color = gizmoColor, radius = 12f, center = Offset(ax, ay), style = Stroke(width = 2.5f))
            drawLine(color = gizmoColor, start = Offset(ax - 16f, ay), end = Offset(ax + 16f, ay), strokeWidth = 2.5f)
            drawLine(color = gizmoColor, start = Offset(ax, ay - 16f), end = Offset(ax, ay + 16f), strokeWidth = 2.5f)
        }

        // Top Stage Pill: Perform Status / Motion Filtering / Anchor Reset
        Row(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = Pc.barShown + 10.dp)
                .clip(RoundedCornerShape(18.dp))
                .background(DreamsBarFill.copy(alpha = 0.94f))
                .border(1.dp, gizmoColor, RoundedCornerShape(18.dp))
                .padding(horizontal = 14.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                Text(
                    text = if (vm.performRecording) "● PERFORMING (${vm.performSamplesCount} samples)" else "● Perform Armed: Touch Stage to Record",
                    color = PerformRed,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
                val filterPct = (tl.motionFiltering * 100).roundToInt()
                Text(
                    text = "Filter: $filterPct%",
                    color = DreamsInk,
                    fontSize = 12.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsClipFill)
                        .clickable {
                            val next = when {
                                tl.motionFiltering < 0.25f -> 0.35f
                                tl.motionFiltering < 0.55f -> 0.70f
                                tl.motionFiltering < 0.85f -> 1.0f
                                else -> 0.0f
                            }
                            vm.updatePerformMotionFiltering(next)
                        }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
            } else {
                Text(
                    text = "Move & Scale  •  (${liveTransform.x.roundToInt()}, ${liveTransform.y.roundToInt()})  •  ${(liveTransform.scaleX * 100).roundToInt()}%",
                    color = KeyframeGold,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "Reset",
                    color = DreamsInk,
                    fontSize = 12.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsClipFill)
                        .clickable {
                            val id = TimelineTransformInfo(0f, 0f, 1f, 1f, 0f, 0.5f, 0.5f)
                            liveTransform = id
                            vm.setClipTransformKeyframe(activeClipId, tl.playhead.toInt(), id, done = true)
                        }
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                )
            }
        }
    }
}

// ---- Popovers & Context Menus ----

@Composable
private fun BoxScope.MovieSettingsPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    bottomOffset: Dp,
) {
    Column(
        modifier = Modifier
            .align(Alignment.BottomStart)
            .padding(start = 16.dp, bottom = bottomOffset)
            .width(320.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(DreamsBarFill)
            .border(1.dp, DreamsEdge, RoundedCornerShape(20.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Movie & Onion Skin", color = DreamsInk, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Text(
                text = "Done",
                color = Pc.Accent,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.clickable { vm.movieSettingsOpen = false },
            )
        }

        // FPS Presets
        Text("Frames Per Second", color = DreamsDimInk, fontSize = 12.sp)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (presetFps in listOf(8, 12, 15, 24, 30, 60)) {
                val chosen = tl.fps.toInt() == presetFps
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (chosen) Pc.Accent else DreamsClipFill)
                        .clickable { vm.setTimelineFps(presetFps) }
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                ) {
                    Text(
                        text = "$presetFps",
                        color = if (chosen) Color.White else DreamsInk,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
        }

        // Playback Mode
        Text("Playback Loop", color = DreamsDimInk, fontSize = 12.sp)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val modes = listOf(
                AnimationPlayback.LOOP to "Loop",
                AnimationPlayback.PING_PONG to "Ping-Pong",
                AnimationPlayback.ONE_SHOT to "One Shot",
            )
            for ((mode, label) in modes) {
                val chosen = tl.playback == mode
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (chosen) Pc.Accent else DreamsClipFill)
                        .clickable { vm.setTimelinePlayback(mode) }
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                ) {
                    Text(label, color = if (chosen) Color.White else DreamsInk, fontSize = 12.sp)
                }
            }
        }

        // Onion Skin Frames & Opacity
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Onion Skin Frames (${tl.onion.beforeFrames})", color = DreamsInk, fontSize = 13.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (count in listOf(0u, 1u, 3u, 6u, 12u)) {
                    val chosen = tl.onion.beforeFrames == count && tl.onion.enabled == (count > 0u)
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
                            .background(if (chosen) KeyframeGold else DreamsClipFill)
                            .clickable {
                                vm.updateTimelineOnion {
                                    it.copy(
                                        enabled = count > 0u,
                                        beforeFrames = count,
                                        afterFrames = count,
                                    )
                                }
                            }
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                    ) {
                        Text(
                            text = if (count == 0u) "Off" else "$count",
                            color = if (chosen) Color.Black else DreamsInk,
                            fontSize = 12.sp,
                        )
                    }
                }
            }
        }

        // Work Area & Backstage Toggles
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            val hasWorkArea = tl.workArea != null
            Text(
                text = if (hasWorkArea) "Clear Work Area" else "Set Work Area Around Playhead",
                color = Pc.Accent,
                fontSize = 13.sp,
                modifier = Modifier.clickable {
                    if (hasWorkArea) {
                        vm.setTimelineWorkArea(null, null)
                    } else {
                        val p = tl.playhead.toInt()
                        val end = (p + tl.fps.toInt()).coerceAtMost(tl.durationFrames.toInt() - 1)
                        vm.setTimelineWorkArea(p, end)
                    }
                },
            )
            Text(
                text = if (tl.showBackstage) "Backstage: ON" else "Backstage: OFF",
                color = DreamsDimInk,
                fontSize = 12.sp,
                modifier = Modifier.clickable { vm.setBackstageShown(!tl.showBackstage) },
            )
        }
    }
}

@Composable
private fun BoxScope.TimelineAddMenuPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    bottomOffset: Dp,
) {
    Column(
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = 16.dp, bottom = bottomOffset)
            .width(240.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsBarFill)
            .border(1.dp, DreamsEdge, RoundedCornerShape(18.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Add to Timeline", color = DreamsDimInk, fontSize = 12.sp, fontWeight = FontWeight.Bold)
        MenuActionRow("+ New Drawing Track (Flipbook)") {
            vm.addTrackMenuOpen = false
            vm.addTimelineTrack(null, withDrawingClip = true)
        }
        MenuActionRow("+ New Content Track (Static/Keyframe)") {
            vm.addTrackMenuOpen = false
            vm.addTimelineTrack("Content Track", withDrawingClip = false)
            vm.addLayerClipToActiveTrack()
        }
        MenuActionRow("+ New Cel / Frame in Active Clip") {
            vm.addTrackMenuOpen = false
            vm.addTimelineCel()
        }
        MenuActionRow("◆ Add Move & Scale Keyframe") {
            vm.addTrackMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipTransformKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedTransform, done = true)
                vm.stageGizmoActive = true
            }
        }
        MenuActionRow("◆ Add Opacity Keyframe") {
            vm.addTrackMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipOpacityKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedOpacity, done = true)
            }
        }
        MenuActionRow("✂ Split Clip at Playhead") {
            vm.addTrackMenuOpen = false
            vm.splitActiveClipAtPlayhead()
        }
    }
}

@Composable
private fun BoxScope.PlayheadActionPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    bottomOffset: Dp,
) {
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(bottom = bottomOffset)
            .width(250.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsBarFill)
            .border(1.dp, PerformRed, RoundedCornerShape(18.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = "Playhead Action (Frame ${tl.playhead + 1u})",
            color = PerformRed,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
        )
        MenuActionRow("Move & Scale (Keyframe)") {
            vm.playheadActionMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipTransformKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedTransform, done = true)
                vm.stageGizmoActive = true
            }
        }
        MenuActionRow("Filter: Opacity Keyframe") {
            vm.playheadActionMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipOpacityKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedOpacity, done = true)
            }
        }
        MenuActionRow("Split Clip Here") {
            vm.playheadActionMenuOpen = false
            vm.splitActiveClipAtPlayhead()
        }
        MenuActionRow("Open in Flipbook") {
            vm.playheadActionMenuOpen = false
            vm.setFlipbookMode(true)
        }
    }
}

@Composable
private fun BoxScope.KeyframeEasingPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    clipId: ULong,
    frame: Int,
    bottomOffset: Dp,
) {
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(bottom = bottomOffset)
            .width(270.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsBarFill)
            .border(1.dp, KeyframeGold, RoundedCornerShape(18.dp))
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Keyframe @ F${frame + 1}", color = KeyframeGold, fontSize = 14.sp, fontWeight = FontWeight.Bold)
            Text(
                text = "Close",
                color = DreamsDimInk,
                fontSize = 12.sp,
                modifier = Modifier.clickable { vm.editingKeyframe = null },
            )
        }
        Text("Set Easing Curve", color = DreamsDimInk, fontSize = 12.sp)
        val easings = listOf(
            TimelineEasingKind.EASE_IN_OUT to "Ease In & Out (Smooth)",
            TimelineEasingKind.LINEAR to "Linear",
            TimelineEasingKind.EASE_IN to "Ease In",
            TimelineEasingKind.EASE_OUT to "Ease Out",
            TimelineEasingKind.HOLD to "Hold (Step / Constant)",
        )
        for ((kind, label) in easings) {
            MenuActionRow(label) {
                vm.setClipTransformEasing(clipId, frame, kind)
                vm.setClipOpacityEasing(clipId, frame, kind)
                vm.editingKeyframe = null
            }
        }
        MenuActionRow("Delete Keyframe", color = PerformRed) {
            vm.deleteClipTransformKeyframe(clipId, frame)
            vm.deleteClipOpacityKeyframe(clipId, frame)
            vm.editingKeyframe = null
        }
    }
}

@Composable
private fun ClipContextMenuCard(
    vm: CanvasViewModel,
    clip: TimelineClipInfo,
    onDismiss: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(DreamsHeaderBg)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(clip.name, color = Pc.Accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
        if (clip.kind == TimelineClipKind.CEL) {
            Text(
                text = "Flipbook",
                color = DreamsInk,
                fontSize = 12.sp,
                modifier = Modifier.clickable {
                    onDismiss()
                    vm.setFlipbookMode(true)
                },
            )
        }
        Text(
            text = "Split",
            color = DreamsInk,
            fontSize = 12.sp,
            modifier = Modifier.clickable {
                onDismiss()
                vm.splitActiveClipAtPlayhead(clip.id)
            },
        )
        Text(
            text = "Duplicate",
            color = DreamsInk,
            fontSize = 12.sp,
            modifier = Modifier.clickable {
                onDismiss()
                vm.duplicateTimelineClip(clip.id)
            },
        )
        Text(
            text = "Delete",
            color = PerformRed,
            fontSize = 12.sp,
            modifier = Modifier.clickable {
                onDismiss()
                vm.deleteTimelineClip(clip.id)
            },
        )
        Spacer(Modifier.weight(1f))
        Text(
            text = "✕",
            color = DreamsDimInk,
            fontSize = 12.sp,
            modifier = Modifier.clickable(onClick = onDismiss),
        )
    }
}

@Composable
private fun MenuActionRow(
    label: String,
    color: Color = DreamsInk,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(DreamsClipFill)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Text(text = label, color = color, fontSize = 13.sp, fontWeight = FontWeight.Medium)
    }
}
