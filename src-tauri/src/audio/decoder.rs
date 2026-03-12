//! Audio decoder module using symphonia.
//!
//! Decodes audio files fetched via HTTP into interleaved f32 PCM samples.
//! Supports FLAC, WAV, MP3, AAC, Vorbis, Opus via symphonia codecs.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

use symphonia::core::audio::{AudioBufferRef, SampleBuffer};
use symphonia::core::codecs::{self, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSource;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Metadata about the decoded audio track.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrackInfo {
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub channels: u16,
    pub format: String,
    pub duration_secs: f64,
}

/// Errors that can occur during decoding.
#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("HTTP fetch failed: {0}")]
    FetchError(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Decode error: {0}")]
    DecodeError(String),

    #[error("Seek error: {0}")]
    SeekError(String),

    #[error("No audio track found")]
    NoAudioTrack,

    #[error("IO error: {0}")]
    IoError(String),
}

impl From<SymphoniaError> for DecoderError {
    fn from(err: SymphoniaError) -> Self {
        match err {
            SymphoniaError::IoError(e) => DecoderError::IoError(e.to_string()),
            SymphoniaError::DecodeError(msg) => DecoderError::DecodeError(msg.to_string()),
            SymphoniaError::SeekError(e) => DecoderError::SeekError(format!("{:?}", e)),
            SymphoniaError::Unsupported(msg) => DecoderError::UnsupportedFormat(msg.to_string()),
            _ => DecoderError::DecodeError(format!("{:?}", err)),
        }
    }
}

impl From<io::Error> for DecoderError {
    fn from(err: io::Error) -> Self {
        DecoderError::IoError(err.to_string())
    }
}

/// Wrapper around a buffered HTTP response that implements `MediaSource`.
///
/// Reads the entire response into memory for seekability and Sync safety.
/// This is acceptable because audio files are finite-sized and symphonia
/// needs to seek for some formats (e.g., MP4/AAC).
struct BufferedSource {
    cursor: io::Cursor<Vec<u8>>,
}

impl Read for BufferedSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for BufferedSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl MediaSource for BufferedSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.cursor.get_ref().len() as u64)
    }
}

/// Audio decoder that reads packets from a symphonia `FormatReader` and
/// decodes them into interleaved f32 PCM samples.
pub struct AudioDecoder {
    format_reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    track_info: TrackInfo,
}

impl AudioDecoder {
    /// Create a decoder from an HTTP URL.
    ///
    /// Fetches the URL with the provided headers and probes the stream format.
    pub fn from_url(
        url: &str,
        headers: HashMap<String, String>,
        hint: Option<&str>,
    ) -> Result<Self, DecoderError> {
        let mut req = reqwest::blocking::Client::new().get(url);
        for (key, value) in &headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let mut response = req
            .send()
            .map_err(|e| DecoderError::FetchError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DecoderError::FetchError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        // Buffer the entire response for seekability and Sync safety
        let mut body = Vec::new();
        response
            .read_to_end(&mut body)
            .map_err(|e| DecoderError::FetchError(format!("Failed to read response: {}", e)))?;

        let source = BufferedSource {
            cursor: io::Cursor::new(body),
        };

        // Extract format hint from URL extension
        let inferred_hint = url
            .split('?')
            .next()
            .and_then(|path| path.rsplit('.').next())
            .map(|ext| ext.to_lowercase());

        Self::from_reader(Box::new(source), hint.or(inferred_hint.as_deref()))
    }

    /// Create a decoder from an existing `MediaSource`.
    ///
    /// `hint` is an optional format hint (e.g., "flac", "mp3") to guide probing.
    pub fn from_reader(
        reader: Box<dyn MediaSource>,
        hint: Option<&str>,
    ) -> Result<Self, DecoderError> {
        let mut probe_hint = Hint::new();
        if let Some(ext) = hint {
            probe_hint.with_extension(ext);
        }

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let metadata_opts = MetadataOptions::default();

        let probed = symphonia::default::get_probe()
            .format(
                &probe_hint,
                symphonia::core::io::MediaSourceStream::new(reader, Default::default()),
                &format_opts,
                &metadata_opts,
            )
            .map_err(|e| DecoderError::UnsupportedFormat(e.to_string()))?;

        let format_reader = probed.format;

        // Find the first audio track
        let track = format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != codecs::CODEC_TYPE_NULL)
            .ok_or(DecoderError::NoAudioTrack)?;

        let codec_params = &track.codec_params;
        let track_id = track.id;

        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params
            .channels
            .map(|ch| ch.count() as u16)
            .unwrap_or(2);
        let bit_depth = codec_params.bits_per_sample.unwrap_or(16);

        // Calculate duration from the track's time base and number of frames
        let duration_secs = codec_params
            .n_frames
            .zip(codec_params.time_base)
            .map(|(frames, tb)| {
                let time = tb.calc_time(frames);
                time.seconds as f64 + time.frac
            })
            .unwrap_or(0.0);

        let format_name = codec_name(codec_params.codec);

        let decoder_opts = DecoderOptions::default();
        let decoder = symphonia::default::get_codecs()
            .make(codec_params, &decoder_opts)
            .map_err(|e| DecoderError::UnsupportedFormat(e.to_string()))?;

        let track_info = TrackInfo {
            sample_rate,
            bit_depth,
            channels,
            format: format_name,
            duration_secs,
        };

        Ok(Self {
            format_reader,
            decoder,
            track_id,
            track_info,
        })
    }

