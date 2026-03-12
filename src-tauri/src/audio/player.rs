//! Audio player orchestrator.
//!
//! Wires together the decoder, output, and preload modules into a high-level
//! player with play/pause/seek/stop controls. Manages the decode thread and
//! position reporting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use super::decoder::AudioDecoder;
use super::output::{AudioOutput, AudioOutputController};
use super::preload::{PreloadManager, PreloadedTrack};
use super::state::{AudioMeta, PlaybackSnapshot, PlaybackState, PlaybackStatus};

#[derive(Clone)]
struct PlaybackSource {
    url: String,
    format: Option<String>,
    headers: HashMap<String, String>,
}

#[derive(Clone)]
struct ActivePlayback {
    stop: Arc<AtomicBool>,
    reporter_stop: Arc<AtomicBool>,
    output: Arc<Mutex<Option<AudioOutputController>>>,
}

struct OutputHandle {
    output: AudioOutput,
}

unsafe impl Send for OutputHandle {}

struct PlaybackError {
    code: &'static str,
    message: String,
    recoverable: bool,
    phase: &'static str,
}

impl PlaybackError {
    fn new(
        code: &'static str,
        message: impl Into<String>,
        recoverable: bool,
        phase: &'static str,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            recoverable,
            phase,
        }
    }
}

