package io.github.artcraftandroid.procraft.ui

import android.view.HapticFeedbackConstants
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateDpAsState
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
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculateRotation
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
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
import io.github.artcraftandroid.procraft.core.TimelineClipLoopMode
import io.github.artcraftandroid.procraft.core.TimelineDeformInfo
import io.github.artcraftandroid.procraft.core.TimelineEasingInfo
import io.github.artcraftandroid.procraft.core.TimelineEasingKind
import io.github.artcraftandroid.procraft.core.TimelineKeyframeChannel
import io.github.artcraftandroid.procraft.core.TimelineKeyframeInfo
import io.github.artcraftandroid.procraft.core.TimelineLiveFilterInfo
import io.github.artcraftandroid.procraft.core.TimelineMarkerInfo
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
 * Recognizes DreamsCraft-style **2-finger tap = Undo** and **3-finger tap = Redo** gestures while
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

// ---- Theme Tokens for DreamsCraft Studio Workspace ----

private val CenterBarHeight = 50.dp
private val RulerHeight = 32.dp
private val TrackHeaderWidth = 156.dp
private val KeySubTrackHeight = 26.dp

private val DreamsSurface get() = if (Pc.light) Color(0xFFE6E6E6) else Color(0xFF1C1C1C)
private val DreamsBarFill get() = if (Pc.light) Color(0xFFEAEAEA) else Color(0xFF222222)
private val DreamsCardFill get() = if (Pc.light) Color(0xFFF2F2F2) else Color(0xF7262626)
private val DreamsTrackBg get() = if (Pc.light) Color(0xFFDFDFDF) else Color(0xFF161616)
private val DreamsHeaderBg get() = if (Pc.light) Color(0xFFE3E3E3) else Color(0xFF202020)
private val DreamsClipFill get() = if (Pc.light) Color(0xFFFFFFFF) else Color(0xFF2E2E2E)
private val DreamsPillBg get() = if (Pc.light) Color(0xFFD5D5D5) else Color(0xFF303030)
private val DreamsInk get() = if (Pc.light) Color(0xFF222222) else Color(0xFFF2F2F2)
private val DreamsDimInk get() = if (Pc.light) Color(0xFF5E5E5E) else Color(0xFF9A9A9A)
private val DreamsEdge get() = if (Pc.light) Color(0xFFC8C8C8) else Color(0xFF3A3A3A)
private val DreamsGlassEdge get() = if (Pc.light) Color(0x1F000000) else Color(0x24FFFFFF)

private fun evalCubicBezierY(x: Float, x1: Float, y1: Float, x2: Float, y2: Float): Float {
    val xc = x.coerceIn(0f, 1f)
    var lo = 0f
    var hi = 1f
    for (i in 0 until 14) {
        val mid = (lo + hi) * 0.5f
        val inv = 1f - mid
        val bx = 3f * inv * inv * mid * x1 + 3f * inv * mid * mid * x2 + mid * mid * mid
        if (bx < xc) lo = mid else hi = mid
    }
    val t = (lo + hi) * 0.5f
    val inv = 1f - t
    return 3f * inv * inv * t * y1 + 3f * inv * t * t * y2 + t * t * t
}

private fun evalEasingInfo(easing: TimelineEasingInfo, t: Float): Float {
    val u = t.coerceIn(0f, 1f)
    return when (easing.kind) {
        TimelineEasingKind.LINEAR -> u
        TimelineEasingKind.HOLD -> if (u >= 1f) 1f else 0f
        TimelineEasingKind.EASE_IN -> evalCubicBezierY(u, 0.42f, 0f, 1f, 1f)
        TimelineEasingKind.EASE_OUT -> evalCubicBezierY(u, 0f, 0f, 0.58f, 1f)
        TimelineEasingKind.EASE_IN_OUT -> evalCubicBezierY(u, 0.42f, 0f, 0.58f, 1f)
        TimelineEasingKind.CUBIC_BEZIER -> evalCubicBezierY(u, easing.x1, easing.y1, easing.x2, easing.y2)
    }
}

private val DreamsCyan = Color(0xFF0EDAF5)
private val DreamsViolet = Color(0xFF8B5CF6)
private val PerformRed = Color(0xFFFF2D55)
private val KeyframeGold = Color(0xFFFFB800)
private val OpacityCyan = Color(0xFF06D6F2)
private val FilterPurple = Color(0xFFA855F7)
private val WarpPink = Color(0xFFEC4899)
private val AudioGreen = Color(0xFF22C55E)
private val CameraSky = Color(0xFF38BDF8)

private val DreamsAccentBrush = Brush.horizontalGradient(listOf(DreamsCyan, DreamsViolet, WarpPink))

private val TrackColorPalette = listOf(
    0xFF3B82F6u, // Blue
    0xFFEC4899u, // Pink
    0xFF10B981u, // Emerald
    0xFFF59E0Bu, // Amber
    0xFF8B5CF6u, // Violet
    0xFFEF4444u, // Red
    0xFF06D6F2u, // Cyan
    0xFF64748Bu, // Slate
)

private val EffectColorPresets: List<List<Float>> = listOf(
    listOf(1.0f, 1.0f, 1.0f, 1.0f), // White
    listOf(0.05f, 0.05f, 0.08f, 1.0f), // Ink Black
    listOf(0.05f, 0.85f, 0.96f, 1.0f), // Electric Cyan
    listOf(0.96f, 0.24f, 0.54f, 1.0f), // Neon Magenta
    listOf(1.0f, 0.72f, 0.0f, 1.0f), // Studio Gold
    listOf(0.55f, 0.36f, 0.96f, 1.0f), // Dreams Violet
    listOf(0.13f, 0.77f, 0.37f, 1.0f), // Emerald
    listOf(1.0f, 0.22f, 0.28f, 1.0f), // Crimson
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

private fun closeAllPopovers(vm: CanvasViewModel) {
    vm.movieSettingsOpen = false
    vm.addTrackMenuOpen = false
    vm.playheadActionMenuOpen = false
    vm.liveFilterPopoverOpen = false
    vm.trackOptionsTrackId = null
    vm.editingKeyframe = null
}

// ---- Crisp Custom Vector Studio Icons (Zero Raw Emoji Clutter) ----

@Composable
private fun DreamsPlayPauseIcon(playing: Boolean, tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        if (playing) {
            val barW = this.size.width * 0.28f
            val barH = this.size.height * 0.76f
            val top = (this.size.height - barH) * 0.5f
            drawRoundRect(tint, Offset(this.size.width * 0.16f, top), Size(barW, barH), CornerRadius(3f, 3f))
            drawRoundRect(tint, Offset(this.size.width * 0.56f, top), Size(barW, barH), CornerRadius(3f, 3f))
        } else {
            val path = Path().apply {
                moveTo(this@Canvas.size.width * 0.22f, this@Canvas.size.height * 0.14f)
                lineTo(this@Canvas.size.width * 0.88f, this@Canvas.size.height * 0.50f)
                lineTo(this@Canvas.size.width * 0.22f, this@Canvas.size.height * 0.86f)
                close()
            }
            drawPath(path, tint)
        }
    }
}

@Composable
private fun DreamsUndoRedoIcon(redo: Boolean, tint: Color, size: Dp = 15.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val stroke = Stroke(width = w * 0.13f, cap = StrokeCap.Round, join = StrokeJoin.Round)
        val path = Path().apply {
            if (!redo) {
                moveTo(w * 0.36f, h * 0.22f)
                lineTo(w * 0.14f, h * 0.42f)
                lineTo(w * 0.36f, h * 0.62f)
                moveTo(w * 0.18f, h * 0.42f)
                lineTo(w * 0.62f, h * 0.42f)
                cubicTo(w * 0.88f, h * 0.42f, w * 0.88f, h * 0.78f, w * 0.56f, h * 0.78f)
                lineTo(w * 0.42f, h * 0.78f)
            } else {
                moveTo(w * 0.64f, h * 0.22f)
                lineTo(w * 0.86f, h * 0.42f)
                lineTo(w * 0.64f, h * 0.62f)
                moveTo(w * 0.82f, h * 0.42f)
                lineTo(w * 0.38f, h * 0.42f)
                cubicTo(w * 0.12f, h * 0.42f, w * 0.12f, h * 0.78f, w * 0.44f, h * 0.78f)
                lineTo(w * 0.58f, h * 0.78f)
            }
        }
        drawPath(path, tint, style = stroke)
    }
}

@Composable
private fun DreamsOnionIcon(tint: Color, size: Dp = 15.dp) {
    Canvas(Modifier.size(size)) {
        val r = this.size.minDimension * 0.30f
        val cy = this.size.height * 0.5f
        val sw = this.size.minDimension * 0.11f
        drawCircle(tint.copy(alpha = 0.45f), r, Offset(this.size.width * 0.35f, cy), style = Stroke(sw))
        drawCircle(tint, r, Offset(this.size.width * 0.65f, cy), style = Stroke(sw))
    }
}

@Composable
private fun DreamsCameraIcon(tint: Color, size: Dp = 15.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val sw = w * 0.11f
        drawRoundRect(
            color = tint,
            topLeft = Offset(w * 0.10f, h * 0.24f),
            size = Size(w * 0.56f, h * 0.52f),
            cornerRadius = CornerRadius(w * 0.12f, w * 0.12f),
            style = Stroke(sw),
        )
        val lens = Path().apply {
            moveTo(w * 0.68f, h * 0.40f)
            lineTo(w * 0.92f, h * 0.26f)
            lineTo(w * 0.92f, h * 0.74f)
            lineTo(w * 0.68f, h * 0.60f)
            close()
        }
        drawPath(lens, tint)
    }
}

@Composable
private fun DreamsGhostIcon(tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val r = min(w, h) * 0.24f
        drawCircle(tint.copy(alpha = 0.28f), r, Offset(w * 0.28f, h * 0.50f))
        drawCircle(tint.copy(alpha = 0.58f), r, Offset(w * 0.50f, h * 0.50f))
        drawCircle(tint, r, Offset(w * 0.74f, h * 0.50f))
    }
}

@Composable
private fun DreamsFlipbookIcon(tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val sw = w * 0.11f
        drawRoundRect(tint.copy(alpha = 0.45f), Offset(w * 0.26f, h * 0.12f), Size(w * 0.62f, h * 0.62f), CornerRadius(3f, 3f), style = Stroke(sw))
        drawRoundRect(tint, Offset(w * 0.12f, h * 0.26f), Size(w * 0.62f, h * 0.62f), CornerRadius(3f, 3f), style = Stroke(sw))
    }
}

@Composable
private fun DreamsEyeIcon(visible: Boolean, tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val sw = w * 0.11f
        val eye = Path().apply {
            moveTo(w * 0.08f, h * 0.50f)
            quadraticTo(w * 0.50f, h * 0.12f, w * 0.92f, h * 0.50f)
            quadraticTo(w * 0.50f, h * 0.88f, w * 0.08f, h * 0.50f)
            close()
        }
        drawPath(eye, tint, style = Stroke(sw, join = StrokeJoin.Round))
        if (visible) {
            drawCircle(tint, w * 0.15f, Offset(w * 0.50f, h * 0.50f))
        } else {
            drawLine(tint, Offset(w * 0.18f, h * 0.82f), Offset(w * 0.82f, h * 0.18f), strokeWidth = sw * 1.15f, cap = StrokeCap.Round)
        }
    }
}

@Composable
private fun DreamsMoreDotsIcon(tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        val r = this.size.minDimension * 0.11f
        val cy = this.size.height * 0.5f
        drawCircle(tint, r, Offset(this.size.width * 0.20f, cy))
        drawCircle(tint, r, Offset(this.size.width * 0.50f, cy))
        drawCircle(tint, r, Offset(this.size.width * 0.80f, cy))
    }
}

@Composable
private fun DreamsPlusIcon(tint: Color, size: Dp = 14.dp) {
    Canvas(Modifier.size(size)) {
        val sw = this.size.minDimension * 0.16f
        val pad = this.size.minDimension * 0.18f
        drawLine(tint, Offset(this.size.width * 0.5f, pad), Offset(this.size.width * 0.5f, this.size.height - pad), sw, StrokeCap.Round)
        drawLine(tint, Offset(pad, this.size.height * 0.5f), Offset(this.size.width - pad, this.size.height * 0.5f), sw, StrokeCap.Round)
    }
}

@Composable
private fun DreamsSparkIcon(tint: Color, size: Dp = 13.dp) {
    Canvas(Modifier.size(size)) {
        val cx = this.size.width * 0.5f
        val cy = this.size.height * 0.5f
        val r = this.size.minDimension * 0.46f
        val ri = r * 0.28f
        val path = Path().apply {
            moveTo(cx, cy - r)
            lineTo(cx + ri, cy - ri)
            lineTo(cx + r, cy)
            lineTo(cx + ri, cy + ri)
            lineTo(cx, cy + r)
            lineTo(cx - ri, cy + ri)
            lineTo(cx - r, cy)
            lineTo(cx - ri, cy - ri)
            close()
        }
        drawPath(path, tint)
    }
}

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
    val timelineHeightDp = (windowHeight * vm.timelineSplitRatio).coerceIn(146.dp, windowHeight * 0.70f)
    val dockBottomPx = with(density) {
        if (tl.flipbookOpen) 132.dp.toPx() else (timelineHeightDp + CenterBarHeight).toPx()
    }

    LaunchedEffect(dockBottomPx, tl.flipbookOpen) {
        vm.updateStageBottomInset(dockBottomPx)
    }

    // 1. Stage Backstage Frame & Interactive Keyframe / Warp / Distort / Camera / Live Perform Gizmo
    val stageHeightDp = if (tl.flipbookOpen) windowHeight else (windowHeight - timelineHeightDp - CenterBarHeight)
    if (vm.dreamsMode == DreamsWorkMode.PERFORM || vm.stageGizmoActive) {
        StageKeyTransformOverlay(
            vm = vm,
            tl = tl,
            stageWidth = windowWidth,
            stageHeight = stageHeightDp.coerceAtLeast(120.dp),
        )
    } else if (tl.cameraEnabled && !isTransformIdentity(tl.cameraTransform)) {
        CameraActiveDrawIndicator(
            vm = vm,
            tl = tl,
            stageWidth = windowWidth,
            stageHeight = stageHeightDp.coerceAtLeast(120.dp),
        )
    } else if (tl.showBackstage) {
        // Subtle crisp Stage boundary outline so the artist clearly distinguishes Stage vs Backstage
        val (cwInt, chInt) = remember(vm.session) { vm.canvasSize() }
        val cw = cwInt.coerceAtLeast(1).toFloat()
        val ch = chInt.coerceAtLeast(1).toFloat()
        val viewM = vm.viewMatrix
        Canvas(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .width(windowWidth)
                .height(stageHeightDp.coerceAtLeast(120.dp)),
        ) {
            val corners = listOf(
                applyViewMatrix(viewM, 0f, 0f),
                applyViewMatrix(viewM, cw, 0f),
                applyViewMatrix(viewM, cw, ch),
                applyViewMatrix(viewM, 0f, ch),
            )
            val path = Path().apply {
                moveTo(corners[0].x, corners[0].y)
                lineTo(corners[1].x, corners[1].y)
                lineTo(corners[2].x, corners[2].y)
                lineTo(corners[3].x, corners[3].y)
                close()
            }
            drawPath(
                path = path,
                color = Color.White.copy(alpha = 0.28f),
                style = Stroke(width = 1.5f),
            )
        }
    }

    // 2. True DreamsCraft Flipbook Mode vs Full Multi-Track Split View
    if (tl.flipbookOpen) {
        DreamsFlipbookDock(
            vm = vm,
            tl = tl,
        )
        return
    }

    // 3. Stage and multitrack timeline with the center action bar
    val dockCover = remember { Any() }
    val dockCorner = LocalClearance.current.corner
    DisposableEffect(dockCover) { onDispose { vm.covers(dockCover, null) } }
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .fillMaxWidth()
            .height(timelineHeightDp + CenterBarHeight)
            .background(DreamsSurface)
            // Consume all touches so no accidental strokes reach the Stage underneath,
            // and register bounds so hovering pens hide the brush cursor over the dock.
            .keepsItsTouches()
            .onGloballyPositioned { vm.covers(dockCover, it.boundsInRoot().translate(-dockCorner)) },
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

    // 4. Tap-outside dismiss backdrop whenever any popover is open
    val anyPopoverOpen = vm.movieSettingsOpen ||
        vm.addTrackMenuOpen ||
        vm.playheadActionMenuOpen ||
        vm.liveFilterPopoverOpen ||
        vm.trackOptionsTrackId != null ||
        vm.editingKeyframe != null
    if (anyPopoverOpen) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .pointerInput(Unit) {
                    detectTapGestures { closeAllPopovers(vm) }
                },
        )
    }

    val popoverBottom = timelineHeightDp + CenterBarHeight + 8.dp
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
 * Dedicated DreamsCraft **Flipbook Dock** for the active `CelClip`:
 * - Maximizes the upper Stage for focused hand-drawn frame-by-frame animation
 * - Shows ONLY the active `CelClip`'s cel filmstrip with real thumbnails, frame numbers, and hold badges
 * - Quick cel actions: `+ Cel`, `Dup`, `Hold -/+`, `Del`, `Cel Layers` + `+Sub`, `Ghost`, and inline **Onion Skin** controls
 * - Pull up the top grabber (or tap `Timeline`) to smoothly return to the full multi-track Timeline!
 */
