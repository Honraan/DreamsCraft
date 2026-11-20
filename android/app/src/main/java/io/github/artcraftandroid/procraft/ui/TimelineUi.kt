package io.github.artcraftandroid.procraft.ui

import android.view.HapticFeedbackConstants
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.DreamsWorkMode
import io.github.artcraftandroid.procraft.canvas.StageGizmoSubMode
import io.github.artcraftandroid.procraft.core.AnimationPlayback
import io.github.artcraftandroid.procraft.core.TimelineClipInfo
import io.github.artcraftandroid.procraft.core.TimelineClipKind
import io.github.artcraftandroid.procraft.core.TimelineDeformInfo
import io.github.artcraftandroid.procraft.core.TimelineEasingKind
import io.github.artcraftandroid.procraft.core.TimelineKeyframeInfo
import io.github.artcraftandroid.procraft.core.TimelineLiveFilterInfo
import io.github.artcraftandroid.procraft.core.TimelineState
import io.github.artcraftandroid.procraft.core.TimelineTrackEffectsInfo
import io.github.artcraftandroid.procraft.core.TimelineTrackInfo
import io.github.artcraftandroid.procraft.core.TimelineTransformInfo
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin

// ---- Pure Layout, Timecode, Pinch-Zoom, Stylus Gestures, Warp & Snap Math (Unit-Tested) ----

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

/** Even-odd ray-casting polygon test for Stylus Lasso Selection on the Dopesheet. */
internal fun pointInPolygon(x: Float, y: Float, poly: List<Pair<Float, Float>>): Boolean {
    if (poly.size < 3) return false
    var inside = false
    var j = poly.size - 1
    for (i in poly.indices) {
        val (xi, yi) = poly[i]
        val (xj, yj) = poly[j]
        val intersects = ((yi > y) != (yj > y)) &&
            (x < (xj - xi) * (y - yi) / ((yj - yi).let { if (abs(it) < 1e-6f) 1e-6f else it }) + xi)
        if (intersects) inside = !inside
        j = i
    }
    return inside
}

/** Bounding box of a clip in Dopesheet coordinates for Lasso hit-testing. */
internal data class ClipBounds(
    val id: ULong,
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
)

/** Returns the IDs of all clips whose center or any corner falls inside [lasso]. */
internal fun clipsInLasso(lasso: List<Pair<Float, Float>>, clips: List<ClipBounds>): Set<ULong> {
    if (lasso.size < 3) return emptySet()
    val out = mutableSetOf<ULong>()
    for (c in clips) {
        val cx = (c.left + c.right) * 0.5f
        val cy = (c.top + c.bottom) * 0.5f
        if (pointInPolygon(cx, cy, lasso) ||
            pointInPolygon(c.left, c.top, lasso) ||
            pointInPolygon(c.right, c.top, lasso) ||
            pointInPolygon(c.left, c.bottom, lasso) ||
            pointInPolygon(c.right, c.bottom, lasso)
        ) {
            out.add(c.id)
        }
    }
    return out
}

/**
 * Detects a Stylus **Scratch-to-Delete** zig-zag gesture: at least 3 horizontal direction reversals
 * of meaningful amplitude (`>= 18px`) in a compact vertical band.
 */
internal fun isScratchToDeleteGesture(points: List<Pair<Float, Float>>): Boolean {
    if (points.size < 6) return false
    var reversals = 0
    var lastDir = 0
    var accumDx = 0f
    var totalPath = 0f
    var minX = Float.MAX_VALUE
    var maxX = -Float.MAX_VALUE
    var minY = Float.MAX_VALUE
    var maxY = -Float.MAX_VALUE
    for (i in points.indices) {
        val (x, y) = points[i]
        minX = min(minX, x)
        maxX = max(maxX, x)
        minY = min(minY, y)
        maxY = max(maxY, y)
        if (i > 0) {
            val dx = x - points[i - 1].first
            val dy = y - points[i - 1].second
            totalPath += hypot(dx, dy)
            if (abs(dx) > 1.5f) {
                val dir = if (dx > 0f) 1 else -1
                if (lastDir == 0) {
                    lastDir = dir
                    accumDx = abs(dx)
                } else if (dir == lastDir) {
                    accumDx += abs(dx)
                } else if (accumDx >= 18f) {
                    reversals++
                    lastDir = dir
                    accumDx = abs(dx)
                }
            }
        }
    }
    val spanX = (maxX - minX).coerceAtLeast(1f)
    val spanY = (maxY - minY).coerceAtLeast(1f)
    return reversals >= 3 && spanX >= 28f && totalPath >= spanX * 2.2f && spanY <= spanX * 2.5f
}

/** Returns the 32-float identity 4x4 Warp mesh (`[u, v]` pairs in `0..1`). */
internal fun defaultIdentityDeformPoints(): List<Float> = buildList(32) {
    for (row in 0..3) {
        for (col in 0..3) {
            add(col / 3f)
            add(row / 3f)
        }
    }
}

/** Bilinearly expands 4 normalized corner pins `[TL, TR, BR, BL]` into a 4x4 Warp mesh (32 floats). */
internal fun distortCornersToDeformPoints(corners: List<Pair<Float, Float>>): List<Float> {
    if (corners.size < 4) return defaultIdentityDeformPoints()
    val tl = corners[0]
    val tr = corners[1]
    val br = corners[2]
    val bl = corners[3]
    return buildList(32) {
        for (row in 0..3) {
            val v = row / 3f
            for (col in 0..3) {
                val u = col / 3f
                val topX = tl.first + (tr.first - tl.first) * u
                val topY = tl.second + (tr.second - tl.second) * u
                val botX = bl.first + (br.first - bl.first) * u
                val botY = bl.second + (br.second - bl.second) * u
                add(topX + (botX - topX) * v)
                add(topY + (botY - topY) * v)
            }
        }
    }
}

/** Classification of a short multi-finger tap on the Stage Gizmo or Timeline. */
internal enum class MultiFingerTapAction { NONE, UNDO, REDO }

/**
 * Recognizes Procreate-style **2-finger tap = Undo** and **3-finger tap = Redo** gestures while
 * rejecting pinches, drags (`maxTravelPx > 24f`), or long presses (`durationMs > 320L`).
 */
internal fun classifyMultiFingerTap(
    maxPointers: Int,
    maxTravelPx: Float,
    durationMs: Long,
): MultiFingerTapAction {
    if (!maxTravelPx.isFinite() || maxTravelPx > 24f || durationMs < 0L || durationMs > 320L) {
        return MultiFingerTapAction.NONE
    }
    return when (maxPointers) {
        2 -> MultiFingerTapAction.UNDO
        3 -> MultiFingerTapAction.REDO
        else -> MultiFingerTapAction.NONE
    }
}

/**
 * Passive observer for 2-finger tap (Undo) and 3-finger tap (Redo) on surfaces above the canvas
 * (`StageKeyTransformOverlay` and `MultiTrackDopesheet`) without blocking single-finger drags or
 * 2-finger pinch-zoom gestures.
 */
