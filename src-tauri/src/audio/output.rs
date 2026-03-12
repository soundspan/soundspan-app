//! Audio output module using cpal.
//!
//! Outputs decoded PCM audio at native sample rate via platform audio APIs:
//! - Windows: WASAPI (shared or exclusive mode)
//! - Android: AAudio/Oboe
//! - Linux/macOS: ALSA/CoreAudio (not normally used since WebKit handles audio)
//!
//! Uses a lock-free SPSC ring buffer for zero-allocation audio callback.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
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
            OutputError::NoDevice(msg) => write!(f, "No audio device: {}", msg),
            OutputError::DeviceError(msg) => write!(f, "Device error: {}", msg),
            OutputError::StreamError(msg) => write!(f, "Stream error: {}", msg),
            OutputError::ExclusiveDenied(msg) => write!(f, "Exclusive mode denied: {}", msg),
        }
    }
}

impl std::error::Error for OutputError {}

/// Shared state between the audio callback and the main thread.
struct SharedState {
    volume: AtomicU32,
    paused: AtomicBool,
    samples_written: AtomicU64,
}

/// Audio output stream manager.
///
/// Owns the cpal stream and provides transport controls (pause, resume, volume).
/// Position tracking is derived from the sample counter in the audio callback.
pub struct AudioOutput {
    _stream: cpal::Stream,
    sample_rate: u32,
    channels: u16,
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
    /// Creates a ring buffer and returns both the output handle and a writer for
    /// the decode thread. The cpal callback reads samples from the ring buffer.
    ///
    /// # Arguments
    /// * `sample_rate` — desired output sample rate (source file's native rate)
    /// * `channels` — number of audio channels (typically 2)
    /// * `exclusive` — if true, attempt WASAPI exclusive mode (Windows only)
    pub fn open(
        sample_rate: u32,
        channels: u16,
        _exclusive: bool,
    ) -> Result<(Self, RingBufferWriter), OutputError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| OutputError::NoDevice("No default output device".to_string()))?;

        // Ring buffer: 2 seconds of audio
        let buffer_capacity = (sample_rate as usize) * (channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (producer, consumer) = rb.split();

        let shared = Arc::new(SharedState {
            volume: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            samples_written: AtomicU64::new(0),
        });

        // Try exclusive mode on Windows if requested
        #[cfg(target_os = "windows")]
        let (stream, actual_exclusive) = if exclusive {
            match Self::try_exclusive_stream(&device, sample_rate, channels, consumer, shared.clone()) {
                Ok(stream) => (stream, true),
                Err(e) => {
                    log::warn!("Exclusive mode denied, falling back to shared: {}", e);
                    let stream = Self::build_shared_stream(&device, sample_rate, channels, consumer, shared.clone())?;
                    (stream, false)
                }
            }
        } else {
            let stream = Self::build_shared_stream(&device, sample_rate, channels, consumer, shared.clone())?;
            (stream, false)
        };

        #[cfg(not(target_os = "windows"))]
        let (stream, actual_exclusive) = {
            let stream = Self::build_shared_stream(&device, sample_rate, channels, consumer, shared.clone())?;
            (stream, false)
        };

        stream
            .play()
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        Ok((
            AudioOutput {
                _stream: stream,
                sample_rate,
                channels,
                shared,
                exclusive: actual_exclusive,
            },
            RingBufferWriter { producer },
        ))
    }

    /// Build a shared-mode output stream.
    fn build_shared_stream(
        device: &cpal::Device,
        _sample_rate: u32,
        _channels: u16,
        consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
    ) -> Result<cpal::Stream, OutputError> {
        let default_config = device
            .default_output_config()
            .map_err(|e| OutputError::DeviceError(e.to_string()))?;

        let config: StreamConfig = default_config.clone().into();
        let device_channels = config.channels as usize;

        match default_config.sample_format() {
            SampleFormat::F32 => Self::build_stream_f32(device, &config, consumer, shared, device_channels),
            SampleFormat::I16 => Self::build_stream_i16(device, &config, consumer, shared, device_channels),
            fmt => Err(OutputError::StreamError(format!(
                "Unsupported sample format: {:?}",
                fmt
            ))),
        }
    }

    /// Build an f32 output stream.
    fn build_stream_f32(
        device: &cpal::Device,
        config: &StreamConfig,
        mut consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
        device_channels: usize,
    ) -> Result<cpal::Stream, OutputError> {
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let paused = shared.paused.load(Ordering::Relaxed);
                    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));

                    if paused {
                        data.fill(0.0);
                        return;
                    }

                    let mut written = 0usize;
                    for frame in data.chunks_mut(device_channels) {
                        // Read one frame (source channels) from ring buffer
                        let mut got_sample = false;
                        for ch in 0..device_channels {
                            if let Some(sample) = consumer.try_pop() {
                                frame[ch] = sample * volume;
                                got_sample = true;
                            } else {
                                frame[ch] = 0.0;
                            }
                        }
                        if got_sample {
                            written += device_channels;
                        }
                    }

                    shared
                        .samples_written
                        .fetch_add(written as u64, Ordering::Relaxed);
                },
                |err| {
                    log::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        Ok(stream)
    }

    /// Build an i16 output stream (for devices that don't support f32).
    fn build_stream_i16(
        device: &cpal::Device,
        config: &StreamConfig,
        mut consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
        device_channels: usize,
    ) -> Result<cpal::Stream, OutputError> {
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let paused = shared.paused.load(Ordering::Relaxed);
                    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));

                    if paused {
                        data.fill(0);
                        return;
                    }

                    let mut written = 0usize;
                    for frame in data.chunks_mut(device_channels) {
                        let mut got_sample = false;
                        for ch in 0..device_channels {
                            if let Some(sample) = consumer.try_pop() {
                                frame[ch] = f32_to_i16(sample * volume);
                                got_sample = true;
                            } else {
                                frame[ch] = 0;
                            }
                        }
                        if got_sample {
                            written += device_channels;
                        }
                    }

                    shared
                        .samples_written
                        .fetch_add(written as u64, Ordering::Relaxed);
                },
                |err| {
                    log::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| OutputError::StreamError(e.to_string()))?;

        Ok(stream)
    }

    /// Attempt to open a WASAPI exclusive mode stream (Windows only).
    #[cfg(target_os = "windows")]
    fn try_exclusive_stream(
        device: &cpal::Device,
        sample_rate: u32,
        channels: u16,
        consumer: ringbuf::HeapCons<f32>,
        shared: Arc<SharedState>,
    ) -> Result<cpal::Stream, OutputError> {
        let supported_configs = device
            .supported_output_configs()
            .map_err(|e| OutputError::ExclusiveDenied(e.to_string()))?;

        // Find a config matching our exact sample rate, preferring f32
        let target_rate = SampleRate(sample_rate);
        let mut best_config = None;

        for cfg in supported_configs {
            if cfg.min_sample_rate() <= target_rate && cfg.max_sample_rate() >= target_rate {
                let with_rate = cfg.with_sample_rate(target_rate);
                match with_rate.sample_format() {
                    SampleFormat::F32 => {
                        best_config = Some(with_rate);
                        break; // f32 is preferred
                    }
                    SampleFormat::I16 => {
                        if best_config.is_none() {
                            best_config = Some(with_rate);
                        }
                    }
                    _ => {}
                }
            }
        }

        let config = best_config
            .ok_or_else(|| {
                OutputError::ExclusiveDenied(format!(
                    "No supported config for {}Hz exclusive",
                    sample_rate
                ))
            })?;

        let device_channels = config.channels() as usize;
        let stream_config: StreamConfig = config.clone().into();

        match config.sample_format() {
            SampleFormat::F32 => {
                Self::build_stream_f32(device, &stream_config, consumer, shared, device_channels)
            }
            SampleFormat::I16 => {
                Self::build_stream_i16(device, &stream_config, consumer, shared, device_channels)
            }
            fmt => Err(OutputError::ExclusiveDenied(format!(
                "Unsupported exclusive format: {:?}",
                fmt
            ))),
        }
    }

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
    ///
    /// Derived from the total number of samples written to the audio device.
    pub fn position_secs(&self) -> f64 {
        let samples = self.shared.samples_written.load(Ordering::Relaxed);
        samples as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    /// Reset the position counter (e.g., after a seek).
    pub fn reset_position(&self) {
        self.shared.samples_written.store(0, Ordering::Relaxed);
    }

    /// Get the output sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether exclusive mode is active.
    pub fn is_exclusive(&self) -> bool {
        self.exclusive
    }
}

impl RingBufferWriter {
    /// Push samples into the ring buffer (non-blocking).
    ///
    /// Returns the number of samples actually written. May be less than
    /// `samples.len()` if the buffer is full.
    pub fn write(&mut self, samples: &[f32]) -> usize {
        self.producer.push_slice(samples)
    }

    /// Push all samples, yielding the thread when the buffer is full.
    pub fn write_blocking(&mut self, samples: &[f32]) {
        let mut offset = 0;
        while offset < samples.len() {
            let written = self.producer.push_slice(&samples[offset..]);
            offset += written;
            if offset < samples.len() {
                std::thread::yield_now();
            }
        }
    }

    /// How many samples can be written without blocking.
    pub fn available(&self) -> usize {
        self.producer.vacant_len()
    }
}

/// Convert f32 sample to i16 with clamping.
#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    let scaled = sample * 32767.0;
    let clamped = scaled.clamp(-32768.0, 32767.0);
    clamped as i16
}
