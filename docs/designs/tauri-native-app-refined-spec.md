# Tauri 2 Native App — Refined Spec

**ACM Plan**: `plan:receipt-672c713169d81cb4572f3eed`
**Stage**: `refined_spec`
**Depends on**: spec_outline

---

## 1. TauriNativeEngineAdapter Design

The adapter implements the existing `AudioEngine` interface from `frontend/lib/audio-engine/types.ts`, making it a drop-in replacement for `HowlerEngineAdapter` on Chromium-based Tauri platforms.

### Interface Mapping

```typescript
// frontend/lib/audio-engine/tauriNativeEngineAdapter.ts

import type {
  AudioEngine,
  AudioEngineSource,
  AudioEngineLoadOptions,
  AudioEngineEventType,
  AudioEngineEventHandler,
} from "@/lib/audio-engine/types";

export class TauriNativeEngineAdapter implements AudioEngine {
  // --- State ---
  private currentPosition: number = 0;
  private currentDuration: number = 0;
  private playing: boolean = false;
  private seeking: boolean = false;
  private seekTarget: number | null = null;
  private seekTimeoutId: ReturnType<typeof setTimeout> | null = null;
  private listeners: Map<AudioEngineEventType, Set<Function>> = new Map();
  private tauriUnlisteners: Array<() => void> = [];

  // --- AudioEngine methods → IPC mapping ---

  async load(source: AudioEngineSource | string, options?: AudioEngineLoadOptions): Promise<void> {
    // 1. Resolve source to { url, mimeType, ... }
    // 2. Build headers from options.requestHeaders
    // 3. invoke('native_audio_play', { url, format, headers, autoplay: options?.autoplay })
    // 4. Wait for 'native-audio-loaded' event before emitting 'load'
  }

  async play(): Promise<void> {
    // invoke('native_audio_resume') — resume is used since load already starts playback
  }

  async pause(): Promise<void> {
    // invoke('native_audio_pause')
  }

  async stop(): Promise<void> {
    // invoke('native_audio_stop')
  }

  async seek(timeSec: number): Promise<void> {
    // Set seek lock (same pattern as HowlerEngine)
    // invoke('native_audio_seek', { position_secs: timeSec })
  }

  setVolume(value: number): void {
    // invoke('native_audio_set_volume', { level: value })
    // Fire-and-forget (no await) to match sync interface
  }

  setMuted(value: boolean): void {
    // invoke('native_audio_set_volume', { level: value ? 0 : this.lastVolume })
  }

  getCurrentTime(): number {
    return this.currentPosition;  // Updated by timeupdate events
  }

  getDuration(): number {
    return this.currentDuration;
  }

  isPlaying(): boolean {
    return this.playing;
  }

  on<T extends AudioEngineEventType>(event: T, handler: AudioEngineEventHandler<T>): void {
    // Register in local listener map
  }

  off<T extends AudioEngineEventType>(event: T, handler: AudioEngineEventHandler<T>): void {
    // Remove from local listener map
  }

  destroy(): void {
    // invoke('native_audio_stop')
    // Unlisten all Tauri events
    // Clear all local listeners
  }

  // --- Optional AudioEngine methods ---

  async preload(source: AudioEngineSource | string, options?: AudioEngineLoadOptions): Promise<void> {
    // invoke('native_audio_preload', { url, format, headers })
  }

  getActualCurrentTime(): number {
    return this.currentPosition;  // Native backend position is authoritative
  }

  hasTrackEnded(): boolean {
    return this.currentDuration > 0 && this.currentPosition >= this.currentDuration - 0.1;
  }

  isCurrentlySeeking(): boolean {
    return this.seeking;
  }

  getSeekTarget(): number | null {
    return this.seekTarget;
  }
}
```

### Event Translation Layer

Tauri events from the Rust backend are translated to `AudioEngine` events:

