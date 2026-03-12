# Tauri 2 Native App — Spec Outline

**ACM Plan**: `plan:receipt-672c713169d81cb4572f3eed`
**Stage**: `spec_outline`
**Status**: in_progress

---

## 1. Platform / Webview / Audio Matrix

| Platform | Tauri Webview Engine            | Audio API (via webview)     | Native SR? | Native Backend Needed? |
|----------|---------------------------------|-----------------------------|------------|------------------------|
| Linux    | WebKitGTK (GStreamer → PipeWire)| Web Audio / HTMLMediaElement | Yes        | No                     |
| macOS    | WebKit (CoreAudio)              | Web Audio / HTMLMediaElement | Yes        | No                     |
| iOS      | WKWebView (CoreAudio)           | Web Audio / HTMLMediaElement | Yes        | No                     |
| Windows  | WebView2 (Chromium)             | Chromium mixer (48kHz cap)  | No         | **Yes**                |
| Android  | Android WebView (Chromium)      | Chromium mixer (48kHz cap)  | No         | **Yes**                |

**Key insight**: Chromium hardcodes its internal audio mixer to 48kHz regardless of system audio configuration or source file sample rate. This affects WebView2 (Windows) and Android WebView. WebKit-based webviews (Linux, macOS, iOS) respect the native audio pipeline and output at source sample rate.

On Windows and Android, even when the system audio output is configured to 384kHz/32-bit, audio routed through the Chromium webview will be downsampled to 48kHz before reaching the system audio stack. The native Rust backend bypasses this entirely by using platform audio APIs (WASAPI, AAudio) directly.