@Composable
private fun Modifier.multiFingerUndoRedo(
    onUndo: () -> Unit,
    onRedo: () -> Unit,
): Modifier {
    val currentUndo by rememberUpdatedState(onUndo)
    val currentRedo by rememberUpdatedState(onRedo)
    val view = LocalView.current
    return pointerInput(Unit) {
        awaitEachGesture {
            val first = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val startMs = first.uptimeMillis
            val origins = mutableMapOf(first.id to first.position)
            var maxPointers = 1
            var maxTravel = 0f
            var endMs = startMs
            do {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val pressed = event.changes.filter { it.pressed }
                if (pressed.size > maxPointers) maxPointers = pressed.size
                for (c in event.changes) {
                    endMs = max(endMs, c.uptimeMillis)
                    val origin = origins.getOrPut(c.id) { c.position }
                    val travel = hypot(c.position.x - origin.x, c.position.y - origin.y)
                    if (travel > maxTravel) maxTravel = travel
                }
            } while (event.changes.any { it.pressed })

            when (classifyMultiFingerTap(maxPointers, maxTravel, endMs - startMs)) {
                MultiFingerTapAction.UNDO -> {
                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                    currentUndo()
                }
                MultiFingerTapAction.REDO -> {
                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                    currentRedo()
                }
                MultiFingerTapAction.NONE -> Unit
            }
        }
    }
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
private val FilterPurple = Color(0xFFA855F7)
private val WarpPink = Color(0xFFEC4899)
private val AudioGreen = Color(0xFF22C55E)
private val CameraSky = Color(0xFF38BDF8)

private val TrackColorPalette = listOf(
    0xFF3B82F6u, // Blue
    0xFFEC4899u, // Pink
    0xFF10B981u, // Emerald
    0xFFF59E0Bu, // Amber
    0xFF8B5CF6u, // Violet
    0xFFEF4444u, // Red
    0xFF06B6D4u, // Cyan
    0xFF6B7280u, // Slate
)

private val TrackBlendPresets = listOf(
    0u to "Normal",
    1u to "Multiply",
    7u to "Screen",
    11u to "Overlay",
    8u to "Add",
    2u to "Darken",
    6u to "Lighten",
)

private fun activeClipOf(tl: TimelineState): TimelineClipInfo? {
    val cid = tl.activeClipId ?: return tl.tracks.lastOrNull()?.clips?.firstOrNull()
    return tl.tracks.asSequence().flatMap { it.clips.asSequence() }.firstOrNull { it.id == cid }
}

private fun hasAnyTrackEffect(fx: TimelineTrackEffectsInfo): Boolean =
    fx.outlineEnabled || fx.shadowEnabled || fx.glowEnabled || fx.colorOverlayEnabled

/**
 * Root DreamsCraft Animation Workspace overlay:
 * - When `timeline == null`, falls back to legacy `AnimationBar` (for Page Assist).
 * - When `timeline.flipbookOpen == true`, maximizes the Stage and shows the floating Flipbook dock
 *   with 1-tap expand, Undo/Redo, Onion Skin, **Perform Motion Ghost**, and **Multi-Layer Cel** controls.
 * - Otherwise, shows the Stage above the timeline:
 *   1. Stage Backstage & Non-Destructive Keyframe / Warp / Distort / Camera / Live Perform Gizmo
 *   2. Draggable **Center Action Bar** in the middle
 *   3. Touch-first **Multi-Track Dopesheet, Camera Track, Audio Waveforms & KeyTracks** on bottom
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
    val view = LocalView.current
    val totalHeightPx = with(density) { windowHeight.toPx() }
    val timelineHeightDp = (windowHeight * vm.timelineSplitRatio).coerceIn(140.dp, windowHeight * 0.70f)
    val dockBottomPx = with(density) {
        if (tl.flipbookOpen) 126.dp.toPx() else (timelineHeightDp + CenterBarHeight).toPx()
    }

    LaunchedEffect(dockBottomPx, tl.flipbookOpen) {
        vm.updateStageBottomInset(dockBottomPx)
    }

    // 1. Stage Backstage Frame & Interactive Keyframe / Warp / Distort / Camera / Live Perform Gizmo
    val stageHeightDp = if (tl.flipbookOpen) windowHeight else (windowHeight - timelineHeightDp - CenterBarHeight)
    if (vm.dreamsMode == DreamsWorkMode.PERFORM || vm.stageGizmoActive || vm.stageGizmoMode == StageGizmoSubMode.CAMERA) {
        StageKeyTransformOverlay(
            vm = vm,
            tl = tl,
            stageWidth = windowWidth,
            stageHeight = stageHeightDp.coerceAtLeast(120.dp),
        )
    }

    // 2. Flipbook Mode vs Full Multi-Track Split View
    if (tl.flipbookOpen) {
        val activeClip = remember(tl) { activeClipOf(tl) }
        Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) {
            AnimationBar(vm)
            // Floating Pill above the Flipbook bar: Timeline, Undo/Redo, Onion Skin, Perform Motion Ghost, Cel Sub-Layers, Timecode
            Row(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 126.dp)
                    .clip(RoundedCornerShape(20.dp))
                    .background(DreamsBarFill.copy(alpha = 0.95f))
                    .border(1.dp, DreamsEdge, RoundedCornerShape(20.dp))
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
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
                    text = "↶",
                    color = if (vm.canUndo || vm.performRecording) DreamsInk else DreamsDimInk.copy(alpha = 0.4f),
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier
                        .clickable(enabled = vm.canUndo || vm.performRecording) {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.undo()
                        }
                        .padding(horizontal = 2.dp),
                )
                Text(
                    text = "↷",
                    color = if (vm.canRedo) DreamsInk else DreamsDimInk.copy(alpha = 0.4f),
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier
                        .clickable(enabled = vm.canRedo) {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.redo()
                        }
                        .padding(horizontal = 2.dp),
                )
                Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))
                Text(
                    text = if (tl.onion.enabled) "Onion: ON" else "Onion: OFF",
                    color = if (tl.onion.enabled) KeyframeGold else DreamsDimInk,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.clickable {
                        vm.updateTimelineOnion { it.copy(enabled = !it.enabled) }
                    },
                )
                if (activeClip != null) {
                    Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))
                    Text(
                        text = if (activeClip.performOnionGuide) "👻 Ghost: ON" else "👻 Ghost",
                        color = if (activeClip.performOnionGuide) OpacityCyan else DreamsDimInk,
                        fontSize = 13.sp,
                        fontWeight = if (activeClip.performOnionGuide) FontWeight.Bold else FontWeight.Medium,
                        modifier = Modifier.clickable {
                            vm.setPerformOnionGuide(
                                clipId = activeClip.id,
                                enabled = !activeClip.performOnionGuide,
                                isolateFirstFrame = true,
                                opacity = activeClip.performGuideOpacity.coerceIn(0.15f, 0.9f),
                                colorRgb = activeClip.performGuideColor.ifEmpty { listOf(0.15f, 0.85f, 1.0f) },
                            )
                        },
                    )
                    if (activeClip.kind == TimelineClipKind.CEL && activeClip.cels.isNotEmpty()) {
                        val relFrame = (tl.playhead.toInt() - activeClip.startFrame.toInt()).coerceAtLeast(0)
                        var cursor = 0
                        var celIdx = 0
                        for ((idx, cel) in activeClip.cels.withIndex()) {
                            val end = cursor + cel.durationFrames.toInt()
                            if (relFrame < end) {
                                celIdx = idx
                                break
                            }
                            cursor = end
                            celIdx = idx
                        }
                        val sublayers = remember(activeClip.id, celIdx, vm.layers) {
                            vm.celSublayerIds(activeClip.id, celIdx)
                        }
                        Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))
                        Text(
                            text = "Cel L${sublayers.size.coerceAtLeast(1)} +",
                            color = FilterPurple,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.clickable {
                                vm.addCelSublayer(activeClip.id, celIdx)
                            },
                        )
                    }
                }
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

    val popoverBottom = timelineHeightDp + CenterBarHeight + 6.dp
    // Popovers standing above the Center Action Bar
    if (vm.movieSettingsOpen) {
        MovieSettingsPopover(
            vm = vm,
            tl = tl,
            bottomOffset = popoverBottom,
        )
    }
    if (vm.addTrackMenuOpen) {
        TimelineAddMenuPopover(
            vm = vm,
            tl = tl,
            bottomOffset = popoverBottom,
        )
    }
    if (vm.playheadActionMenuOpen) {
        PlayheadActionPopover(
            vm = vm,
            tl = tl,
            bottomOffset = popoverBottom,
        )
    }
    if (vm.liveFilterPopoverOpen) {
        LiveFilterPopover(
            vm = vm,
            tl = tl,
            bottomOffset = popoverBottom,
        )
    }
    vm.trackOptionsTrackId?.let { tid ->
        tl.tracks.firstOrNull { it.id == tid }?.let { targetTrack ->
            TrackOptionsPopover(
                vm = vm,
                tl = tl,
                track = targetTrack,
                bottomOffset = popoverBottom,
            )
        }
    }
    vm.editingKeyframe?.let { (clipId, frame) ->
        KeyframeEasingPopover(
            vm = vm,
            tl = tl,
            clipId = clipId,
            frame = frame.toInt(),
            bottomOffset = popoverBottom,
        )
    }
}

/**
 * The center action bar between the Stage and the timeline:
 * - Left: Play/Pause + Undo/Redo + Timecode (`MM:SS:FF` & FPS) + Onion Skin badge + Camera badge
 * - Center: Draggable Split Handle (pull down to collapse into Flipbook!) + Flipbook button
 * - Right: Mode switches (`Draw`, `Perform`, `Move`, `FX`, `Edit`, `+ Add`)
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
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Left: Play/Pause + Undo/Redo + Timecode + Quick Onion Toggle + Camera Track toggle
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
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
                    .padding(horizontal = 10.dp, vertical = 6.dp),
            ) {
                Text(
                    text = if (isPlaying) "Pause" else "Play",
                    color = if (isPlaying) Color.White else DreamsInk,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Quick Undo / Redo buttons (works for Perform, Timeline, Keyframes, Tracks & Painting)
            val undoReady = vm.canUndo || vm.performRecording
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (undoReady) DreamsClipFill else Color.Transparent)
                    .border(1.dp, if (undoReady) DreamsEdge else DreamsEdge.copy(alpha = 0.4f), RoundedCornerShape(8.dp))
                    .clickable(enabled = undoReady) {
                        view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                        vm.undo()
                    }
                    .padding(horizontal = 7.dp, vertical = 5.dp),
            ) {
                Text(
                    text = "↶",
                    color = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.4f),
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (vm.canRedo) DreamsClipFill else Color.Transparent)
                    .border(1.dp, if (vm.canRedo) DreamsEdge else DreamsEdge.copy(alpha = 0.4f), RoundedCornerShape(8.dp))
                    .clickable(enabled = vm.canRedo) {
                        view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                        vm.redo()
                    }
                    .padding(horizontal = 7.dp, vertical = 5.dp),
            ) {
                Text(
                    text = "↷",
                    color = if (vm.canRedo) DreamsInk else DreamsDimInk.copy(alpha = 0.4f),
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Timecode Pill -> opens Movie, Auto-Draw & Onion Skin Settings
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
                        vm.liveFilterPopoverOpen = false
                        vm.trackOptionsTrackId = null
                    }
                    .padding(horizontal = 8.dp, vertical = 5.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Text(
                    text = formatTimecode(tl.playhead.toInt(), tl.fps.toInt()),
                    color = DreamsInk,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "${tl.fps}fps",
                    color = DreamsDimInk,
                    fontSize = 10.sp,
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
                    .padding(horizontal = 7.dp, vertical = 5.dp),
            ) {
                Text(
                    text = "Onion",
                    color = if (tl.onion.enabled) KeyframeGold else DreamsDimInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Medium,
                )
            }

            // Camera Track gizmo pill
            val camActive = vm.stageGizmoActive && vm.stageGizmoMode == StageGizmoSubMode.CAMERA
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (camActive || tl.cameraEnabled) CameraSky.copy(alpha = 0.2f) else Color.Transparent)
                    .border(1.dp, if (camActive) CameraSky else DreamsEdge, RoundedCornerShape(8.dp))
                    .clickable {
                        if (camActive) {
                            vm.stageGizmoActive = false
                            vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                        } else {
                            vm.setCameraEnabled(true)
                            vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                            vm.stageGizmoActive = true
                            vm.dreamsMode = DreamsWorkMode.TIMELINE
                        }
                    }
                    .padding(horizontal = 7.dp, vertical = 5.dp),
            ) {
                Text(
                    text = "Cam",
                    color = if (camActive || tl.cameraEnabled) CameraSky else DreamsDimInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.SemiBold,
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
                        .width(36.dp)
                        .height(5.dp)
                        .clip(CircleShape)
                        .background(DreamsDimInk.copy(alpha = 0.6f)),
                )
                Text(
                    text = "Flipbook",
                    color = DreamsDimInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { vm.setFlipbookMode(true) }
                        .padding(horizontal = 5.dp, vertical = 3.dp),
                )
            }
        }

        // Right: Workspace mode controls (Draw, Perform, Move/Warp, FX, Edit, + Add)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            ModeChip(
                label = "Draw",
                selected = vm.dreamsMode == DreamsWorkMode.DRAW_PAINT && !vm.stageGizmoActive,
                activeColor = Pc.Accent,
                onClick = {
                    vm.dreamsMode = DreamsWorkMode.DRAW_PAINT
                    vm.stageGizmoActive = false
                    vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
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
                        if (vm.stageGizmoMode == StageGizmoSubMode.CAMERA) {
                            vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                        }
                    }
                },
            )
            ModeChip(
                label = if (vm.stageGizmoMode == StageGizmoSubMode.WARP) "Warp" else "Move",
                selected = vm.stageGizmoActive && vm.dreamsMode != DreamsWorkMode.PERFORM && vm.stageGizmoMode != StageGizmoSubMode.CAMERA,
                activeColor = if (vm.stageGizmoMode == StageGizmoSubMode.WARP) WarpPink else KeyframeGold,
                onClick = {
                    if (vm.performRecording) vm.finishPerformRecording()
                    vm.dreamsMode = DreamsWorkMode.TIMELINE
                    if (vm.stageGizmoMode == StageGizmoSubMode.CAMERA) {
                        vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                    }
                    vm.stageGizmoActive = !vm.stageGizmoActive
                },
            )
            ModeChip(
                label = "FX",
                selected = vm.liveFilterPopoverOpen,
                activeColor = FilterPurple,
                onClick = {
                    vm.liveFilterPopoverOpen = !vm.liveFilterPopoverOpen
                    vm.movieSettingsOpen = false
                    vm.addTrackMenuOpen = false
                    vm.playheadActionMenuOpen = false
                    vm.trackOptionsTrackId = null
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
            if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT && vm.selectedClipIds.isNotEmpty()) {
                ModeChip(
                    label = "Del (${vm.selectedClipIds.size})",
                    selected = true,
                    activeColor = PerformRed,
                    onClick = { vm.deleteSelectedClips() },
                )
            }
            // + Add Button
            Box(
                modifier = Modifier
                    .size(30.dp)
                    .clip(CircleShape)
                    .background(if (vm.addTrackMenuOpen) Pc.Accent else DreamsClipFill)
                    .border(1.dp, DreamsEdge, CircleShape)
                    .clickable {
                        vm.addTrackMenuOpen = !vm.addTrackMenuOpen
                        vm.movieSettingsOpen = false
                        vm.playheadActionMenuOpen = false
                        vm.liveFilterPopoverOpen = false
                        vm.trackOptionsTrackId = null
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
            // Procreate-style 2-finger tap = Undo, 3-finger tap = Redo right over the Timeline
            .multiFingerUndoRedo(
                onUndo = { vm.undo() },
                onRedo = { vm.redo() },
            )
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
        val showCameraRow = tl.cameraEnabled || tl.cameraTransformKeyframes.isNotEmpty() || tl.cameraFilterKeyframes.isNotEmpty()
        var lassoPoints by remember { mutableStateOf<List<Pair<Float, Float>>>(emptyList()) }
        val trackHeightPx = with(density) { trackHeight.toPx() }
        val keySubHeightPx = with(density) { KeySubTrackHeight.toPx() }
        val camRowHeight = 32.dp
        val camRowHeightPx = with(density) { camRowHeight.toPx() }

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(verticalScroll),
        ) {
            // Left Column: Camera Header (if active) + Track Headers
            Column(
                modifier = Modifier
                    .width(TrackHeaderWidth)
                    .background(DreamsHeaderBg),
            ) {
                if (showCameraRow) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(camRowHeight)
                            .background(CameraSky.copy(alpha = 0.14f))
                            .border(0.5.dp, CameraSky.copy(alpha = 0.5f))
                            .clickable {
                                vm.setCameraEnabled(true)
                                vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                                vm.stageGizmoActive = true
                            }
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            text = "🎥 Camera",
                            color = CameraSky,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            text = if (tl.cameraEnabled) "ON" else "OFF",
                            color = if (tl.cameraEnabled) CameraSky else DreamsDimInk,
                            fontSize = 10.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable { vm.setCameraEnabled(!tl.cameraEnabled) },
                        )
                    }
                }
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

            // Right Column: Scrollable Camera Lane, Track Lanes, Clips, Waveforms, KeyTracks & Playhead
            Box(
                modifier = Modifier
                    .weight(1f)
                    .horizontalScroll(horizontalScroll)
                    .then(
                        if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT) {
                            Modifier.pointerInput(tracksTopDown, zoomPx, trackHeightPx, showCameraRow) {
                                detectDragGestures(
                                    onDragStart = { start ->
                                        lassoPoints = listOf(start.x to start.y)
                                    },
                                    onDrag = { change, _ ->
                                        change.consume()
                                        lassoPoints = lassoPoints + (change.position.x to change.position.y)
                                    },
                                    onDragEnd = {
                                        val pts = lassoPoints
                                        lassoPoints = emptyList()
                                        if (pts.size < 4) return@detectDragGestures
                                        // Build current clip bounds in lane coordinates
                                        val bounds = mutableListOf<ClipBounds>()
                                        var yCursor = if (showCameraRow) camRowHeightPx else 0f
                                        for (t in tracksTopDown) {
                                            for (c in t.clips) {
                                                val left = c.startFrame.toFloat() * zoomPx
                                                val right = (c.startFrame + c.durationFrames).toFloat() * zoomPx
                                                bounds.add(ClipBounds(c.id, left, yCursor, right, yCursor + trackHeightPx))
                                            }
                                            yCursor += trackHeightPx + trackSubTrackCount(t) * keySubHeightPx
                                        }
                                        val hit = clipsInLasso(pts, bounds)
                                        if (isScratchToDeleteGesture(pts) && hit.isNotEmpty()) {
                                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                                            vm.deleteTimelineClips(hit.toList())
                                        } else if (hit.isNotEmpty()) {
                                            vm.selectClips(hit)
                                        }
                                    },
                                    onDragCancel = { lassoPoints = emptyList() },
                                )
                            }
                        } else {
                            Modifier
                        },
                    ),
            ) {
                Column(modifier = Modifier.width(contentWidthDp)) {
                    if (showCameraRow) {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(camRowHeight)
                                .background(CameraSky.copy(alpha = 0.08f))
                                .border(0.5.dp, CameraSky.copy(alpha = 0.35f)),
                        ) {
                            for (kf in tl.cameraTransformKeyframes) {
                                KeyframeDiamond(
                                    frame = kf.frame.toInt(),
                                    zoomDp = zoomDp,
                                    color = CameraSky,
                                    onClick = {
                                        vm.setTimelinePlayhead(kf.frame.toInt())
                                        vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                                        vm.stageGizmoActive = true
                                    },
                                )
                            }
                            for (kf in tl.cameraFilterKeyframes) {
                                KeyframeDiamond(
                                    frame = kf.frame.toInt(),
                                    zoomDp = zoomDp,
                                    color = FilterPurple,
                                    onClick = {
                                        vm.setTimelinePlayhead(kf.frame.toInt())
                                        vm.liveFilterPopoverOpen = true
                                    },
                                )
                            }
                        }
                    }
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

                // Stylus Lasso / Scratch-to-Delete live feedback stroke
                if (lassoPoints.size >= 2) {
                    val scratch = isScratchToDeleteGesture(lassoPoints)
                    Canvas(modifier = Modifier.matchParentSize()) {
                        val path = Path().apply {
                            moveTo(lassoPoints[0].first, lassoPoints[0].second)
                            for (i in 1 until lassoPoints.size) {
                                lineTo(lassoPoints[i].first, lassoPoints[i].second)
                            }
                        }
                        drawPath(
                            path = path,
                            color = if (scratch) PerformRed else Pc.Accent,
                            style = Stroke(
                                width = 3f,
                                pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 6f)),
                            ),
                        )
                    }
                }

                // Continuous Playhead Line across all tracks
                val playheadX = zoomDp * tl.playhead.toInt() + zoomDp * 0.5f
                val totalSubTracks = tracksTopDown.sumOf { trackSubTrackCount(it) }
                Box(
                    modifier = Modifier
                        .offset(x = playheadX)
                        .width(2.dp)
                        .height(
                            (if (showCameraRow) camRowHeight else 0.dp) +
                                trackHeight * tracksTopDown.size.coerceAtLeast(1) +
                                KeySubTrackHeight * totalSubTracks +
                                36.dp,
                        )
                        .background(PerformRed),
                )
            }
        }
    }
}

private fun trackSubTrackCount(track: TimelineTrackInfo): Int {
    if (!track.expanded) return 0
    var count = 0
    val hasTransform = track.transformKeyframes.isNotEmpty() || track.clips.any { it.transformKeyframes.isNotEmpty() }
    val hasOpacity = track.opacityKeyframes.isNotEmpty() || track.clips.any { it.opacityKeyframes.isNotEmpty() }
    val hasFilter = track.filterKeyframes.isNotEmpty() || track.clips.any { it.filterKeyframes.isNotEmpty() }
    val hasDeform = track.clips.any { it.deformKeyframes.isNotEmpty() }
    val hasAudio = track.clips.any { it.kind == TimelineClipKind.AUDIO || it.volumeKeyframes.isNotEmpty() }
    if (hasTransform) count++
    if (hasOpacity) count++
    if (hasFilter) count++
    if (hasDeform) count++
    if (hasAudio) count++
    return count
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TrackHeaderRow(
    vm: CanvasViewModel,
    tl: TimelineState,
    track: TimelineTrackInfo,
    trackHeight: Dp,
) {
    val isSelected = tl.activeTrackId == track.id || vm.selectedTrackIds.contains(track.id)
    val hasTransform = track.expanded && (track.transformKeyframes.isNotEmpty() || track.clips.any { it.transformKeyframes.isNotEmpty() })
    val hasOpacity = track.expanded && (track.opacityKeyframes.isNotEmpty() || track.clips.any { it.opacityKeyframes.isNotEmpty() })
    val hasFilter = track.expanded && (track.filterKeyframes.isNotEmpty() || track.clips.any { it.filterKeyframes.isNotEmpty() })
    val hasDeform = track.expanded && (track.clips.any { it.deformKeyframes.isNotEmpty() })
    val hasAudio = track.expanded && track.clips.any { it.kind == TimelineClipKind.AUDIO || it.volumeKeyframes.isNotEmpty() }
    val subCount = listOf(hasTransform, hasOpacity, hasFilter, hasDeform, hasAudio).count { it }
    val totalRowHeight = trackHeight + KeySubTrackHeight * subCount
    val tagColor = Color(0xFF000000L or (track.colorTag.toLong() and 0xFFFFFFL))
    val hasFx = hasAnyTrackEffect(track.effects)

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(totalRowHeight)
            .background(if (isSelected) Pc.Accent.copy(alpha = 0.16f) else DreamsHeaderBg)
            .border(0.5.dp, DreamsEdge)
            .combinedClickable(
                onClick = {
                    if (vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT) {
                        vm.toggleTrackMultiSelection(track.id)
                    } else {
                        vm.selectTimelineTrack(track.id)
                    }
                },
                onLongClick = {
                    vm.selectTimelineTrack(track.id)
                    vm.trackOptionsTrackId = track.id
                    vm.movieSettingsOpen = false
                    vm.addTrackMenuOpen = false
                    vm.playheadActionMenuOpen = false
                    vm.liveFilterPopoverOpen = false
                },
            ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(trackHeight)
                .padding(start = if (track.parentGroup != null) 10.dp else 4.dp, end = 4.dp),
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
            Spacer(Modifier.width(4.dp))

            // Expand/Collapse KeyTracks chevron
            Text(
                text = if (track.expanded) "▾" else "▸",
                color = DreamsDimInk,
                fontSize = 11.sp,
                modifier = Modifier
                    .clickable { vm.toggleTrackExpanded(track.id, !track.expanded) }
                    .padding(2.dp),
            )

            // Track Name + Status badges (Opacity, FX, CLIP, α, Mute)
            Column(modifier = Modifier.weight(1f).padding(horizontal = 3.dp)) {
                Text(
                    text = if (track.isGroup) "📁 ${track.name}" else track.name,
                    color = if (track.visible) DreamsInk else DreamsDimInk,
                    fontSize = 11.sp,
                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("${(track.opacity * 100).roundToInt()}%", color = DreamsDimInk, fontSize = 9.sp)
                    if (hasFx) {
                        Text("FX", color = FilterPurple, fontSize = 8.sp, fontWeight = FontWeight.Bold)
                    }
                    if (track.clipToBelow) {
                        Text("CLIP", color = OpacityCyan, fontSize = 8.sp, fontWeight = FontWeight.Bold)
                    }
                    if (track.alphaLock) {
                        Text("α", color = KeyframeGold, fontSize = 9.sp, fontWeight = FontWeight.Bold)
                    }
                    if (track.muted) {
                        Text("🔇", fontSize = 8.sp)
                    }
                }
            }

            // Track Options & Layer Effects trigger ("⋯")
            Box(
                modifier = Modifier
                    .size(20.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (vm.trackOptionsTrackId == track.id) Pc.Accent else Color.Transparent)
                    .clickable {
                        vm.selectTimelineTrack(track.id)
                        vm.trackOptionsTrackId = if (vm.trackOptionsTrackId == track.id) null else track.id
                        vm.movieSettingsOpen = false
                        vm.addTrackMenuOpen = false
                        vm.playheadActionMenuOpen = false
                        vm.liveFilterPopoverOpen = false
                    },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "⋯",
                    color = if (vm.trackOptionsTrackId == track.id) Color.White else DreamsInk,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Solo toggle ("S")
            Box(
                modifier = Modifier
                    .size(18.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(if (track.solo) KeyframeGold else Color.Transparent)
                    .clickable { vm.toggleTrackSolo(track.id, !track.solo) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "S",
                    color = if (track.solo) Color.Black else DreamsDimInk,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            // Visibility toggle
            Box(
                modifier = Modifier
                    .size(20.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .clickable { vm.toggleTrackVisible(track.id, !track.visible) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = if (track.visible) "👁" else "—",
                    color = if (track.visible) DreamsInk else DreamsDimInk,
                    fontSize = 10.sp,
                )
            }
        }

        if (hasTransform) {
            SubTrackLabelRow("◆ Move & Scale", KeyframeGold, 0.5f)
        }
        if (hasOpacity) {
            SubTrackLabelRow("◆ Opacity", OpacityCyan, 0.65f)
        }
        if (hasFilter) {
            SubTrackLabelRow("◆ Live Filters", FilterPurple, 0.5f)
        }
        if (hasDeform) {
            SubTrackLabelRow("◆ Warp / Distort", WarpPink, 0.65f)
        }
        if (hasAudio) {
            SubTrackLabelRow("◆ Vol / Pan", AudioGreen, 0.5f)
        }
    }
}

@Composable
private fun SubTrackLabelRow(label: String, color: Color, alpha: Float) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(KeySubTrackHeight)
            .background(DreamsTrackBg.copy(alpha = alpha))
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = color, fontSize = 10.sp, fontWeight = FontWeight.Medium)
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
    val hasTransform = track.expanded && (track.transformKeyframes.isNotEmpty() || track.clips.any { it.transformKeyframes.isNotEmpty() })
    val hasOpacity = track.expanded && (track.opacityKeyframes.isNotEmpty() || track.clips.any { it.opacityKeyframes.isNotEmpty() })
    val hasFilter = track.expanded && (track.filterKeyframes.isNotEmpty() || track.clips.any { it.filterKeyframes.isNotEmpty() })
    val hasDeform = track.expanded && (track.clips.any { it.deformKeyframes.isNotEmpty() })
    val hasAudio = track.expanded && track.clips.any { it.kind == TimelineClipKind.AUDIO || it.volumeKeyframes.isNotEmpty() }
    val subCount = listOf(hasTransform, hasOpacity, hasFilter, hasDeform, hasAudio).count { it }
    val totalRowHeight = trackHeight + KeySubTrackHeight * subCount
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
                val clipAccent = when (clip.kind) {
                    TimelineClipKind.AUDIO -> AudioGreen
                    TimelineClipKind.VIDEO -> CameraSky
                    TimelineClipKind.TEXT -> FilterPurple
                    else -> tagColor
                }

                Box(
                    modifier = Modifier
                        .offset(x = clipStartDp, y = 3.dp)
                        .width(clipWidthDp)
                        .height(trackHeight - 6.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (isClipSelected) clipAccent.copy(alpha = 0.28f) else DreamsClipFill)
                        .border(
                            width = if (isClipSelected) 2.dp else 1.dp,
                            color = if (isClipSelected) clipAccent else DreamsEdge,
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
                    // Render Audio Waveform, Cel strips, Video/Text badge, or LayerClip thumbnail
                    when {
                        clip.kind == TimelineClipKind.AUDIO -> {
                            Canvas(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(horizontal = 4.dp, vertical = 4.dp),
                            ) {
                                val peaks = clip.waveformPeaks
                                val midY = size.height * 0.5f
                                drawLine(
                                    color = AudioGreen.copy(alpha = 0.35f),
                                    start = Offset(0f, midY),
                                    end = Offset(size.width, midY),
                                    strokeWidth = 1f,
                                )
                                val pairCount = peaks.size / 2
                                if (pairCount > 0) {
                                    val barW = (size.width / pairCount).coerceAtLeast(1.5f)
                                    for (i in 0 until pairCount) {
                                        val lo = peaks[i * 2].coerceIn(-1f, 1f)
                                        val hi = peaks[i * 2 + 1].coerceIn(-1f, 1f)
                                        val yTop = (midY - hi * size.height * 0.46f).coerceIn(0f, size.height)
                                        val yBot = (midY - lo * size.height * 0.46f).coerceIn(0f, size.height)
                                        val x = (i.toFloat() / pairCount) * size.width
                                        drawLine(
                                            color = if (track.muted) DreamsDimInk else AudioGreen,
                                            start = Offset(x, min(yTop, midY - 1.5f)),
                                            end = Offset(x, max(yBot, midY + 1.5f)),
                                            strokeWidth = (barW * 0.7f).coerceIn(1.5f, 4f),
                                        )
                                    }
                                }
                            }
                            Text(
                                text = "🔊 ${clip.name}",
                                color = DreamsInk,
                                fontSize = 10.sp,
                                fontWeight = FontWeight.SemiBold,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier
                                    .align(Alignment.TopStart)
                                    .padding(start = 6.dp, top = 2.dp),
                            )
                        }

                        clip.kind == TimelineClipKind.CEL && clip.cels.isNotEmpty() -> {
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
                        }

                        else -> {
                            Row(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(horizontal = 6.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                val prefix = when (clip.kind) {
                                    TimelineClipKind.VIDEO -> "🎞 "
                                    TimelineClipKind.TEXT -> "T "
                                    else -> ""
                                }
                                vm.frameThumbnails[clip.primaryLayerId]?.let { thumb ->
                                    Image(
                                        bitmap = thumb,
                                        contentDescription = null,
                                        contentScale = ContentScale.Fit,
                                        modifier = Modifier.size(trackHeight - 14.dp),
                                    )
                                }
                                Text(
                                    text = "$prefix${clip.name}",
                                    color = DreamsInk,
                                    fontSize = 11.sp,
                                    fontWeight = FontWeight.Medium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }

                    // Perform Motion Ghost badge on clips with Ghost 1st Frame enabled
                    if (clip.performOnionGuide) {
                        Text(
                            text = "👻",
                            fontSize = 10.sp,
                            modifier = Modifier
                                .align(Alignment.TopEnd)
                                .padding(end = 14.dp, top = 2.dp),
                        )
                    }

                    // Right-edge Drag Handle to resize duration / hold
                    Box(
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .width(12.dp)
                            .fillMaxHeight()
                            .background(clipAccent.copy(alpha = 0.65f))
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

        // Sub-tracks for Move, Opacity, Live Filters, Warp/Distort, and Audio Volume/Pan Keyframes
        if (hasTransform) {
            KeyframeSubTrackLane(zoomDp, KeyframeGold, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.transformKeyframes) {
                        KeyframeDiamond(kf.frame.toInt(), zoomDp, KeyframeGold) {
                            vm.setTimelinePlayhead(kf.frame.toInt())
                            vm.editingKeyframe = clip.id to kf.frame
                        }
                    }
                }
            }
        }
        if (hasOpacity) {
            KeyframeSubTrackLane(zoomDp, OpacityCyan, 0.65f) {
                for (clip in track.clips) {
                    for (kf in clip.opacityKeyframes) {
                        KeyframeDiamond(kf.frame.toInt(), zoomDp, OpacityCyan) {
                            vm.setTimelinePlayhead(kf.frame.toInt())
                            vm.editingKeyframe = clip.id to kf.frame
                        }
                    }
                }
            }
        }
        if (hasFilter) {
            KeyframeSubTrackLane(zoomDp, FilterPurple, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.filterKeyframes) {
                        KeyframeDiamond(kf.frame.toInt(), zoomDp, FilterPurple) {
                            vm.setTimelinePlayhead(kf.frame.toInt())
                            vm.editingKeyframe = clip.id to kf.frame
                        }
                    }
                }
            }
        }
        if (hasDeform) {
            KeyframeSubTrackLane(zoomDp, WarpPink, 0.65f) {
                for (clip in track.clips) {
                    for (kf in clip.deformKeyframes) {
                        KeyframeDiamond(kf.frame.toInt(), zoomDp, WarpPink) {
                            vm.setTimelinePlayhead(kf.frame.toInt())
                            vm.stageGizmoMode = StageGizmoSubMode.WARP
                            vm.stageGizmoActive = true
                        }
                    }
                }
            }
        }
        if (hasAudio) {
            KeyframeSubTrackLane(zoomDp, AudioGreen, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.volumeKeyframes) {
                        KeyframeDiamond(kf.frame.toInt(), zoomDp, AudioGreen) {
                            vm.setTimelinePlayhead(kf.frame.toInt())
                            vm.editingKeyframe = clip.id to kf.frame
                        }
                    }
                }
            }
        }
    }

    // Long-press Clip Context Menu Card
    contextMenuClip?.let { clip ->
        ClipContextMenuCard(
            vm = vm,
            clip = clip,
            onDismiss = { contextMenuClip = null },
        )
    }
}

@Composable
private fun KeyframeSubTrackLane(
    @Suppress("UNUSED_PARAMETER") zoomDp: Dp,
    @Suppress("UNUSED_PARAMETER") color: Color,
    alpha: Float,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(KeySubTrackHeight)
            .background(DreamsTrackBg.copy(alpha = alpha)),
        content = content,
    )
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
 * Interactive **Stage Keyframe, Warp/Distort, Camera & Live Perform Gizmo** rendered over the upper Stage:
 * - **Move & Scale**: 1-finger pan, 2-finger scale/rotate, Anchor Point pivot, and Motion Path curve.
 * - **Distort**: 4 interactive corner pins driving non-destructive `TimelineDeformInfo.distortCorners`.
 * - **Warp**: 4x4 interactive 16-node mesh driving `TimelineDeformInfo.warpMesh` (plus live **Perform Warp**!).
 * - **Camera**: Global Camera Track framing rectangle over Backstage + Pan/Zoom/Rotate camera keyframes.
 * - **Perform Banner**: Includes **Motion Filtering** and the **Perform Motion Ghost (Ghost 1st Frame)** toggle!
 */
@Composable
private fun BoxScope.StageKeyTransformOverlay(
    vm: CanvasViewModel,
    tl: TimelineState,
    stageWidth: Dp,
    stageHeight: Dp,
) {
    val activeClip = remember(tl) { activeClipOf(tl) }
    val activeClipId = activeClip?.id
    val isCamera = vm.stageGizmoMode == StageGizmoSubMode.CAMERA
    val isDistort = vm.stageGizmoMode == StageGizmoSubMode.DISTORT
    val isWarp = vm.stageGizmoMode == StageGizmoSubMode.WARP
    if (!isCamera && activeClipId == null) return

    val view = LocalView.current
    val currentPlayhead by rememberUpdatedState(tl.playhead.toInt())
    val baseTransform = if (isCamera) tl.cameraTransform else tl.activeEvaluatedTransform
    var isInteracting by remember { mutableStateOf(false) }
    var didMutateTransform by remember { mutableStateOf(false) }
    var liveTransform by remember(activeClipId, isCamera) {
        mutableStateOf(baseTransform)
    }
    var warpPoints by remember(activeClipId) {
        val raw = tl.activeEvaluatedDeform.warpMesh
        mutableStateOf(if (raw.size >= 32) raw.take(32) else defaultIdentityDeformPoints())
    }
    var distortCorners by remember(activeClipId) {
        val raw = tl.activeEvaluatedDeform.distortCorners
        mutableStateOf(
            if (raw.size >= 8) {
                listOf(raw[0] to raw[1], raw[2] to raw[3], raw[4] to raw[5], raw[6] to raw[7])
            } else {
                listOf(0f to 0f, 1f to 0f, 1f to 1f, 0f to 1f)
            },
        )
    }
    // Sync Stage gizmo state from evaluated timeline ONLY when not actively dragging/recording Perform,
    // so live Perform playback never resets liveTransform or warp handles mid-gesture!
    LaunchedEffect(activeClipId, isCamera, tl.playhead, baseTransform, isInteracting, vm.performRecording) {
        if (!isInteracting && !vm.performRecording) {
            liveTransform = baseTransform
        }
    }
    LaunchedEffect(activeClipId, tl.playhead, tl.activeEvaluatedDeform, isInteracting, vm.performRecording) {
        if (!isInteracting && !vm.performRecording) {
            val rawW = tl.activeEvaluatedDeform.warpMesh
            warpPoints = if (rawW.size >= 32) rawW.take(32) else defaultIdentityDeformPoints()
            val rawD = tl.activeEvaluatedDeform.distortCorners
            distortCorners = if (rawD.size >= 8) {
                listOf(rawD[0] to rawD[1], rawD[2] to rawD[3], rawD[4] to rawD[5], rawD[6] to rawD[7])
            } else {
                listOf(0f to 0f, 1f to 0f, 1f to 1f, 0f to 1f)
            }
        }
    }
    var activeDeformHandle by remember { mutableIntStateOf(-1) }

    val gizmoColor = when {
        vm.dreamsMode == DreamsWorkMode.PERFORM -> PerformRed
        isCamera -> CameraSky
        isWarp || isDistort -> WarpPink
        else -> KeyframeGold
    }

    Box(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .width(stageWidth)
            .height(stageHeight)
            // Procreate-style 2-finger tap = Undo, 3-finger tap = Redo right over Perform & Stage Gizmo
            .multiFingerUndoRedo(
                onUndo = { vm.undo() },
                onRedo = { vm.redo() },
            )
            // Move & Scale / Camera multi-touch gestures
            .then(
                if (!isWarp && !isDistort) {
                    Modifier
                        .pointerInput(activeClipId, isCamera, vm.dreamsMode) {
                            detectTransformGestures { _, pan, zoom, rotationDeg ->
                                val hasDelta = pan != Offset.Zero || abs(zoom - 1f) > 0.0005f || abs(rotationDeg) > 0.01f
                                if (!hasDelta) return@detectTransformGestures
                                isInteracting = true
                                didMutateTransform = true
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
                                when {
                                    isCamera -> vm.setCameraTransformKeyframe(currentPlayhead, updated, done = false)
                                    vm.dreamsMode == DreamsWorkMode.PERFORM -> vm.recordPerformTransform(updated)
                                    activeClipId != null -> vm.setClipTransformKeyframe(activeClipId, currentPlayhead, updated, done = false)
                                }
                            }
                        }
                        .pointerInput(activeClipId, isCamera, vm.dreamsMode) {
                            awaitEachGesture {
                                awaitFirstDown(requireUnconsumed = false)
                                do {
                                    val event = awaitPointerEvent()
                                } while (event.changes.any { it.pressed })
                                val mutated = didMutateTransform
                                isInteracting = false
                                didMutateTransform = false
                                when {
                                    vm.dreamsMode == DreamsWorkMode.PERFORM && vm.performRecording -> vm.finishPerformRecording()
                                    mutated && isCamera -> vm.setCameraTransformKeyframe(currentPlayhead, liveTransform, done = true)
                                    mutated && vm.stageGizmoActive && activeClipId != null -> vm.setClipTransformKeyframe(activeClipId, currentPlayhead, liveTransform, done = true)
                                }
                            }
                        }
                } else {
                    // Distort (4 corners) or Warp (4x4 16 nodes) drag interaction (+ Perform Warp!)
                    Modifier.pointerInput(activeClipId, isWarp, isDistort, vm.dreamsMode) {
                        detectDragGestures(
                            onDragStart = { pos ->
                                val bx = size.width * 0.25f
                                val by = size.height * 0.22f
                                val bw = size.width * 0.50f
                                val bh = size.height * 0.56f
                                var bestIdx = -1
                                var bestDist = 72f
                                if (isDistort) {
                                    for (i in 0..3) {
                                        val hx = bx + distortCorners[i].first * bw
                                        val hy = by + distortCorners[i].second * bh
                                        val d = hypot(pos.x - hx, pos.y - hy)
                                        if (d < bestDist) {
                                            bestDist = d
                                            bestIdx = i
                                        }
                                    }
                                } else {
                                    for (i in 0 until 16) {
                                        val hx = bx + warpPoints[i * 2] * bw
                                        val hy = by + warpPoints[i * 2 + 1] * bh
                                        val d = hypot(pos.x - hx, pos.y - hy)
                                        if (d < bestDist) {
                                            bestDist = d
                                            bestIdx = i
                                        }
                                    }
                                }
                                activeDeformHandle = bestIdx
                                if (bestIdx >= 0) {
                                    isInteracting = true
                                    if (vm.dreamsMode == DreamsWorkMode.PERFORM && !vm.performRecording) {
                                        vm.beginPerformRecording()
                                    }
                                }
                            },
                            onDrag = { change, drag ->
                                val idx = activeDeformHandle
                                if (idx < 0 || activeClipId == null) return@detectDragGestures
                                change.consume()
                                val bw = (size.width * 0.50f).coerceAtLeast(1f)
                                val bh = (size.height * 0.56f).coerceAtLeast(1f)
                                val du = drag.x / bw
                                val dv = drag.y / bh
                                val info = if (isDistort) {
                                    val next = distortCorners.toMutableList()
                                    val cur = next[idx]
                                    next[idx] = (cur.first + du).coerceIn(-0.5f, 1.5f) to (cur.second + dv).coerceIn(-0.5f, 1.5f)
                                    distortCorners = next
                                    TimelineDeformInfo(
                                        distortCorners = next.flatMap { listOf(it.first, it.second) },
                                        warpMesh = emptyList(),
                                    )
                                } else {
                                    val next = warpPoints.toMutableList()
                                    next[idx * 2] = (next[idx * 2] + du).coerceIn(-0.5f, 1.5f)
                                    next[idx * 2 + 1] = (next[idx * 2 + 1] + dv).coerceIn(-0.5f, 1.5f)
                                    warpPoints = next
                                    TimelineDeformInfo(
                                        distortCorners = emptyList(),
                                        warpMesh = next,
                                    )
                                }
                                if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                                    vm.recordPerformDeform(info)
                                } else {
                                    vm.setClipDeformKeyframe(activeClipId, currentPlayhead, info, done = false)
                                }
                            },
                            onDragEnd = {
                                if (activeDeformHandle >= 0 && activeClipId != null) {
                                    if (vm.dreamsMode == DreamsWorkMode.PERFORM && vm.performRecording) {
                                        vm.finishPerformRecording()
                                    } else {
                                        val info = if (isDistort) {
                                            TimelineDeformInfo(
                                                distortCorners = distortCorners.flatMap { listOf(it.first, it.second) },
                                                warpMesh = emptyList(),
                                            )
                                        } else {
                                            TimelineDeformInfo(
                                                distortCorners = emptyList(),
                                                warpMesh = warpPoints,
                                            )
                                        }
                                        vm.setClipDeformKeyframe(activeClipId, currentPlayhead, info, done = true)
                                    }
                                }
                                isInteracting = false
                                activeDeformHandle = -1
                            },
                            onDragCancel = {
                                if (vm.performRecording) {
                                    vm.cancelPerformRecording()
                                }
                                isInteracting = false
                                activeDeformHandle = -1
                            },
                        )
                    }
                },
            ),
    ) {
        // Visual Stage Gizmo Canvas (Move & Scale + Motion Path, Distort 4-pin, Warp 4x4 Mesh, or Camera Viewport)
        Canvas(modifier = Modifier.fillMaxSize()) {
            // Draw Motion Path across transform keyframes when in Move/Perform/Camera mode
            val pathKeyframes = if (isCamera) tl.cameraTransformKeyframes else activeClip?.transformKeyframes.orEmpty()
            if (!isWarp && !isDistort && pathKeyframes.size >= 2) {
                val motionPath = Path()
                pathKeyframes.forEachIndexed { i, kf ->
                    val t = kf.transform ?: return@forEachIndexed
                    val px = size.width * 0.5f + t.x * 0.25f
                    val py = size.height * 0.5f + t.y * 0.25f
                    if (i == 0) motionPath.moveTo(px, py) else motionPath.lineTo(px, py)
                    drawCircle(color = gizmoColor.copy(alpha = 0.85f), radius = 4.5f, center = Offset(px, py))
                }
                drawPath(
                    path = motionPath,
                    color = gizmoColor.copy(alpha = 0.65f),
                    style = Stroke(width = 2.5f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(8f, 6f))),
                )
            }

            if (isDistort || isWarp) {
                val bx = size.width * 0.25f
                val by = size.height * 0.22f
                val bw = size.width * 0.50f
                val bh = size.height * 0.56f
                val mesh = if (isDistort) distortCornersToDeformPoints(distortCorners) else warpPoints
                fun pt(row: Int, col: Int): Offset {
                    val idx = (row * 4 + col) * 2
                    return Offset(bx + mesh[idx] * bw, by + mesh[idx + 1] * bh)
                }
                // Draw 4x4 grid lines
                for (r in 0..3) {
                    for (c in 0..2) {
                        drawLine(gizmoColor.copy(alpha = 0.7f), pt(r, c), pt(r, c + 1), strokeWidth = 2f)
                    }
                }
                for (c in 0..3) {
                    for (r in 0..2) {
                        drawLine(gizmoColor.copy(alpha = 0.7f), pt(r, c), pt(r + 1, c), strokeWidth = 2f)
                    }
                }
                // Draw draggable handles
                if (isDistort) {
                    for ((i, c) in distortCorners.withIndex()) {
                        val o = Offset(bx + c.first * bw, by + c.second * bh)
                        drawCircle(Color.White, radius = if (i == activeDeformHandle) 12f else 9f, center = o)
                        drawCircle(gizmoColor, radius = if (i == activeDeformHandle) 9f else 6.5f, center = o)
                    }
                } else {
                    for (i in 0 until 16) {
                        val o = Offset(bx + warpPoints[i * 2] * bw, by + warpPoints[i * 2 + 1] * bh)
                        drawCircle(Color.White, radius = if (i == activeDeformHandle) 10f else 7f, center = o)
                        drawCircle(gizmoColor, radius = if (i == activeDeformHandle) 7.5f else 5f, center = o)
                    }
                }
            } else {
                val cx = size.width * 0.5f + liveTransform.x * 0.25f
                val cy = size.height * 0.5f + liveTransform.y * 0.25f
                val boxW = (size.width * (if (isCamera) 0.60f else 0.42f) * liveTransform.scaleX).coerceIn(40f, size.width * 0.92f)
                val boxH = (size.height * (if (isCamera) 0.58f else 0.42f) * liveTransform.scaleY).coerceIn(40f, size.height * 0.92f)

                drawRoundRect(
                    color = gizmoColor,
                    topLeft = Offset(cx - boxW / 2f, cy - boxH / 2f),
                    size = Size(boxW, boxH),
                    cornerRadius = CornerRadius(if (isCamera) 2f else 8f, if (isCamera) 2f else 8f),
                    style = Stroke(
                        width = 3f,
                        pathEffect = if (isCamera) PathEffect.dashPathEffect(floatArrayOf(12f, 6f)) else null,
                    ),
                )
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
                val ax = (cx - boxW / 2f) + boxW * liveTransform.anchorX
                val ay = (cy - boxH / 2f) + boxH * liveTransform.anchorY
                drawCircle(color = gizmoColor, radius = 12f, center = Offset(ax, ay), style = Stroke(width = 2.5f))
                drawLine(color = gizmoColor, start = Offset(ax - 16f, ay), end = Offset(ax + 16f, ay), strokeWidth = 2.5f)
                drawLine(color = gizmoColor, start = Offset(ax, ay - 16f), end = Offset(ax, ay + 16f), strokeWidth = 2.5f)
            }
        }

        // Top Stage Pill: Sub-mode tabs (Move | Distort | Warp | Cam) + Perform Status & Ghost 1st Frame toggle!
        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = Pc.barShown + 8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(18.dp))
                    .background(DreamsBarFill.copy(alpha = 0.95f))
                    .border(1.dp, gizmoColor, RoundedCornerShape(18.dp))
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // Sub-mode switcher
                for ((sub, label) in listOf(
                    StageGizmoSubMode.MOVE_SCALE to "Move",
                    StageGizmoSubMode.DISTORT to "Distort",
                    StageGizmoSubMode.WARP to "Warp",
                    StageGizmoSubMode.CAMERA to "Camera",
                )) {
                    val chosen = vm.stageGizmoMode == sub
                    Text(
                        text = label,
                        color = if (chosen) Color.White else DreamsDimInk,
                        fontSize = 11.sp,
                        fontWeight = if (chosen) FontWeight.Bold else FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (chosen) gizmoColor else Color.Transparent)
                            .clickable {
                                vm.stageGizmoMode = sub
                                if (sub == StageGizmoSubMode.CAMERA) vm.setCameraEnabled(true)
                            }
                            .padding(horizontal = 8.dp, vertical = 3.dp),
                    )
                }

                Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))

                // Reset current gizmo property
                Text(
                    text = "Reset",
                    color = DreamsInk,
                    fontSize = 11.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsClipFill)
                        .clickable {
                            when {
                                isCamera -> {
                                    val id = TimelineTransformInfo(0f, 0f, 1f, 1f, 0f, 0.5f, 0.5f)
                                    liveTransform = id
                                    vm.setCameraTransformKeyframe(tl.playhead.toInt(), id, done = true)
                                }
                                isDistort && activeClipId != null -> {
                                    distortCorners = listOf(0f to 0f, 1f to 0f, 1f to 1f, 0f to 1f)
                                    vm.deleteClipDeformKeyframe(activeClipId, tl.playhead.toInt())
                                }
                                isWarp && activeClipId != null -> {
                                    warpPoints = defaultIdentityDeformPoints()
                                    vm.deleteClipDeformKeyframe(activeClipId, tl.playhead.toInt())
                                }
                                activeClipId != null -> {
                                    val id = TimelineTransformInfo(0f, 0f, 1f, 1f, 0f, 0.5f, 0.5f)
                                    liveTransform = id
                                    vm.setClipTransformKeyframe(activeClipId, tl.playhead.toInt(), id, done = true)
                                }
                            }
                        }
                        .padding(horizontal = 7.dp, vertical = 3.dp),
                )
            }

            // Second Row when in Perform Mode: Recording indicator + Undo + Motion Filtering + Perform Motion Ghost!
            if (vm.dreamsMode == DreamsWorkMode.PERFORM && activeClip != null) {
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(16.dp))
                        .background(DreamsBarFill.copy(alpha = 0.94f))
                        .border(1.dp, PerformRed, RoundedCornerShape(16.dp))
                        .padding(horizontal = 12.dp, vertical = 5.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        text = if (vm.performRecording) "● REC (${vm.performSamplesCount})" else "● Perform Ready",
                        color = PerformRed,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    val undoReady = vm.canUndo || vm.performRecording
                    Text(
                        text = "↶ Undo",
                        color = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.45f),
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
                            .background(if (undoReady) DreamsClipFill else Color.Transparent)
                            .clickable(enabled = undoReady) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                vm.undo()
                            }
                            .padding(horizontal = 7.dp, vertical = 3.dp),
                    )
                    val filterPct = (tl.motionFiltering * 100).roundToInt()
                    Text(
                        text = "Filter: $filterPct%",
                        color = DreamsInk,
                        fontSize = 11.sp,
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
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
                            .padding(horizontal = 7.dp, vertical = 3.dp),
                    )
                    // DreamsCraft v2.0 Pilar 2: Perform Motion Ghost (Onion Skin of 1st frame along Perform motion)
                    val ghostOn = activeClip.performOnionGuide
                    Text(
                        text = if (ghostOn) "👻 Ghost 1st Frame: ON (${(activeClip.performGuideOpacity * 100).roundToInt()}%)" else "👻 Ghost 1st Frame: OFF",
                        color = if (ghostOn) Color.Black else OpacityCyan,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (ghostOn) OpacityCyan else DreamsClipFill)
                            .clickable {
                                vm.setPerformOnionGuide(
                                    clipId = activeClip.id,
                                    enabled = !ghostOn,
                                    isolateFirstFrame = true,
                                    opacity = activeClip.performGuideOpacity.coerceIn(0.2f, 0.85f),
                                    colorRgb = activeClip.performGuideColor.ifEmpty { listOf(0.15f, 0.85f, 1.0f) },
                                )
                            }
                            .padding(horizontal = 8.dp, vertical = 3.dp),
                    )
                }
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
            .width(340.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(DreamsBarFill)
            .border(1.dp, DreamsEdge, RoundedCornerShape(20.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Movie, Auto-Draw & Onion Skin", color = DreamsInk, fontSize = 16.sp, fontWeight = FontWeight.Bold)
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
                        .padding(horizontal = 10.dp, vertical = 5.dp),
                ) {
                    Text(
                        text = "$presetFps",
                        color = if (chosen) Color.White else DreamsInk,
                        fontSize = 12.sp,
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
                        .padding(horizontal = 10.dp, vertical = 5.dp),
                ) {
                    Text(label, color = if (chosen) Color.White else DreamsInk, fontSize = 12.sp)
                }
            }
        }

        // Onion Skin Frames
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Onion Skin Frames (${tl.onion.beforeFrames})", color = DreamsInk, fontSize = 12.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
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
                            .padding(horizontal = 7.dp, vertical = 4.dp),
                    ) {
                        Text(
                            text = if (count == 0u) "Off" else "$count",
                            color = if (chosen) Color.Black else DreamsInk,
                            fontSize = 11.sp,
                        )
                    }
                }
            }
        }

        // DreamsCraft v2.0 Pilar 1: Auto-Draw Frame & Auto-Split Hold Toggles
        ToggleRow("Auto-Create Frame on Empty Draw", tl.autoCreateFrameOnDraw) {
            vm.setAutoDrawOptions(!tl.autoCreateFrameOnDraw, tl.autoSplitCelOnDraw)
        }
        ToggleRow("Auto-Split Held Cel on Draw", tl.autoSplitCelOnDraw) {
            vm.setAutoDrawOptions(tl.autoCreateFrameOnDraw, !tl.autoSplitCelOnDraw)
        }
        ToggleRow("Lip-Sync Audio Scrubbing", vm.audioScrubbingEnabled) {
            vm.audioScrubbingEnabled = !vm.audioScrubbingEnabled
        }
        ToggleRow("Global Camera Track", tl.cameraEnabled) {
            vm.setCameraEnabled(!tl.cameraEnabled)
        }

        // Work Area & Backstage Toggles
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            val hasWorkArea = tl.workArea != null
            Text(
                text = if (hasWorkArea) "Clear Work Area" else "Set Work Area",
                color = Pc.Accent,
                fontSize = 12.sp,
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
    val audioPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        vm.addTrackMenuOpen = false
        uri?.let { vm.importTimelineAudio(it) }
    }
    val videoPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        vm.addTrackMenuOpen = false
        uri?.let { vm.importTimelineVideo(it) }
    }

    Column(
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = 16.dp, bottom = bottomOffset)
            .width(265.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsBarFill)
            .border(1.dp, DreamsEdge, RoundedCornerShape(18.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
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
        MenuActionRow("+ New Text Clip (Animated Title)") {
            vm.addTrackMenuOpen = false
            vm.addTextClipToActiveTrack("Title")
        }
        MenuActionRow("🔊 Import Audio Track (WAV/MP3/AAC)", color = AudioGreen) {
            audioPicker.launch(arrayOf("audio/*"))
        }
        MenuActionRow("🎞 Import Video Clip (Rotoscope)", color = CameraSky) {
            videoPicker.launch(arrayOf("video/*"))
        }
        MenuActionRow("+ New Cel / Frame in Active Clip") {
            vm.addTrackMenuOpen = false
            vm.addTimelineCel()
        }
        MenuActionRow("◆ Add Live Filter Keyframe (FX)", color = FilterPurple) {
            vm.addTrackMenuOpen = false
            vm.liveFilterPopoverOpen = true
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
            .width(260.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsBarFill)
            .border(1.dp, PerformRed, RoundedCornerShape(18.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Text(
            text = "Playhead Action (Frame ${tl.playhead + 1u})",
            color = PerformRed,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
        )
        MenuActionRow("Move & Scale (Keyframe)", color = KeyframeGold) {
            vm.playheadActionMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipTransformKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedTransform, done = true)
                vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                vm.stageGizmoActive = true
            }
        }
        MenuActionRow("Warp / Distort (Keyframe)", color = WarpPink) {
            vm.playheadActionMenuOpen = false
            vm.stageGizmoMode = StageGizmoSubMode.WARP
            vm.stageGizmoActive = true
        }
        MenuActionRow("Live Filters (Blur / HSB / Noise)", color = FilterPurple) {
            vm.playheadActionMenuOpen = false
            vm.liveFilterPopoverOpen = true
        }
        MenuActionRow("Camera Track Keyframe", color = CameraSky) {
            vm.playheadActionMenuOpen = false
            vm.setCameraTransformKeyframe(tl.playhead.toInt(), tl.cameraTransform, done = true)
            vm.stageGizmoMode = StageGizmoSubMode.CAMERA
            vm.stageGizmoActive = true
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

/**
 * DreamsCraft v2.0 Pilar 3: **Complete Track Options & GPU Layer Effects Popover**:
 * - Tab 1 (**Options**): Rename, Color Swatches, Blend Mode, Opacity, Clip to Track Below,
 *   Alpha Lock, Mute Audio, Lock, Move Up/Down, Duplicate, Merge Down, Clear Clips, Delete Track.
 * - Tab 2 (**Layer Effects**): Non-destructive GPU Outline/Stroke, Drop Shadow, Outer/Inner Glow,
 *   and Color Overlay applied across the entire track!
 */
@Composable
private fun BoxScope.TrackOptionsPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    track: TimelineTrackInfo,
    bottomOffset: Dp,
) {
    var tab by remember(track.id) { mutableIntStateOf(0) } // 0 = Options, 1 = Layer Effects
    var editName by remember(track.id, track.name) { mutableStateOf(track.name) }
    var trackOpacity by remember(track.id, track.opacity) { mutableFloatStateOf(track.opacity) }
    var fx by remember(track.id, track.effects) { mutableStateOf(track.effects) }
    val scroll = rememberScrollState()

    Column(
        modifier = Modifier
            .align(Alignment.BottomStart)
            .padding(start = 12.dp, bottom = bottomOffset)
            .width(340.dp)
            .heightIn(max = 420.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(DreamsBarFill)
            .border(1.dp, Pc.Accent, RoundedCornerShape(20.dp))
            .padding(14.dp)
            .verticalScroll(scroll),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Header + Tab Switcher
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                ModeChip("Track Options", tab == 0, Pc.Accent) { tab = 0 }
                ModeChip("Layer Effects", tab == 1, FilterPurple) { tab = 1 }
            }
            Text(
                text = "Done",
                color = Pc.Accent,
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.clickable { vm.trackOptionsTrackId = null },
            )
        }

        if (tab == 0) {
            // Rename Track
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BasicTextField(
                    value = editName,
                    onValueChange = { editName = it },
                    singleLine = true,
                    textStyle = TextStyle(color = DreamsInk, fontSize = 13.sp, fontWeight = FontWeight.Medium),
                    cursorBrush = SolidColor(Pc.Accent),
                    modifier = Modifier
                        .weight(1f)
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsClipFill)
                        .border(1.dp, DreamsEdge, RoundedCornerShape(8.dp))
                        .padding(horizontal = 10.dp, vertical = 7.dp),
                )
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(Pc.Accent)
                        .clickable { vm.renameTimelineTrack(track.id, editName) }
                        .padding(horizontal = 10.dp, vertical = 7.dp),
                ) {
                    Text("Save", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                }
            }

            // Color Tag Swatches
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Color", color = DreamsDimInk, fontSize = 12.sp)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (argb in TrackColorPalette) {
                        val c = Color(0xFF000000L or (argb.toLong() and 0xFFFFFFL))
                        val chosen = (track.colorTag and 0xFFFFFFu) == (argb and 0xFFFFFFu)
                        Box(
                            modifier = Modifier
                                .size(22.dp)
                                .clip(CircleShape)
                                .background(c)
                                .border(if (chosen) 2.5.dp else 1.dp, if (chosen) Color.White else DreamsEdge, CircleShape)
                                .clickable { vm.setTimelineTrackColor(track.id, argb) },
                        )
                    }
                }
            }

            // Opacity Slider
            CompactSliderRow(
                label = "Opacity",
                valueText = "${(trackOpacity * 100).roundToInt()}%",
                value = trackOpacity,
                range = 0f..1f,
                onValueChange = {
                    trackOpacity = it
                    vm.setTimelineTrackOpacity(track.id, it, done = false)
                },
                onValueChangeFinished = { vm.setTimelineTrackOpacity(track.id, trackOpacity, done = true) },
            )

            // Blend Mode Chips
            Text("Blend Mode", color = DreamsDimInk, fontSize = 11.sp)
            Row(
                modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for ((blendId, blendName) in TrackBlendPresets) {
                    val chosen = track.blend == blendId
                    Text(
                        text = blendName,
                        color = if (chosen) Color.White else DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = if (chosen) FontWeight.Bold else FontWeight.Normal,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (chosen) Pc.Accent else DreamsClipFill)
                            .clickable { vm.setTimelineTrackBlend(track.id, blendId) }
                            .padding(horizontal = 9.dp, vertical = 5.dp),
                    )
                }
            }

            // Masking, Alpha Lock, Mute & Lock toggles
            ToggleRow("Clip to Track Below (Mask)", track.clipToBelow) {
                vm.setTimelineTrackClipToBelow(track.id, !track.clipToBelow)
            }
            ToggleRow("Alpha Lock (All Cels)", track.alphaLock) {
                vm.setTimelineTrackAlphaLock(track.id, !track.alphaLock)
            }
            ToggleRow("Mute Track Audio", track.muted) {
                vm.setTimelineTrackMuted(track.id, !track.muted)
            }
            ToggleRow("Lock Track", track.locked) {
                vm.toggleTrackLocked(track.id, !track.locked)
            }

            // Reorder & Management Actions
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Box(Modifier.weight(1f)) { MenuActionRow("▲ Move Up") { vm.moveTimelineTrack(track.id, 1) } }
                Box(Modifier.weight(1f)) { MenuActionRow("▼ Move Down") { vm.moveTimelineTrack(track.id, -1) } }
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Box(Modifier.weight(1f)) { MenuActionRow("Duplicate") { vm.duplicateTimelineTrack(track.id) } }
                Box(Modifier.weight(1f)) { MenuActionRow("Merge Down") { vm.mergeTimelineTrackDown(track.id) } }
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Box(Modifier.weight(1f)) { MenuActionRow("Clear Clips") { vm.clearTimelineTrackClips(track.id) } }
                Box(Modifier.weight(1f)) {
                    MenuActionRow("Delete Track", color = PerformRed) {
                        vm.deleteTimelineTrack(track.id)
                    }
                }
            }
        } else {
            // GPU Non-Destructive Track Layer Effects
            ToggleRow("Outline / Stroke", fx.outlineEnabled) {
                fx = fx.copy(outlineEnabled = !fx.outlineEnabled)
                vm.setTimelineTrackEffects(track.id, fx, done = true)
            }
            if (fx.outlineEnabled) {
                CompactSliderRow(
                    label = "Outline Size",
                    valueText = "${fx.outlineWidth.roundToInt()}px",
                    value = fx.outlineWidth,
                    range = 1f..24f,
                    onValueChange = {
                        fx = fx.copy(outlineWidth = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
            }

            ToggleRow("Drop Shadow", fx.shadowEnabled) {
                fx = fx.copy(shadowEnabled = !fx.shadowEnabled)
                vm.setTimelineTrackEffects(track.id, fx, done = true)
            }
            if (fx.shadowEnabled) {
                CompactSliderRow(
                    label = "Shadow Offset X",
                    valueText = "${fx.shadowOffsetX.roundToInt()}px",
                    value = fx.shadowOffsetX,
                    range = -32f..32f,
                    onValueChange = {
                        fx = fx.copy(shadowOffsetX = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
                CompactSliderRow(
                    label = "Shadow Offset Y",
                    valueText = "${fx.shadowOffsetY.roundToInt()}px",
                    value = fx.shadowOffsetY,
                    range = -32f..32f,
                    onValueChange = {
                        fx = fx.copy(shadowOffsetY = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
            }

            ToggleRow(if (fx.glowInner) "Inner Glow" else "Outer Glow", fx.glowEnabled) {
                fx = fx.copy(glowEnabled = !fx.glowEnabled)
                vm.setTimelineTrackEffects(track.id, fx, done = true)
            }
            if (fx.glowEnabled) {
                ToggleRow("Inner Glow Mode", fx.glowInner) {
                    fx = fx.copy(glowInner = !fx.glowInner)
                    vm.setTimelineTrackEffects(track.id, fx, done = true)
                }
                CompactSliderRow(
                    label = "Glow Radius",
                    valueText = "${fx.glowRadius.roundToInt()}px",
                    value = fx.glowRadius,
                    range = 1f..24f,
                    onValueChange = {
                        fx = fx.copy(glowRadius = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
                CompactSliderRow(
                    label = "Glow Intensity",
                    valueText = "${(fx.glowIntensity * 100).roundToInt()}%",
                    value = fx.glowIntensity,
                    range = 0f..1f,
                    onValueChange = {
                        fx = fx.copy(glowIntensity = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
            }

            ToggleRow("Color Overlay", fx.colorOverlayEnabled) {
                fx = fx.copy(colorOverlayEnabled = !fx.colorOverlayEnabled)
                vm.setTimelineTrackEffects(track.id, fx, done = true)
            }
            if (fx.colorOverlayEnabled) {
                val curAlpha = fx.colorOverlayColor.getOrNull(3) ?: 0.5f
                CompactSliderRow(
                    label = "Overlay Opacity",
                    valueText = "${(curAlpha * 100).roundToInt()}%",
                    value = curAlpha,
                    range = 0f..1f,
                    onValueChange = { a ->
                        val base = fx.colorOverlayColor
                        val r = base.getOrNull(0) ?: 1f
                        val g = base.getOrNull(1) ?: 0.2f
                        val b = base.getOrNull(2) ?: 0.4f
                        fx = fx.copy(colorOverlayColor = listOf(r, g, b, a))
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
            }
        }
    }
}

/**
 * DreamsCraft v2.0 Pilar 5: **Animated Live Filters Popover** (Opacity, Gaussian Blur, HSB,
 * Sharpen, Noise). Every slider change automatically inserts/updates a non-destructive keyframe
 * at `tl.playhead` on the active clip (or Camera Track when Camera mode is active).
 */
@Composable
private fun BoxScope.LiveFilterPopover(
    vm: CanvasViewModel,
    tl: TimelineState,
    bottomOffset: Dp,
) {
    val activeClipId = tl.activeClipId
    val isCam = vm.stageGizmoMode == StageGizmoSubMode.CAMERA
    val baseFilter = if (isCam) tl.cameraFilter else tl.activeEvaluatedFilter
    var filter by remember(activeClipId, isCam, tl.playhead, baseFilter) { mutableStateOf(baseFilter) }
    var opacity by remember(activeClipId, tl.playhead, tl.activeEvaluatedOpacity) { mutableFloatStateOf(tl.activeEvaluatedOpacity) }

    fun pushFilter(next: TimelineLiveFilterInfo, done: Boolean) {
        filter = next
        if (isCam) {
            vm.setCameraFilterKeyframe(tl.playhead.toInt(), next, done)
        } else if (activeClipId != null) {
            vm.setClipFilterKeyframe(activeClipId, tl.playhead.toInt(), next, done)
        }
    }

    Column(
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = 16.dp, bottom = bottomOffset)
            .width(320.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(DreamsBarFill)
            .border(1.dp, FilterPurple, RoundedCornerShape(20.dp))
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (isCam) "Camera Live Filters @ F${tl.playhead + 1u}" else "Live Filters @ F${tl.playhead + 1u}",
                color = FilterPurple,
                fontSize = 14.sp,
                fontWeight = FontWeight.Bold,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    text = "Reset",
                    color = DreamsDimInk,
                    fontSize = 12.sp,
                    modifier = Modifier.clickable {
                        pushFilter(TimelineLiveFilterInfo(0f, 0f, 1f, 0f, 0f, 0f), done = true)
                    },
                )
                Text(
                    text = "Done",
                    color = Pc.Accent,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.clickable { vm.liveFilterPopoverOpen = false },
                )
            }
        }

        if (!isCam && activeClipId != null) {
            CompactSliderRow(
                label = "Opacity",
                valueText = "${(opacity * 100).roundToInt()}%",
                value = opacity,
                range = 0f..1f,
                onValueChange = {
                    opacity = it
                    vm.setClipOpacityKeyframe(activeClipId, tl.playhead.toInt(), it, done = false)
                },
                onValueChangeFinished = {
                    vm.setClipOpacityKeyframe(activeClipId, tl.playhead.toInt(), opacity, done = true)
                },
            )
        }

        CompactSliderRow(
            label = "Gaussian Blur",
            valueText = "${filter.gaussianBlur.roundToInt()}px",
            value = filter.gaussianBlur,
            range = 0f..48f,
            onValueChange = { pushFilter(filter.copy(gaussianBlur = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
        CompactSliderRow(
            label = "Hue Shift",
            valueText = "${filter.hueShift.roundToInt()}°",
            value = filter.hueShift,
            range = -180f..180f,
            onValueChange = { pushFilter(filter.copy(hueShift = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
        CompactSliderRow(
            label = "Saturation",
            valueText = "${(filter.saturation * 100).roundToInt()}%",
            value = filter.saturation,
            range = 0f..2f,
            onValueChange = { pushFilter(filter.copy(saturation = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
        CompactSliderRow(
            label = "Brightness",
            valueText = "${(filter.brightness * 100).roundToInt()}%",
            value = filter.brightness,
            range = -1f..1f,
            onValueChange = { pushFilter(filter.copy(brightness = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
        CompactSliderRow(
            label = "Sharpen",
            valueText = "${(filter.sharpen * 100).roundToInt()}%",
            value = filter.sharpen,
            range = 0f..2f,
            onValueChange = { pushFilter(filter.copy(sharpen = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
        CompactSliderRow(
            label = "Noise / Grain",
            valueText = "${(filter.noise * 100).roundToInt()}%",
            value = filter.noise,
            range = 0f..1f,
            onValueChange = { pushFilter(filter.copy(noise = it), done = false) },
            onValueChangeFinished = { pushFilter(filter, done = true) },
        )
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
                vm.setClipFilterEasing(clipId, frame, kind)
                vm.editingKeyframe = null
            }
        }
        MenuActionRow("Delete Keyframe", color = PerformRed) {
            vm.deleteClipTransformKeyframe(clipId, frame)
            vm.deleteClipOpacityKeyframe(clipId, frame)
            vm.deleteClipFilterKeyframe(clipId, frame)
            vm.deleteClipDeformKeyframe(clipId, frame)
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
            text = if (clip.performOnionGuide) "👻 Ghost:ON" else "👻 Ghost",
            color = if (clip.performOnionGuide) OpacityCyan else DreamsInk,
            fontSize = 12.sp,
            modifier = Modifier.clickable {
                vm.setPerformOnionGuide(
                    clipId = clip.id,
                    enabled = !clip.performOnionGuide,
                    isolateFirstFrame = true,
                    opacity = clip.performGuideOpacity.coerceIn(0.2f, 0.85f),
                    colorRgb = clip.performGuideColor.ifEmpty { listOf(0.15f, 0.85f, 1.0f) },
                )
            },
        )
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
private fun ToggleRow(
    label: String,
    checked: Boolean,
    onToggle: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(DreamsClipFill)
            .clickable(onClick = onToggle)
            .padding(horizontal = 10.dp, vertical = 7.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = DreamsInk, fontSize = 12.sp)
        Text(
            text = if (checked) "ON" else "OFF",
            color = if (checked) Pc.Accent else DreamsDimInk,
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun CompactSliderRow(
    label: String,
    valueText: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    onValueChange: (Float) -> Unit,
    onValueChangeFinished: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(label, color = DreamsInk, fontSize = 11.sp)
            Text(valueText, color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
        }
        Slider(
            value = value.coerceIn(range.start, range.endInclusive),
            onValueChange = onValueChange,
            onValueChangeFinished = onValueChangeFinished,
            valueRange = range,
            colors = SliderDefaults.colors(
                thumbColor = Pc.Accent,
                activeTrackColor = Pc.Accent,
            ),
            modifier = Modifier.height(26.dp),
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
        Text(text = label, color = color, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}