| Tauri Event               | AudioEngine Event | Payload Translation                                             |
|---------------------------|-------------------|-----------------------------------------------------------------|
| `native-audio-timeupdate` | `timeupdate`      | `{ position_secs }` → `{ timeSec: position_secs }`             |
| `native-audio-loaded`     | `load`            | `{ duration_secs, ... }` → `{ durationSec: duration_secs }`    |
| `native-audio-ended`      | `end`             | `{}` → `void`                                                  |
| `native-audio-error`      | `loaderror` or `playerror` | `{ code, message }` → `{ error: message, code, recoverable }` |
| `native-audio-state`      | `play` / `pause` / `stop` | Derived from `status` field transitions                  |
| `native-audio-buffering`  | `buffering`       | `{ is_buffering }` → `{ isBuffering: is_buffering }`           |

Event listeners are registered on adapter construction via `window.__TAURI__.event.listen()`, which returns unlisten functions stored for cleanup.

### Preload and Gapless Transition Strategy

1. `AudioPlaybackOrchestrator` calls `preload(nextTrackUrl)` during current track playback
2. Adapter invokes `native_audio_preload` → Rust backend fetches and decodes the next track into a memory buffer
3. When current track ends and `load()` is called for the preloaded URL, the Rust backend detects the preloaded buffer and starts playback immediately with zero gap
4. Only one preloaded track is held at a time; preloading a new URL discards any previous preload

## 2. Rust Audio Backend Module Design

### Module Structure

```
src-tauri/src/
├── main.rs                    # Tauri app entry, plugin registration
├── audio/
│   ├── mod.rs                 # Module re-exports
│   ├── commands.rs            # Tauri IPC command handlers (#[tauri::command])
│   ├── decoder.rs             # symphonia-based audio decoder
│   ├── output.rs              # cpal-based audio output
│   ├── state.rs               # Shared playback state (Arc<Mutex<PlaybackState>>)
│   ├── player.rs              # Orchestrates decoder → output pipeline
│   └── preload.rs             # Preload buffer management
└── config/
    ├── mod.rs                 # App configuration
    └── store.rs               # tauri-plugin-store integration
```

### symphonia Format Support Matrix

| Format | Codec           | symphonia Support | Notes                    |
|--------|-----------------|-------------------|--------------------------|
| FLAC   | FLAC            | Native            | Primary hi-res format    |
| WAV    | PCM             | Native            | Uncompressed             |
| MP3    | MPEG Layer 3    | Native            | Lossy, common            |
| AAC    | AAC-LC / HE-AAC | Via `symphonia-codec-aac` feature | TIDAL streams |
| OGG    | Vorbis          | Native            | Open lossy format        |
| OGG    | Opus            | Via `symphonia-codec-opus` feature | Low-latency   |
| MP4/M4A| AAC             | Via `symphonia-format-isomp4` | Container support |

### Thread Model

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  IPC Thread  │     │ Decode Thread│     │ Output Thread│
│  (Tauri)     │     │              │     │  (cpal cb)   │
│              │     │              │     │              │
│ invoke() ────┼────→│ fetch URL    │     │              │
│              │     │ symphonia    │     │              │
│              │     │ decode loop  │     │              │
│              │     │    │         │     │              │
│              │     │    ▼         │     │              │
│              │     │ ring buffer ─┼────→│ cpal callback│
│              │     │              │     │ read samples │
│              │     │              │     │ write to DAC │
│              │     │              │     │              │
│ ←────────────┼─────┼─ events ────┼─────┼──────────────│
│ emit events  │     │              │     │              │
└──────────────┘     └──────────────┘     └──────────────┘
```

- **IPC thread**: Handles Tauri command invocations, manages state, emits events to frontend
- **Decode thread**: Spawned per track. Fetches audio via HTTP (reqwest), pipes through symphonia, fills ring buffer
- **Output thread**: cpal's audio callback runs on a dedicated OS thread. Reads PCM from ring buffer, writes to audio device

### Ring Buffer Sizing

- **Buffer**: Lock-free SPSC ring buffer (e.g., `ringbuf` crate)
- **Capacity**: 2 seconds of audio at max expected config (384kHz × 32-bit × 2ch = ~6MB)
- **Underrun handling**: Output silence, emit `native-audio-buffering { is_buffering: true }`
- **Overrun handling**: Decode thread blocks until space is available

### Decoder Pipeline

```rust
// Pseudocode for decoder.rs

