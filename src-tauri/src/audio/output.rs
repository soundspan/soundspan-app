//! Audio output module using cpal.
//!
//! Outputs decoded PCM audio at native sample rate via platform audio APIs:
//! - Windows: WASAPI (shared or exclusive mode)
//! - Android: AAudio/Oboe
//! - Linux/macOS: ALSA/CoreAudio (not normally used since WebKit handles audio)
//!
//! Uses a lock-free SPSC ring buffer for zero-allocation audio callback.
#![allow(dead_code)]

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig, SupportedStreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;

/// Errors from the audio output subsystem.
#[derive(Debug)]
pub enum OutputError {
    NoDevice(String),
    DeviceError(String),
    StreamError(String),
    ExclusiveDenied(String),
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::NoDevice(msg) => write!(f, "No audio device: {msg}"),
            OutputError::DeviceError(msg) => write!(f, "Device error: {msg}"),
            OutputError::StreamError(msg) => write!(f, "Stream error: {msg}"),
            OutputError::ExclusiveDenied(msg) => write!(f, "Exclusive mode denied: {msg}"),
        }
    }
}

impl std::error::Error for OutputError {}

/// Shared state between the audio callback and the main thread.
struct SharedState {
    volume: AtomicU32,
    paused: AtomicBool,
    frames_played: AtomicU64,
    buffering: AtomicBool,
}

/// Audio output stream manager.
///
/// Owns the cpal stream while `AudioOutputController` exposes transport
/// controls and playback telemetry for other threads.
pub struct AudioOutput {
    _stream: cpal::Stream,
}

/// Thread-safe transport controls for an active output stream.
#[derive(Clone)]
pub struct AudioOutputController {
    sample_rate: u32,
    source_channels: u16,
    device_channels: u16,
    shared: Arc<SharedState>,
    exclusive: bool,
}

/// Writer end of the ring buffer for the decode thread to push samples into.
pub struct RingBufferWriter {
    producer: ringbuf::HeapProd<f32>,
}

