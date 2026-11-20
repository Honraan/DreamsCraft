package io.github.artcraftandroid.procraft.canvas

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import io.github.artcraftandroid.procraft.AppLog
import java.util.concurrent.Executors

/**
 * Low-latency stereo PCM audio playback and frame-accurate **Audio Scrubbing** for the DreamsCraft
 * multi-track timeline (uses Android [AudioTrack] on a dedicated background audio executor).
 */
class TimelineAudioPlayer(private val sampleRate: Int = 44_100) {
    private val executor = Executors.newSingleThreadExecutor { r ->
        Thread(r, "DreamsCraftAudio").apply { isDaemon = true }
    }

    @Volatile
    private var playbackTrack: AudioTrack? = null

    @Volatile
    private var scrubTrack: AudioTrack? = null

    private fun buildTrack(bufferBytes: Int): AudioTrack? = runCatching {
        val minBuf = AudioTrack.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_OUT_STEREO,
            AudioFormat.ENCODING_PCM_16BIT,
        ).coerceAtLeast(4096)
        AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                    .build(),
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setSampleRate(sampleRate)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                    .build(),
            )
            .setBufferSizeInBytes(maxOf(minBuf, bufferBytes))
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
    }.onFailure { AppLog.w("DreamsCraft", "AudioTrack init failed", it) }.getOrNull()

    /** Plays a mixed stereo 16-bit PCM clip for continuous timeline playback. */
    fun startPlayback(pcmStereo: ShortArray) {
        if (pcmStereo.isEmpty()) return
        stopPlayback()
        executor.execute {
            val track = buildTrack(pcmStereo.size.coerceAtMost(sampleRate * 2) * 2) ?: return@execute
            playbackTrack = track
            runCatching {
                track.play()
                var offset = 0
                while (offset < pcmStereo.size && playbackTrack === track) {
                    val chunk = minOf(4096, pcmStereo.size - offset)
                    val written = track.write(pcmStereo, offset, chunk, AudioTrack.WRITE_BLOCKING)
                    if (written <= 0) break
                    offset += written
                }
            }
        }
    }

    fun playPcm(pcmStereo: ShortArray) = startPlayback(pcmStereo)

    /** Stops continuous timeline audio playback immediately. */
    fun stopPlayback() {
        val old = playbackTrack
        playbackTrack = null
        if (old != null) {
            runCatching { old.pause() }
            runCatching { old.flush() }
            runCatching { old.stop() }
            runCatching { old.release() }
        }
    }

    fun stop() = stopPlayback()

    companion object {
        const val SAMPLE_RATE: Int = 44_100
    }

    /**
     * Plays a short single-frame slice (`1 / fps` seconds) of stereo 16-bit PCM while the user
     * scrubs the playhead across the ruler (lip-sync Audio Scrubbing).
     */
    fun scrubSlice(pcmStereo: ShortArray) {
        if (pcmStereo.isEmpty()) return
        executor.execute {
            val track = scrubTrack ?: buildTrack(pcmStereo.size.coerceAtLeast(2048) * 2)?.also {
                scrubTrack = it
                runCatching { it.play() }
            } ?: return@execute
            runCatching {
                track.flush()
                track.write(pcmStereo, 0, pcmStereo.size, AudioTrack.WRITE_NON_BLOCKING)
            }
        }
    }

    fun release() {
        stopPlayback()
        val oldScrub = scrubTrack
        scrubTrack = null
        if (oldScrub != null) {
            runCatching { oldScrub.stop() }
            runCatching { oldScrub.release() }
        }
        executor.shutdownNow()
    }
}
