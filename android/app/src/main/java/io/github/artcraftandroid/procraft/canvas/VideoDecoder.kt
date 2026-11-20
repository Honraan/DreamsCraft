package io.github.artcraftandroid.procraft.canvas

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.net.Uri
import io.github.artcraftandroid.procraft.AppLog
import java.io.Closeable
import java.io.File

/** Metadata probed from an imported video file before creating a `VideoClip` on the timeline. */
data class VideoProbe(
    val width: Int,
    val height: Int,
    val durationMs: Long,
    val fps: Float,
    val frameCount: Int,
)

/** Decoded RGBA frame ready to be uploaded to a managed DreamsCraft `VideoClip` layer. */
data class DecodedVideoFrame(
    val width: Int,
    val height: Int,
    val rgba: ByteArray,
)

/**
 * Lightweight on-demand video frame decoder for DreamsCraft `VideoClip` rotoscoping and compositing.
 */
object VideoDecoder {
    /** Probes basic video geometry, duration, and frame rate from [uri]. */
    fun probe(context: Context, uri: Uri): VideoProbe? {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(context, uri)
            val w = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
            val h = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
            val durMs = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L
            val countMeta = if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.P) {
                retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_FRAME_COUNT)?.toIntOrNull()
            } else {
                null
            }
            val fps = if (countMeta != null && countMeta > 0 && durMs > 0L) {
                (countMeta * 1000f / durMs).coerceIn(1f, 120f)
            } else {
                24f
            }
            val frameCount = countMeta ?: ((durMs / 1000f) * fps).toInt().coerceAtLeast(1)
            if (w <= 0 || h <= 0) return null
            VideoProbe(width = w, height = h, durationMs = durMs, fps = fps, frameCount = frameCount)
        } catch (e: Exception) {
            AppLog.w("DreamsCraft", "video probe failed", e)
            null
        } finally {
            runCatching { retriever.release() }
        }
    }

    /** One retriever per source for sequential export or playback requests. */
    class Reader(file: File, private val maxWidth: Int, private val maxHeight: Int) : Closeable {
        private val retriever = MediaMetadataRetriever()
        private val geometry: Pair<Int, Int>

        init {
            try {
                retriever.setDataSource(file.path)
                val width = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
                val height = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
                val rotation = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)?.toIntOrNull() ?: 0
                require(width > 0 && height > 0)
                geometry = if (rotation % 180 == 0) width to height else height to width
            } catch (e: Exception) {
                runCatching { retriever.release() }
                throw e
            }
        }

        fun frame(timeUs: Long): DecodedVideoFrame? {
            val (width, height) = geometry
            val scale = minOf(1f, maxWidth.toFloat() / width, maxHeight.toFloat() / height)
            val w = (width * scale).toInt().coerceAtLeast(1)
            val h = (height * scale).toInt().coerceAtLeast(1)
            val raw = if (android.os.Build.VERSION.SDK_INT >= 27 && scale < 1f) {
                retriever.getScaledFrameAtTime(timeUs, MediaMetadataRetriever.OPTION_CLOSEST, w, h)
            } else {
                retriever.getFrameAtTime(timeUs, MediaMetadataRetriever.OPTION_CLOSEST)
            } ?: return null
            try {
                val factor = minOf(1f, maxWidth.toFloat() / raw.width, maxHeight.toFloat() / raw.height)
                val fitted = if (factor < 1f) Bitmap.createScaledBitmap(
                    raw, (raw.width * factor).toInt().coerceAtLeast(1),
                    (raw.height * factor).toInt().coerceAtLeast(1), true,
                ) else raw
                try {
                    val argb = IntArray(fitted.width * fitted.height)
                    fitted.getPixels(argb, 0, fitted.width, 0, 0, fitted.width, fitted.height)
                    val rgba = ByteArray(argb.size * 4)
                    for (i in argb.indices) {
                        val pixel = argb[i]
                        val a = pixel ushr 24
                        rgba[i * 4] = (((pixel ushr 16 and 255) * a + 127) / 255).toByte()
                        rgba[i * 4 + 1] = (((pixel ushr 8 and 255) * a + 127) / 255).toByte()
                        rgba[i * 4 + 2] = (((pixel and 255) * a + 127) / 255).toByte()
                        rgba[i * 4 + 3] = a.toByte()
                    }
                    return DecodedVideoFrame(fitted.width, fitted.height, rgba)
                } finally {
                    if (fitted !== raw) fitted.recycle()
                }
            } finally {
                raw.recycle()
            }
        }

        override fun close() { runCatching { retriever.release() } }
    }
}

/** The same source, timestamp and pixel format for Stage playback and animated export. */
internal class ProjectVideoFrames(
    private val project: File,
    private val width: Int,
    private val height: Int,
) : io.github.artcraftandroid.procraft.core.TimelineMediaProvider, Closeable {
    private val mediaFilename = Regex("video-[a-fA-F0-9]{32}\\.media")
    private val readers = LinkedHashMap<String, VideoDecoder.Reader>(4, 0.75f, true)

    @Synchronized
    override fun videoFrame(
        request: io.github.artcraftandroid.procraft.core.TimelineVideoRequest,
    ): io.github.artcraftandroid.procraft.core.TimelineVideoPixels? = runCatching {
        val filename = request.file
        if (!mediaFilename.matches(filename)) return null
        val reader = readers[filename] ?: run {
            if (readers.size >= 4) {
                val eldest = readers.entries.iterator()
                eldest.next().value.close()
                eldest.remove()
            }
            VideoDecoder.Reader(File(File(project, "media"), filename), width, height).also { readers[filename] = it }
        }
        val decoded = reader.frame(request.timeUs.toLong().coerceAtLeast(0)) ?: return null
        io.github.artcraftandroid.procraft.core.TimelineVideoPixels(decoded.width.toUInt(), decoded.height.toUInt(), decoded.rgba)
    }.onFailure { AppLog.w("DreamsCraft", "video frame decode failed", it) }.getOrNull()

    @Synchronized
    override fun close() {
        readers.values.forEach { it.close() }
        readers.clear()
    }
}