impl AudioOutput {
    /// Open an audio output stream.
    ///
    /// Creates a ring buffer and returns the output handle, a controller for
    /// cross-thread transport commands, and a writer for the decode thread.
    pub fn open(
        sample_rate: u32,
        channels: u16,
        exclusive: bool,
    ) -> Result<(Self, AudioOutputController, RingBufferWriter), OutputError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| OutputError::NoDevice("No default output device".to_string()))?;

        let config = {
            #[cfg(target_os = "windows")]
            {
                if exclusive {
                    Self::select_output_config(
                        &device,
                        sample_rate,
                        channels,
                        OutputError::ExclusiveDenied,
                    )?
                } else {
                    Self::select_output_config(
                        &device,
                        sample_rate,
                        channels,
                        OutputError::DeviceError,
                    )?
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = exclusive;
                Self::select_output_config(
                    &device,
                    sample_rate,
                    channels,
                    OutputError::DeviceError,
                )?
            }
        };

        let device_channels = config.channels();

        // Ring buffer: 2 seconds of source audio.
        let buffer_capacity = (sample_rate as usize) * (channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity.max(1));
        let (producer, consumer) = rb.split();

        let shared = Arc::new(SharedState {
            volume: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            frames_played: AtomicU64::new(0),
            buffering: AtomicBool::new(false),
        });

        let stream_config: StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream_f32(
                &device,
                &stream_config,
                consumer,
                shared.clone(),
                channels as usize,
                device_channels as usize,
            ),
            SampleFormat::I16 => Self::build_stream_i16(
                &device,
                &stream_config,
                consumer,
                shared.clone(),
                channels as usize,
                device_channels as usize,
            ),
            fmt => Err(OutputError::StreamError(format!(
                "Unsupported sample format: {fmt:?}"
            ))),
        }?;

        stream
            .play()
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        let controller = AudioOutputController {
            sample_rate,
            source_channels: channels,
            device_channels,
            shared,
            exclusive: cfg!(target_os = "windows") && exclusive,
        };

        Ok((
            AudioOutput { _stream: stream },
            controller,
            RingBufferWriter { producer },
        ))
    }

    fn select_output_config<F>(
        device: &cpal::Device,
        sample_rate: u32,
        channels: u16,
        make_error: F,
    ) -> Result<SupportedStreamConfig, OutputError>
    where
        F: Fn(String) -> OutputError,
    {
        let target_rate = SampleRate(sample_rate);
        let supported_configs = device
            .supported_output_configs()
            .map_err(|e| make_error(e.to_string()))?;

        let mut best_config = None;
        let mut best_score = (u16::MAX, u8::MAX);

        for config in supported_configs {
            if config.min_sample_rate() > target_rate || config.max_sample_rate() < target_rate {
                continue;
            }

            let candidate = config.with_sample_rate(target_rate);
            let format_rank = match candidate.sample_format() {
                SampleFormat::F32 => 0,
                SampleFormat::I16 => 1,
                _ => continue,
            };
            let channel_rank = candidate.channels().abs_diff(channels);
            let score = (channel_rank, format_rank);

            if score < best_score {
                best_score = score;
                best_config = Some(candidate);
                if score == (0, 0) {
                    break;
                }
            }
        }

        best_config.ok_or_else(|| {
            make_error(format!(
                "No supported output config for {sample_rate}Hz on the default device"
            ))
        })
    }

    fn build_stream_f32(
        device: &cpal::Device,
        config: &StreamConfig,
        mut consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
        source_channels: usize,
        device_channels: usize,
    ) -> Result<cpal::Stream, OutputError> {
        let mut source_frame = vec![0.0f32; source_channels];
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if shared.paused.load(Ordering::Relaxed) {
                        shared.buffering.store(false, Ordering::Relaxed);
                        data.fill(0.0);
                        return;
                    }

                    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                    let mut callback_buffering = false;
                    let mut frames_played = 0u64;

                    for frame in data.chunks_mut(device_channels) {
                        let complete_frame =
                            populate_source_frame(&mut consumer, &mut source_frame);
                        if !complete_frame {
                            callback_buffering = true;
                        } else {
                            frames_played += 1;
                        }
                        mix_source_frame_to_f32(&source_frame, frame, volume);
                    }

                    shared
                        .frames_played
                        .fetch_add(frames_played, Ordering::Relaxed);
                    shared
                        .buffering
                        .store(callback_buffering, Ordering::Relaxed);
                },
                |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        Ok(stream)
    }

    fn build_stream_i16(
        device: &cpal::Device,
        config: &StreamConfig,
        mut consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
        source_channels: usize,
        device_channels: usize,
    ) -> Result<cpal::Stream, OutputError> {
        let mut source_frame = vec![0.0f32; source_channels];
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    if shared.paused.load(Ordering::Relaxed) {
                        shared.buffering.store(false, Ordering::Relaxed);
                        data.fill(0);
                        return;
                    }

                    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                    let mut callback_buffering = false;
                    let mut frames_played = 0u64;

                    for frame in data.chunks_mut(device_channels) {
                        let complete_frame =
                            populate_source_frame(&mut consumer, &mut source_frame);
                        if !complete_frame {
                            callback_buffering = true;
                        } else {
                            frames_played += 1;
                        }
                        mix_source_frame_to_i16(&source_frame, frame, volume);
                    }

                    shared
                        .frames_played
                        .fetch_add(frames_played, Ordering::Relaxed);
                    shared
                        .buffering
                        .store(callback_buffering, Ordering::Relaxed);
                },
                |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        Ok(stream)
    }
}

impl AudioOutputController {
    /// Pause audio output. The callback will write silence.
    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::Relaxed);
    }

    /// Resume audio output.
    pub fn resume(&self) {
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    /// Set output volume (0.0–1.0).
    pub fn set_volume(&self, level: f32) {
        let clamped = level.clamp(0.0, 1.0);
        self.shared
            .volume
            .store(clamped.to_bits(), Ordering::Relaxed);
    }

    /// Get the current playback position in seconds.
    pub fn position_secs(&self) -> f64 {
        let frames = self.shared.frames_played.load(Ordering::Relaxed);
        frames as f64 / self.sample_rate as f64
    }

    /// Reset the position counter (e.g., after a seek).
    pub fn reset_position(&self) {
        self.shared.frames_played.store(0, Ordering::Relaxed);
    }

    pub fn is_buffering(&self) -> bool {
        self.shared.buffering.load(Ordering::Relaxed)
    }

    pub fn source_channels(&self) -> u16 {
        self.source_channels
    }

    pub fn device_channels(&self) -> u16 {
        self.device_channels
    }

    /// Whether exclusive mode is active.
    pub fn is_exclusive(&self) -> bool {
        self.exclusive
    }
}