/// The main audio player. Stored as Tauri managed state.
pub struct AudioPlayer {
    state: PlaybackState,
    preload: PreloadManager,
    active: Mutex<Option<ActivePlayback>>,
    source: Mutex<Option<PlaybackSource>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::new(),
            preload: PreloadManager::new(),
            active: Mutex::new(None),
            source: Mutex::new(None),
        }
    }

    /// Start playback of a track.
    pub fn play(
        &self,
        app: &AppHandle,
        url: String,
        format: Option<String>,
        headers: Option<HashMap<String, String>>,
        autoplay: bool,
    ) -> Result<(), String> {
        let source = PlaybackSource {
            url,
            format,
            headers: headers.unwrap_or_default(),
        };
        self.start_playback(app, source, autoplay, 0.0)
    }

    fn start_playback(
        &self,
        app: &AppHandle,
        source: PlaybackSource,
        autoplay: bool,
        start_position_secs: f64,
    ) -> Result<(), String> {
        self.stop_internal();

        self.state.clear_track();
        self.state.set_status(PlaybackStatus::Loading);
        self.state.set_current_url(Some(source.url.clone()));
        self.state.set_position_secs(start_position_secs.max(0.0));
        Self::emit_state(app, &self.state);

        *self.source.lock().unwrap() = Some(source.clone());

        let session = ActivePlayback {
            stop: Arc::new(AtomicBool::new(false)),
            reporter_stop: Arc::new(AtomicBool::new(false)),
            output: Arc::new(Mutex::new(None)),
        };
        *self.active.lock().unwrap() = Some(session.clone());

        let preloaded = if start_position_secs <= f64::EPSILON {
            self.preload.take_if_matches(&source.url)
        } else {
            None
        };

        let state = self.state.clone();
        let app_handle = app.clone();
        thread::Builder::new()
            .name("audio-decode".to_string())
            .spawn(move || {
                let result = match preloaded {
                    Some(preloaded) => {
                        Self::play_preloaded(preloaded, &state, &app_handle, &session, autoplay)
                    }
                    None => Self::play_from_source(
                        source,
                        &state,
                        &app_handle,
                        &session,
                        autoplay,
                        start_position_secs,
                    ),
                };

                if let Err(err) = result {
                    if !session.stop.load(Ordering::Relaxed) {
                        log::error!("Playback error ({}): {}", err.code, err.message);
                        state.set_status(PlaybackStatus::Error);
                        Self::emit_state(&app_handle, &state);
                        Self::emit_buffering(&app_handle, false, Some("error"));
                        let _ = app_handle.emit(
                            "native-audio-error",
                            serde_json::json!({
                                "code": err.code,
                                "message": err.message,
                                "recoverable": err.recoverable,
                                "phase": err.phase,
                            }),
                        );
                    }
                    session.reporter_stop.store(true, Ordering::Relaxed);
                    *session.output.lock().unwrap() = None;
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn play_from_source(
        source: PlaybackSource,
        state: &PlaybackState,
        app: &AppHandle,
        session: &ActivePlayback,
        autoplay: bool,
        start_position_secs: f64,
    ) -> Result<(), PlaybackError> {
        let mut decoder = AudioDecoder::from_url(
            &source.url,
            source.headers.clone(),
            source.format.as_deref(),
        )
        .map_err(|e| map_decoder_error(e.to_string(), "load"))?;

        if start_position_secs > 0.0 {
            decoder
                .seek(start_position_secs)
                .map_err(|e| PlaybackError::new("seek_error", e.to_string(), false, "load"))?;
        }

        let info = decoder.track_info().clone();
        let (output, controller, mut writer) =
            AudioOutput::open(info.sample_rate, info.channels, state.exclusive_mode())
                .map_err(map_output_error)?;

        Self::activate_output(session, &controller);
        Self::emit_loaded(app, state, &info, autoplay, &controller);
        Self::spawn_reporter(app, state, session, controller.clone(), info.duration_secs);

        let output = OutputHandle { output };
        loop {
            if session.stop.load(Ordering::Relaxed) {
                break;
            }

            match decoder.decode_next() {
                Ok(Some(samples)) => {
                    if !writer.write_interruptible(&samples, &session.stop) {
                        break;
                    }
                }
                Ok(None) => {
                    Self::wait_for_completion(
                        session,
                        &controller,
                        info.duration_secs,
                        Some(Duration::from_millis(500)),
                    );
                    break;
                }
                Err(e) => {
                    return Err(PlaybackError::new(
                        "decode_error",
                        e.to_string(),
                        false,
                        "play",
                    ));
                }
            }
        }

        Self::finish_playback(app, state, session, output, info.duration_secs);
        Ok(())
    }

    fn play_preloaded(
        preloaded: PreloadedTrack,
        state: &PlaybackState,
        app: &AppHandle,
        session: &ActivePlayback,
        autoplay: bool,
    ) -> Result<(), PlaybackError> {
        let info = preloaded.track_info.clone();
        let expected_duration =
            preloaded.samples.len() as f64 / (info.sample_rate as f64 * info.channels as f64);

        let (output, controller, mut writer) =
            AudioOutput::open(info.sample_rate, info.channels, state.exclusive_mode())
                .map_err(map_output_error)?;

        Self::activate_output(session, &controller);
        Self::emit_loaded(app, state, &info, autoplay, &controller);
        Self::spawn_reporter(app, state, session, controller.clone(), expected_duration);

        if !writer.write_interruptible(&preloaded.samples, &session.stop) {
            session.reporter_stop.store(true, Ordering::Relaxed);
            *session.output.lock().unwrap() = None;
            return Ok(());
        }

        let output = OutputHandle { output };
        Self::wait_for_completion(
            session,
            &controller,
            expected_duration,
            Some(Duration::from_secs_f64(expected_duration + 0.5)),
        );
        Self::finish_playback(app, state, session, output, expected_duration);
        Ok(())
    }

    fn activate_output(session: &ActivePlayback, controller: &AudioOutputController) {
        controller.reset_position();
        *session.output.lock().unwrap() = Some(controller.clone());
    }

    fn emit_loaded(
        app: &AppHandle,
        state: &PlaybackState,
        info: &super::decoder::TrackInfo,
        autoplay: bool,
        controller: &AudioOutputController,
    ) {
        state.set_meta(AudioMeta {
            sample_rate: info.sample_rate,
            bit_depth: info.bit_depth,
            channels: info.channels,
            format: info.format.clone(),
            duration_secs: info.duration_secs,
        });

        controller.set_volume(state.volume());
        if autoplay {
            controller.resume();
            state.set_status(PlaybackStatus::Playing);
        } else {
            controller.pause();
            state.set_status(PlaybackStatus::Paused);
        }

        let _ = app.emit(
            "native-audio-loaded",
            serde_json::json!({
                "duration_secs": info.duration_secs,
                "sample_rate": info.sample_rate,
                "bit_depth": info.bit_depth,
                "channels": info.channels,
                "format": info.format,
            }),
        );
        Self::emit_state(app, state);
        Self::emit_buffering(app, false, None);
    }

    fn spawn_reporter(
        app: &AppHandle,
        state: &PlaybackState,
        session: &ActivePlayback,
        controller: AudioOutputController,
        duration_secs: f64,
    ) {
        let app = app.clone();
        let state = state.clone();
        let reporter_stop = session.reporter_stop.clone();

        thread::Builder::new()
            .name("audio-position".to_string())
            .spawn(move || {
                let mut last_buffering = false;
                while !reporter_stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(250));

                    let status = state.status();
                    let raw_position_secs = controller.position_secs();
                    let position_secs = if duration_secs > 0.0 {
                        raw_position_secs.min(duration_secs)
                    } else {
                        raw_position_secs
                    };
                    state.set_position_secs(position_secs);

                    if matches!(status, PlaybackStatus::Playing | PlaybackStatus::Paused) {
                        let _ = app.emit(
                            "native-audio-timeupdate",
                            serde_json::json!({
                                "position_secs": position_secs,
                                "duration_secs": duration_secs,
                            }),
                        );
                    }

                    let buffering = status == PlaybackStatus::Playing && controller.is_buffering();
                    if buffering != last_buffering {
                        last_buffering = buffering;
                        Self::emit_buffering(&app, buffering, None);
                    }
                }
            })
            .ok();
    }

    fn wait_for_completion(
        session: &ActivePlayback,
        controller: &AudioOutputController,
        duration_secs: f64,
        fallback_timeout: Option<Duration>,
    ) {
        let start = Instant::now();
        loop {
            if session.stop.load(Ordering::Relaxed) {
                break;
            }

            if duration_secs > 0.0 && controller.position_secs() >= (duration_secs - 0.05).max(0.0)
            {
                break;
            }

            if let Some(timeout) = fallback_timeout {
                if duration_secs <= 0.0 && start.elapsed() >= timeout {
                    break;
                }
                if duration_secs > 0.0 && start.elapsed() >= timeout && controller.is_buffering() {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    fn finish_playback(
        app: &AppHandle,
        state: &PlaybackState,
        session: &ActivePlayback,
        output: OutputHandle,
        duration_secs: f64,
    ) {
        session.reporter_stop.store(true, Ordering::Relaxed);
        *session.output.lock().unwrap() = None;

        if !session.stop.load(Ordering::Relaxed) {
            state.set_position_secs(duration_secs.max(0.0));
            state.set_status(PlaybackStatus::Stopped);
            Self::emit_state(app, state);
            Self::emit_buffering(app, false, None);
            let _ = app.emit("native-audio-ended", serde_json::json!({}));
        }

        drop(output.output);
    }

    fn emit_state(app: &AppHandle, state: &PlaybackState) {
        let _ = app.emit("native-audio-state", state.snapshot());
    }

    fn emit_buffering(app: &AppHandle, is_buffering: bool, reason: Option<&str>) {
        let _ = app.emit(
            "native-audio-buffering",
            serde_json::json!({
                "is_buffering": is_buffering,
                "reason": reason,
            }),
        );
    }

    /// Pause playback.
    pub fn pause(&self, app: &AppHandle) {
        if let Some(active) = self.active.lock().unwrap().clone() {
            if let Some(output) = active.output.lock().unwrap().clone() {
                output.pause();
            }
        }
        self.state.set_status(PlaybackStatus::Paused);
        Self::emit_state(app, &self.state);
        Self::emit_buffering(app, false, Some("paused"));
    }

    /// Resume playback.
    pub fn resume(&self, app: &AppHandle) {
        if let Some(active) = self.active.lock().unwrap().clone() {
            if let Some(output) = active.output.lock().unwrap().clone() {
                output.resume();
            }
        }
        self.state.set_status(PlaybackStatus::Playing);
        Self::emit_state(app, &self.state);
    }

    /// Stop playback.
    pub fn stop(&self, app: &AppHandle) {
        self.stop_internal();
        self.state.set_status(PlaybackStatus::Stopped);
        self.state.set_current_url(None);
        self.state.set_position_secs(0.0);
        *self.source.lock().unwrap() = None;
        Self::emit_state(app, &self.state);
        Self::emit_buffering(app, false, Some("stopped"));
    }

    fn stop_internal(&self) {
        if let Some(active) = self.active.lock().unwrap().take() {
            active.stop.store(true, Ordering::SeqCst);
            active.reporter_stop.store(true, Ordering::SeqCst);
            if let Some(output) = active.output.lock().unwrap().clone() {
                output.pause();
            }
        }
    }

    /// Seek to position by restarting the current source from the requested offset.
    pub fn seek(&self, app: &AppHandle, position_secs: f64) -> Result<(), String> {
        let source = self
            .source
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "No active track to seek".to_string())?;
        let duration_secs = self.state.meta().duration_secs;
        let target = if duration_secs > 0.0 {
            position_secs.clamp(0.0, duration_secs)
        } else {
            position_secs.max(0.0)
        };
        let autoplay = self.state.status() == PlaybackStatus::Playing;
        self.start_playback(app, source, autoplay, target)
    }

    /// Set volume.
    pub fn set_volume(&self, app: &AppHandle, level: f32) {
        self.state.set_volume(level.clamp(0.0, 1.0));
        if let Some(active) = self.active.lock().unwrap().clone() {
            if let Some(output) = active.output.lock().unwrap().clone() {
                output.set_volume(level);
            }
        }
        Self::emit_state(app, &self.state);
    }

    /// Get state snapshot.
    pub fn get_state(&self) -> PlaybackSnapshot {
        self.state.snapshot()
    }

    /// Preload a track.
    pub fn preload(
        &self,
        url: String,
        format: Option<String>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<(), String> {
        let preload = self.preload.clone();
        let headers = headers.unwrap_or_default();

        thread::Builder::new()
            .name("audio-preload".to_string())
            .spawn(move || {
                if let Err(e) = preload.preload(&url, format.as_deref(), headers) {
                    log::warn!("Preload failed: {e}");
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Set exclusive mode.
    pub fn set_exclusive(&self, app: &AppHandle, enabled: bool) -> Result<(), String> {
        let was_active = matches!(
            self.state.status(),
            PlaybackStatus::Playing | PlaybackStatus::Paused
        );
        let position_secs = self.current_position_secs();
        let autoplay = self.state.status() == PlaybackStatus::Playing;
        let source = self.source.lock().unwrap().clone();

        self.state.set_exclusive_mode(enabled);
        Self::emit_state(app, &self.state);

        if was_active {
            if let Some(source) = source {
                self.start_playback(app, source, autoplay, position_secs)?;
            }
        }

        Ok(())
    }

    pub fn restore_exclusive(&self, enabled: bool) {
        self.state.set_exclusive_mode(enabled);
    }

    fn current_position_secs(&self) -> f64 {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|active| active.output.lock().unwrap().clone())
            .map(|output| output.position_secs())
            .unwrap_or_else(|| self.state.position_secs())
    }
}

fn map_decoder_error(message: String, phase: &'static str) -> PlaybackError {
    let code = if message.starts_with("HTTP fetch failed:") {
        "fetch_failed"
    } else if message.starts_with("Unsupported format:") || message == "No audio track found" {
        "format_unsupported"
    } else {
        "decode_error"
    };
    let recoverable = code == "fetch_failed";
    PlaybackError::new(code, message, recoverable, phase)
}

fn map_output_error(err: super::output::OutputError) -> PlaybackError {
    let recoverable = matches!(
        err,
        super::output::OutputError::NoDevice(_)
            | super::output::OutputError::DeviceError(_)
            | super::output::OutputError::StreamError(_)
            | super::output::OutputError::ExclusiveDenied(_)
    );
    let code = match err {
        super::output::OutputError::ExclusiveDenied(_) => "exclusive_denied",
        _ => "output_error",
    };
    PlaybackError::new(code, err.to_string(), recoverable, "load")
}
