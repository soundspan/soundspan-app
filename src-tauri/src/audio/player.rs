//! Audio player orchestrator.
//!
//! Wires together the decoder, output, and preload modules into a high-level
//! player with play/pause/seek/stop controls. Manages the decode thread and
//! position reporting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::{AppHandle, Emitter};

use super::decoder::{AudioDecoder, TrackInfo};
use super::output::AudioOutput;
use super::preload::PreloadManager;
use super::state::{AudioMeta, PlaybackState, PlaybackStatus, PlaybackSnapshot};

/// Shared position value updated by the output thread, read by the reporter.
struct OutputHandle {
    output: AudioOutput,
}

// AudioOutput contains cpal::Stream which has a raw pointer internally.
// It's safe to send between threads because the stream is managed by cpal.
unsafe impl Send for OutputHandle {}

/// The main audio player. Stored as Tauri managed state.
pub struct AudioPlayer {
    state: PlaybackState,
    preload: PreloadManager,
    stop_signal: Arc<AtomicBool>,
    output: Mutex<Option<OutputHandle>>,
    reporter_stop: Arc<AtomicBool>,
}

// Safety: all fields are behind Arc/Mutex, and OutputHandle is Send.
unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::new(),
            preload: PreloadManager::new(),
            stop_signal: Arc::new(AtomicBool::new(false)),
            output: Mutex::new(None),
            reporter_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start playback of a track.
    pub fn play(
        &self,
        app: &AppHandle,
        url: String,
        _format: Option<String>,
        headers: Option<HashMap<String, String>>,
        autoplay: bool,
    ) -> Result<(), String> {
        self.stop_internal();

        let headers = headers.unwrap_or_default();
        let exclusive = self.state.exclusive_mode();

        self.state.set_status(PlaybackStatus::Loading);
        self.state.set_current_url(Some(url.clone()));

        let preloaded = self.preload.take_if_matches(&url);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        // Update the shared stop signal
        self.stop_signal.store(false, Ordering::SeqCst);

        let state = self.state.clone();
        let app_handle = app.clone();

        thread::Builder::new()
            .name("audio-decode".to_string())
            .spawn(move || {
                let result = if let Some(pre) = preloaded {
                    Self::play_preloaded(pre, exclusive, &state, &app_handle, &stop_clone, autoplay)
                } else {
                    Self::play_from_url(&url, headers, exclusive, &state, &app_handle, &stop_clone, autoplay)
                };

                if let Err(e) = result {
                    log::error!("Playback error: {}", e);
                    state.set_status(PlaybackStatus::Error);
                    let _ = app_handle.emit("native-audio-error", serde_json::json!({
                        "code": "playback_error",
                        "message": e,
                        "recoverable": false,
                    }));
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn play_from_url(
        url: &str,
        headers: HashMap<String, String>,
        exclusive: bool,
        state: &PlaybackState,
        app: &AppHandle,
        stop: &Arc<AtomicBool>,
        autoplay: bool,
    ) -> Result<(), String> {
        let decoder = AudioDecoder::from_url(url, headers)
            .map_err(|e| e.to_string())?;

        let info = decoder.track_info().clone();
        Self::run_decode_loop(decoder, info, exclusive, state, app, stop, autoplay)
    }

    fn play_preloaded(
        preloaded: super::preload::PreloadedTrack,
        exclusive: bool,
        state: &PlaybackState,
        app: &AppHandle,
        stop: &Arc<AtomicBool>,
        autoplay: bool,
    ) -> Result<(), String> {
        let info = preloaded.track_info.clone();

        let (output, mut writer) = AudioOutput::open(info.sample_rate, info.channels, exclusive)
            .map_err(|e| e.to_string())?;

        Self::emit_loaded(app, &info);
        Self::update_meta(state, &info);

        if !autoplay {
            output.pause();
            state.set_status(PlaybackStatus::Paused);
        } else {
            state.set_status(PlaybackStatus::Playing);
        }

        output.set_volume(state.volume());

        // Position reporter
        let reporter_stop = Arc::new(AtomicBool::new(false));
        {
            let reporter_stop = reporter_stop.clone();
            let app = app.clone();
            let state = state.clone();
            let _sample_rate = info.sample_rate;
            let _channels = info.channels;
            let duration = info.duration_secs;

            // We track position from samples written by output
            // Since we can't share AudioOutput across threads (non-Send stream),
            // we'll use a time-based estimator from the state
            thread::Builder::new()
                .name("audio-position".to_string())
                .spawn(move || {
                    let start = std::time::Instant::now();
                    while !reporter_stop.load(Ordering::Relaxed) {
                        thread::sleep(std::time::Duration::from_millis(250));
                        if state.status() == PlaybackStatus::Playing {
                            let pos = start.elapsed().as_secs_f64().min(duration);
                            state.set_position_secs(pos);
                            let _ = app.emit("native-audio-timeupdate", serde_json::json!({
                                "position_secs": pos,
                            }));
                        }
                    }
                })
                .ok();
        }

        // Write preloaded samples to ring buffer
        writer.write_blocking(&preloaded.samples);

        // Wait for playback to finish
        let total_samples = preloaded.samples.len() as f64;
        let samples_per_sec = info.sample_rate as f64 * info.channels as f64;
        let expected_duration = total_samples / samples_per_sec;

        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < expected_duration + 0.5 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }

        reporter_stop.store(true, Ordering::Relaxed);

        if !stop.load(Ordering::Relaxed) {
            state.set_status(PlaybackStatus::Stopped);
            let _ = app.emit("native-audio-ended", serde_json::json!({}));
        }

        Ok(())
    }

    fn run_decode_loop(
        mut decoder: AudioDecoder,
        info: TrackInfo,
        exclusive: bool,
        state: &PlaybackState,
        app: &AppHandle,
        stop: &Arc<AtomicBool>,
        autoplay: bool,
    ) -> Result<(), String> {
        let (output, mut writer) = AudioOutput::open(info.sample_rate, info.channels, exclusive)
            .map_err(|e| e.to_string())?;

        Self::emit_loaded(app, &info);
        Self::update_meta(state, &info);

        if !autoplay {
            output.pause();
            state.set_status(PlaybackStatus::Paused);
        } else {
            state.set_status(PlaybackStatus::Playing);
        }

        output.set_volume(state.volume());

        // Position reporter
        let reporter_stop = Arc::new(AtomicBool::new(false));
        {
            let reporter_stop = reporter_stop.clone();
            let app = app.clone();
            let state = state.clone();
            let duration = info.duration_secs;

            thread::Builder::new()
                .name("audio-position".to_string())
                .spawn(move || {
                    let start = std::time::Instant::now();
                    while !reporter_stop.load(Ordering::Relaxed) {
                        thread::sleep(std::time::Duration::from_millis(250));
                        if state.status() == PlaybackStatus::Playing {
                            let pos = start.elapsed().as_secs_f64().min(duration);
                            state.set_position_secs(pos);
                            let _ = app.emit("native-audio-timeupdate", serde_json::json!({
                                "position_secs": pos,
                            }));
                        }
                    }
                })
                .ok();
        }

        // Decode loop
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            match decoder.decode_next() {
                Ok(Some(samples)) => {
                    writer.write_blocking(&samples);
                }
                Ok(None) => {
                    // Wait for output to drain
                    thread::sleep(std::time::Duration::from_millis(500));
                    break;
                }
                Err(e) => {
                    log::error!("Decode error: {}", e);
                    let _ = app.emit("native-audio-error", serde_json::json!({
                        "code": "decode_error",
                        "message": e.to_string(),
                        "recoverable": false,
                    }));
                    break;
                }
            }
        }

        reporter_stop.store(true, Ordering::Relaxed);

        if !stop.load(Ordering::Relaxed) {
            state.set_status(PlaybackStatus::Stopped);
            let _ = app.emit("native-audio-ended", serde_json::json!({}));
        }

        // Output is dropped here, closing the cpal stream
        drop(output);

        Ok(())
    }

    fn emit_loaded(app: &AppHandle, info: &TrackInfo) {
        let _ = app.emit("native-audio-loaded", serde_json::json!({
            "duration_secs": info.duration_secs,
            "sample_rate": info.sample_rate,
            "bit_depth": info.bit_depth,
            "channels": info.channels,
            "format": info.format,
        }));
    }

    fn update_meta(state: &PlaybackState, info: &TrackInfo) {
        state.set_meta(AudioMeta {
            sample_rate: info.sample_rate,
            bit_depth: info.bit_depth,
            channels: info.channels,
            format: info.format.clone(),
            duration_secs: info.duration_secs,
        });
    }

    /// Pause playback.
    pub fn pause(&self) {
        // The output lives on the decode thread, so we signal via state
        self.state.set_status(PlaybackStatus::Paused);
    }

    /// Resume playback.
    pub fn resume(&self) {
        self.state.set_status(PlaybackStatus::Playing);
    }

    /// Stop playback.
    pub fn stop(&self) {
        self.stop_internal();
        self.state.set_status(PlaybackStatus::Stopped);
        self.state.set_current_url(None);
    }

    fn stop_internal(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        self.reporter_stop.store(true, Ordering::SeqCst);
        *self.output.lock().unwrap() = None;
    }

    /// Seek to position.
    pub fn seek(&self, _position_secs: f64) -> Result<(), String> {
        // Full seek requires stopping decode thread, seeking decoder,
        // flushing ring buffer, and restarting. Deferred to refinement.
        Ok(())
    }

    /// Set volume.
    pub fn set_volume(&self, level: f32) {
        self.state.set_volume(level);
        if let Some(handle) = self.output.lock().unwrap().as_ref() {
            handle.output.set_volume(level);
        }
    }

    /// Get state snapshot.
    pub fn get_state(&self) -> PlaybackSnapshot {
        self.state.snapshot()
    }

    /// Preload a track.
    pub fn preload(
        &self,
        url: String,
        headers: Option<HashMap<String, String>>,
    ) -> Result<(), String> {
        let headers = headers.unwrap_or_default();
        let preload = self.preload.clone();

        thread::Builder::new()
            .name("audio-preload".to_string())
            .spawn(move || {
                if let Err(e) = preload.preload(&url, headers) {
                    log::warn!("Preload failed: {}", e);
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Set exclusive mode.
    pub fn set_exclusive(&self, enabled: bool) {
        self.state.set_exclusive_mode(enabled);
    }
}