impl RingBufferWriter {
    /// Push samples into the ring buffer (non-blocking).
    pub fn write(&mut self, samples: &[f32]) -> usize {
        self.producer.push_slice(samples)
    }

    /// Push all samples, yielding the thread when the buffer is full.
    ///
    /// Returns `false` if the write was interrupted by a stop signal.
    pub fn write_interruptible(&mut self, samples: &[f32], stop: &AtomicBool) -> bool {
        let mut offset = 0;
        while offset < samples.len() {
            if stop.load(Ordering::Relaxed) {
                return false;
            }

            let written = self.producer.push_slice(&samples[offset..]);
            offset += written;

            if offset < samples.len() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        true
    }

    /// How many samples can be written without blocking.
    pub fn available(&self) -> usize {
        self.producer.vacant_len()
    }
}

fn populate_source_frame(consumer: &mut ringbuf::HeapCons<f32>, frame: &mut [f32]) -> bool {
    let mut complete_frame = true;
    for sample in frame.iter_mut() {
        if let Some(value) = consumer.try_pop() {
            *sample = value;
        } else {
            *sample = 0.0;
            complete_frame = false;
        }
    }
    complete_frame
}

fn mix_source_frame_to_f32(source: &[f32], dest: &mut [f32], volume: f32) {
    if dest.is_empty() {
        return;
    }

    if source.len() == 1 {
        let sample = source[0] * volume;
        dest.fill(sample);
        return;
    }

    if dest.len() == 1 {
        let sample = source.iter().copied().sum::<f32>() / source.len() as f32;
        dest[0] = sample * volume;
        return;
    }

    for (idx, sample_out) in dest.iter_mut().enumerate() {
        let sample = source
            .get(idx)
            .copied()
            .unwrap_or_else(|| *source.last().unwrap_or(&0.0));
        *sample_out = sample * volume;
    }
}

fn mix_source_frame_to_i16(source: &[f32], dest: &mut [i16], volume: f32) {
    if dest.is_empty() {
        return;
    }

    if source.len() == 1 {
        let sample = f32_to_i16(source[0] * volume);
        dest.fill(sample);
        return;
    }

    if dest.len() == 1 {
        let sample = source.iter().copied().sum::<f32>() / source.len() as f32;
        dest[0] = f32_to_i16(sample * volume);
        return;
    }

    for (idx, sample_out) in dest.iter_mut().enumerate() {
        let sample = source
            .get(idx)
            .copied()
            .unwrap_or_else(|| *source.last().unwrap_or(&0.0));
        *sample_out = f32_to_i16(sample * volume);
    }
}

/// Convert f32 sample to i16 with clamping.
#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    let scaled = sample * 32767.0;
    let clamped = scaled.clamp(-32768.0, 32767.0);
    clamped as i16
}

#[cfg(test)]
mod tests {
    use super::{mix_source_frame_to_f32, mix_source_frame_to_i16};

    #[test]
    fn duplicates_mono_into_stereo() {
        let mut dest = [0.0f32; 2];
        mix_source_frame_to_f32(&[0.25], &mut dest, 1.0);
        assert_eq!(dest, [0.25, 0.25]);
    }

    #[test]
    fn averages_stereo_into_mono() {
        let mut dest = [0.0f32; 1];
        mix_source_frame_to_f32(&[0.25, 0.75], &mut dest, 1.0);
        assert_eq!(dest, [0.5]);
    }

    #[test]
    fn converts_to_i16_after_mixing() {
        let mut dest = [0i16; 2];
        mix_source_frame_to_i16(&[0.5], &mut dest, 1.0);
        assert_eq!(dest, [16383, 16383]);
    }
}
