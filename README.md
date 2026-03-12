# soundspan-app

Native desktop and mobile app for [soundspan](https://github.com/soundspan/soundspan), built with [Tauri 2](https://v2.tauri.app).

Delivers hi-res audio playback at native sample rates on all platforms by bypassing Chromium's 48kHz audio mixer cap on Windows and Android through a Rust audio backend.

## How It Works

The app is a thin native shell that loads your self-hosted soundspan instance in a webview. On platforms where the webview uses WebKit (Linux, macOS, iOS), audio plays through the native audio stack at full sample rate with no changes needed. On Chromium-based webview platforms (Windows, Android), audio is intercepted and routed through a Rust backend that decodes and outputs audio directly via platform APIs (WASAPI, AAudio).

| Platform | Webview Engine    | Audio Path                  | Native Sample Rate |
|----------|-------------------|-----------------------------|--------------------|
| Linux    | WebKitGTK         | Howler.js → WebKit → GStreamer | Yes             |
| macOS    | WebKit            | Howler.js → WebKit → CoreAudio | Yes             |
| iOS      | WKWebView         | Howler.js → WebKit → CoreAudio | Yes             |
| Windows  | WebView2 (Chromium)| Rust → symphonia → cpal → WASAPI | Yes           |
| Android  | Android WebView   | Rust → symphonia → cpal → AAudio | Yes            |

## Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- Platform-specific dependencies:
  - **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`
  - **macOS**: Xcode Command Line Tools
  - **Windows**: WebView2 (pre-installed on Windows 10/11), Visual Studio Build Tools

## Development

```bash
cargo install tauri-cli --version "^2"
cargo tauri dev
```

On first launch, enter your soundspan instance URL (e.g., `https://listen.example.com`).

## Building

```bash
cargo tauri build
```

Platform-specific installers are output to `src-tauri/target/release/bundle/`.

## License

GPL-3.0 — see [LICENSE](LICENSE).
