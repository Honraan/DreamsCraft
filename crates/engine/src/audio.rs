//! Clean-room audio asset model, WAV PCM decoder, multi-resolution waveform peak generator,
//! and deterministic frame-accurate timeline audio mixer for DreamsCraft.

/// Unique identifier of an audio asset stored in a [`crate::timeline::Timeline`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AudioAssetId(pub u64);

/// Decoded interleaved PCM audio ready to be registered as an [`AudioAsset`].
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: u8,
    /// Interleaved normalized samples in `-1.0..=1.0`.
    pub samples: Vec<f32>,
}

/// Metadata and precomputed waveform peaks for an audio file attached to a timeline.
///
/// The raw PCM samples are kept alongside in memory (and skipped in lightweight document JSON)
/// so the UI can draw waveforms immediately without holding huge JSON arrays.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AudioAsset {
    pub id: AudioAssetId,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u8,
    /// Total frames (sample-frames per channel, i.e. `pcm.len() / channels`).
    pub duration_samples: u64,
    /// Precomputed waveform peaks `[min, max]` in `-1.0..=1.0` evenly spanning the asset.
    pub peaks: Vec<[f32; 2]>,
    /// Interleaved PCM float samples (`-1.0..=1.0`), kept in memory for scrubbing/playback/muxing.
    #[serde(skip)]
    pub pcm: Vec<f32>,
}

impl Default for AudioAsset {
    fn default() -> Self {
        AudioAsset {
            id: AudioAssetId(0),
            name: String::new(),
            sample_rate: 44_100,
            channels: 2,
            duration_samples: 0,
            peaks: Vec::new(),
            pcm: Vec::new(),
        }
    }
}

impl AudioAsset {
    /// Builds an [`AudioAsset`] from interleaved float PCM samples (`-1.0..=1.0`).
    pub fn from_pcm(id: AudioAssetId, name: String, sample_rate: u32, channels: u8, pcm: Vec<f32>) -> AudioAsset {
        let safe_rate = sample_rate.clamp(8_000, 192_000);
        let safe_ch = channels.clamp(1, 2);
        let frame_count = pcm.len() / usize::from(safe_ch);
        let bucket_count = (frame_count / 256).clamp(16, 4096);
        let peaks = compute_waveform_peaks(&pcm, safe_ch, bucket_count);
        AudioAsset {
            id,
            name: if name.trim().is_empty() { "Audio".to_owned() } else { name },
            sample_rate: safe_rate,
            channels: safe_ch,
            duration_samples: frame_count as u64,
            peaks,
            pcm,
        }
    }

    /// Builds an [`AudioAsset`] from interleaved signed 16-bit PCM samples.
    pub fn from_i16_pcm(id: AudioAssetId, name: String, sample_rate: u32, channels: u8, pcm_i16: &[i16]) -> AudioAsset {
        let floats: Vec<f32> = pcm_i16.iter().map(|&s| f32::from(s) / 32_768.0).collect();
        AudioAsset::from_pcm(id, name, sample_rate, channels, floats)
    }

    /// Duration in timeline frames at `fps`.
    pub fn duration_in_timeline_frames(&self, fps: u32) -> u32 {
        let fps = u64::from(fps.clamp(1, 60));
        let rate = u64::from(self.sample_rate.max(1));
        let frames = self.duration_samples.saturating_mul(fps).div_ceil(rate);
        u32::try_from(frames.max(1)).unwrap_or(u32::MAX)
    }

