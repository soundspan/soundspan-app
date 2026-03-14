# Changelog

All notable changes to soundspan-app are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

- ACM cross-review now exposes `--sandbox` in `scripts/acm-cross-review.sh`, and the active sandbox mode is pinned from `.acm/acm-workflows.yaml` so operators can tune it per host runtime.

### Removed

## [1.0.1]

### Changed

- Extended the local setup-command gate to also allow the configured Tauri `devUrl` origin during development while keeping remote-instance IPC restricted to the configured soundspan origin.
- Corrected release metadata and package versioning so Cargo, Tauri bundle config, AUR packaging, and shipped release references align on `1.0.1`.

### Fixed

- Fixed the Windows setup-shell connection flow by treating Tauri-hosted WebView2 content as local app content during initial instance bootstrap.
- Improved setup-shell error reporting so failed instance bootstrap attempts surface the underlying connection or permission error instead of only a generic connect failure.

## [1.0.0]

### Added

- Tauri 2 native shell that loads a self-hosted soundspan instance in a platform webview.
- Rust audio backend for hi-res playback on Windows (WASAPI) and Android (AAudio), bypassing Chromium's 48 kHz audio mixer cap.
- Symphonia-based decoder supporting FLAC, WAV, MP3, AAC, Vorbis, and Opus via HTTP streaming.
- Lock-free SPSC ring buffer audio output with f32/i16 stream support and automatic channel mapping.
- WASAPI exclusive mode option on Windows for bit-perfect output at the source sample rate.
- Gapless track preloading — pre-decodes the next track into memory for instant transitions.
- Instance URL configuration screen with soundspan branding.
- IPC security model restricting native audio commands to the configured instance origin.
- Desktop builds for Linux (AppImage, .deb, .rpm), macOS (.dmg universal binary), and Windows (NSIS installer).
- Mobile builds for Android (.apk) and iOS (.ipa).
- AUR packaging scaffold for Arch Linux distribution via `soundspan-bin`.