## 2. Two-Path Audio Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Tauri 2 App Shell                        │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Remote Soundspan Webapp                   │  │
│  │         (loaded from user's instance URL)              │  │
│  │                                                        │  │
│  │  AudioPlaybackOrchestrator                             │  │
│  │       │                                                │  │
│  │       ├─ WebKit platforms ──→ HowlerEngineAdapter      │  │
│  │       │   (Linux/macOS/iOS)    │                       │  │
│  │       │                        └──→ Howler.js          │  │
│  │       │                             └──→ WebKit Audio  │  │
│  │       │                                  (native SR)   │  │
│  │       │                                                │  │
│  │       └─ Chromium platforms ─→ TauriNativeEngineAdapter│  │
│  │           (Windows/Android)    │                       │  │
│  │                                └──→ Tauri IPC Bridge   │  │
│  └────────────────────────────────────┼───────────────────┘  │
│                                       │                      │
│  ┌────────────────────────────────────▼───────────────────┐  │
│  │           Rust Native Audio Backend                    │  │
│  │                                                        │  │
│  │  URL Fetch ──→ symphonia (decode) ──→ cpal (output)    │  │
│  │                                        │               │  │
│  │                              ┌─────────┴─────────┐     │  │
│  │                              │                   │     │  │
│  │                         WASAPI (Win)       AAudio (And)│  │
│  │                         shared/exclusive               │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Runtime Path Selection

```
function selectAudioEngine():
  if NOT window.__TAURI__:
    return HowlerEngineAdapter          # standard web browser

  platform = await Tauri.os.platform()
  if platform in ['windows', 'android']:
    return TauriNativeEngineAdapter     # bypass Chromium 48kHz cap
  else:
    return HowlerEngineAdapter          # WebKit handles native SR
```

### Data Flow — WebKit Path (no changes)

```
Audio URL → Howler.js → HTMLMediaElement / Web Audio API → WebKit → CoreAudio/GStreamer → DAC
```

### Data Flow — Native Path (Windows/Android)

```
Audio URL → Tauri IPC → Rust: HTTP fetch → symphonia decode → PCM buffer → cpal → WASAPI/AAudio → DAC
                                                                              ↑
                                                                    source sample rate preserved
```

## 3. IPC Command Contract

All commands are invoked via `window.__TAURI__.core.invoke()` and return JSON-serializable responses.

### Commands

| Command            | Input                                          | Output                        | Description                                    |
|--------------------|------------------------------------------------|-------------------------------|------------------------------------------------|
| `native_audio_play`| `{ url: string, format?: string, headers?: Record<string, string>, autoplay?: boolean }` | `{ ok: boolean }` | Fetch, decode, and begin playback |
| `native_audio_pause` | `{}`                                         | `{ ok: boolean }`             | Pause current playback                         |
| `native_audio_resume` | `{}`                                       | `{ ok: boolean }`             | Resume from pause                              |
| `native_audio_stop`| `{}`                                           | `{ ok: boolean }`             | Stop and release resources                     |
| `native_audio_seek`| `{ position_secs: number }`                    | `{ ok: boolean }`             | Seek to position in seconds                    |
| `native_audio_set_volume` | `{ level: number }`                     | `{ ok: boolean }`             | Set volume (0.0–1.0)                           |
| `native_audio_get_state` | `{}`                                    | `NativeAudioState`            | Query current playback state                   |
| `native_audio_preload` | `{ url: string, format?: string, headers?: Record<string, string> }` | `{ ok: boolean }` | Preload next track for gapless transition |
| `native_audio_set_exclusive` | `{ enabled: boolean }`              | `{ ok: boolean, error?: string }` | Toggle WASAPI exclusive mode (Windows only) |

### NativeAudioState

```typescript
interface NativeAudioState {
  status: 'idle' | 'loading' | 'playing' | 'paused' | 'stopped' | 'error';
  position_secs: number;
  duration_secs: number;
  sample_rate: number;       // e.g., 96000, 192000
  bit_depth: number;         // e.g., 16, 24, 32
  channels: number;          // e.g., 2
  format: string;            // e.g., "flac", "mp3"
  exclusive_mode: boolean;   // WASAPI exclusive active?
}
```

### Events (Rust → Frontend via Tauri event system)

| Event                    | Payload                                   | Frequency        |
|--------------------------|-------------------------------------------|------------------|
| `native-audio-timeupdate`| `{ position_secs: number }`               | Every 250ms      |
| `native-audio-loaded`    | `{ duration_secs, sample_rate, bit_depth, channels, format }` | Once per load |
| `native-audio-ended`     | `{}`                                      | Once per track   |
| `native-audio-error`     | `{ code: string, message: string, recoverable: boolean }` | On error |
| `native-audio-state`     | `NativeAudioState`                        | On state change  |
| `native-audio-buffering` | `{ is_buffering: boolean }`               | On buffer state  |

### Error Types

| Code               | Description                                    | Recoverable? |
|--------------------|------------------------------------------------|--------------|
| `fetch_failed`     | HTTP request for audio URL failed              | Yes (retry)  |
| `decode_error`     | symphonia could not decode the audio stream    | No           |
| `output_error`     | cpal could not open or write to audio device   | Yes (retry)  |
| `format_unsupported` | Audio format not supported by symphonia      | No           |
| `device_lost`      | Audio output device disconnected               | Yes (retry)  |
| `exclusive_denied` | WASAPI exclusive mode denied by system/another app | Yes (fallback to shared) |

## 4. Instance URL Configuration

### First-Run Flow

1. App launches → detects no saved instance URL
2. Shows a minimal setup screen:
   - Text input: "Enter your soundspan instance URL"
   - Placeholder: `https://listen.example.com`
   - Validation: HTTP GET to `{url}/api/health` (backend health endpoint)
3. On success → save URL, load webapp in webview
4. On failure → show error, allow retry

### Persistence

- **Mechanism**: Tauri `tauri-plugin-store` (JSON key-value store, persisted to app data directory)
- **Key**: `instance_url`
- **Location**: OS-standard app data path (e.g., `~/.local/share/com.soundspan.app/` on Linux)

### URL Validation Requirements

- Must be a valid HTTPS or HTTP URL (HTTP allowed for local network instances)
- Must respond to `GET /api/health` with 200 status
- Trailing slashes stripped before saving
- Stored as the base URL only (no path)

### Changing Instance URL

- Accessible from the app's native menu or settings
- Changing URL clears webview cache and reloads

## 5. WASAPI Exclusive Mode Toggle

### Behavior

- **Default**: Off (WASAPI shared mode)
- **Location**: Settings screen, visible only when `platform === 'windows'`
- **Label**: "Exclusive Audio Mode"
- **Description shown to user**: "Takes exclusive control of your audio output device for bit-perfect playback. Other applications will not be able to produce sound while this is active."

### Toggle Behavior

| Action                | Effect                                                       |
|-----------------------|--------------------------------------------------------------|
| Enable while playing  | Current playback stops → device reopened in exclusive mode → playback resumes from same position |
| Enable while paused   | Device mode switches silently, takes effect on next play     |
| Disable while playing | Current playback stops → device reopened in shared mode → playback resumes from same position |
| Enable fails          | Falls back to shared mode, shows toast: "Exclusive mode unavailable — another application may have the audio device" |

### Interaction with System Audio

- **Shared mode**: Audio goes through Windows Audio Session mixer. Output sample rate = whatever Windows Sound settings specify (e.g., user's 384kHz/32-bit setting). No other apps affected.
- **Exclusive mode**: App bypasses Windows mixer entirely. Output sample rate = source file's native rate (e.g., 96kHz for a 96kHz FLAC). All other system audio is muted until exclusive mode is released.

### Persistence

- Setting persisted via `tauri-plugin-store` under key `wasapi_exclusive_enabled`
- Applied on next playback start (not retroactively on app launch)