    /// Samples a stereo `(left, right)` pair at fractional source sample index `pos` using linear
    /// interpolation, returning `(0.0, 0.0)` out of range.
    pub fn sample_stereo(&self, pos: f64) -> (f32, f32) {
        if !pos.is_finite() || pos < 0.0 || self.pcm.is_empty() {
            return (0.0, 0.0);
        }
        let ch = usize::from(self.channels.clamp(1, 2));
        let total_frames = self.pcm.len() / ch;
        if total_frames == 0 {
            return (0.0, 0.0);
        }
        let idx0 = pos.floor() as usize;
        if idx0 >= total_frames {
            return (0.0, 0.0);
        }
        let idx1 = (idx0 + 1).min(total_frames - 1);
        let frac = (pos - idx0 as f64) as f32;
        let read_pair = |i: usize| -> (f32, f32) {
            let base = i.saturating_mul(ch);
            let l = self.pcm.get(base).copied().unwrap_or(0.0);
            let r = if ch >= 2 { self.pcm.get(base + 1).copied().unwrap_or(l) } else { l };
            (l, r)
        };
        let (l0, r0) = read_pair(idx0);
        let (l1, r1) = read_pair(idx1);
        (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
    }
}

/// Computes `buckets` waveform `[min, max]` peaks from interleaved PCM samples.
pub fn compute_waveform_peaks(pcm: &[f32], channels: u8, buckets: usize) -> Vec<[f32; 2]> {
    let ch = usize::from(channels.clamp(1, 2));
    let total_frames = pcm.len() / ch;
    let buckets = buckets.clamp(1, 8192);
    if total_frames == 0 {
        return vec![[0.0, 0.0]; buckets];
    }
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        let start = b.saturating_mul(total_frames) / buckets;
        let end = ((b + 1).saturating_mul(total_frames) / buckets).max(start + 1).min(total_frames);
        let mut lo = 0.0_f32;
        let mut hi = 0.0_f32;
        for f in start..end {
            let base = f.saturating_mul(ch);
            let l = pcm.get(base).copied().unwrap_or(0.0);
            let s = if ch >= 2 {
                let r = pcm.get(base + 1).copied().unwrap_or(l);
                0.5 * (l + r)
            } else {
                l
            };
            if s.is_finite() {
                lo = lo.min(s);
                hi = hi.max(s);
            }
        }
        out.push([lo.clamp(-1.0, 1.0), hi.clamp(-1.0, 1.0)]);
    }
    out
}

