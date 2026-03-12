//! Preload buffer management for gapless track transitions.
//!
//! Holds one pre-decoded track in memory. When the next track matches the
//! preloaded URL, playback starts instantly from the buffer instead of
//! fetching and decoding from scratch.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::decoder::{AudioDecoder, DecoderError, TrackInfo};

/// A fully decoded track held in memory for instant playback.
pub struct PreloadedTrack {
    pub url: String,
    pub samples: Vec<f32>,
    pub track_info: TrackInfo,
}

/// Manages a single preloaded track buffer.
#[derive(Clone)]
pub struct PreloadManager {
    inner: Arc<Mutex<Option<PreloadedTrack>>>,
}

impl PreloadManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Preload a track by fetching and fully decoding it into memory.
    ///
    /// Replaces any previously preloaded track.
    pub fn preload(
        &self,
        url: &str,
        headers: HashMap<String, String>,
    ) -> Result<(), DecoderError> {
        let mut decoder = AudioDecoder::from_url(url, headers)?;
        let track_info = decoder.track_info().clone();

        let mut all_samples = Vec::new();
        while let Some(samples) = decoder.decode_next()? {
            all_samples.extend_from_slice(&samples);
        }

        let mut guard = self.inner.lock().unwrap();
        *guard = Some(PreloadedTrack {
            url: url.to_string(),
            samples: all_samples,
            track_info,
        });

        Ok(())
    }

    /// Take the preloaded track if it matches the given URL.
    ///
    /// Consumes the preload — subsequent calls return `None` until a new
    /// track is preloaded.
    pub fn take_if_matches(&self, url: &str) -> Option<PreloadedTrack> {
        let mut guard = self.inner.lock().unwrap();
        if guard.as_ref().map(|t| t.url.as_str()) == Some(url) {
            guard.take()
        } else {
            None
        }
    }

    /// Discard any preloaded track.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().unwrap();
        *guard = None;
    }

    /// Check if a track is preloaded for the given URL.
    pub fn has(&self, url: &str) -> bool {
        let guard = self.inner.lock().unwrap();
        guard.as_ref().map(|t| t.url.as_str()) == Some(url)
    }
}
