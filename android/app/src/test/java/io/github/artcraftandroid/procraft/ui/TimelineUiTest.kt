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
        assertEquals(0.38f, collapsed.ratio, 0.001f)
    }
}
