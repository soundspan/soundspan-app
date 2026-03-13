# Changelog

All notable changes to soundspan-app are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0]

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
