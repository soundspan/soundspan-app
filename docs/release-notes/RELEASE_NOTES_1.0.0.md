# soundspan-app 1.0.0 Release Notes

## Release Summary

Initial release of the soundspan native desktop and mobile app. A Tauri 2 shell that connects to your self-hosted soundspan instance and delivers hi-res audio playback at native sample rates on all platforms.

## Highlights

- Tauri 2 native shell that loads a self-hosted soundspan instance in a platform webview.
- Rust audio backend for hi-res playback on Windows (WASAPI) and Android (AAudio), bypassing Chromium's 48 kHz audio mixer cap.
- Symphonia-based decoder supporting FLAC, WAV, MP3, AAC, Vorbis, and Opus via HTTP streaming.
- Lock-free SPSC ring buffer audio output with f32/i16 stream support and automatic channel mapping.
- WASAPI exclusive mode option on Windows for bit-perfect output at the source sample rate.
- Gapless track preloading — pre-decodes the next track into memory for instant transitions.
- Instance URL configuration screen with soundspan branding.
- IPC security model restricting native audio commands to the configured instance origin.

## Install

- **Linux**: Download `.AppImage`, `.deb`, or `.rpm` from the [Releases page](https://github.com/soundspan/soundspan-app/releases/tag/v1.0.0)
- **macOS**: Download `.dmg` (universal binary)
- **Windows**: Download `.exe` (NSIS installer)
- **Android**: Download `.apk`
- **Arch Linux**: `yay -S soundspan-bin`

## Known Issues

None at this time.

## Full Changelog

- Full changelog: https://github.com/soundspan/soundspan-app/blob/main/CHANGELOG.md
