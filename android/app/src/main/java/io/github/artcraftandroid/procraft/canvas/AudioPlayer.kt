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

    private val trackLock = Any()
    private var generation = 0L
    private var closed = false

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
    fun startPlayback(pcmStereo: ShortArray, first: Int, end: Int, reverse: Boolean) {
        val from = first.coerceIn(0, pcmStereo.size) and -2
        val until = end.coerceIn(from, pcmStereo.size) and -2
        if (from == until) return
        synchronized(trackLock) {
            if (closed) return
            stopPlaybackLocked()
            val ticket = generation
            executor.execute {
                val track = buildTrack((until - from).coerceAtMost(sampleRate / 5) * 2) ?: return@execute
                synchronized(trackLock) {
                    if (ticket != generation || closed) {
                        runCatching { track.release() }
                        return@execute
                    }
                    playbackTrack = track
                    runCatching { track.play() }
                }
                runCatching {
                    val reversed = if (reverse) ShortArray(4096) else null
                    var offset = 0
                    while (offset < until - from && playbackTrack === track) {
                        val chunk = minOf(4096, until - from - offset)
                        val written = if (reversed == null) {
                            track.write(pcmStereo, from + offset, chunk, AudioTrack.WRITE_BLOCKING)
                        } else {
                            for (i in 0 until chunk) {
                                val sample = offset + i
                                reversed[i] = pcmStereo[until - 2 - (sample and -2) + (sample and 1)]
                            }
                            track.write(reversed, 0, chunk, AudioTrack.WRITE_BLOCKING)
                        }
                        if (written <= 0) break
                        offset += written
                    }
                }
            }
        }
    }

    /** Stops continuous timeline audio playback immediately. */
    fun stopPlayback() = synchronized(trackLock) { stopPlaybackLocked() }

    private fun stopPlaybackLocked() {
        generation++
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
        synchronized(trackLock) {
            if (closed || playbackTrack != null) return
            val ticket = generation
            executor.execute {
                synchronized(trackLock) {
                    if (closed || ticket != generation || playbackTrack != null) return@execute
                    val track = scrubTrack ?: buildTrack(pcmStereo.size.coerceAtLeast(2048) * 2)?.also {
                        scrubTrack = it
                    } ?: return@execute
                    runCatching {
                        track.pause()
                        track.flush()
                        track.write(pcmStereo, 0, pcmStereo.size, AudioTrack.WRITE_NON_BLOCKING)
                        track.play()
                    }
                }
            }
        }
    }

    fun release(): Unit = synchronized(trackLock) {
        if (!closed) {
            closed = true
            stopPlaybackLocked()
            val old = scrubTrack
            scrubTrack = null
            if (old != null) {
                runCatching { old.stop() }
                runCatching { old.release() }
            }
            executor.shutdownNow()
        }
    }
}