/// Clean-room RIFF/WAVE PCM decoder supporting 8-bit unsigned, 16-bit signed, 24-bit signed,
/// and 32-bit IEEE float PCM (mono or stereo). Returns `None` for invalid or unsupported data.
pub fn decode_wav_pcm(bytes: &[u8]) -> Option<DecodedAudio> {
    if bytes.len() < 44 {
        return None;
    }
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }

    let mut cursor = 12usize;
    let mut audio_format = 0u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut data_slice: Option<&[u8]> = None;

    while let Some(header) = bytes.get(cursor..cursor.saturating_add(8)) {
        let id = header.get(0..4)?;
        let size_bytes: [u8; 4] = header.get(4..8)?.try_into().ok()?;
        let chunk_size = u32::from_le_bytes(size_bytes) as usize;
        let payload_start = cursor.saturating_add(8);
        let payload_end = payload_start.saturating_add(chunk_size).min(bytes.len());
        let payload = bytes.get(payload_start..payload_end)?;

        if id == b"fmt " && payload.len() >= 16 {
            audio_format = u16::from_le_bytes(payload.get(0..2)?.try_into().ok()?);
            channels = u16::from_le_bytes(payload.get(2..4)?.try_into().ok()?);
            sample_rate = u32::from_le_bytes(payload.get(4..8)?.try_into().ok()?);
            bits_per_sample = u16::from_le_bytes(payload.get(14..16)?.try_into().ok()?);
        } else if id == b"data" {
            data_slice = Some(payload);
            break;
        }

        // RIFF chunks are padded to even byte boundaries
        let padded = chunk_size.saturating_add(chunk_size & 1);
        cursor = payload_start.saturating_add(padded);
        if cursor >= bytes.len() {
            break;
        }
    }

    let raw = data_slice?;
    if channels == 0 || sample_rate == 0 {
        return None;
    }
    let src_ch = usize::from(channels);
    let out_ch: u8 = if src_ch == 1 { 1 } else { 2 };

    let mut samples = Vec::new();
    match (audio_format, bits_per_sample) {
        // 8-bit unsigned PCM
        (1, 8) => {
            let frames = raw.len() / src_ch;
            samples.reserve(frames * usize::from(out_ch));
            for f in 0..frames {
                let base = f * src_ch;
                for c in 0..usize::from(out_ch) {
                    let b = raw.get(base + c).copied().unwrap_or(128);
                    samples.push((f32::from(b) - 128.0) / 128.0);
                }
            }
        }
        // 16-bit signed PCM
        (1, 16) => {
            let stride = src_ch * 2;
            let frames = raw.len() / stride;
            samples.reserve(frames * usize::from(out_ch));
            for f in 0..frames {
                let base = f * stride;
                for c in 0..usize::from(out_ch) {
                    let at = base + c * 2;
                    let pair: [u8; 2] = raw.get(at..at + 2)?.try_into().ok()?;
                    samples.push(f32::from(i16::from_le_bytes(pair)) / 32_768.0);
                }
            }
        }
        // 24-bit signed PCM
        (1, 24) => {
            let stride = src_ch * 3;
            let frames = raw.len() / stride;
            samples.reserve(frames * usize::from(out_ch));
            for f in 0..frames {
                let base = f * stride;
                for c in 0..usize::from(out_ch) {
                    let at = base + c * 3;
                    let b0 = i32::from(*raw.get(at)?);
                    let b1 = i32::from(*raw.get(at + 1)?);
                    let b2 = i32::from(*raw.get(at + 2)? as i8);
                    let val = b0 | (b1 << 8) | (b2 << 16);
                    samples.push(val as f32 / 8_388_608.0);
                }
            }
        }
        // 32-bit IEEE float PCM
        (3, 32) => {
            let stride = src_ch * 4;
            let frames = raw.len() / stride;
            samples.reserve(frames * usize::from(out_ch));
            for f in 0..frames {
                let base = f * stride;
                for c in 0..usize::from(out_ch) {
                    let at = base + c * 4;
                    let quad: [u8; 4] = raw.get(at..at + 4)?.try_into().ok()?;
                    let v = f32::from_le_bytes(quad);
                    samples.push(if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 });
                }
            }
        }
        _ => return None,
    }

    Some(DecodedAudio { sample_rate: sample_rate.clamp(8_000, 192_000), channels: out_ch, samples })
}

/// Converts interleaved float PCM (`-1.0..=1.0`) into signed 16-bit PCM for Android `AudioTrack`
/// and AAC `MediaCodec` encoding.
pub fn float_pcm_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| {
            let c = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
            (c * 32_767.0).round() as i16
        })
        .collect()
}

/// Constant-power stereo pan gains `(left_gain, right_gain)` for `pan` in `-1.0..=1.0`.
pub fn stereo_pan_gains(pan: f32) -> (f32, f32) {
    let p = if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 };
    let angle = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_wav_16bit(sample_rate: u32, channels: u16, pcm: &[i16]) -> Vec<u8> {
        let data_bytes = (pcm.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_bytes as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * u32::from(channels) * 2;
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_bytes.to_le_bytes());
        for &s in pcm {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    #[test]
    fn decodes_16bit_wav_and_computes_waveform_peaks() {
        let pcm = vec![0i16, 16384, -16384, 32767, -32768, 0, 8192, -8192];
        let wav = make_test_wav_16bit(44_100, 1, &pcm);
        let decoded = decode_wav_pcm(&wav).unwrap();
        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.samples.len(), 8);
        assert!((decoded.samples[1] - 0.5).abs() < 1e-3);
        assert!((decoded.samples[4] + 1.0).abs() < 1e-3);

        let peaks = compute_waveform_peaks(&decoded.samples, 1, 2);
        assert_eq!(peaks.len(), 2);
        assert!(peaks[0][1] > 0.99);
        assert!(peaks[1][0] < -0.99);
    }

    #[test]
    fn rejects_corrupt_or_short_wav_safely() {
        assert!(decode_wav_pcm(b"").is_none());
        assert!(decode_wav_pcm(b"NOT_A_WAV_FILE_AT_ALL_000000000000000000000000").is_none());
    }
}