pub struct AudioDecoder {
    format_reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_info: TrackInfo,  // sample_rate, bit_depth, channels, format
}

impl AudioDecoder {
    pub fn from_url(url: &str, headers: HashMap<String, String>) -> Result<Self> {
        // 1. HTTP GET with streaming response (reqwest)
        // 2. Wrap response body in symphonia MediaSource
        // 3. Probe format with symphonia::probe()
        // 4. Select first audio track
        // 5. Create decoder for the track's codec
    }

    pub fn decode_next(&mut self) -> Result<Option<AudioBuffer<f32>>> {
        // Read next packet from format reader
        // Decode packet to PCM samples
        // Convert to f32 normalized samples
    }
}
```

### Output Pipeline

```rust
// Pseudocode for output.rs

pub struct AudioOutput {
    stream: cpal::Stream,
    config: StreamConfig,
    exclusive: bool,
}

impl AudioOutput {
    pub fn open(sample_rate: u32, channels: u16, exclusive: bool) -> Result<Self> {
        // 1. Get default output device
        // 2. If exclusive (Windows only):
        //    - Query supported exclusive configs
        //    - Request exclusive stream at source sample rate
        //    - Fallback to shared if denied
        // 3. If shared:
        //    - Use device's default config (respects system settings)
        //    - If source SR differs from device SR, let WASAPI/AAudio resample
        // 4. Build stream with callback that reads from ring buffer
    }
}
```

## 3. Platform-Specific Audio Backend Behavior

### Windows (WASAPI via cpal)

**Shared mode (default)**:
- Audio goes through Windows Audio Session mixer
- Output sample rate = Windows Sound Settings value (e.g., 384kHz if user configured it)
- If source sample rate differs from system setting, WASAPI resamples
- All system audio continues to work normally
- cpal host: `cpal::host::wasapi`

**Exclusive mode (opt-in)**:
- Bypasses Windows Audio Session mixer entirely
- Output sample rate = source file's native rate (e.g., 96kHz FLAC → 96kHz output)
- App gets exclusive access to audio device; all other audio silenced
- On device open failure (another app has exclusive): fall back to shared mode, report error
- cpal exclusive mode: use `cpal::traits::DeviceTrait::build_output_stream_raw()` with exclusive config
- On track change with different sample rate: stop stream → reopen at new rate → start stream (brief gap acceptable)

**Sample rate negotiation**:
1. Decode first few frames to determine source sample rate
2. Shared mode: open stream at device default rate, let WASAPI resample if needed
3. Exclusive mode: open stream at source sample rate; if device doesn't support it, try nearest supported rate; if none close, fall back to shared

### Android (AAudio via cpal)

**Default behavior**:
- cpal on Android uses AAudio (or falls back to OpenSL ES on older devices)
- AAudio respects the device's native sample rate capabilities
- No exclusive mode concept on Android
- Output at source sample rate when device supports it; AAudio handles resampling otherwise
- cpal host: `cpal::host::oboe` (via `oboe` crate which wraps AAudio)

**Considerations**:
- Android audio latency is historically higher; ring buffer may need tuning
- Background playback requires a foreground service notification (Tauri Android plugin)
- Audio focus handling: request audio focus on play, release on stop

### Fallback Behavior

When the requested audio configuration is unavailable:

| Situation                           | Fallback                                    |
|-------------------------------------|---------------------------------------------|
| Exclusive mode denied               | Shared mode + error event                   |
| Sample rate unsupported (exclusive) | Nearest supported rate, then shared mode     |
| Audio device disconnected           | Retry default device, emit `device_lost`    |
| Format unsupported by symphonia     | Emit `format_unsupported` error, no playback |
| HTTP fetch fails                    | Retry 3x with backoff, then `fetch_failed`  |

## 4. Playback State Sync Protocol

### Position Reporting

- **Mechanism**: Tauri event `native-audio-timeupdate` emitted from a dedicated timer in the Rust backend
- **Frequency**: Every 250ms (matching existing HowlerEngine interval)
- **Source**: Read current position from the cpal output callback's sample counter, converted to seconds
- **Accuracy**: Sub-millisecond (derived from actual samples written to device)

### State Machine

```
                    load()
    ┌──────┐     ┌─────────┐     loaded event
    │ Idle │────→│ Loading │──────────────────→┐
    └──────┘     └─────────┘                   │
       ↑                                       ▼
       │         ┌─────────┐    play()    ┌─────────┐
       │    ┌───→│ Paused  │←────────────→│ Playing │
       │    │    └─────────┘    pause()   └─────────┘
       │    │         │                        │
       │    │         │ stop()                 │ end / stop()
       │    │         ▼                        │
       │    │    ┌─────────┐                   │
       │    │    │ Stopped │←──────────────────┘
       │    │    └─────────┘
       │    │         │
       │    │         │ load()
       │    │         ▼
       │    │    ┌─────────┐
       │    └────│ Loading │
       │         └─────────┘
       │
       │    ┌─────────┐
       └────│  Error  │  (from any state)
            └─────────┘
