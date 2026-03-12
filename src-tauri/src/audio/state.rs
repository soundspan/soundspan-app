//! Shared playback state for the native audio backend.

use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Current playback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
    Error,
}

/// Audio format metadata for the current track.
#[derive(Debug, Clone, Serialize)]
pub struct AudioMeta {
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub channels: u16,
    pub format: String,
    pub duration_secs: f64,
}

impl Default for AudioMeta {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            bit_depth: 0,
            channels: 0,
            format: String::new(),
            duration_secs: 0.0,
        }
    }
}

/// Thread-safe playback state shared between IPC handlers, the decode thread,
/// and the position-reporting timer.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    inner: Arc<Mutex<PlaybackStateInner>>,
}

#[derive(Debug)]
struct PlaybackStateInner {
    pub status: PlaybackStatus,
    pub position_secs: f64,
    pub meta: AudioMeta,
    pub exclusive_mode: bool,
    pub current_url: Option<String>,
    pub volume: f32,
}

impl PlaybackState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlaybackStateInner {
                status: PlaybackStatus::Idle,
                position_secs: 0.0,
                meta: AudioMeta::default(),
                exclusive_mode: false,
                current_url: None,
                volume: 1.0,
            })),
        }
    }

    pub fn status(&self) -> PlaybackStatus {
        self.inner.lock().unwrap().status
    }

    pub fn set_status(&self, status: PlaybackStatus) {
        self.inner.lock().unwrap().status = status;
    }

    pub fn position_secs(&self) -> f64 {
        self.inner.lock().unwrap().position_secs
    }

    pub fn set_position_secs(&self, pos: f64) {
        self.inner.lock().unwrap().position_secs = pos;
    }

    pub fn meta(&self) -> AudioMeta {
        self.inner.lock().unwrap().meta.clone()
    }

    pub fn set_meta(&self, meta: AudioMeta) {
        self.inner.lock().unwrap().meta = meta;
    }

    pub fn exclusive_mode(&self) -> bool {
        self.inner.lock().unwrap().exclusive_mode
    }

    pub fn set_exclusive_mode(&self, enabled: bool) {
        self.inner.lock().unwrap().exclusive_mode = enabled;
    }

    pub fn current_url(&self) -> Option<String> {
        self.inner.lock().unwrap().current_url.clone()
    }

    pub fn set_current_url(&self, url: Option<String>) {
        self.inner.lock().unwrap().current_url = url;
    }

    pub fn volume(&self) -> f32 {
        self.inner.lock().unwrap().volume
    }

    pub fn set_volume(&self, vol: f32) {
        self.inner.lock().unwrap().volume = vol;
    }

    /// Snapshot for serialization to the frontend.
    pub fn snapshot(&self) -> PlaybackSnapshot {
        let inner = self.inner.lock().unwrap();
        PlaybackSnapshot {
            status: inner.status,
            position_secs: inner.position_secs,
            duration_secs: inner.meta.duration_secs,
            sample_rate: inner.meta.sample_rate,
            bit_depth: inner.meta.bit_depth,
            channels: inner.meta.channels,
            format: inner.meta.format.clone(),
            exclusive_mode: inner.exclusive_mode,
        }
    }
}

/// Serializable snapshot of playback state for IPC responses.
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackSnapshot {
    pub status: PlaybackStatus,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub channels: u16,
    pub format: String,
    pub exclusive_mode: bool,
}
