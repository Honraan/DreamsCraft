package io.github.artcraftandroid.procraft.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TimelineUiTest {
    @Test
    fun formatTimecodeFormatsMinutesSecondsAndFrames() {
        assertEquals("00:00:00", formatTimecode(0, 12))
        assertEquals("00:00:05", formatTimecode(5, 12))
        assertEquals("00:01:00", formatTimecode(12, 12))
        assertEquals("00:02:06", formatTimecode(54, 24))
        assertEquals("01:00:00", formatTimecode(30 * 60, 30))
        // Clamps negative frames or out-of-range FPS gracefully
        assertEquals("00:00:00", formatTimecode(-4, 0))
    }

    @Test
    fun frameToPixelAndPixelToFrameRoundTrip() {
        val zoom = 24f
        val scroll = 48f
        val x = frameToPixel(10f, zoom, scroll)
        assertEquals(192f, x, 0.01f)
        assertEquals(10, pixelToFrame(x + 2f, zoom, scroll, 120))
        assertEquals(0, pixelToFrame(-500f, zoom, scroll, 120))
        assertEquals(119, pixelToFrame(10_000f, zoom, scroll, 120))
    }

    @Test
    fun pinchTimeZoomAndTrackHeightClampToBounds() {
        assertEquals(48f, pinchTimeZoom(24f, 2f), 0.01f)
        assertEquals(MIN_TIME_ZOOM_DP, pinchTimeZoom(10f, 0.25f), 0.01f)
        assertEquals(MAX_TIME_ZOOM_DP, pinchTimeZoom(60f, 3f), 0.01f)
        assertEquals(24f, pinchTimeZoom(24f, Float.NaN), 0.01f)

        assertEquals(81f, pinchTrackHeight(54f, 1.5f), 0.01f)
        assertEquals(MIN_TRACK_HEIGHT_DP, pinchTrackHeight(40f, 0.5f), 0.01f)
        assertEquals(MAX_TRACK_HEIGHT_DP, pinchTrackHeight(90f, 2f), 0.01f)
    }

    @Test
    fun snapClipFrameSnapsToNearestCandidateWithinThreshold() {
        val candidates = listOf(0, 12, 24, 36)
        assertEquals(12, snapClipFrame(11, candidates, snapThresholdFrames = 2))
        assertEquals(12, snapClipFrame(14, candidates, snapThresholdFrames = 2))
        assertEquals(16, snapClipFrame(16, candidates, snapThresholdFrames = 2))
        assertEquals(24, snapClipFrame(23, candidates, snapThresholdFrames = 2))
    }

    @Test
    fun splitViewHandleDragResizesAndCollapsesIntoFlipbookWhenPulledDown() {
        val resized = splitViewHandleDrag(currentRatio = 0.40f, deltaY = -100f, containerHeight = 1000f)
        assertFalse(resized.enterFlipbook)
        assertEquals(0.50f, resized.ratio, 0.001f)

        val clampedTop = splitViewHandleDrag(currentRatio = 0.60f, deltaY = -400f, containerHeight = 1000f)
        assertFalse(clampedTop.enterFlipbook)
        assertEquals(MAX_SPLIT_RATIO, clampedTop.ratio, 0.001f)

        // Pulling the Center Action Bar handle all the way down enters Flipbook Mode automatically
        val collapsed = splitViewHandleDrag(currentRatio = 0.30f, deltaY = 200f, containerHeight = 1000f)
        assertTrue(collapsed.enterFlipbook)
        assertEquals(0.30f, collapsed.ratio, 0.001f)
    }

    @Test
    fun lassoSelectionAndPointInPolygonSelectEnclosedClips() {
        val square = listOf(0f to 0f, 200f to 0f, 200f to 100f, 0f to 100f)
        assertTrue(pointInPolygon(100f, 50f, square))
        assertFalse(pointInPolygon(250f, 50f, square))

        val clips = listOf(
            ClipBounds(1uL, 20f, 10f, 80f, 60f),
            ClipBounds(2uL, 120f, 20f, 180f, 80f),
            ClipBounds(3uL, 300f, 20f, 360f, 80f),
        )
        val selected = clipsInLasso(square, clips)
        assertEquals(setOf(1uL, 2uL), selected)
    }

    @Test
    fun scratchToDeleteDetectsZigZagAndRejectsSimpleLines() {
        val straightLine = listOf(0f to 20f, 30f to 20f, 60f to 20f, 90f to 20f, 120f to 20f, 150f to 20f)
        assertFalse(isScratchToDeleteGesture(straightLine))

        val zigZag = listOf(
            10f to 20f, 60f to 22f,
            15f to 28f, 65f to 30f,
            12f to 36f, 62f to 38f,
            15f to 42f,
        )
        assertTrue(isScratchToDeleteGesture(zigZag))
    }

    @Test
    fun distortCornersExpandBilinearlyIntoFourByFourWarpMesh() {
        val id = defaultIdentityDeformPoints()
        assertEquals(32, id.size)
        assertEquals(0f, id[0], 0.001f)
        assertEquals(0f, id[1], 0.001f)
        assertEquals(1f, id[30], 0.001f)
        assertEquals(1f, id[31], 0.001f)

        val skewed = distortCornersToDeformPoints(
            listOf(0.1f to 0f, 0.9f to 0f, 1f to 1f, 0f to 1f),
        )
        assertEquals(32, skewed.size)
        assertEquals(0.1f, skewed[0], 0.001f)
        assertEquals(0.9f, skewed[6], 0.001f)
        assertEquals(0.5f, (skewed[0] + skewed[6]) / 2f, 0.001f)
    }
}
