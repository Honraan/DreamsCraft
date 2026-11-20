package io.github.artcraftandroid.procraft.canvas

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.net.Uri
import io.github.artcraftandroid.procraft.AppLog
import java.nio.ByteBuffer

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

    /** Extracts the RGBA frame nearest to [timeUs] microseconds, scaled to fit [maxWidth] × [maxHeight]. */
    fun decodeFrameAt(
        context: Context,
        uri: Uri,
        timeUs: Long,
        maxWidth: Int,
        maxHeight: Int,
    ): DecodedVideoFrame? {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(context, uri)
            val raw = retriever.getFrameAtTime(timeUs.coerceAtLeast(0L), MediaMetadataRetriever.OPTION_CLOSEST)
                ?: return null
            val scaled = if (raw.width > maxWidth || raw.height > maxHeight) {
                val k = minOf(maxWidth.toFloat() / raw.width, maxHeight.toFloat() / raw.height)
                val tw = (raw.width * k).toInt().coerceAtLeast(1)
                val th = (raw.height * k).toInt().coerceAtLeast(1)
                Bitmap.createScaledBitmap(raw, tw, th, true).also {
                    if (it !== raw) raw.recycle()
                }
            } else {
                raw
            }
            val argb = if (scaled.config == Bitmap.Config.ARGB_8888) {
                scaled
            } else {
                scaled.copy(Bitmap.Config.ARGB_8888, false).also { scaled.recycle() }
            }
            val buf = ByteBuffer.allocate(argb.byteCount)
            argb.copyPixelsToBuffer(buf)
            val out = DecodedVideoFrame(width = argb.width, height = argb.height, rgba = buf.array())
            argb.recycle()
            out
        } catch (e: Exception) {
            AppLog.w("DreamsCraft", "video frame decode failed", e)
            null
        } finally {
            runCatching { retriever.release() }
        }
    }
}