    /// Decode the next packet and return interleaved f32 samples.
    ///
    /// Returns `Ok(None)` at end-of-stream.
    pub fn decode_next(&mut self) -> Result<Option<Vec<f32>>, DecoderError> {
        loop {
            let packet = match self.format_reader.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            };

            // Skip packets for other tracks
            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(audio_buf) => {
                    return Ok(Some(audio_buf_to_f32(&audio_buf)));
                }
                Err(SymphoniaError::DecodeError(msg)) => {
                    // Non-fatal decode error — skip this packet
                    log::warn!("Skipping corrupted packet: {}", msg);
                    continue;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Seek to the given time in seconds.
    pub fn seek(&mut self, time_secs: f64) -> Result<(), DecoderError> {
        let time = Time {
            seconds: time_secs as u64,
            frac: time_secs.fract(),
        };

        self.format_reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        )?;

        self.decoder.reset();
        Ok(())
    }

    /// Get the track info (sample rate, bit depth, channels, format, duration).
    pub fn track_info(&self) -> &TrackInfo {
        &self.track_info
    }
}

/// Convert a symphonia `AudioBufferRef` to interleaved f32 samples.
fn audio_buf_to_f32(buf: &AudioBufferRef) -> Vec<f32> {
    let spec = *buf.spec();
    let num_frames = buf.frames();
    let num_channels = spec.channels.count();

    if num_frames == 0 || num_channels == 0 {
        return Vec::new();
    }

    let total_samples = num_frames * num_channels;
    let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
    sample_buf.copy_interleaved_ref(buf.clone());

    sample_buf.samples()[..total_samples].to_vec()
}

/// Map a symphonia codec type to a short format string.
fn codec_name(codec: codecs::CodecType) -> String {
    match codec {
        codecs::CODEC_TYPE_FLAC => "flac".to_string(),
        codecs::CODEC_TYPE_MP3 => "mp3".to_string(),
        codecs::CODEC_TYPE_AAC => "aac".to_string(),
        codecs::CODEC_TYPE_VORBIS => "vorbis".to_string(),
        codecs::CODEC_TYPE_OPUS => "opus".to_string(),
        codecs::CODEC_TYPE_PCM_S16LE
        | codecs::CODEC_TYPE_PCM_S24LE
        | codecs::CODEC_TYPE_PCM_S32LE
        | codecs::CODEC_TYPE_PCM_F32LE => "wav".to_string(),
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_info_serializes() {
        let info = TrackInfo {
            sample_rate: 96000,
            bit_depth: 24,
            channels: 2,
            format: "flac".to_string(),
            duration_secs: 245.3,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("96000"));
        assert!(json.contains("flac"));
    }

    #[test]
    fn decoder_error_display() {
        let err = DecoderError::FetchError("timeout".to_string());
        assert_eq!(err.to_string(), "HTTP fetch failed: timeout");
    }

    #[test]
    fn codec_name_mapping() {
        assert_eq!(codec_name(codecs::CODEC_TYPE_FLAC), "flac");
        assert_eq!(codec_name(codecs::CODEC_TYPE_MP3), "mp3");
        assert_eq!(codec_name(codecs::CODEC_TYPE_AAC), "aac");
        assert_eq!(codec_name(codecs::CODEC_TYPE_VORBIS), "vorbis");
        assert_eq!(codec_name(codecs::CODEC_TYPE_OPUS), "opus");
    }
}