@Composable
private fun BoxScope.DreamsFlipbookDock(
    vm: CanvasViewModel,
    tl: TimelineState,
) {
    val view = LocalView.current
    val activeClip = remember(tl) { activeClipOf(tl) }
    val isCelClip = activeClip != null && activeClip.kind == TimelineClipKind.CEL
    val playing = vm.playingFrame != null
    var onionDrawerOpen by remember { mutableStateOf(false) }

    val cels = activeClip?.cels.orEmpty()
    val playhead = tl.playhead.toInt()
    val activeCelIndex = activeClip?.activeCelIndex?.toInt() ?: -1
    val activeCel = cels.getOrNull(activeCelIndex)

    val flipbookCover = remember { Any() }
    val flipbookCorner = LocalClearance.current.corner
    DisposableEffect(flipbookCover) { onDispose { vm.covers(flipbookCover, null) } }
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(horizontal = 12.dp, vertical = 10.dp)
            .widthIn(max = 760.dp)
            .fillMaxWidth()
            .shadow(14.dp, RoundedCornerShape(22.dp))
            .clip(RoundedCornerShape(22.dp))
            .background(DreamsCardFill.copy(alpha = 0.96f))
            .border(1.dp, DreamsGlassEdge, RoundedCornerShape(22.dp))
            .keepsItsTouches()
            .onGloballyPositioned { vm.covers(flipbookCover, it.boundsInRoot().translate(-flipbookCorner)) },
    ) {
        // Optional inline Onion Skin quick drawer right inside Flipbook
        if (onionDrawerOpen) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(DreamsBarFill)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Onion Skin", color = KeyframeGold, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        for (count in listOf(0u, 1u, 2u, 3u, 5u)) {
                            val chosen = tl.onion.beforeFrames == count && tl.onion.enabled == (count > 0u)
                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(8.dp))
                                    .background(if (chosen) KeyframeGold else DreamsPillBg)
                                    .clickable {
                                        vm.updateTimelineOnion {
                                            it.copy(enabled = count > 0u, beforeFrames = count, afterFrames = count)
                                        }
                                    }
                                    .padding(horizontal = 8.dp, vertical = 3.dp),
                            ) {
                                Text(
                                    if (count == 0u) "Off" else "$count",
                                    color = if (chosen) Color.Black else DreamsInk,
                                    fontSize = 11.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                }
                CompactSliderRow(
                    label = "Onion Opacity",
                    valueText = "${(tl.onion.opacity * 100f).roundToInt()}%",
                    value = tl.onion.opacity,
                    range = 0.05f..1f,
                    onValueChange = { op ->
                        vm.updateTimelineOnion { it.copy(enabled = true, opacity = op) }
                    },
                    onValueChangeFinished = {},
                )
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(DreamsEdge))
        }

        // Top Grabber + Compact Flipbook Studio Controls
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { dy ->
                        if (dy < -18f) vm.setFlipbookMode(false)
                    },
                ),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                modifier = Modifier
                    .padding(top = 5.dp, bottom = 3.dp)
                    .width(38.dp)
                    .height(4.dp)
                    .clip(CircleShape)
                    .background(DreamsDimInk.copy(alpha = 0.55f))
                    .clickable { vm.setFlipbookMode(false) },
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 10.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // Return to full Multi-Track Timeline
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(12.dp))
                        .background(Pc.Accent.copy(alpha = 0.16f))
                        .clickable { vm.setFlipbookMode(false) }
                        .padding(horizontal = 9.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    DreamsFlipbookIcon(Pc.Accent, 13.dp)
                    Text("Timeline", color = Pc.Accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                }

                Box(
                    modifier = Modifier.size(48.dp)
                        .semantics { contentDescription = if (playing) "Pause animation" else "Play animation" }
                        .clickable(role = Role.Button) {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.togglePlaying()
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Box(Modifier.size(28.dp).background(if (playing) PerformRed else Pc.Accent, CircleShape),
                        contentAlignment = Alignment.Center) {
                        DreamsPlayPauseIcon(playing = playing, tint = Color.White, size = 12.dp)
                    }
                }

                // Frame Stepper + Timecode
                Box(
                    modifier = Modifier
                        .size(24.dp)
                        .clip(RoundedCornerShape(7.dp))
                        .background(DreamsPillBg)
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.stepTimelineFrame(-1)
                        },
                    contentAlignment = Alignment.Center,
                ) { Text("‹", color = DreamsInk, fontSize = 14.sp, fontWeight = FontWeight.Bold) }
                Text(
                    text = "F${playhead + 1} · ${formatTimecode(playhead, tl.fps.toInt())}",
                    color = DreamsInk,
                    fontSize = 11.sp,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                )
                Box(
                    modifier = Modifier
                        .size(24.dp)
                        .clip(RoundedCornerShape(7.dp))
                        .background(DreamsPillBg)
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.stepTimelineFrame(1)
                        },
                    contentAlignment = Alignment.Center,
                ) { Text("›", color = DreamsInk, fontSize = 14.sp, fontWeight = FontWeight.Bold) }

                Box(Modifier.width(1.dp).height(16.dp).background(DreamsEdge))

                // Undo / Redo
                val undoReady = vm.canUndo || vm.performRecording
                Box(
                    modifier = Modifier
                        .size(26.dp)
                        .clip(CircleShape)
                        .background(if (undoReady) DreamsPillBg else Color.Transparent)
                        .clickable(enabled = undoReady) {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.undo()
                        },
                    contentAlignment = Alignment.Center,
                ) { DreamsUndoRedoIcon(redo = false, tint = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.35f), size = 13.dp) }
                Box(
                    modifier = Modifier
                        .size(26.dp)
                        .clip(CircleShape)
                        .background(if (vm.canRedo) DreamsPillBg else Color.Transparent)
                        .clickable(enabled = vm.canRedo) {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.redo()
                        },
                    contentAlignment = Alignment.Center,
                ) { DreamsUndoRedoIcon(redo = true, tint = if (vm.canRedo) DreamsInk else DreamsDimInk.copy(alpha = 0.35f), size = 13.dp) }

                Box(Modifier.width(1.dp).height(16.dp).background(DreamsEdge))

                // Onion Skin toggle + quick drawer
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(12.dp))
                        .background(if (tl.onion.enabled || onionDrawerOpen) KeyframeGold.copy(alpha = 0.18f) else Color.Transparent)
                        .clickable { onionDrawerOpen = !onionDrawerOpen }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    DreamsOnionIcon(if (tl.onion.enabled) KeyframeGold else DreamsDimInk, 13.dp)
                    Text(
                        text = if (tl.onion.enabled) "Onion ${tl.onion.beforeFrames}" else "Onion",
                        color = if (tl.onion.enabled) KeyframeGold else DreamsDimInk,
                        fontSize = 11.sp,
                        fontWeight = if (tl.onion.enabled) FontWeight.Bold else FontWeight.Medium,
                    )
                }

                // Perform Motion Ghost toggle
                if (activeClip != null) {
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(12.dp))
                            .background(if (activeClip.performOnionGuide) OpacityCyan.copy(alpha = 0.20f) else Color.Transparent)
                            .clickable {
                                vm.setPerformOnionGuide(
                                    clipId = activeClip.id,
                                    enabled = !activeClip.performOnionGuide,
                                    isolateFirstFrame = true,
                                    opacity = activeClip.performGuideOpacity.coerceIn(0.15f, 0.9f),
                                    colorRgb = activeClip.performGuideColor.ifEmpty { listOf(0.15f, 0.85f, 1.0f) },
                                )
                            }
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        DreamsGhostIcon(if (activeClip.performOnionGuide) OpacityCyan else DreamsDimInk, 13.dp)
                        Text("Ghost", color = if (activeClip.performOnionGuide) OpacityCyan else DreamsDimInk, fontSize = 11.sp, fontWeight = if (activeClip.performOnionGuide) FontWeight.Bold else FontWeight.Medium)
                    }
                }

                // Native Cel Sub-layers shortcut
                if (activeCel != null && activeClip != null) {
                    val sublayers = remember(activeClip.id, activeCelIndex, vm.layers) {
                        vm.celSublayerIds(activeClip.id, activeCelIndex)
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .background(FilterPurple.copy(alpha = 0.16f))
                            .clickable { vm.addActiveCelSublayer() }
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                    ) {
                        Text("Sub-Layer (${sublayers.size.coerceAtLeast(1)}) +", color = FilterPurple, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }

        Box(Modifier.fillMaxWidth().height(1.dp).background(DreamsEdge.copy(alpha = 0.7f)))

        // Bottom Active-Clip Cel Filmstrip + Hold / Dup / Del / + Cel buttons
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(68.dp)
                .padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (isCelClip && activeClip != null) {
                Row(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    for ((idx, cel) in cels.withIndex()) {
                        val selected = idx == activeCelIndex
                        Box(
                            modifier = Modifier
                                .width(56.dp)
                                .fillMaxHeight()
                                .clip(RoundedCornerShape(10.dp))
                                .background(if (selected) Pc.Accent.copy(alpha = 0.16f) else DreamsClipFill)
                                .border(
                                    width = if (selected) 2.dp else 1.dp,
                                    color = if (selected) Pc.Accent else DreamsEdge,
                                    shape = RoundedCornerShape(10.dp),
                                )
                                .clickable(enabled = cel.firstExposure != null) {
                                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                    cel.firstExposure?.let { vm.setTimelinePlayhead(it.toInt()) }
                                },
                        ) {
                            vm.frameThumbnails[cel.layerId]?.let { thumb ->
                                Image(
                                    bitmap = thumb,
                                    contentDescription = "Cel ${idx + 1}",
                                    contentScale = ContentScale.Fit,
                                    modifier = Modifier.fillMaxSize().padding(3.dp),
                                )
                            }
                            // Cel index badge
                            Box(
                                modifier = Modifier
                                    .align(Alignment.TopStart)
                                    .padding(3.dp)
                                    .clip(RoundedCornerShape(4.dp))
                                    .background(if (selected) Pc.Accent else DreamsSurface.copy(alpha = 0.8f))
                                    .padding(horizontal = 4.dp, vertical = 0.5.dp),
                            ) {
                                Text(
                                    "${idx + 1}",
                                    color = if (selected) Color.White else DreamsDimInk,
                                    fontSize = 8.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                    fontWeight = FontWeight.Bold,
                                )
                            }
                            // Hold duration badge (+N)
                            if (cel.hold > 0u) {
                                Box(
                                    modifier = Modifier
                                        .align(Alignment.BottomEnd)
                                        .padding(3.dp)
                                        .clip(RoundedCornerShape(4.dp))
                                        .background(DreamsSurface.copy(alpha = 0.85f))
                                        .padding(horizontal = 3.5.dp, vertical = 0.5.dp),
                                ) {
                                    Text(
                                        "+${cel.hold}",
                                        color = KeyframeGold,
                                        fontSize = 8.5.sp,
                                        fontFamily = FontFamily.Monospace,
                                        fontWeight = FontWeight.Bold,
                                    )
                                }
                            }
                        }
                    }
                }

                // Active Cel Quick Actions: Hold -/+, Dup, Del, + Cel
                Row(
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val curHold = activeCel?.hold?.toInt() ?: 0
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(9.dp))
                            .background(DreamsPillBg)
                            .clickable(enabled = curHold > 0) {
                                vm.setTimelineCelHold(activeClip.id, activeCelIndex, (curHold - 1).coerceAtLeast(0), done = true)
                            }
                            .padding(horizontal = 7.dp, vertical = 8.dp),
                    ) {
                        Text("H-", color = if (curHold > 0) DreamsInk else DreamsDimInk.copy(alpha = 0.4f), fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(9.dp))
                            .background(DreamsPillBg)
                            .clickable(enabled = activeCel != null) {
                                vm.setTimelineCelHold(activeClip.id, activeCelIndex, curHold + 1, done = true)
                            }
                            .padding(horizontal = 7.dp, vertical = 8.dp),
                    ) {
                        Text("H+", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(9.dp))
                            .background(DreamsPillBg)
                            .clickable(enabled = activeCel != null) { vm.duplicateTimelineCel(activeClip.id, activeCelIndex) }
                            .padding(horizontal = 8.dp, vertical = 8.dp),
                    ) {
                        Text("Dup", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                    }
                    if (cels.size > 1) {
                        Box(
                            modifier = Modifier
                                .clip(RoundedCornerShape(9.dp))
                                .background(PerformRed.copy(alpha = 0.16f))
                                .clickable(enabled = activeCel != null) { vm.deleteTimelineCel(activeClip.id, activeCelIndex) }
                                .padding(horizontal = 8.dp, vertical = 8.dp),
                        ) {
                            Text("Del", color = PerformRed, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                        }
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .background(Pc.Accent)
                            .clickable { vm.addTimelineCel(activeClip.id) }
                            .padding(horizontal = 11.dp, vertical = 8.dp),
                    ) {
                        Text("+ Cel", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    }
                }
            } else {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Select or create a Drawing (Cel) clip to animate frame-by-frame",
                        color = DreamsDimInk,
                        fontSize = 12.sp,
                        modifier = Modifier.weight(1f),
                    )
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .background(Pc.Accent)
                            .clickable { vm.addCelClipToActiveTrack() }
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text("+ Drawing Clip", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }
    }
}

/**
 * The signature **DreamsCraft Center Action Bar** sitting between the Stage and the Timeline:
 * - Left: Glowing Play/Pause circle + Undo/Redo capsule + Monospace Timecode & FPS + Onion + Camera
 * - Center: Sleek Draggable Split Handle (pull down to collapse into Flipbook!) + Flipbook button
 * - Right: Responsive mode switches (`Draw`, `Perform`, `Move`, `FX`, `Edit`, `+`)
 */
@Composable
private fun CenterActionBar(
    vm: CanvasViewModel,
    tl: TimelineState,
    totalHeightPx: Float,
) {
    val view = LocalView.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(CenterBarHeight)
            .background(DreamsBarFill),
    ) {
        // Subtle luminous top accent hairline separating Stage from Timeline
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(DreamsGlassEdge),
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Left: Play/Pause + Undo/Redo + Timecode + Onion + Camera
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                val isPlaying = vm.playingFrame != null
                Box(
                    modifier = Modifier
                        .size(34.dp)
                        .clip(CircleShape)
                        .background(
                            if (isPlaying) {
                                Brush.linearGradient(listOf(DreamsCyan, Pc.Accent))
                            } else {
                                Brush.linearGradient(listOf(DreamsClipFill, DreamsClipFill))
                            },
                        )
                        .border(1.dp, if (isPlaying) DreamsCyan else DreamsEdge, CircleShape)
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.togglePlaying()
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    DreamsPlayPauseIcon(playing = isPlaying, tint = if (isPlaying) Color.White else DreamsInk, size = 14.dp)
                }

                // Segmented Undo / Redo Capsule
                val undoReady = vm.canUndo || vm.performRecording
                Row(
                    modifier = Modifier
                        .height(32.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(DreamsPillBg)
                        .border(1.dp, DreamsEdge, RoundedCornerShape(10.dp)),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        modifier = Modifier
                            .width(30.dp)
                            .fillMaxHeight()
                            .clickable(enabled = undoReady) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                vm.undo()
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        DreamsUndoRedoIcon(
                            redo = false,
                            tint = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.35f),
                            size = 14.dp,
                        )
                    }
                    Box(Modifier.width(1.dp).height(16.dp).background(DreamsEdge))
                    Box(
                        modifier = Modifier
                            .width(30.dp)
                            .fillMaxHeight()
                            .clickable(enabled = vm.canRedo) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                vm.redo()
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        DreamsUndoRedoIcon(
                            redo = true,
                            tint = if (vm.canRedo) DreamsInk else DreamsDimInk.copy(alpha = 0.35f),
                            size = 14.dp,
                        )
                    }
                }

                // Monospace Timecode & FPS Pill -> opens Movie, Auto-Draw & Onion Skin Settings
                Row(
                    modifier = Modifier
                        .height(32.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(if (vm.movieSettingsOpen) Pc.Accent.copy(alpha = 0.22f) else DreamsPillBg)
                        .border(
                            1.dp,
                            if (vm.movieSettingsOpen) Pc.Accent else DreamsEdge,
                            RoundedCornerShape(10.dp),
                        )
                        .clickable {
                            val next = !vm.movieSettingsOpen
                            closeAllPopovers(vm)
                            vm.movieSettingsOpen = next
                        }
                        .padding(horizontal = 9.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(
                        text = formatTimecode(tl.playhead.toInt(), tl.fps.toInt()),
                        color = DreamsInk,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "${tl.fps}p",
                        color = DreamsDimInk,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }

                // Onion Skin Icon Toggle
                Box(
                    modifier = Modifier
                        .size(32.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(if (tl.onion.enabled) KeyframeGold.copy(alpha = 0.20f) else DreamsPillBg)
                        .border(1.dp, if (tl.onion.enabled) KeyframeGold else DreamsEdge, RoundedCornerShape(10.dp))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.updateTimelineOnion { it.copy(enabled = !it.enabled) }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    DreamsOnionIcon(if (tl.onion.enabled) KeyframeGold else DreamsDimInk, 16.dp)
                }

                // Camera Track Icon Toggle
                val camActive = vm.stageGizmoActive && vm.stageGizmoMode == StageGizmoSubMode.CAMERA
                Box(
                    modifier = Modifier
                        .size(32.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(if (camActive || tl.cameraEnabled) CameraSky.copy(alpha = 0.20f) else DreamsPillBg)
                        .border(1.dp, if (camActive) CameraSky else DreamsEdge, RoundedCornerShape(10.dp))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            if (camActive) {
                                vm.stageGizmoActive = false
                                vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                            } else {
                                vm.setCameraEnabled(true)
                                vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                                vm.stageGizmoActive = true
                                vm.dreamsMode = DreamsWorkMode.TIMELINE
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    DreamsCameraIcon(if (camActive || tl.cameraEnabled) CameraSky else DreamsDimInk, 16.dp)
                }
            }

            // Center: Draggable Split Handle (resizes Stage/Timeline; pull down to enter Flipbook!)
            Box(
                modifier = Modifier
                    .weight(1f)
                    .widthIn(min = 56.dp)
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
                            .width(34.dp)
                            .height(4.dp)
                            .clip(CircleShape)
                            .background(DreamsDimInk.copy(alpha = 0.55f)),
                    )
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(DreamsPillBg.copy(alpha = 0.7f))
                            .clickable { vm.setFlipbookMode(true) }
                            .padding(horizontal = 7.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        DreamsFlipbookIcon(DreamsDimInk, 12.dp)
                        Text("Flipbook", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                    }
                }
            }

            // Right: Scrollable Studio Work Mode Buttons (Draw, Perform, Move/Warp, FX, Edit, +)
            Row(
                modifier = Modifier.horizontalScroll(rememberScrollState()),
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
                    label = if (vm.performRecording) "REC" else "Perform",
                    selected = vm.dreamsMode == DreamsWorkMode.PERFORM,
                    activeColor = PerformRed,
                    leadingDot = true,
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
                        val next = !vm.liveFilterPopoverOpen
                        closeAllPopovers(vm)
                        vm.liveFilterPopoverOpen = next
                    },
                )
                ModeChip(
                    label = "Edit",
                    selected = vm.dreamsMode == DreamsWorkMode.TIMELINE_EDIT,
                    activeColor = DreamsViolet,
                    onClick = {
                        if (vm.performRecording) vm.finishPerformRecording()
                        vm.stageGizmoActive = false
                        if (vm.stageGizmoMode == StageGizmoSubMode.CAMERA) {
                            vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                        }
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
                        activeColor = DreamsViolet,
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
                        .size(32.dp)
                        .clip(CircleShape)
                        .background(
                            if (vm.addTrackMenuOpen) DreamsAccentBrush else SolidColor(DreamsClipFill),
                        )
                        .border(1.dp, if (vm.addTrackMenuOpen) DreamsCyan else DreamsEdge, CircleShape)
                        .clickable {
                            val next = !vm.addTrackMenuOpen
                            closeAllPopovers(vm)
                            vm.addTrackMenuOpen = next
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    DreamsPlusIcon(if (vm.addTrackMenuOpen) Color.White else DreamsInk, 14.dp)
                }
            }
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(0.5.dp)
                .background(DreamsEdge),
        )
    }
}

@Composable
private fun ModeChip(
    label: String,
    selected: Boolean,
    activeColor: Color,
    leadingDot: Boolean = false,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .height(32.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) activeColor else DreamsPillBg)
            .border(1.dp, if (selected) activeColor else DreamsEdge, RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (leadingDot) {
            Box(
                modifier = Modifier
                    .size(7.dp)
                    .clip(CircleShape)
                    .background(if (selected) Color.White else activeColor),
            )
        }
        Text(
            text = label,
            color = if (selected) Color.White else DreamsInk,
            fontSize = 12.sp,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
            maxLines = 1,
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
            // DreamsCraft-style 2-finger tap = Undo, 3-finger tap = Redo right over the Timeline
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
            // Left Header Corner: Prev/Next Keyframe + Prev/Next Frame + Frame Counter
            Row(
                modifier = Modifier
                    .width(TrackHeaderWidth)
                    .fillMaxHeight()
                    .padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Box(
                    modifier = Modifier
                        .size(22.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsPillBg.copy(alpha = 0.7f))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.jumpTimelineKeyframe(forward = false)
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("‹◆", color = KeyframeGold, fontSize = 9.5.sp, fontWeight = FontWeight.Bold)
                }
                Box(
                    modifier = Modifier
                        .size(20.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsPillBg.copy(alpha = 0.7f))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.stepTimelineFrame(-1)
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("‹", color = DreamsInk, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                }
                Text(
                    text = "F${tl.playhead + 1u}/${tl.durationFrames}",
                    color = DreamsDimInk,
                    fontSize = 10.sp,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                )
                Box(
                    modifier = Modifier
                        .size(20.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsPillBg.copy(alpha = 0.7f))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.stepTimelineFrame(1)
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("›", color = DreamsInk, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                }
                Box(
                    modifier = Modifier
                        .size(22.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .background(DreamsPillBg.copy(alpha = 0.7f))
                        .clickable {
                            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                            vm.jumpTimelineKeyframe(forward = true)
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("◆›", color = KeyframeGold, fontSize = 9.5.sp, fontWeight = FontWeight.Bold)
                }
            }

            Box(Modifier.width(1.dp).fillMaxHeight().background(DreamsEdge))

            // Right: Time Ruler with second numbers, Timeline Markers, 1-finger scrubbing & Work Area bracket
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
                val rulerMeasurer = rememberTextMeasurer()
                val rulerLabelStyle = remember {
                    TextStyle(
                        color = Color(0xFF8E8EA4),
                        fontSize = 9.sp,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Medium,
                    )
                }
                Canvas(
                    modifier = Modifier
                        .width(contentWidthDp)
                        .fillMaxHeight(),
                ) {
                    // Draw Work Area In/Out range highlight with crisp edge caps if active
                    if (workArea != null) {
                        val wx0 = workArea.start.toFloat() * zoomPx
                        val wx1 = (workArea.end + 1u).toFloat() * zoomPx
                        drawRect(
                            color = DreamsCyan.copy(alpha = 0.16f),
                            topLeft = Offset(wx0, 0f),
                            size = Size((wx1 - wx0).coerceAtLeast(4f), size.height),
                        )
                        drawLine(DreamsCyan, Offset(wx0, 0f), Offset(wx0, size.height), strokeWidth = 2.5f)
                        drawLine(DreamsCyan, Offset(wx1, 0f), Offset(wx1, size.height), strokeWidth = 2.5f)
                        drawLine(DreamsCyan, Offset(wx0, 1.5f), Offset(wx1, 1.5f), strokeWidth = 2.5f)
                    }

                    // Frame ticks & second markers with readable second labels
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
                        val tickH = if (isSecond) size.height * 0.48f else size.height * 0.24f
                        drawLine(
                            color = if (isSecond) Color(0xFF9898AE) else Color(0xFF4A4A5A),
                            start = Offset(x, size.height - tickH),
                            end = Offset(x, size.height),
                            strokeWidth = if (isSecond) 1.75f else 1f,
                        )
                        if (isSecond && x + 28f < size.width) {
                            val sec = f / fps
                            drawText(
                                textMeasurer = rulerMeasurer,
                                text = "${sec}s",
                                style = rulerLabelStyle,
                                topLeft = Offset(x + 4f, 2f),
                            )
                        }
                        f += step
                    }

                    // Playhead needle marker on the ruler
                    val px = playheadFrame * zoomPx + zoomPx * 0.5f
                    drawLine(
                        color = PerformRed,
                        start = Offset(px, size.height * 0.45f),
                        end = Offset(px, size.height),
                        strokeWidth = 2.5f,
                    )
                }

                // Timeline Beat / Section Markers on Ruler (tap to jump, long-press to delete)
                for (marker in tl.markers) {
                    val mColor = Color(0xFF000000L or (marker.color.toLong() and 0xFFFFFFL))
                    val mxDp = (zoomDp * marker.frame.toInt() + zoomDp * 0.5f - 7.dp).coerceAtLeast(0.dp)
                    Box(
                        modifier = Modifier
                            .offset(x = mxDp, y = 2.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(mColor.copy(alpha = 0.92f))
                            .combinedClickable(
                                onClick = {
                                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                    vm.setTimelinePlayhead(marker.frame.toInt())
                                },
                                onLongClick = {
                                    view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                                    vm.deleteTimelineMarker(marker.id)
                                },
                            )
                            .padding(horizontal = 4.dp, vertical = 1.dp),
                    ) {
                        Text(
                            text = marker.label.ifBlank { "⚑" }.take(6),
                            color = Color.Black,
                            fontSize = 8.sp,
                            fontWeight = FontWeight.Bold,
                            maxLines = 1,
                        )
                    }
                }

                // Sleek Action Button Badge anchored to the Playhead
                val playheadDp = zoomDp * tl.playhead.toInt()
                Row(
                    modifier = Modifier
                        .offset(x = (playheadDp - 24.dp).coerceAtLeast(2.dp), y = 4.dp)
                        .shadow(4.dp, RoundedCornerShape(10.dp))
                        .clip(RoundedCornerShape(10.dp))
                        .background(PerformRed)
                        .clickable {
                            val next = !vm.playheadActionMenuOpen
                            closeAllPopovers(vm)
                            vm.playheadActionMenuOpen = next
                        }
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    DreamsSparkIcon(Color.White, 9.dp)
                    Text(
                        text = "Action",
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
                            .border(0.5.dp, CameraSky.copy(alpha = 0.45f))
                            .clickable {
                                vm.setCameraEnabled(true)
                                vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                                vm.stageGizmoActive = true
                            }
                            .padding(horizontal = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(5.dp),
                        ) {
                            DreamsCameraIcon(CameraSky, 13.dp)
                            Text(
                                text = "Camera",
                                color = CameraSky,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                        Box(
                            modifier = Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(if (tl.cameraEnabled) CameraSky.copy(alpha = 0.24f) else DreamsPillBg)
                                .clickable { vm.setCameraEnabled(!tl.cameraEnabled) }
                                .padding(horizontal = 6.dp, vertical = 2.dp),
                        ) {
                            Text(
                                text = if (tl.cameraEnabled) "ON" else "OFF",
                                color = if (tl.cameraEnabled) CameraSky else DreamsDimInk,
                                fontSize = 9.sp,
                                fontWeight = FontWeight.Bold,
                            )
                        }
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
                                    zoomPx = zoomPx,
                                    color = CameraSky,
                                    onMove = { fromF, toF, done ->
                                        vm.moveTimelineKeyframe(0uL, TimelineKeyframeChannel.TRANSFORM, fromF, toF, done)
                                    },
                                    onClick = {
                                        vm.setTimelinePlayhead(kf.frame.toInt())
                                        closeAllPopovers(vm)
                                        vm.editingKeyframe = 0uL to kf.frame
                                    },
                                )
                            }
                            for (kf in tl.cameraFilterKeyframes) {
                                KeyframeDiamond(
                                    frame = kf.frame.toInt(),
                                    zoomDp = zoomDp,
                                    zoomPx = zoomPx,
                                    color = FilterPurple,
                                    onMove = { fromF, toF, done ->
                                        vm.moveTimelineKeyframe(0uL, TimelineKeyframeChannel.FILTER, fromF, toF, done)
                                    },
                                    onClick = {
                                        vm.setTimelinePlayhead(kf.frame.toInt())
                                        closeAllPopovers(vm)
                                        vm.editingKeyframe = 0uL to kf.frame
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
            .background(if (isSelected) tagColor.copy(alpha = 0.14f) else DreamsHeaderBg)
            .border(if (isSelected) 0.75.dp else 0.5.dp, if (isSelected) tagColor.copy(alpha = 0.55f) else DreamsEdge)
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
                    closeAllPopovers(vm)
                    vm.trackOptionsTrackId = track.id
                },
            ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(trackHeight)
                .padding(start = if (track.parentGroup != null) 10.dp else 5.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Color accent bar
            Box(
                modifier = Modifier
                    .width(3.5.dp)
                    .height((trackHeight - 12.dp).coerceAtLeast(18.dp))
                    .clip(CircleShape)
                    .background(if (track.visible) tagColor else tagColor.copy(alpha = 0.35f)),
            )
            Spacer(Modifier.width(4.dp))

            // Expand/Collapse KeyTracks chevron
            Text(
                text = if (track.expanded) "▾" else "▸",
                color = DreamsDimInk,
                fontSize = 11.sp,
                modifier = Modifier
                    .clip(RoundedCornerShape(4.dp))
                    .clickable { vm.toggleTrackExpanded(track.id, !track.expanded) }
                    .padding(horizontal = 2.dp, vertical = 2.dp),
            )

            // Track Name + Micro-badges (Opacity, GRP, FX, CLIP, α, MUTE)
            Column(modifier = Modifier.weight(1f).padding(horizontal = 3.dp)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    if (track.isGroup) {
                        TrackMicroBadge("GRP", DreamsCyan)
                    }
                    Text(
                        text = track.name,
                        color = if (track.visible) DreamsInk else DreamsDimInk,
                        fontSize = 11.sp,
                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Row(
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "${(track.opacity * 100).roundToInt()}%",
                        color = DreamsDimInk,
                        fontSize = 9.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    if (hasFx) {
                        TrackMicroBadge("FX", FilterPurple)
                    }
                    if (track.clipToBelow) {
                        TrackMicroBadge("CLIP", OpacityCyan)
                    }
                    if (track.alphaLock) {
                        TrackMicroBadge("α", KeyframeGold)
                    }
                    if (track.muted) {
                        TrackMicroBadge("MUTE", PerformRed)
                    }
                }
            }

            // Track Options & Layer Effects trigger
            val optionsOpen = vm.trackOptionsTrackId == track.id
            Box(
                modifier = Modifier
                    .size(21.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .background(if (optionsOpen) Pc.Accent else Color.Transparent)
                    .clickable {
                        vm.selectTimelineTrack(track.id)
                        val next = if (optionsOpen) null else track.id
                        closeAllPopovers(vm)
                        vm.trackOptionsTrackId = next
                    },
                contentAlignment = Alignment.Center,
            ) {
                DreamsMoreDotsIcon(
                    tint = if (optionsOpen) Color.White else DreamsDimInk,
                    size = 13.dp,
                )
            }

            // Solo toggle ("S")
            Box(
                modifier = Modifier
                    .size(19.dp)
                    .clip(RoundedCornerShape(6.dp))
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
                    .size(21.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .clickable { vm.toggleTrackVisible(track.id, !track.visible) },
                contentAlignment = Alignment.Center,
            ) {
                DreamsEyeIcon(
                    visible = track.visible,
                    tint = if (track.visible) DreamsInk else DreamsDimInk.copy(alpha = 0.55f),
                    size = 13.dp,
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
private fun TrackMicroBadge(text: String, color: Color) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(4.dp))
            .background(color.copy(alpha = 0.18f))
            .padding(horizontal = 3.dp, vertical = 0.5.dp),
    ) {
        Text(
            text = text,
            color = color,
            fontSize = 7.5.sp,
            fontWeight = FontWeight.Bold,
            lineHeight = 9.sp,
        )
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
            for (clip in track.clips) {
                val isClipSelected = tl.activeClipId == clip.id || vm.selectedClipIds.contains(clip.id)
                val clipStartDp = zoomDp * clip.startFrame.toInt()
                val clipWidthDp = (zoomDp * clip.durationFrames.toInt()).coerceAtLeast(20.dp)
                val latestStart by rememberUpdatedState(clip.startFrame.toInt())
                val latestDuration by rememberUpdatedState(clip.durationFrames.toInt())
                val clipSnapCandidates = remember(track.clips, tl.playhead, tl.workArea, clip.id) {
                    buildList {
                        add(0)
                        add(tl.playhead.toInt())
                        tl.workArea?.let {
                            add(it.start.toInt())
                            add(it.end.toInt() + 1)
                        }
                        for (c in track.clips) {
                            if (c.id == clip.id) continue
                            add(c.startFrame.toInt())
                            add((c.startFrame + c.durationFrames).toInt())
                        }
                    }
                }
                val latestSnapCandidates by rememberUpdatedState(clipSnapCandidates)
                val clipAccent = when (clip.kind) {
                    TimelineClipKind.AUDIO -> AudioGreen
                    TimelineClipKind.VIDEO -> CameraSky
                    TimelineClipKind.TEXT -> FilterPurple
                    else -> tagColor
                }
                val clipShape = RoundedCornerShape(10.dp)

                Box(
                    modifier = Modifier
                        .offset(x = clipStartDp, y = 3.dp)
                        .width(clipWidthDp)
                        .height(trackHeight - 6.dp)
                        .clip(clipShape)
                        .background(if (isClipSelected) clipAccent.copy(alpha = 0.24f) else DreamsClipFill)
                        .border(
                            width = if (isClipSelected) 1.75.dp else 1.dp,
                            color = if (isClipSelected) clipAccent else DreamsEdge.copy(alpha = 0.85f),
                            shape = clipShape,
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
                        // Dragging the clip body moves its start frame smoothly without cancelling mid-drag
                        .pointerInput(clip.id, zoomPx) {
                            var dragOrigin = 0
                            var dragAccumPx = 0f
                            detectHorizontalDragGestures(
                                onDragStart = {
                                    dragOrigin = latestStart
                                    dragAccumPx = 0f
                                },
                                onHorizontalDrag = { change, dragAmount ->
                                    change.consume()
                                    dragAccumPx += dragAmount
                                    val deltaFrames = (dragAccumPx / zoomPx).roundToInt()
                                    val rawStart = (dragOrigin + deltaFrames).coerceAtLeast(0)
                                    val snapped = snapClipFrame(rawStart, latestSnapCandidates)
                                    vm.moveTimelineClip(clip.id, snapped, null, done = false)
                                },
                                onDragEnd = {
                                    val deltaFrames = (dragAccumPx / zoomPx).roundToInt()
                                    val rawStart = (dragOrigin + deltaFrames).coerceAtLeast(0)
                                    val snapped = snapClipFrame(rawStart, latestSnapCandidates)
                                    vm.moveTimelineClip(clip.id, snapped, null, done = true)
                                },
                                onDragCancel = {
                                    vm.moveTimelineClip(clip.id, dragOrigin, null, done = true)
                                },
                            )
                        },
                ) {
                    // Subtle colored top studio accent stripe
                    Box(
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .fillMaxWidth()
                            .height(2.5.dp)
                            .background(clipAccent.copy(alpha = if (isClipSelected) 0.95f else 0.65f)),
                    )

                    // Render Audio Waveform, Cel strips, Video/Text badge, or LayerClip thumbnail
                    when {
                        clip.kind == TimelineClipKind.AUDIO -> {
                            Canvas(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(horizontal = 4.dp, vertical = 5.dp),
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
                                            cap = StrokeCap.Round,
                                        )
                                    }
                                }
                            }
                            Row(
                                modifier = Modifier
                                    .align(Alignment.TopStart)
                                    .padding(start = 6.dp, top = 4.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                TrackMicroBadge("AUD", AudioGreen)
                                Text(
                                    text = clip.name,
                                    color = DreamsInk,
                                    fontSize = 10.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }

                        clip.kind == TimelineClipKind.CEL && clip.cels.isNotEmpty() -> {
                            Row(modifier = Modifier.fillMaxSize().padding(top = 2.dp)) {
                                for (cel in clip.cels) {
                                    val celWidthDp = (zoomDp * cel.durationFrames.toInt()).coerceAtLeast(10.dp)
                                    Box(
                                        modifier = Modifier
                                            .width(celWidthDp)
                                            .fillMaxHeight()
                                            .border(0.5.dp, DreamsEdge.copy(alpha = 0.55f)),
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
                                            Box(
                                                modifier = Modifier
                                                    .align(Alignment.BottomEnd)
                                                    .padding(end = 3.dp, bottom = 2.dp)
                                                    .clip(RoundedCornerShape(4.dp))
                                                    .background(DreamsSurface.copy(alpha = 0.75f))
                                                    .padding(horizontal = 3.dp),
                                            ) {
                                                Text(
                                                    text = "+${cel.hold}",
                                                    color = DreamsDimInk,
                                                    fontSize = 8.5.sp,
                                                    fontFamily = FontFamily.Monospace,
                                                )
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        else -> {
                            Row(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(start = 6.dp, end = 16.dp, top = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(5.dp),
                            ) {
                                when (clip.kind) {
                                    TimelineClipKind.VIDEO -> TrackMicroBadge("VID", CameraSky)
                                    TimelineClipKind.TEXT -> TrackMicroBadge("TXT", FilterPurple)
                                    else -> {}
                                }
                                vm.frameThumbnails[clip.primaryLayerId]?.let { thumb ->
                                    Image(
                                        bitmap = thumb,
                                        contentDescription = null,
                                        contentScale = ContentScale.Fit,
                                        modifier = Modifier
                                            .size((trackHeight - 16.dp).coerceAtLeast(16.dp))
                                            .clip(RoundedCornerShape(4.dp)),
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
                    }

                    // Perform Motion Ghost + Loop Mode badges on clip top-right
                    Row(
                        modifier = Modifier
                            .align(Alignment.TopEnd)
                            .padding(end = 15.dp, top = 5.dp),
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        if (clip.loopMode != TimelineClipLoopMode.NONE) {
                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(6.dp))
                                    .background(DreamsSurface.copy(alpha = 0.85f))
                                    .border(0.75.dp, KeyframeGold.copy(alpha = 0.7f), RoundedCornerShape(6.dp))
                                    .padding(horizontal = 4.dp, vertical = 0.5.dp),
                            ) {
                                Text(
                                    text = if (clip.loopMode == TimelineClipLoopMode.PING_PONG) "⇄" else "⟳",
                                    color = KeyframeGold,
                                    fontSize = 9.sp,
                                    fontWeight = FontWeight.Bold,
                                )
                            }
                        }
                        if (clip.performOnionGuide) {
                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(6.dp))
                                    .background(DreamsSurface.copy(alpha = 0.85f))
                                    .border(0.75.dp, OpacityCyan.copy(alpha = 0.7f), RoundedCornerShape(6.dp))
                                    .padding(horizontal = 3.dp, vertical = 1.dp),
                            ) {
                                DreamsGhostIcon(OpacityCyan, 10.dp)
                            }
                        }
                    }

                    // Left-edge Drag Handle to trim clip start frame while keeping end frame fixed
                    if (isClipSelected) {
                        Box(
                            modifier = Modifier
                                .align(Alignment.CenterStart)
                                .width(10.dp)
                                .fillMaxHeight()
                                .background(clipAccent.copy(alpha = 0.72f))
                                .pointerInput(clip.id, zoomPx) {
                                    var trimOrigin = 0
                                    var trimAccumPx = 0f
                                    detectHorizontalDragGestures(
                                        onDragStart = {
                                            trimOrigin = latestStart
                                            trimAccumPx = 0f
                                        },
                                        onHorizontalDrag = { change, dragAmount ->
                                            change.consume()
                                            trimAccumPx += dragAmount
                                            val deltaFrames = (trimAccumPx / zoomPx).roundToInt()
                                            val nextStart = (trimOrigin + deltaFrames).coerceAtLeast(0)
                                            vm.trimTimelineClipLeft(clip.id, nextStart, done = false)
                                        },
                                        onDragEnd = {
                                            val deltaFrames = (trimAccumPx / zoomPx).roundToInt()
                                            val nextStart = (trimOrigin + deltaFrames).coerceAtLeast(0)
                                            vm.trimTimelineClipLeft(clip.id, nextStart, done = true)
                                        },
                                        onDragCancel = {
                                            vm.trimTimelineClipLeft(clip.id, trimOrigin, done = true)
                                        },
                                    )
                                },
                            contentAlignment = Alignment.Center,
                        ) {
                            Box(
                                modifier = Modifier
                                    .width(2.dp)
                                    .height(14.dp)
                                    .clip(CircleShape)
                                    .background(Color.White.copy(alpha = 0.85f)),
                            )
                        }
                    }

                    // Right-edge Drag Handle to resize duration / hold smoothly without cancelling mid-drag
                    Box(
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .width(12.dp)
                            .fillMaxHeight()
                            .background(clipAccent.copy(alpha = if (isClipSelected) 0.80f else 0.45f))
                            .pointerInput(clip.id, zoomPx) {
                                var resizeOrigin = 1
                                var resizeAccumPx = 0f
                                detectHorizontalDragGestures(
                                    onDragStart = {
                                        resizeOrigin = latestDuration
                                        resizeAccumPx = 0f
                                    },
                                    onHorizontalDrag = { change, dragAmount ->
                                        change.consume()
                                        resizeAccumPx += dragAmount
                                        val deltaFrames = (resizeAccumPx / zoomPx).roundToInt()
                                        val nextDur = (resizeOrigin + deltaFrames).coerceAtLeast(1)
                                        vm.resizeTimelineClip(clip.id, nextDur, done = false)
                                    },
                                    onDragEnd = {
                                        val deltaFrames = (resizeAccumPx / zoomPx).roundToInt()
                                        val nextDur = (resizeOrigin + deltaFrames).coerceAtLeast(1)
                                        vm.resizeTimelineClip(clip.id, nextDur, done = true)
                                    },
                                    onDragCancel = {
                                        vm.resizeTimelineClip(clip.id, resizeOrigin, done = true)
                                    },
                                )
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        // Three tactile vertical grip dots
                        Column(verticalArrangement = Arrangement.spacedBy(2.5.dp)) {
                            repeat(3) {
                                Box(
                                    modifier = Modifier
                                        .size(2.5.dp)
                                        .clip(CircleShape)
                                        .background(Color.White.copy(alpha = 0.85f)),
                                )
                            }
                        }
                    }
                }
            }
        }

        // Sub-tracks for Move, Opacity, Live Filters, Warp/Distort, and Audio Volume/Pan Keyframes
        if (hasTransform) {
            KeyframeSubTrackLane(zoomDp, KeyframeGold, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.transformKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                            color = KeyframeGold,
                            onMove = { fromF, toF, done ->
                                vm.moveTimelineKeyframe(clip.id, TimelineKeyframeChannel.TRANSFORM, fromF, toF, done)
                            },
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                closeAllPopovers(vm)
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
        if (hasOpacity) {
            KeyframeSubTrackLane(zoomDp, OpacityCyan, 0.65f) {
                for (clip in track.clips) {
                    for (kf in clip.opacityKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                            color = OpacityCyan,
                            onMove = { fromF, toF, done ->
                                vm.moveTimelineKeyframe(clip.id, TimelineKeyframeChannel.OPACITY, fromF, toF, done)
                            },
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                closeAllPopovers(vm)
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
        if (hasFilter) {
            KeyframeSubTrackLane(zoomDp, FilterPurple, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.filterKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                            color = FilterPurple,
                            onMove = { fromF, toF, done ->
                                vm.moveTimelineKeyframe(clip.id, TimelineKeyframeChannel.FILTER, fromF, toF, done)
                            },
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                closeAllPopovers(vm)
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
        if (hasDeform) {
            KeyframeSubTrackLane(zoomDp, WarpPink, 0.65f) {
                for (clip in track.clips) {
                    for (kf in clip.deformKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                            color = WarpPink,
                            onMove = { fromF, toF, done ->
                                vm.moveTimelineKeyframe(clip.id, TimelineKeyframeChannel.DEFORM, fromF, toF, done)
                            },
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                closeAllPopovers(vm)
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
        if (hasAudio) {
            KeyframeSubTrackLane(zoomDp, AudioGreen, 0.45f) {
                for (clip in track.clips) {
                    for (kf in clip.volumeKeyframes) {
                        KeyframeDiamond(
                            frame = kf.frame.toInt(),
                            zoomDp = zoomDp,
                            zoomPx = zoomPx,
                            color = AudioGreen,
                            onMove = { fromF, toF, done ->
                                vm.moveTimelineKeyframe(clip.id, TimelineKeyframeChannel.VOLUME, fromF, toF, done)
                            },
                            onClick = {
                                vm.setTimelinePlayhead(kf.frame.toInt())
                                closeAllPopovers(vm)
                                vm.editingKeyframe = clip.id to kf.frame
                            },
                        )
                    }
                }
            }
        }
    }

    // Long-press Clip Context Menu Card
    contextMenuClip?.let { clip ->
        ClipContextMenuCard(
            vm = vm,
            tl = tl,
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
    zoomPx: Float,
    color: Color,
    onMove: ((fromFrame: Int, toFrame: Int, done: Boolean) -> Unit)? = null,
    onClick: () -> Unit,
) {
    val latestFrame by rememberUpdatedState(frame)
    val latestOnMove by rememberUpdatedState(onMove)
    val xDp = zoomDp * frame + zoomDp * 0.5f - 9.dp
    Box(
        modifier = Modifier
            .offset(x = xDp.coerceAtLeast(0.dp), y = 3.dp)
            .size(20.dp)
            .clickable(onClick = onClick)
            .then(
                if (onMove != null) {
                    Modifier.pointerInput(zoomPx) {
                        var dragOrigin = 0
                        var lastSent = 0
                        var accumPx = 0f
                        detectHorizontalDragGestures(
                            onDragStart = {
                                dragOrigin = latestFrame
                                lastSent = latestFrame
                                accumPx = 0f
                            },
                            onHorizontalDrag = { change, dragAmount ->
                                change.consume()
                                accumPx += dragAmount
                                val target = (dragOrigin + (accumPx / zoomPx).roundToInt()).coerceAtLeast(0)
                                if (target != lastSent) {
                                    latestOnMove?.invoke(lastSent, target, false)
                                    lastSent = target
                                }
                            },
                            onDragEnd = {
                                latestOnMove?.invoke(lastSent, lastSent, true)
                            },
                            onDragCancel = {
                                if (lastSent != dragOrigin) {
                                    latestOnMove?.invoke(lastSent, dragOrigin, true)
                                }
                            },
                        )
                    }
                } else {
                    Modifier
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(modifier = Modifier.size(13.dp)) {
            val path = Path().apply {
                moveTo(size.width / 2f, 1f)
                lineTo(size.width - 1f, size.height / 2f)
                lineTo(size.width / 2f, size.height - 1f)
                lineTo(1f, size.height / 2f)
                close()
            }
            drawPath(path, color = color)
            drawPath(path, color = Color(0xCC0A0A12), style = Stroke(width = 1.4f))
            drawCircle(Color.White.copy(alpha = 0.85f), radius = 1.8f, center = Offset(size.width / 2f, size.height / 2f))
        }
    }
}

private fun isTransformIdentity(t: TimelineTransformInfo): Boolean =
    abs(t.x) < 0.5f &&
        abs(t.y) < 0.5f &&
        abs(t.scaleX - 1f) < 0.005f &&
        abs(t.scaleY - 1f) < 0.005f &&
        abs(t.rotation) < 0.002f

private fun applyViewMatrix(m: List<Float>, cx: Float, cy: Float): Offset {
    if (m.size < 6) return Offset(cx, cy)
    return Offset(m[0] * cx + m[2] * cy + m[4], m[1] * cx + m[3] * cy + m[5])
}

private fun canvasDeltaFromScreen(m: List<Float>, dx: Float, dy: Float): Offset {
    if (m.size < 4) return Offset(dx, dy)
    val det = m[0] * m[3] - m[1] * m[2]
    if (abs(det) < 1e-6f) return Offset(dx, dy)
    return Offset(
        x = (m[3] * dx - m[2] * dy) / det,
        y = (-m[1] * dx + m[0] * dy) / det,
    )
}

/**
 * Projects the 4 corners of the drawable layer `[0..cw, 0..ch]` through `t` (or `t.inverse()` when
 * `invert == true`, i.e. Camera Track) and then through the current viewport matrix [viewMatrix].
 */
private fun projectedLayerCorners(
    cw: Float,
    ch: Float,
    t: TimelineTransformInfo,
    invert: Boolean,
    viewMatrix: List<Float>,
): List<Offset> {
    val ax = cw * t.anchorX
    val ay = ch * t.anchorY
    val sx = t.scaleX.let { if (abs(it) < 1e-3f) 1e-3f else it }
    val sy = t.scaleY.let { if (abs(it) < 1e-3f) 1e-3f else it }
    val cosR = cos(t.rotation)
    val sinR = sin(t.rotation)
    val raw = listOf(0f to 0f, cw to 0f, cw to ch, 0f to ch)
    return raw.map { (lx, ly) ->
        val (cx, cy) = if (!invert) {
            val dx = (lx - ax) * sx
            val dy = (ly - ay) * sy
            val rx = dx * cosR - dy * sinR
            val ry = dx * sinR + dy * cosR
            (rx + ax + t.x) to (ry + ay + t.y)
        } else {
            val dx = lx - ax - t.x
            val dy = ly - ay - t.y
            val rx = (dx * cosR + dy * sinR) / sx
            val ry = (-dx * sinR + dy * cosR) / sy
            (rx + ax) to (ry + ay)
        }
        applyViewMatrix(viewMatrix, cx, cy)
    }
}

/**
 * Non-blocking Draw-mode indicator shown when the global Camera Track is actively transforming the
 * viewport. Draws a subtle dashed cyan outline around the real drawable layer bounds (so the artist
 * always sees where layer edges sit under a panned/zoomed camera) plus a 1-tap `Draw 1:1` pill to
 * toggle Camera Preview off while keeping all Camera keyframes intact for Play & Export.
 */
@Composable
private fun BoxScope.CameraActiveDrawIndicator(
    vm: CanvasViewModel,
    tl: TimelineState,
    stageWidth: Dp,
    stageHeight: Dp,
) {
    val (cwInt, chInt) = remember(vm.session) { vm.canvasSize() }
    val cw = cwInt.coerceAtLeast(1).toFloat()
    val ch = chInt.coerceAtLeast(1).toFloat()
    val viewM = vm.viewMatrix
    val camZoomPct = (100f / tl.cameraTransform.scaleX.coerceIn(0.05f, 16f)).roundToInt()

    // Non-interactive dashed outline of the real drawable layer bounds under the active Camera
    Canvas(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .width(stageWidth)
            .height(stageHeight),
    ) {
        val corners = projectedLayerCorners(cw, ch, tl.cameraTransform, invert = true, viewMatrix = viewM)
        if (corners.size == 4) {
            val path = Path().apply {
                moveTo(corners[0].x, corners[0].y)
                lineTo(corners[1].x, corners[1].y)
                lineTo(corners[2].x, corners[2].y)
                lineTo(corners[3].x, corners[3].y)
                close()
            }
            drawPath(
                path = path,
                color = CameraSky.copy(alpha = 0.55f),
                style = Stroke(width = 2f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 8f))),
            )
        }
    }

    // Compact top pill to inspect/edit Camera or switch to 1:1 unclipped Stage drawing
    Row(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .padding(top = Pc.barShown + 8.dp)
            .shadow(8.dp, RoundedCornerShape(18.dp))
            .clip(RoundedCornerShape(18.dp))
            .background(DreamsCardFill.copy(alpha = 0.94f))
            .border(1.dp, CameraSky.copy(alpha = 0.65f), RoundedCornerShape(18.dp))
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .clickable {
                    vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                    vm.stageGizmoActive = true
                }
                .padding(horizontal = 4.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            DreamsCameraIcon(CameraSky, 12.dp)
            Text(
                text = "Cam $camZoomPct%",
                color = CameraSky,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
            )
        }
        Box(Modifier.width(1.dp).height(12.dp).background(DreamsEdge))
        Text(
            text = "Draw 1:1",
            color = DreamsInk,
            fontSize = 11.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .background(DreamsPillBg)
                .clickable { vm.setCameraEnabled(false) }
                .padding(horizontal = 8.dp, vertical = 3.dp),
        )
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
    val isAnchor = vm.stageGizmoMode == StageGizmoSubMode.ANCHOR
    val isDistort = vm.stageGizmoMode == StageGizmoSubMode.DISTORT
    val isWarp = vm.stageGizmoMode == StageGizmoSubMode.WARP
    if (!isCamera && activeClipId == null) return

    val view = LocalView.current
    val currentPlayhead by rememberUpdatedState(tl.playhead.toInt())
    val viewMatrix = vm.viewMatrix
    val latestViewMatrix by rememberUpdatedState(remember(tl, viewMatrix, isCamera) {
        if (isCamera || activeClipId == null) viewMatrix else vm.session?.timelineStageEditMatrix(activeClipId) ?: viewMatrix
    })
    val (cwInt, chInt) = remember(vm.session) { vm.canvasSize() }
    val stageCanvasW = cwInt.coerceAtLeast(1).toFloat()
    val stageCanvasH = chInt.coerceAtLeast(1).toFloat()
    val baseTransform = if (isCamera) tl.cameraTransform else tl.activeEvaluatedTransform
    var isInteracting by remember { mutableStateOf(false) }
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
    fun warpHandle(u: Float, v: Float): Offset {
        val t = liveTransform
        val dx = (u - t.anchorX) * stageCanvasW * t.scaleX
        val dy = (v - t.anchorY) * stageCanvasH * t.scaleY
        return applyViewMatrix(latestViewMatrix,
            t.anchorX * stageCanvasW + t.x + dx * cos(t.rotation) - dy * sin(t.rotation),
            t.anchorY * stageCanvasH + t.y + dx * sin(t.rotation) + dy * cos(t.rotation))
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
        isAnchor -> OpacityCyan
        isWarp || isDistort -> WarpPink
        else -> KeyframeGold
    }

    Box(
        modifier = Modifier
            .align(Alignment.TopCenter)
            .width(stageWidth)
            .height(stageHeight)
            .clipToBounds()
            // DreamsCraft-style 2-finger tap = Undo, 3-finger tap = Redo right over Perform & Stage Gizmo
            .multiFingerUndoRedo(
                onUndo = { vm.undo() },
                onRedo = { vm.redo() },
            )
            // Move & Scale / Anchor / Camera / Warp / Distort gestures (1:1 calibrated with vm.viewMatrix)
            .then(
                if (isAnchor && activeClipId != null) {
                    Modifier.pointerInput(activeClipId) {
                        detectDragGestures(
                            onDragStart = { isInteracting = true },
                            onDrag = { change, drag ->
                                change.consume()
                                val cd = canvasDeltaFromScreen(latestViewMatrix, drag.x, drag.y)
                                val cosR = cos(liveTransform.rotation)
                                val sinR = sin(liveTransform.rotation)
                                val sx = liveTransform.scaleX.let { if (abs(it) < 0.01f) 0.01f else it }
                                val sy = liveTransform.scaleY.let { if (abs(it) < 0.01f) 0.01f else it }
                                val localDx = (cd.x * cosR + cd.y * sinR) / sx
                                val localDy = (-cd.x * sinR + cd.y * cosR) / sy
                                val nextAx = (liveTransform.anchorX + localDx / stageCanvasW).coerceIn(-1f, 2f)
                                val nextAy = (liveTransform.anchorY + localDy / stageCanvasH).coerceIn(-1f, 2f)
                                vm.setClipAnchorKeepPosition(activeClipId, currentPlayhead, nextAx, nextAy, done = false)?.let {
                                    liveTransform = it
                                }
                            },
                            onDragEnd = {
                                isInteracting = false
                                vm.setClipAnchorKeepPosition(activeClipId, currentPlayhead, liveTransform.anchorX, liveTransform.anchorY, done = true)?.let {
                                    liveTransform = it
                                }
                            },
                            onDragCancel = { isInteracting = false },
                        )
                    }
                } else if (!isWarp && !isDistort) {
                    Modifier.pointerInput(activeClipId, isCamera, vm.dreamsMode) {
                        awaitEachGesture {
                            awaitFirstDown(requireUnconsumed = false)
                            val perform = vm.dreamsMode == DreamsWorkMode.PERFORM
                            var started = false
                            var released = false
                            var travel = Offset.Zero
                            var zoomTravel = 1f
                            var turnTravel = 0f
                            try {
                                while (true) {
                                    val event = awaitPointerEvent()
                                    if (event.changes.none { it.pressed }) {
                                        released = true
                                        break
                                    }
                                    if (event.changes.any { it.isConsumed }) break
                                    fun bounded(p: Offset) = Offset(p.x.coerceIn(0f, size.width.toFloat()), p.y.coerceIn(0f, size.height.toFloat()))
                                    val from = event.calculateCentroid(useCurrent = false)
                                    val to = event.calculateCentroid()
                                    if (!from.x.isFinite() || !to.x.isFinite()) continue
                                    val pan = bounded(to) - bounded(from)
                                    val zoom = event.calculateZoom()
                                    val rotationDeg = event.calculateRotation()
                                    if (!started) {
                                        travel += pan
                                        zoomTravel *= zoom
                                        turnTravel += rotationDeg
                                        val radius = event.calculateCentroidSize(useCurrent = false)
                                        val slop = viewConfiguration.touchSlop
                                        if (travel.getDistance() <= slop && abs(1f - zoomTravel) * radius <= slop &&
                                            abs(turnTravel) * Math.PI / 180.0 * radius <= slop) continue
                                        if (perform) vm.beginPerformRecording(isCamera = isCamera, initialTransform = liveTransform)
                                        started = true
                                        isInteracting = true
                                    }
                                    if (perform && !vm.performRecording) break
                                    val rotRad = (rotationDeg * Math.PI / 180.0).toFloat()
                                    val canvasDelta = canvasDeltaFromScreen(latestViewMatrix, pan.x, pan.y)
                                    val updated = if (isCamera) {
                                        // Camera applies camera_inv to the stage: pinch-out zooms IN (smaller
                                        // camera scale), drag moves the stage 1:1 under the fingers, rotate turns with fingers.
                                        val safeZoom = zoom.coerceIn(0.25f, 4f)
                                        val nextSx = (liveTransform.scaleX / safeZoom).coerceIn(0.08f, 12f)
                                        val nextSy = (liveTransform.scaleY / safeZoom).coerceIn(0.08f, 12f)
                                        val nextRot = liveTransform.rotation - rotRad
                                        val cosR = cos(nextRot)
                                        val sinR = sin(nextRot)
                                        val camDx = (canvasDelta.x * cosR - canvasDelta.y * sinR) * nextSx
                                        val camDy = (canvasDelta.x * sinR + canvasDelta.y * cosR) * nextSy
                                        TimelineTransformInfo(
                                            x = liveTransform.x - camDx,
                                            y = liveTransform.y - camDy,
                                            scaleX = nextSx,
                                            scaleY = nextSy,
                                            rotation = nextRot,
                                            anchorX = liveTransform.anchorX,
                                            anchorY = liveTransform.anchorY,
                                        )
                                    } else {
                                        TimelineTransformInfo(
                                            x = liveTransform.x + canvasDelta.x,
                                            y = liveTransform.y + canvasDelta.y,
                                            scaleX = (liveTransform.scaleX * zoom).coerceIn(0.05f, 16f),
                                            scaleY = (liveTransform.scaleY * zoom).coerceIn(0.05f, 16f),
                                            rotation = liveTransform.rotation + rotRad,
                                            anchorX = liveTransform.anchorX,
                                            anchorY = liveTransform.anchorY,
                                        )
                                    }
                                    liveTransform = updated
                                    when {
                                        vm.dreamsMode == DreamsWorkMode.PERFORM -> vm.recordPerformTransform(updated, isCamera = isCamera)
                                        isCamera -> vm.setCameraTransformKeyframe(currentPlayhead, updated, done = false)
                                        activeClipId != null -> vm.setClipTransformKeyframe(activeClipId, currentPlayhead, updated, done = false)
                                    }
                                    event.changes.filter { it.position != it.previousPosition }.forEach { it.consume() }
                                }
                            } finally {
                                if (started) {
                                    when {
                                        perform -> if (vm.performRecording) {
                                            if (released) vm.finishPerformRecording() else vm.cancelPerformRecording()
                                        }
                                        isCamera -> vm.setCameraTransformKeyframe(currentPlayhead, liveTransform, done = true)
                                        activeClipId != null -> vm.setClipTransformKeyframe(activeClipId, currentPlayhead, liveTransform, done = true)
                                    }
                                }
                                isInteracting = false
                            }
                        }
                    }
                } else {
                    // Distort (4 corners) or Warp (4x4 16 nodes) drag interaction (+ Perform Warp!)
                    Modifier.pointerInput(activeClipId, isWarp, isDistort, vm.dreamsMode) {
                        detectDragGestures(
                            onDragStart = { pos ->
                                val points = if (isDistort) distortCorners else warpPoints.chunked(2).map { it[0] to it[1] }
                                fun distance(i: Int) = (pos - warpHandle(points[i].first, points[i].second)).getDistance()
                                val bestIdx = points.indices.minByOrNull(::distance)?.takeIf { distance(it) <= 40.dp.toPx() } ?: -1
                                activeDeformHandle = bestIdx
                                if (bestIdx >= 0) {
                                    isInteracting = true
                                    if (vm.dreamsMode == DreamsWorkMode.PERFORM && !vm.performRecording) {
                                        val initDeform = if (isDistort) {
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
                                        vm.beginPerformRecording(isCamera = false, initialDeform = initDeform)
                                    }
                                }
                            },
                            onDrag = { change, drag ->
                                val idx = activeDeformHandle
                                if (idx < 0 || activeClipId == null) return@detectDragGestures
                                change.consume()
                                if (vm.dreamsMode == DreamsWorkMode.PERFORM && !vm.performRecording) return@detectDragGestures
                                val delta = canvasDeltaFromScreen(latestViewMatrix, drag.x, drag.y)
                                val t = liveTransform
                                val safeSx = if (abs(t.scaleX) < 0.01f) (if (t.scaleX < 0f) -0.01f else 0.01f) else t.scaleX
                                val safeSy = if (abs(t.scaleY) < 0.01f) (if (t.scaleY < 0f) -0.01f else 0.01f) else t.scaleY
                                val du = (delta.x * cos(t.rotation) + delta.y * sin(t.rotation)) / (stageCanvasW * safeSx)
                                val dv = (delta.y * cos(t.rotation) - delta.x * sin(t.rotation)) / (stageCanvasH * safeSy)
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
                                    if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                                        if (vm.performRecording) vm.finishPerformRecording()
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
        // Visual Stage Gizmo Canvas (Move & Scale + Eased Motion Path, Anchor, Distort 4-pin, Warp 4x4 Mesh, or Camera Viewport)
        Canvas(modifier = Modifier.fillMaxSize()) {
            // Live red Perform trail following the finger in real time
            val liveTrail = vm.performTrailPoints
            if (vm.performRecording && !isWarp && !isDistort && !isAnchor && liveTrail.size >= 2) {
                val trailPath = Path()
                liveTrail.forEachIndexed { idx, (tx, ty) ->
                    val pt = applyViewMatrix(
                        latestViewMatrix,
                        stageCanvasW * 0.5f + if (isCamera) -tx else tx,
                        stageCanvasH * 0.5f + if (isCamera) -ty else ty,
                    )
                    if (idx == 0) trailPath.moveTo(pt.x, pt.y) else trailPath.lineTo(pt.x, pt.y)
                }
                drawPath(
                    path = trailPath,
                    color = PerformRed.copy(alpha = 0.28f),
                    style = Stroke(width = 9f),
                )
                drawPath(
                    path = trailPath,
                    color = PerformRed.copy(alpha = 0.92f),
                    style = Stroke(width = 3f),
                )
            }
            // Draw Eased Motion Path with speed-spaced dots across transform keyframes in screen space
            val pathKeyframes = if (isCamera) tl.cameraTransformKeyframes else activeClip?.transformKeyframes.orEmpty()
            if (!isWarp && !isDistort && pathKeyframes.size >= 2) {
                val motionPath = Path()
                var started = false
                for (i in pathKeyframes.indices) {
                    val kf = pathKeyframes[i]
                    val t = kf.transform ?: continue
                    val pt = applyViewMatrix(
                        latestViewMatrix,
                        stageCanvasW * t.anchorX + if (isCamera) -t.x else t.x,
                        stageCanvasH * t.anchorY + if (isCamera) -t.y else t.y,
                    )
                    if (!started) {
                        motionPath.moveTo(pt.x, pt.y)
                        started = true
                    } else {
                        motionPath.lineTo(pt.x, pt.y)
                    }
                    // Speed-spaced eased trajectory dots between consecutive keyframes
                    if (i + 1 < pathKeyframes.size) {
                        val nextKf = pathKeyframes[i + 1]
                        val nt = nextKf.transform
                        if (nt != null) {
                            val span = (nextKf.frame.toInt() - kf.frame.toInt()).coerceIn(1, 32)
                            val dotCount = span.coerceIn(3, 18)
                            for (s in 1 until dotCount) {
                                val u = s.toFloat() / dotCount.toFloat()
                                val e = evalEasingInfo(kf.easing, u)
                                val ix = t.x + (nt.x - t.x) * e
                                val iy = t.y + (nt.y - t.y) * e
                                val iax = t.anchorX + (nt.anchorX - t.anchorX) * e
                                val iay = t.anchorY + (nt.anchorY - t.anchorY) * e
                                val dotPt = applyViewMatrix(
                                    latestViewMatrix,
                                    stageCanvasW * iax + if (isCamera) -ix else ix,
                                    stageCanvasH * iay + if (isCamera) -iy else iy,
                                )
                                drawCircle(color = gizmoColor.copy(alpha = 0.58f), radius = 2.4f, center = dotPt)
                            }
                        }
                    }
                    // Keyframe anchor dot
                    drawCircle(color = Color.White, radius = 5.5f, center = pt)
                    drawCircle(color = gizmoColor, radius = 4f, center = pt)
                }
                if (started) {
                    drawPath(
                        path = motionPath,
                        color = gizmoColor.copy(alpha = 0.55f),
                        style = Stroke(width = 2f, pathEffect = PathEffect.dashPathEffect(floatArrayOf(8f, 6f))),
                    )
                }
            }

            if (isDistort || isWarp) {
                val mesh = if (isDistort) distortCornersToDeformPoints(distortCorners) else warpPoints
                fun pt(row: Int, col: Int): Offset {
                    val idx = (row * 4 + col) * 2
                    return warpHandle(mesh[idx], mesh[idx + 1])
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
                        val o = warpHandle(c.first, c.second)
                        drawCircle(Color.White, radius = if (i == activeDeformHandle) 12f else 9f, center = o)
                        drawCircle(gizmoColor, radius = if (i == activeDeformHandle) 9f else 6.5f, center = o)
                    }
                } else {
                    for (i in 0 until 16) {
                        val o = warpHandle(warpPoints[i * 2], warpPoints[i * 2 + 1])
                        drawCircle(Color.White, radius = if (i == activeDeformHandle) 10f else 7f, center = o)
                        drawCircle(gizmoColor, radius = if (i == activeDeformHandle) 7.5f else 5f, center = o)
                    }
                }
            } else {
                val cw = stageCanvasW
                val ch = stageCanvasH
                val viewM = latestViewMatrix
                val corners = projectedLayerCorners(cw, ch, liveTransform, invert = isCamera, viewMatrix = viewM)
                if (corners.size == 4) {
                    val path = Path().apply {
                        moveTo(corners[0].x, corners[0].y)
                        lineTo(corners[1].x, corners[1].y)
                        lineTo(corners[2].x, corners[2].y)
                        lineTo(corners[3].x, corners[3].y)
                        close()
                    }
                    drawPath(
                        path = path,
                        color = gizmoColor.copy(alpha = if (isAnchor) 0.5f else 1f),
                        style = Stroke(
                            width = 2.5f,
                            pathEffect = if (isCamera || isAnchor) PathEffect.dashPathEffect(floatArrayOf(12f, 6f)) else null,
                        ),
                    )
                    if (!isAnchor) {
                        for (c in corners) {
                            drawCircle(color = Color.White, radius = 7.5f, center = c)
                            drawCircle(color = gizmoColor, radius = 5.5f, center = c)
                        }
                    }
                    // Real Anchor Point Pivot crosshair
                    val pivot = if (isCamera) {
                        Offset((corners[0].x + corners[2].x) * 0.5f, (corners[0].y + corners[2].y) * 0.5f)
                    } else {
                        applyViewMatrix(viewM, cw * liveTransform.anchorX + liveTransform.x, ch * liveTransform.anchorY + liveTransform.y)
                    }
                    val ringR = if (isAnchor) 18f else 11f
                    val armR = if (isAnchor) 26f else 15f
                    if (isAnchor) {
                        drawCircle(color = OpacityCyan.copy(alpha = 0.22f), radius = 26f, center = pivot)
                    }
                    drawCircle(color = Color.White, radius = ringR + 1.5f, center = pivot, style = Stroke(width = 3.2f))
                    drawCircle(color = if (isAnchor) OpacityCyan else gizmoColor, radius = ringR, center = pivot, style = Stroke(width = 2.2f))
                    drawLine(color = if (isAnchor) OpacityCyan else gizmoColor, start = Offset(pivot.x - armR, pivot.y), end = Offset(pivot.x + armR, pivot.y), strokeWidth = 2.4f)
                    drawLine(color = if (isAnchor) OpacityCyan else gizmoColor, start = Offset(pivot.x, pivot.y - armR), end = Offset(pivot.x, pivot.y + armR), strokeWidth = 2.4f)
                }
                // In Camera mode also highlight the fixed output frame [0..cw, 0..ch] clearly
                if (isCamera) {
                    val frameCorners = listOf(
                        applyViewMatrix(viewM, 0f, 0f),
                        applyViewMatrix(viewM, cw, 0f),
                        applyViewMatrix(viewM, cw, ch),
                        applyViewMatrix(viewM, 0f, ch),
                    )
                    val framePath = Path().apply {
                        moveTo(frameCorners[0].x, frameCorners[0].y)
                        lineTo(frameCorners[1].x, frameCorners[1].y)
                        lineTo(frameCorners[2].x, frameCorners[2].y)
                        lineTo(frameCorners[3].x, frameCorners[3].y)
                        close()
                    }
                    drawPath(path = framePath, color = Color.White.copy(alpha = 0.85f), style = Stroke(width = 2f))
                }
            }
        }

        // Top Stage Pill: Sub-mode tabs (Move | Anchor | Distort | Warp | Camera) + Edit Pixels + Reset
        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = Pc.barShown + 8.dp, start = 12.dp, end = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                modifier = Modifier
                    .shadow(10.dp, RoundedCornerShape(20.dp))
                    .clip(RoundedCornerShape(20.dp))
                    .background(DreamsCardFill.copy(alpha = 0.96f))
                    .border(1.dp, gizmoColor.copy(alpha = 0.72f), RoundedCornerShape(20.dp))
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                // Sub-mode switcher
                for ((sub, label) in listOf(
                    StageGizmoSubMode.MOVE_SCALE to "Move",
                    StageGizmoSubMode.ANCHOR to "Anchor",
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
                            .clip(RoundedCornerShape(10.dp))
                            .background(if (chosen) gizmoColor else Color.Transparent)
                            .clickable {
                                vm.stageGizmoMode = sub
                                if (sub == StageGizmoSubMode.CAMERA) vm.setCameraEnabled(true)
                            }
                            .padding(horizontal = 9.dp, vertical = 4.dp),
                    )
                }

                Box(Modifier.width(1.dp).height(14.dp).background(DreamsEdge))

                // Seamless 1-tap switch to ProCraft's native destructive pixel Transform box
                if (vm.dreamsMode != DreamsWorkMode.PERFORM) {
                    Text(
                        text = "Edit Pixels",
                        color = DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(DreamsPillBg)
                            .clickable { vm.enterPixelTransform() }
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                    )
                }

                if (isCamera) {
                    val zoomPct = (100f / liveTransform.scaleX.coerceIn(0.05f, 16f)).roundToInt()
                    Text(
                        text = "$zoomPct%",
                        color = CameraSky,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                    )
                    Text(
                        text = "Draw 1:1",
                        color = DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(DreamsPillBg)
                            .clickable {
                                vm.setCameraEnabled(false)
                                vm.stageGizmoActive = false
                                vm.dreamsMode = DreamsWorkMode.DRAW_PAINT
                            }
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                    )
                }

                // Reset current gizmo property
                Text(
                    text = if (isAnchor) "Center" else "Reset",
                    color = DreamsInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsPillBg)
                        .clickable {
                            when {
                                isAnchor && activeClipId != null -> {
                                    vm.setClipAnchorKeepPosition(activeClipId, tl.playhead.toInt(), 0.5f, 0.5f, done = true)?.let {
                                        liveTransform = it
                                    }
                                }
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
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )

                // Quick exit gizmo / Perform
                Box(
                    modifier = Modifier
                        .size(22.dp)
                        .clip(CircleShape)
                        .background(DreamsPillBg)
                        .clickable {
                            vm.stageGizmoActive = false
                            if (vm.dreamsMode == DreamsWorkMode.PERFORM) {
                                vm.dreamsMode = DreamsWorkMode.DRAW_PAINT
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("✕", color = DreamsDimInk, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                }
            }

            // Second Row when in Perform Mode: Recording indicator + Replay + Undo + Motion Filtering + Perform Motion Ghost
            if (vm.dreamsMode == DreamsWorkMode.PERFORM && (activeClip != null || isCamera)) {
                Row(
                    modifier = Modifier
                        .shadow(10.dp, RoundedCornerShape(18.dp))
                        .clip(RoundedCornerShape(18.dp))
                        .background(DreamsCardFill.copy(alpha = 0.96f))
                        .border(1.dp, PerformRed.copy(alpha = 0.78f), RoundedCornerShape(18.dp))
                        .horizontalScroll(rememberScrollState())
                        .padding(horizontal = 12.dp, vertical = 5.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        Box(
                            modifier = Modifier
                                .size(7.dp)
                                .clip(CircleShape)
                                .background(PerformRed),
                        )
                        Text(
                            text = when {
                                vm.performRecording -> "REC F${tl.playhead.toInt() + 1} (${vm.performSamplesCount})"
                                isCamera -> "Drag Camera to Perform"
                                else -> "Drag Object to Perform"
                            },
                            color = PerformRed,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = if (vm.performRecording) FontFamily.Monospace else FontFamily.Default,
                        )
                    }
                    if (vm.hasRecentPerformTake && !vm.performRecording) {
                        Row(
                            modifier = Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(Pc.Accent.copy(alpha = 0.22f))
                                .border(1.dp, Pc.Accent.copy(alpha = 0.65f), RoundedCornerShape(8.dp))
                                .clickable {
                                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                    vm.replayLastPerform()
                                }
                                .padding(horizontal = 8.dp, vertical = 3.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Text(
                                text = "▶ Replay",
                                color = DreamsInk,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                    }
                    val undoReady = vm.canUndo || vm.performRecording
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (undoReady) DreamsPillBg else Color.Transparent)
                            .clickable(enabled = undoReady) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                vm.undo()
                            }
                            .padding(horizontal = 7.dp, vertical = 3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        DreamsUndoRedoIcon(
                            redo = false,
                            tint = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.4f),
                            size = 12.dp,
                        )
                        Text(
                            text = "Undo",
                            color = if (undoReady) DreamsInk else DreamsDimInk.copy(alpha = 0.45f),
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                    val filterPct = (tl.motionFiltering * 100).roundToInt()
                    Text(
                        text = "Filter $filterPct%",
                        color = DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(DreamsPillBg)
                            .clickable {
                                val next = when {
                                    tl.motionFiltering < 0.25f -> 0.35f
                                    tl.motionFiltering < 0.55f -> 0.70f
                                    tl.motionFiltering < 0.85f -> 1.0f
                                    else -> 0.0f
                                }
                                vm.updatePerformMotionFiltering(next)
                            }
                            .padding(horizontal = 8.dp, vertical = 3.dp),
                    )
                    // Perform Motion Ghost (Onion Skin of 1st frame along Perform motion)
                    if (activeClip != null && !isCamera) {
                        val ghostOn = activeClip.performOnionGuide
                        Row(
                            modifier = Modifier
                                .clip(RoundedCornerShape(10.dp))
                                .background(if (ghostOn) OpacityCyan else DreamsPillBg)
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
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(5.dp),
                        ) {
                            DreamsGhostIcon(if (ghostOn) Color.Black else OpacityCyan, 12.dp)
                            Text(
                                text = if (ghostOn) "Ghost ${(activeClip.performGuideOpacity * 100).roundToInt()}%" else "Ghost 1st",
                                color = if (ghostOn) Color.Black else OpacityCyan,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                    }
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
    val popShape = RoundedCornerShape(22.dp)
    Column(
        modifier = Modifier
            .align(Alignment.BottomStart)
            .padding(start = 16.dp, bottom = bottomOffset)
            .width(344.dp)
            .heightIn(max = 440.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, DreamsGlassEdge, popShape)
            .pointerInput(Unit) { detectTapGestures { } }
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column {
                Text("Studio & Movie", color = DreamsInk, fontSize = 15.sp, fontWeight = FontWeight.Bold)
                Text("FPS, Onion Skin & Auto-Draw", color = DreamsDimInk, fontSize = 11.sp)
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background(Pc.Accent.copy(alpha = 0.16f))
                    .clickable { vm.movieSettingsOpen = false }
                    .padding(horizontal = 10.dp, vertical = 4.dp),
            ) {
                Text("Done", color = Pc.Accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            }
        }

        // FPS Presets
        Text("Frames Per Second", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (presetFps in listOf(8, 12, 15, 24, 30, 60)) {
                val chosen = tl.fps.toInt() == presetFps
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(9.dp))
                        .background(if (chosen) Pc.Accent else DreamsPillBg)
                        .clickable { vm.setTimelineFps(presetFps) }
                        .padding(horizontal = 10.dp, vertical = 5.dp),
                ) {
                    Text(
                        text = "$presetFps",
                        color = if (chosen) Color.White else DreamsInk,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                        fontFamily = FontFamily.Monospace,
                    )
                }
            }
        }

        // Playback Mode
        Text("Playback Loop", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            val modes = listOf(
                AnimationPlayback.LOOP to "Loop",
                AnimationPlayback.PING_PONG to "Ping-Pong",
                AnimationPlayback.ONE_SHOT to "One Shot",
            )
            for ((mode, label) in modes) {
                val chosen = tl.playback == mode
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(9.dp))
                        .background(if (chosen) Pc.Accent else DreamsPillBg)
                        .clickable { vm.setTimelinePlayback(mode) }
                        .padding(horizontal = 11.dp, vertical = 5.dp),
                ) {
                    Text(
                        text = label,
                        color = if (chosen) Color.White else DreamsInk,
                        fontSize = 12.sp,
                        fontWeight = if (chosen) FontWeight.Bold else FontWeight.Medium,
                    )
                }
            }
        }

        // Onion Skin Frames
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Onion Skin (${tl.onion.beforeFrames})", color = DreamsInk, fontSize = 12.sp, fontWeight = FontWeight.Medium)
            Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                for (count in listOf(0u, 1u, 3u, 6u, 12u)) {
                    val chosen = tl.onion.beforeFrames == count && tl.onion.enabled == (count > 0u)
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (chosen) KeyframeGold else DreamsPillBg)
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
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
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
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val hasWorkArea = tl.workArea != null
            Box(Modifier.weight(1f)) {
                MenuActionRow(
                    label = if (hasWorkArea) "Clear Work Area" else "Set Work Area",
                    color = if (hasWorkArea) PerformRed else DreamsCyan,
                ) {
                    if (hasWorkArea) {
                        vm.setTimelineWorkArea(null, null)
                    } else {
                        val p = tl.playhead.toInt()
                        val end = (p + tl.fps.toInt()).coerceAtMost(tl.durationFrames.toInt() - 1)
                        vm.setTimelineWorkArea(p, end)
                    }
                }
            }
            Box(Modifier.weight(1f)) {
                MenuActionRow(
                    label = if (tl.showBackstage) "Backstage: ON" else "Backstage: OFF",
                    color = if (tl.showBackstage) DreamsInk else DreamsDimInk,
                ) {
                    vm.setBackstageShown(!tl.showBackstage)
                }
            }
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

    val popShape = RoundedCornerShape(22.dp)
    Column(
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = 16.dp, bottom = bottomOffset)
            .width(276.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, DreamsGlassEdge, popShape)
            .pointerInput(Unit) { detectTapGestures { } }
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text("Add to Timeline", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.Bold)
        MenuActionRow("New Drawing Track (Flipbook)", badge = "CEL", badgeColor = DreamsCyan) {
            vm.addTrackMenuOpen = false
            vm.addTimelineTrack(null, withDrawingClip = true)
        }
        MenuActionRow("New Content Track (Keyframe)", badge = "LYR", badgeColor = DreamsViolet) {
            vm.addTrackMenuOpen = false
            vm.addTimelineTrack("Content Track", withDrawingClip = false)
            vm.addLayerClipToActiveTrack()
        }
        MenuActionRow("New Text Clip (Animated Title)", badge = "TXT", badgeColor = FilterPurple) {
            vm.addTrackMenuOpen = false
            vm.addTextClipToActiveTrack("Title")
        }
        MenuActionRow("Import Audio Track (WAV/MP3)", color = AudioGreen, badge = "AUD", badgeColor = AudioGreen) {
            audioPicker.launch(arrayOf("audio/*"))
        }
        MenuActionRow("Import Video Clip (Rotoscope)", color = CameraSky, badge = "VID", badgeColor = CameraSky) {
            videoPicker.launch(arrayOf("video/*"))
        }
        MenuActionRow("New Cel / Frame in Active Clip", badge = "+1", badgeColor = KeyframeGold) {
            vm.addTrackMenuOpen = false
            vm.addTimelineCel()
        }
        MenuActionRow("Add Live Filter Keyframe", color = FilterPurple, badge = "FX", badgeColor = FilterPurple) {
            vm.addTrackMenuOpen = false
            vm.liveFilterPopoverOpen = true
        }
        MenuActionRow("Split Clip at Playhead", badge = "CUT", badgeColor = PerformRed) {
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
    val video = activeClipOf(tl)?.takeIf { it.kind == TimelineClipKind.VIDEO }
    val relink = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        vm.playheadActionMenuOpen = false
        if (video != null && uri != null) vm.importTimelineVideo(uri, video.id)
    }
    val popShape = RoundedCornerShape(22.dp)
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(bottom = bottomOffset)
            .width(272.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, PerformRed.copy(alpha = 0.72f), popShape)
            .pointerInput(Unit) { detectTapGestures { } }
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                DreamsSparkIcon(PerformRed, 13.dp)
                Text(
                    text = "Playhead · F${tl.playhead + 1u}",
                    color = PerformRed,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
            Text(
                text = "Close",
                color = DreamsDimInk,
                fontSize = 11.sp,
                modifier = Modifier.clickable { vm.playheadActionMenuOpen = false },
            )
        }
        if (video != null) {
            MenuActionRow("Relink Video Source", color = CameraSky) { relink.launch(arrayOf("video/*")) }
        }
        MenuActionRow("Move & Scale Keyframe", color = KeyframeGold, badge = "KEY", badgeColor = KeyframeGold) {
            vm.playheadActionMenuOpen = false
            tl.activeClipId?.let { cid ->
                vm.setClipTransformKeyframe(cid, tl.playhead.toInt(), tl.activeEvaluatedTransform, done = true)
                vm.stageGizmoMode = StageGizmoSubMode.MOVE_SCALE
                vm.stageGizmoActive = true
            }
        }
        MenuActionRow("Edit Anchor Point (Pivot)", color = OpacityCyan, badge = "PIV", badgeColor = OpacityCyan) {
            vm.playheadActionMenuOpen = false
            vm.stageGizmoMode = StageGizmoSubMode.ANCHOR
            vm.stageGizmoActive = true
        }
        MenuActionRow("Warp / Distort Keyframe", color = WarpPink, badge = "MESH", badgeColor = WarpPink) {
            vm.playheadActionMenuOpen = false
            vm.stageGizmoMode = StageGizmoSubMode.WARP
            vm.stageGizmoActive = true
        }
        MenuActionRow("Live Filters (Blur / HSB / Noise)", color = FilterPurple, badge = "FX", badgeColor = FilterPurple) {
            vm.playheadActionMenuOpen = false
            vm.liveFilterPopoverOpen = true
        }
        MenuActionRow("Camera Track Keyframe", color = CameraSky, badge = "CAM", badgeColor = CameraSky) {
            vm.playheadActionMenuOpen = false
            vm.setCameraTransformKeyframe(tl.playhead.toInt(), tl.cameraTransform, done = true)
            vm.stageGizmoMode = StageGizmoSubMode.CAMERA
            vm.stageGizmoActive = true
        }
        if (vm.copiedKeyframe != null) {
            MenuActionRow("Paste Keyframe Here", color = KeyframeGold, badge = "PST", badgeColor = KeyframeGold) {
                vm.playheadActionMenuOpen = false
                val targetId = if (vm.stageGizmoMode == StageGizmoSubMode.CAMERA) 0uL else (tl.activeClipId ?: 0uL)
                vm.pasteKeyframeAt(targetId, tl.playhead.toInt())
            }
        }
        MenuActionRow("Add Ruler Marker Here", color = KeyframeGold, badge = "⚑", badgeColor = KeyframeGold) {
            vm.playheadActionMenuOpen = false
            vm.addTimelineMarker(tl.playhead.toInt())
        }
        MenuActionRow("Split Clip Here", badge = "CUT", badgeColor = PerformRed) {
            vm.playheadActionMenuOpen = false
            vm.splitActiveClipAtPlayhead()
        }
        MenuActionRow("Open in Flipbook", badge = "FLIP", badgeColor = DreamsCyan) {
            vm.playheadActionMenuOpen = false
            vm.setFlipbookMode(true)
        }
    }
}

/**
 * DreamsCraft v2.0 Pilar 3: **Complete Track Options & GPU Layer Effects Popover**:
 * - Tab 1 (**Options**): Rename, Color Swatches, Blend Mode, Opacity, Clip to Track Below,
 *   Alpha Lock, Mute Audio, Lock, Move Up/Down, Duplicate, Merge Down, Ungroup, Clear Clips, Delete Track.
 * - Tab 2 (**Layer Effects**): Non-destructive GPU Outline/Stroke, Drop Shadow, Outer/Inner Glow,
 *   and Color Overlay — complete with studio color swatch pickers!
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
    val popShape = RoundedCornerShape(22.dp)

    Column(
        modifier = Modifier
            .align(Alignment.BottomStart)
            .padding(start = 12.dp, bottom = bottomOffset)
            .width(348.dp)
            .heightIn(max = 440.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, if (tab == 1) FilterPurple.copy(alpha = 0.72f) else DreamsGlassEdge, popShape)
            .pointerInput(Unit) { detectTapGestures { } }
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
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background(Pc.Accent.copy(alpha = 0.16f))
                    .clickable { vm.trackOptionsTrackId = null }
                    .padding(horizontal = 10.dp, vertical = 4.dp),
            ) {
                Text("Done", color = Pc.Accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            }
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
                        .clip(RoundedCornerShape(10.dp))
                        .background(DreamsPillBg)
                        .border(1.dp, DreamsEdge, RoundedCornerShape(10.dp))
                        .padding(horizontal = 10.dp, vertical = 7.dp),
                )
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .background(Pc.Accent)
                        .clickable { vm.renameTimelineTrack(track.id, editName) }
                        .padding(horizontal = 12.dp, vertical = 7.dp),
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
                Text("Color", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
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
            Text("Blend Mode", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
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
                        fontWeight = if (chosen) FontWeight.Bold else FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(9.dp))
                            .background(if (chosen) Pc.Accent else DreamsPillBg)
                            .clickable { vm.setTimelineTrackBlend(track.id, blendId) }
                            .padding(horizontal = 10.dp, vertical = 5.dp),
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

            if (track.isGroup) {
                MenuActionRow("Ungroup Track", color = DreamsCyan, badge = "UNGRP", badgeColor = DreamsCyan) {
                    vm.ungroupTimelineTrack(track.id)
                }
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
            // GPU Non-Destructive Track Layer Effects (with Studio Color Swatch Pickers)
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
                EffectColorRow("Outline Color", fx.outlineColor) { rgba ->
                    val a = fx.outlineColor.getOrNull(3) ?: 1f
                    fx = fx.copy(outlineColor = listOf(rgba[0], rgba[1], rgba[2], a))
                    vm.setTimelineTrackEffects(track.id, fx, done = true)
                }
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
                CompactSliderRow(
                    label = "Shadow Blur",
                    valueText = "${fx.shadowBlur.roundToInt()}px",
                    value = fx.shadowBlur,
                    range = 0f..24f,
                    onValueChange = {
                        fx = fx.copy(shadowBlur = it)
                        vm.setTimelineTrackEffects(track.id, fx, done = false)
                    },
                    onValueChangeFinished = { vm.setTimelineTrackEffects(track.id, fx, done = true) },
                )
                EffectColorRow("Shadow Color", fx.shadowColor) { rgba ->
                    val a = fx.shadowColor.getOrNull(3) ?: 0.55f
                    fx = fx.copy(shadowColor = listOf(rgba[0], rgba[1], rgba[2], a))
                    vm.setTimelineTrackEffects(track.id, fx, done = true)
                }
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
                EffectColorRow("Glow Color", fx.glowColor) { rgba ->
                    val a = fx.glowColor.getOrNull(3) ?: 1f
                    fx = fx.copy(glowColor = listOf(rgba[0], rgba[1], rgba[2], a))
                    vm.setTimelineTrackEffects(track.id, fx, done = true)
                }
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
                EffectColorRow("Overlay Color", fx.colorOverlayColor) { rgba ->
                    val a = fx.colorOverlayColor.getOrNull(3) ?: 0.5f
                    fx = fx.copy(colorOverlayColor = listOf(rgba[0], rgba[1], rgba[2], a))
                    vm.setTimelineTrackEffects(track.id, fx, done = true)
                }
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
    val isPerform = vm.dreamsMode == DreamsWorkMode.PERFORM
    val baseFilter = if (isCam) tl.cameraFilter else tl.activeEvaluatedFilter
    var filter by remember(activeClipId, isCam) { mutableStateOf(baseFilter) }
    var opacity by remember(activeClipId) { mutableFloatStateOf(tl.activeEvaluatedOpacity) }

    var sliderTakeStarted by remember(activeClipId, isPerform) { mutableStateOf(false) }

    LaunchedEffect(activeClipId, isCam, tl.playhead, baseFilter, sliderTakeStarted, vm.performRecording) {
        if (!sliderTakeStarted && !vm.performRecording) {
            filter = baseFilter
        }
    }
    LaunchedEffect(activeClipId, tl.playhead, tl.activeEvaluatedOpacity, sliderTakeStarted, vm.performRecording) {
        if (!sliderTakeStarted && !vm.performRecording) {
            opacity = tl.activeEvaluatedOpacity
        }
    }
    androidx.compose.runtime.DisposableEffect(activeClipId, isPerform) {
        onDispose {
            if (sliderTakeStarted && vm.performRecording) {
                vm.finishPerformRecording()
            }
        }
    }

    fun pushOpacity(next: Float, done: Boolean) {
        opacity = next
        if (activeClipId == null) return
        if (isPerform) {
            if (!done) {
                if (!sliderTakeStarted) {
                    sliderTakeStarted = true
                    vm.beginPerformOpacity(next)
                }
                vm.recordPerformOpacity(next)
            } else {
                sliderTakeStarted = false
                if (vm.performRecording) vm.finishPerformRecording()
            }
        } else {
            vm.setClipOpacityKeyframe(activeClipId, tl.playhead.toInt(), next, done)
        }
    }

    fun pushFilter(next: TimelineLiveFilterInfo, done: Boolean) {
        filter = next
        if (isCam) {
            vm.setCameraFilterKeyframe(tl.playhead.toInt(), next, done)
        } else if (activeClipId != null) {
            if (isPerform) {
                if (!done) {
                    if (!sliderTakeStarted) {
                        sliderTakeStarted = true
                        vm.beginPerformFilter(next)
                    }
                    vm.recordPerformFilter(next)
                } else {
                    sliderTakeStarted = false
                    if (vm.performRecording) vm.finishPerformRecording()
                }
            } else {
                vm.setClipFilterKeyframe(activeClipId, tl.playhead.toInt(), next, done)
            }
        }
    }

    val popShape = RoundedCornerShape(22.dp)
    Column(
        modifier = Modifier
            .align(Alignment.BottomEnd)
            .padding(end = 16.dp, bottom = bottomOffset)
            .width(326.dp)
            .heightIn(max = 440.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, if (isPerform) PerformRed.copy(alpha = 0.85f) else FilterPurple.copy(alpha = 0.72f), popShape)
            .pointerInput(Unit) { detectTapGestures { } }
            .verticalScroll(rememberScrollState())
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    text = if (isCam) "Camera Live FX · F${tl.playhead + 1u}" else "Live Filters · F${tl.playhead + 1u}",
                    color = if (isPerform) PerformRed else FilterPurple,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Bold,
                )
                if (isPerform && !isCam) {
                    TrackMicroBadge("REC", PerformRed)
                }
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsPillBg)
                        .clickable {
                            pushFilter(TimelineLiveFilterInfo(0f, 0f, 1f, 0f, 0f, 0f), done = true)
                        }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                ) {
                    Text("Reset", color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                }
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .background(Pc.Accent.copy(alpha = 0.16f))
                        .clickable { vm.liveFilterPopoverOpen = false }
                        .padding(horizontal = 10.dp, vertical = 4.dp),
                ) {
                    Text("Done", color = Pc.Accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                }
            }
        }

        if (!isCam && activeClipId != null) {
            CompactSliderRow(
                label = "Opacity",
                valueText = "${(opacity * 100).roundToInt()}%",
                value = opacity,
                range = 0f..1f,
                onValueChange = { pushOpacity(it, done = false) },
                onValueChangeFinished = { pushOpacity(opacity, done = true) },
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
    val isCamera = clipId == 0uL
    val clip = remember(tl, clipId) {
        if (isCamera) null else tl.tracks.asSequence().flatMap { it.clips.asSequence() }.firstOrNull { it.id == clipId }
    }
    val fU = frame.coerceAtLeast(0).toUInt()
    val existingEasing = remember(tl, clipId, frame) {
        if (isCamera) {
            tl.cameraTransformKeyframes.firstOrNull { it.frame == fU }?.easing
                ?: tl.cameraFilterKeyframes.firstOrNull { it.frame == fU }?.easing
        } else {
            clip?.transformKeyframes?.firstOrNull { it.frame == fU }?.easing
                ?: clip?.opacityKeyframes?.firstOrNull { it.frame == fU }?.easing
                ?: clip?.filterKeyframes?.firstOrNull { it.frame == fU }?.easing
                ?: clip?.deformKeyframes?.firstOrNull { it.frame == fU }?.easing
        } ?: TimelineEasingInfo(TimelineEasingKind.EASE_IN_OUT, 0.42f, 0f, 0.58f, 1f)
    }
    var easing by remember(clipId, frame, existingEasing) { mutableStateOf(existingEasing) }
    var customOpen by remember(clipId, frame) { mutableStateOf(existingEasing.kind == TimelineEasingKind.CUBIC_BEZIER) }
    val hasDeformHere = clip?.deformKeyframes?.any { it.frame == fU } == true

    val popShape = RoundedCornerShape(22.dp)
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(bottom = bottomOffset)
            .width(316.dp)
            .heightIn(max = 460.dp)
            .shadow(14.dp, popShape)
            .clip(popShape)
            .background(DreamsCardFill)
            .border(1.dp, (if (isCamera) CameraSky else KeyframeGold).copy(alpha = 0.72f), popShape)
            .pointerInput(Unit) { detectTapGestures { } }
            .verticalScroll(rememberScrollState())
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (isCamera) "Camera Key · F${frame + 1}" else "Keyframe · F${frame + 1}",
                color = if (isCamera) CameraSky else KeyframeGold,
                fontSize = 14.sp,
                fontWeight = FontWeight.Bold,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsPillBg)
                        .clickable { vm.copyKeyframeAt(clipId, frame) }
                        .padding(horizontal = 7.dp, vertical = 3.dp),
                ) {
                    Text("Copy", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                }
                if (vm.copiedKeyframe != null) {
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(KeyframeGold.copy(alpha = 0.22f))
                            .clickable {
                                vm.pasteKeyframeAt(clipId, frame)
                                vm.editingKeyframe = null
                            }
                            .padding(horizontal = 7.dp, vertical = 3.dp),
                    ) {
                        Text("Paste", color = KeyframeGold, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    }
                }
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsPillBg)
                        .clickable { vm.editingKeyframe = null }
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                ) {
                    Text("Done", color = Pc.Accent, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
        }

        // Visual Live Easing Curve Graph
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(92.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(DreamsTrackBg)
                .border(0.75.dp, DreamsEdge, RoundedCornerShape(12.dp))
                .padding(10.dp),
        ) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val padX = 6f
                val padY = 8f
                val w = (size.width - padX * 2f).coerceAtLeast(1f)
                val h = (size.height - padY * 2f).coerceAtLeast(1f)
                fun gx(u: Float) = padX + u * w
                fun gy(v: Float) = padY + (1f - v) * h
                // Subtle grid lines
                drawLine(Color(0x28FFFFFF), Offset(gx(0f), gy(0f)), Offset(gx(1f), gy(0f)), 1f)
                drawLine(Color(0x28FFFFFF), Offset(gx(0f), gy(1f)), Offset(gx(1f), gy(1f)), 1f)
                drawLine(Color(0x1EFFFFFF), Offset(gx(0.5f), gy(0f)), Offset(gx(0.5f), gy(1f)), 1f)
                drawLine(Color(0x1EFFFFFF), Offset(gx(0f), gy(0.5f)), Offset(gx(1f), gy(0.5f)), 1f)
                // Draw Bezier tangent handles when curve-based
                val (cx1, cy1, cx2, cy2) = when (easing.kind) {
                    TimelineEasingKind.EASE_IN -> listOf(0.42f, 0f, 1f, 1f)
                    TimelineEasingKind.EASE_OUT -> listOf(0f, 0f, 0.58f, 1f)
                    TimelineEasingKind.EASE_IN_OUT -> listOf(0.42f, 0f, 0.58f, 1f)
                    TimelineEasingKind.CUBIC_BEZIER -> listOf(easing.x1, easing.y1, easing.x2, easing.y2)
                    else -> listOf(0f, 0f, 1f, 1f)
                }
                if (easing.kind != TimelineEasingKind.HOLD && easing.kind != TimelineEasingKind.LINEAR) {
                    val h1 = Offset(gx(cx1), gy(cy1).coerceIn(0f, size.height))
                    val h2 = Offset(gx(cx2), gy(cy2).coerceIn(0f, size.height))
                    drawLine(OpacityCyan.copy(alpha = 0.55f), Offset(gx(0f), gy(0f)), h1, 1.5f)
                    drawLine(OpacityCyan.copy(alpha = 0.55f), Offset(gx(1f), gy(1f)), h2, 1.5f)
                    drawCircle(OpacityCyan, 4.5f, h1)
                    drawCircle(OpacityCyan, 4.5f, h2)
                }
                // Plot actual evaluated curve
                val curve = Path()
                val steps = 36
                for (s in 0..steps) {
                    val u = s.toFloat() / steps.toFloat()
                    val v = evalEasingInfo(easing, u)
                    val pt = Offset(gx(u), gy(v).coerceIn(0f, size.height))
                    if (s == 0) curve.moveTo(pt.x, pt.y) else curve.lineTo(pt.x, pt.y)
                }
                drawPath(curve, color = if (isCamera) CameraSky else KeyframeGold, style = Stroke(width = 3f, cap = StrokeCap.Round))
                drawCircle(Color.White, 4f, Offset(gx(0f), gy(0f)))
                drawCircle(Color.White, 4f, Offset(gx(1f), gy(1f)))
            }
        }

        // Preset Easing Chips
        val presets = listOf(
            "Smooth" to TimelineEasingInfo(TimelineEasingKind.EASE_IN_OUT, 0.42f, 0f, 0.58f, 1f),
            "Linear" to TimelineEasingInfo(TimelineEasingKind.LINEAR, 0f, 0f, 1f, 1f),
            "Ease In" to TimelineEasingInfo(TimelineEasingKind.EASE_IN, 0.42f, 0f, 1f, 1f),
            "Ease Out" to TimelineEasingInfo(TimelineEasingKind.EASE_OUT, 0f, 0f, 0.58f, 1f),
            "Hold" to TimelineEasingInfo(TimelineEasingKind.HOLD, 0f, 0f, 1f, 1f),
            "Bounce" to TimelineEasingInfo(TimelineEasingKind.CUBIC_BEZIER, 0.34f, 1.45f, 0.64f, 1f),
        )
        Row(
            modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            for ((label, presetInfo) in presets) {
                val chosen = easing.kind == presetInfo.kind && (presetInfo.kind != TimelineEasingKind.CUBIC_BEZIER || abs(easing.y1 - presetInfo.y1) < 0.05f)
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(9.dp))
                        .background(if (chosen) KeyframeGold else DreamsPillBg)
                        .clickable {
                            easing = presetInfo
                            customOpen = presetInfo.kind == TimelineEasingKind.CUBIC_BEZIER
                            vm.applyKeyframeEasing(clipId, frame, presetInfo)
                        }
                        .padding(horizontal = 9.dp, vertical = 5.dp),
                ) {
                    Text(
                        label,
                        color = if (chosen) Color.Black else DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = if (chosen) FontWeight.Bold else FontWeight.Medium,
                    )
                }
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(9.dp))
                    .background(if (customOpen) OpacityCyan else DreamsPillBg)
                    .clickable {
                        customOpen = !customOpen
                        if (customOpen && easing.kind != TimelineEasingKind.CUBIC_BEZIER) {
                            easing = TimelineEasingInfo(TimelineEasingKind.CUBIC_BEZIER, 0.42f, 0f, 0.58f, 1f)
                        }
                    }
                    .padding(horizontal = 9.dp, vertical = 5.dp),
            ) {
                Text("Custom", color = if (customOpen) Color.Black else DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
            }
        }

        // Custom Cubic Bezier Handle Sliders
        if (customOpen) {
            CompactSliderRow("Handle 1 X", "%.2f".format(easing.x1), easing.x1, 0f..1f,
                onValueChange = { easing = easing.copy(kind = TimelineEasingKind.CUBIC_BEZIER, x1 = it) },
                onValueChangeFinished = { vm.applyKeyframeEasing(clipId, frame, easing) },
            )
            CompactSliderRow("Handle 1 Y", "%.2f".format(easing.y1), easing.y1, -0.5f..1.5f,
                onValueChange = { easing = easing.copy(kind = TimelineEasingKind.CUBIC_BEZIER, y1 = it) },
                onValueChangeFinished = { vm.applyKeyframeEasing(clipId, frame, easing) },
            )
            CompactSliderRow("Handle 2 X", "%.2f".format(easing.x2), easing.x2, 0f..1f,
                onValueChange = { easing = easing.copy(kind = TimelineEasingKind.CUBIC_BEZIER, x2 = it) },
                onValueChangeFinished = { vm.applyKeyframeEasing(clipId, frame, easing) },
            )
            CompactSliderRow("Handle 2 Y", "%.2f".format(easing.y2), easing.y2, -0.5f..1.5f,
                onValueChange = { easing = easing.copy(kind = TimelineEasingKind.CUBIC_BEZIER, y2 = it) },
                onValueChangeFinished = { vm.applyKeyframeEasing(clipId, frame, easing) },
            )
        }

        // Direct jump to Stage Gizmo for Warp / Camera keyframes
        if (isCamera) {
            MenuActionRow("Edit Camera on Stage", color = CameraSky, badge = "CAM", badgeColor = CameraSky) {
                vm.editingKeyframe = null
                vm.stageGizmoMode = StageGizmoSubMode.CAMERA
                vm.stageGizmoActive = true
            }
        } else if (hasDeformHere) {
            MenuActionRow("Edit Warp / Distort on Stage", color = WarpPink, badge = "MESH", badgeColor = WarpPink) {
                vm.editingKeyframe = null
                vm.stageGizmoMode = StageGizmoSubMode.WARP
                vm.stageGizmoActive = true
            }
        }

        MenuActionRow("Delete Keyframe", color = PerformRed, badge = "DEL", badgeColor = PerformRed) {
            if (isCamera) {
                vm.deleteCameraKeyframe(frame)
            } else {
                vm.deleteClipTransformKeyframe(clipId, frame)
                vm.deleteClipOpacityKeyframe(clipId, frame)
                vm.deleteClipFilterKeyframe(clipId, frame)
                vm.deleteClipDeformKeyframe(clipId, frame)
                vm.deleteAudioVolumeKeyframe(clipId, frame)
                vm.deleteAudioPanKeyframe(clipId, frame)
            }
            vm.editingKeyframe = null
        }
    }
}

@Composable
private fun ClipContextMenuCard(
    vm: CanvasViewModel,
    tl: TimelineState,
    clip: TimelineClipInfo,
    onDismiss: () -> Unit,
) {
    val nextLoopMode = when (clip.loopMode) {
        TimelineClipLoopMode.NONE -> TimelineClipLoopMode.LOOP
        TimelineClipLoopMode.LOOP -> TimelineClipLoopMode.PING_PONG
        TimelineClipLoopMode.PING_PONG -> TimelineClipLoopMode.NONE
    }
    val loopLabel = when (clip.loopMode) {
        TimelineClipLoopMode.NONE -> "Once"
        TimelineClipLoopMode.LOOP -> "Loop ⟳"
        TimelineClipLoopMode.PING_PONG -> "Ping-Pong ⇄"
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(DreamsCardFill)
            .border(0.75.dp, DreamsGlassEdge),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 10.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = clip.name,
                color = Pc.Accent,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (clip.kind == TimelineClipKind.CEL) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(DreamsPillBg)
                        .clickable {
                            onDismiss()
                            vm.setFlipbookMode(true)
                        }
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                ) {
                    Text("Flipbook", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                }
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(DreamsPillBg)
                    .clickable {
                        vm.fillTimelineClipDuration(clip.id)
                        onDismiss()
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            ) {
                Text("Fill Duration", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
            }
            if (clip.kind == TimelineClipKind.CEL) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (clip.loopMode != TimelineClipLoopMode.NONE) KeyframeGold.copy(alpha = 0.22f) else DreamsPillBg)
                        .clickable { vm.setClipLoopMode(clip.id, nextLoopMode) }
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                ) {
                    Text(
                        loopLabel,
                        color = if (clip.loopMode != TimelineClipLoopMode.NONE) KeyframeGold else DreamsInk,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(DreamsPillBg)
                    .clickable {
                        vm.reverseTimelineClip(clip.id)
                        onDismiss()
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            ) {
                Text("Reverse", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.Medium)
            }
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (clip.performOnionGuide) OpacityCyan.copy(alpha = 0.22f) else DreamsPillBg)
                    .clickable {
                        vm.setPerformOnionGuide(
                            clipId = clip.id,
                            enabled = !clip.performOnionGuide,
                            isolateFirstFrame = true,
                            opacity = clip.performGuideOpacity.coerceIn(0.2f, 0.85f),
                            colorRgb = clip.performGuideColor.ifEmpty { listOf(0.15f, 0.85f, 1.0f) },
                        )
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                DreamsGhostIcon(if (clip.performOnionGuide) OpacityCyan else DreamsDimInk, 11.dp)
                Text(
                    text = if (clip.performOnionGuide) "Ghost ON" else "Ghost",
                    color = if (clip.performOnionGuide) OpacityCyan else DreamsInk,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(DreamsPillBg)
                    .clickable {
                        onDismiss()
                        vm.splitActiveClipAtPlayhead(clip.id)
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            ) {
                Text("Split", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.Medium)
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(DreamsPillBg)
                    .clickable {
                        onDismiss()
                        vm.duplicateTimelineClip(clip.id)
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            ) {
                Text("Duplicate", color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.Medium)
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(PerformRed.copy(alpha = 0.16f))
                    .clickable {
                        onDismiss()
                        vm.deleteTimelineClip(clip.id)
                    }
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            ) {
                Text("Delete", color = PerformRed, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
            }
            Box(
                modifier = Modifier
                    .size(22.dp)
                    .clip(CircleShape)
                    .background(DreamsPillBg)
                    .clickable(onClick = onDismiss),
                contentAlignment = Alignment.Center,
            ) {
                Text("✕", color = DreamsDimInk, fontSize = 10.sp, fontWeight = FontWeight.Bold)
            }
        }

        // Audio Mixer row (Volume, Stereo Pan & 1-tap Fade In / Fade Out) on Audio clips
        if (clip.kind == TimelineClipKind.AUDIO) {
            val ph = tl.playhead.toInt()
            val curVolInit = clip.volumeKeyframes.lastOrNull { it.frame <= tl.playhead }?.scalar ?: 1f
            val curPanInit = clip.panKeyframes.lastOrNull { it.frame <= tl.playhead }?.scalar ?: 0f
            var vol by remember(clip.id, tl.playhead, curVolInit) { mutableFloatStateOf(curVolInit) }
            var pan by remember(clip.id, tl.playhead, curPanInit) { mutableFloatStateOf(curPanInit) }
            val canFade = clip.durationFrames > 1u
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(DreamsBarFill)
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Box(Modifier.weight(1f)) {
                    CompactSliderRow(
                        label = "Vol · F${ph + 1}",
                        valueText = "${(vol * 100f).roundToInt()}%",
                        value = vol,
                        range = 0f..2f,
                        onValueChange = {
                            vol = it
                            vm.setAudioVolumeKeyframe(clip.id, ph, it, done = false)
                        },
                        onValueChangeFinished = { vm.setAudioVolumeKeyframe(clip.id, ph, vol, done = true) },
                    )
                }
                Box(Modifier.weight(1f)) {
                    val panText = when {
                        pan < -0.05f -> "L${(-pan * 100f).roundToInt()}"
                        pan > 0.05f -> "R${(pan * 100f).roundToInt()}"
                        else -> "C"
                    }
                    CompactSliderRow(
                        label = "Pan",
                        valueText = panText,
                        value = pan,
                        range = -1f..1f,
                        onValueChange = {
                            pan = it
                            vm.setAudioPanKeyframe(clip.id, ph, it, done = false)
                        },
                        onValueChangeFinished = { vm.setAudioPanKeyframe(clip.id, ph, pan, done = true) },
                    )
                }
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(AudioGreen.copy(alpha = if (canFade) 0.20f else 0.08f))
                        .clickable(enabled = canFade) {
                            val s0 = clip.startFrame.toInt()
                            val maxFade = (clip.durationFrames.toInt() - 1).coerceAtLeast(1)
                            val fadeLen = tl.fps.toInt().coerceAtLeast(6).coerceAtMost(maxFade)
                            vm.setAudioVolumeKeyframe(clip.id, s0, 0f, done = false)
                            vm.setAudioVolumeKeyframe(clip.id, s0 + fadeLen, 1f, done = true)
                        }
                        .padding(horizontal = 8.dp, vertical = 5.dp),
                ) {
                    Text("Fade In", color = if (canFade) AudioGreen else DreamsDimInk, fontSize = 10.5.sp, fontWeight = FontWeight.Bold)
                }
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(AudioGreen.copy(alpha = if (canFade) 0.20f else 0.08f))
                        .clickable(enabled = canFade) {
                            val s0 = clip.startFrame.toInt()
                            val endF = (clip.startFrame + clip.durationFrames).toInt() - 1
                            val maxFade = (clip.durationFrames.toInt() - 1).coerceAtLeast(1)
                            val fadeLen = tl.fps.toInt().coerceAtLeast(6).coerceAtMost(maxFade)
                            vm.setAudioVolumeKeyframe(clip.id, (endF - fadeLen).coerceAtLeast(s0), 1f, done = false)
                            vm.setAudioVolumeKeyframe(clip.id, endF.coerceAtLeast(s0), 0f, done = true)
                        }
                        .padding(horizontal = 8.dp, vertical = 5.dp),
                ) {
                    Text("Fade Out", color = if (canFade) AudioGreen else DreamsDimInk, fontSize = 10.5.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
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
            .clip(RoundedCornerShape(10.dp))
            .background(DreamsPillBg)
            .clickable(onClick = onToggle)
            .padding(horizontal = 11.dp, vertical = 7.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            color = DreamsInk,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f).padding(end = 8.dp),
        )
        DreamsPillSwitch(checked = checked)
    }
}

@Composable
private fun DreamsPillSwitch(checked: Boolean) {
    val thumbOffset by animateDpAsState(
        targetValue = if (checked) 16.dp else 2.dp,
        label = "dreamsPillSwitch",
    )
    Box(
        modifier = Modifier
            .width(34.dp)
            .height(20.dp)
            .clip(CircleShape)
            .background(if (checked) Pc.Accent else DreamsEdge),
    ) {
        Box(
            modifier = Modifier
                .offset(x = thumbOffset, y = 2.dp)
                .size(16.dp)
                .shadow(2.dp, CircleShape)
                .clip(CircleShape)
                .background(Color.White),
        )
    }
}

@Composable
private fun EffectColorRow(
    label: String,
    currentRgba: List<Float>,
    onPick: (List<Float>) -> Unit,
) {
    val curR = currentRgba.getOrNull(0) ?: 1f
    val curG = currentRgba.getOrNull(1) ?: 1f
    val curB = currentRgba.getOrNull(2) ?: 1f
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(DreamsPillBg)
            .padding(horizontal = 10.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = DreamsDimInk, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            for (preset in EffectColorPresets) {
                val c = Color(preset[0], preset[1], preset[2], 1f)
                val chosen = abs(curR - preset[0]) < 0.08f && abs(curG - preset[1]) < 0.08f && abs(curB - preset[2]) < 0.08f
                Box(
                    modifier = Modifier
                        .size(20.dp)
                        .clip(CircleShape)
                        .background(c)
                        .border(if (chosen) 2.dp else 1.dp, if (chosen) Pc.Accent else DreamsEdge, CircleShape)
                        .clickable { onPick(preset) },
                )
            }
        }
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
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(label, color = DreamsInk, fontSize = 11.sp, fontWeight = FontWeight.Medium)
            Text(
                text = valueText,
                color = DreamsDimInk,
                fontSize = 11.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = FontFamily.Monospace,
            )
        }
        Slider(
            value = value.coerceIn(range.start, range.endInclusive),
            onValueChange = onValueChange,
            onValueChangeFinished = onValueChangeFinished,
            valueRange = range,
            colors = SliderDefaults.colors(
                thumbColor = Pc.Accent,
                activeTrackColor = Pc.Accent,
                inactiveTrackColor = DreamsEdge.copy(alpha = 0.7f),
            ),
            modifier = Modifier.height(26.dp),
        )
    }
}

@Composable
private fun MenuActionRow(
    label: String,
    color: Color = DreamsInk,
    badge: String? = null,
    badgeColor: Color = Pc.Accent,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(DreamsPillBg)
            .clickable(onClick = onClick)
            .padding(horizontal = 11.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (badge != null) {
            TrackMicroBadge(badge, badgeColor)
        }
        Text(
            text = label,
            color = color,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