```

### Seek Lock Behavior

Mirrors the existing `HowlerEngine` pattern:

1. `seek()` called → set `seeking = true`, `seekTarget = timeSec`
2. Timeupdate events during seek: if reported position is not near seek target, suppress emission to prevent UI flicker
3. When position reaches within 2 seconds of target → clear seek lock
4. Safety timeout: clear seek lock after 300ms regardless (same as HowlerEngine)

### Metadata Reporting

On load, the `native-audio-loaded` event includes:

```json
{
  "duration_secs": 245.3,
  "sample_rate": 96000,
  "bit_depth": 24,
  "channels": 2,
  "format": "flac"
}
```

This metadata can be displayed in the player UI to show the user the actual playback quality (e.g., "96kHz / 24-bit FLAC").

## 5. Instance URL and Webview Configuration

### Tauri Plugin Choice

- **Store**: `tauri-plugin-store` v2 — JSON key-value persistence to OS app data directory
- **OS Info**: `tauri-plugin-os` — platform detection (`platform()` returns `'windows'`, `'android'`, etc.)
- **HTTP**: Not needed — the Rust backend uses `reqwest` directly for audio fetching

### URL Persistence

```rust
// Store schema
{
  "instance_url": "https://listen.example.com",
  "wasapi_exclusive_enabled": false
}
```

Store file location (per Tauri defaults):
- Linux: `~/.local/share/com.soundspan.app/store.json`
- macOS: `~/Library/Application Support/com.soundspan.app/store.json`
- Windows: `%APPDATA%\com.soundspan.app\store.json`
- iOS: App sandbox documents directory
- Android: App internal storage

### URL Health Check

On launch:

1. Read `instance_url` from store
2. If absent → show setup screen
3. If present → HTTP GET `{instance_url}/api/health` with 5s timeout
4. On success → load URL in webview
5. On failure → show error overlay with "Retry" and "Change URL" buttons
6. Health check runs in Rust (not webview) to avoid CORS issues

### Webview Security Configuration

In `tauri.conf.json`:

```json
{
  "app": {
    "security": {
      "dangerousRemoteDomainIpcAccess": [
        {
          "domain": "*",
          "enableTauriAPI": true,
          "plugins": ["store", "os"]
        }
      ]
    }
  }
}
```

- `dangerousRemoteDomainIpcAccess` is required because the webview loads a remote URL, not a local one. Tauri normally restricts IPC to local content.
- The domain should be restricted to the configured instance URL's domain in production, but since users configure their own instance, wildcard is used with acknowledgment of the trust model (the user trusts their own instance).

### Tauri Capabilities

```json
{
  "identifier": "main-window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "store:default",
    "os:default",
    "event:default",
    "event:allow-listen",
    "event:allow-emit"
  ]
}
```
