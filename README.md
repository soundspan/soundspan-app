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

## Install

Download the latest release for your platform from the [Releases page](https://github.com/soundspan/soundspan-app/releases/latest).

| Platform | Format | Notes |
|----------|--------|-------|
| Linux    | `.AppImage`, `.deb`, `.rpm` | AppImage is portable — just `chmod +x` and run |
| macOS    | `.dmg` | Universal binary (Intel + Apple Silicon) |
| Windows  | `.exe` (NSIS installer) | WebView2 is included on Windows 10/11 |
| Android  | `.apk` | Sideload or install from release assets |

### Arch Linux

A `soundspan-bin` package is available on the [AUR](https://aur.archlinux.org/packages/soundspan-bin):

```bash
yay -S soundspan-bin
```

### First launch

On first launch you'll be prompted to enter your soundspan instance URL (e.g., `https://listen.example.com`). The app connects to your self-hosted instance and loads it in a native webview.

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- Platform-specific dependencies:
  - **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`
  - **macOS**: Xcode Command Line Tools
  - **Windows**: WebView2 (pre-installed on Windows 10/11), Visual Studio Build Tools

### Development

```bash
cargo install tauri-cli --version "^2"
cargo tauri dev
```

### Building

```bash
cargo tauri build
```

Platform-specific installers are output to `src-tauri/target/release/bundle/`.

Arch Linux packaging is provided via the [`aur/`](./aur) scaffold, which consumes the released `.deb` artifacts for `x86_64` and `aarch64`.

## License

GPL-3.0 — see [LICENSE](LICENSE).
