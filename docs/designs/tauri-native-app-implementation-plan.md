# Tauri 2 Native App — Implementation Plan

**ACM Plan**: `plan:receipt-672c713169d81cb4572f3eed`
**Stage**: `implementation_plan`
**Depends on**: refined_spec

---

## Task Dependency Graph

```
ip.01 Tauri scaffold ─────────────────┬──→ ip.04 IPC handlers ──→ ip.05 Frontend adapter ──→ ip.06 Platform detection
                                      │                    ↑                                          │
ip.02 Rust decoder ───────────────────┤                    │                                          ▼
                                      │                    │                                   verify:tests
ip.03 Rust output ────────────────────┘                    │
                                                           │
ip.07 Instance URL UI ─────────────────────────────────────┤
                                                           │
ip.08 WASAPI toggle ───────────────────────────────────────┘

ip.09 Platform builds (parallel, after ip.04 + ip.07)
```

---

## ip.01 — Scaffold Tauri 2 Project

**Goal**: Minimal Tauri 2 app that loads a configurable remote URL.

### Steps

1. Initialize Tauri 2 project:
   ```bash
   cd /home/joshd/git/soundspan
   cargo install create-tauri-app
   # Or manually create src-tauri/ directory
   ```

2. Create `src-tauri/Cargo.toml`:
   ```toml
   [package]
   name = "soundspan-desktop"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   tauri = { version = "2", features = ["protocol-asset"] }
   tauri-plugin-store = "2"
   tauri-plugin-os = "2"
   serde = { version = "1", features = ["derive"] }
   serde_json = "1"
   tokio = { version = "1", features = ["full"] }
   ```

3. Create `src-tauri/tauri.conf.json`:
   - App identifier: `com.soundspan.app`
   - Window: title "soundspan", default size 1280×800
   - No `frontendDist` or `devUrl` (remote URL loaded programmatically)
   - Security: `dangerousRemoteDomainIpcAccess` for IPC from remote content
   - Capabilities: core, store, os, event

4. Create `src-tauri/src/main.rs`:
   - Read instance URL from store on startup
   - If no URL: create window with embedded setup HTML (inline, minimal)
   - If URL exists: health-check, then load in webview
   - Register audio command handlers (stubs initially)

5. Create `src-tauri/capabilities/main.json`:
   - Window permissions for main window
   - Plugin permissions for store, os, event

### Acceptance Criteria
- `cargo tauri dev` launches a window on the current platform
- Window loads a configurable remote URL
- Setup screen appears on first launch
- Builds on at least one desktop platform

---

## ip.02 — Rust Audio Decoder Module

**Goal**: Decode audio files from HTTP URLs using symphonia.

### Steps

1. Add dependencies to `Cargo.toml`:
   ```toml
   symphonia = { version = "0.5", features = [
     "flac", "mp3", "aac", "vorbis", "opus",
     "isomp4", "ogg", "wav", "pcm"
   ] }
   reqwest = { version = "0.12", features = ["stream"] }
   ```

2. Create `src-tauri/src/audio/decoder.rs`:
   - `AudioDecoder::from_url(url, headers)` → HTTP GET with streaming body
   - Wrap `reqwest` streaming response as `symphonia::core::io::MediaSource`
   - Probe format, select audio track, create codec decoder
   - `decode_next() → Option<AudioBuffer<f32>>` — decode one packet
   - `track_info() → TrackInfo { sample_rate, bit_depth, channels, format, duration }`

3. Create `src-tauri/src/audio/mod.rs`:
   - Re-export decoder types

4. Write unit tests:
   - Test decoding a local FLAC file
   - Test decoding a local MP3 file
   - Test decoding a local WAV file
   - Test format detection from stream
   - Test error handling for unsupported format

### Acceptance Criteria
- Decodes FLAC, WAV, MP3, AAC, OGG from URL or local path
- Outputs f32 PCM samples with correct metadata
- Unit tests pass for each supported format

---

## ip.03 — Rust Audio Output Module

**Goal**: Output PCM audio at native sample rate via cpal.

### Steps

1. Add dependencies to `Cargo.toml`:
   ```toml
   cpal = "0.15"
   ringbuf = "0.4"
   ```

2. Create `src-tauri/src/audio/output.rs`:
   - `AudioOutput::open(sample_rate, channels, exclusive)` → configure cpal stream
   - Ring buffer: SPSC, 2 seconds capacity at given sample rate
   - `write_samples(samples: &[f32])` → push to ring buffer (blocks if full)
   - `set_volume(level: f32)` → atomic volume multiplier applied in callback
   - `pause() / resume()` → cpal stream play/pause
   - `close()` → drop stream, release device
   - cpal callback: read from ring buffer, apply volume, write to device

3. WASAPI exclusive mode (Windows only, behind `#[cfg(target_os = "windows")]`):
   - Query device supported exclusive configurations
   - Attempt exclusive stream at exact source sample rate
   - On failure: fall back to shared mode, return error info

4. Write unit tests:
   - Test stream opens in shared mode
   - Test volume control
   - Test ring buffer underrun produces silence
   - Test stream configuration for various sample rates

### Acceptance Criteria
- Outputs PCM at source sample rate via WASAPI (shared) and AAudio
- WASAPI exclusive mode switchable at runtime
- Volume control works
- Unit tests pass

---

## ip.04 — Tauri IPC Command Handlers

**Goal**: Wire decoder + output into a player orchestrator, expose via Tauri commands.

### Steps

1. Create `src-tauri/src/audio/state.rs`:
   - `PlaybackState` struct: status, position, duration, track_info, exclusive_mode
   - Wrapped in `Arc<Mutex<>>` for thread-safe access
   - Position updated by output callback's sample counter

2. Create `src-tauri/src/audio/preload.rs`:
   - `PreloadManager`: holds one pre-decoded buffer
   - `preload(url, headers)` → fetch + decode into memory
   - `take_if_matches(url)` → return buffer if URL matches, consume preload

3. Create `src-tauri/src/audio/player.rs`:
   - `AudioPlayer`: owns decoder, output, state, preload manager
   - `play(url, format, headers)`:
     1. Check preload manager for cached decode
     2. If not preloaded: create decoder from URL
     3. Open output at decoder's sample rate
     4. Spawn decode thread: read packets → ring buffer
     5. Start position reporting timer (250ms)
   - `pause()` / `resume()` / `stop()` / `seek()` / `set_volume()`
   - `set_exclusive(enabled)`: stop stream → reopen → resume

4. Create `src-tauri/src/audio/commands.rs`:
   - `#[tauri::command] async fn native_audio_play(...)`
   - `#[tauri::command] async fn native_audio_pause(...)`
   - `#[tauri::command] async fn native_audio_resume(...)`
   - `#[tauri::command] async fn native_audio_stop(...)`
   - `#[tauri::command] async fn native_audio_seek(...)`
   - `#[tauri::command] async fn native_audio_set_volume(...)`
   - `#[tauri::command] async fn native_audio_get_state(...)`
   - `#[tauri::command] async fn native_audio_preload(...)`
   - `#[tauri::command] async fn native_audio_set_exclusive(...)`
   - Each command accesses `AudioPlayer` via Tauri managed state

5. Register commands in `main.rs`:
   ```rust
   tauri::Builder::default()
       .manage(AudioPlayer::new())
       .invoke_handler(tauri::generate_handler![
           native_audio_play,
           native_audio_pause,
           // ...
       ])
   ```

6. Emit events from player:
   - `app.emit("native-audio-timeupdate", payload)` every 250ms
   - `app.emit("native-audio-loaded", payload)` on decode start
   - `app.emit("native-audio-ended", payload)` when decode + output complete
   - `app.emit("native-audio-error", payload)` on any error

### Acceptance Criteria
- All 9 IPC commands callable from JavaScript
- Play/pause/seek/volume work end-to-end
- Position reporting at 250ms intervals
- Errors propagated to frontend

---

## ip.05 — Frontend TauriNativeEngineAdapter

**Goal**: Implement `AudioEngine` interface backed by Tauri IPC.

### Steps

1. Create `frontend/lib/audio-engine/tauriNativeEngineAdapter.ts`:
   - Import types from `types.ts`
   - Implement full `AudioEngine` interface per refined spec
   - Use `window.__TAURI__.core.invoke()` for commands
   - Use `window.__TAURI__.event.listen()` for events
   - Translate Tauri events to AudioEngine events
   - Seek lock implementation (matching HowlerEngine pattern)

2. Create `frontend/lib/audio-engine/tauriDetection.ts`:
   - `isTauriEnvironment(): boolean` — checks `window.__TAURI__`
   - `needsNativeAudio(): Promise<boolean>` — checks platform via `@tauri-apps/plugin-os`
   - Returns `true` for Windows and Android only

3. Add type declarations for Tauri globals:
   - `frontend/types/tauri.d.ts` — declare `window.__TAURI__` shape
   - Or use `@tauri-apps/api` package types

4. Write unit tests with mocked IPC:
   - Mock `window.__TAURI__.core.invoke` and `window.__TAURI__.event.listen`
   - Test load → emits load event with duration
   - Test play/pause/stop state transitions
   - Test seek with seek lock behavior
   - Test volume/mute
   - Test event translation (timeupdate, ended, error)
   - Test destroy cleans up all listeners
   - Test preload triggers IPC

### Acceptance Criteria
- Implements full AudioEngine interface
- All Tauri events translated correctly
- Seek lock prevents UI flicker
- Unit tests pass with mocked IPC

---

## ip.06 — Platform Detection and Engine Routing

**Goal**: Automatically select TauriNativeEngineAdapter on Windows/Android Tauri, HowlerEngineAdapter everywhere else.

### Steps

1. Update `frontend/lib/audio-engine/engineMode.ts`:
   - Add `"tauri-native"` to `StreamingEngineMode` type
   - Add `resolveEngineForPlatform()`: detects Tauri + platform, returns mode

2. Create `frontend/lib/audio-engine/engineFactory.ts` (or update existing factory):
   - `createAudioEngine(): AudioEngine`
   - If Tauri + Chromium platform → `new TauriNativeEngineAdapter()`
   - Otherwise → `new HowlerEngineAdapter()`

3. Update `AudioPlaybackOrchestrator.tsx`:
   - Use engine factory instead of direct HowlerEngineAdapter construction
   - No other orchestrator changes needed — it operates on the AudioEngine interface

4. Write tests:
   - Test engine selection: non-Tauri → Howler
   - Test engine selection: Tauri + Windows → TauriNative
   - Test engine selection: Tauri + macOS → Howler
   - Test engine selection: Tauri + Android → TauriNative

### Acceptance Criteria
- Correct engine selected automatically based on platform
- Transparent to AudioPlaybackOrchestrator consumers
- Existing Howler behavior unchanged on web and WebKit Tauri platforms

---

## ip.07 — Instance URL Configuration Screen

**Goal**: First-run setup and settings UI for configuring the soundspan instance URL.

### Steps

1. Create setup screen as inline HTML in Rust (loaded when no instance URL configured):
   - Minimal HTML/CSS — not part of the soundspan webapp, loaded locally
   - Text input for URL, submit button, error display
   - On submit: Rust validates URL via health check, saves to store, reloads webview

2. Add Tauri command for URL management:
   - `#[tauri::command] async fn set_instance_url(url: String)` → validate + save + reload
   - `#[tauri::command] fn get_instance_url()` → read from store

3. Add "Change Instance" option:
   - Tauri system tray menu item or app menu item
   - Triggers a URL change flow: show setup screen in current window

4. URL validation in Rust:
   - Parse as URL (reject non-http(s))
   - Strip trailing slashes
   - GET `{url}/api/health` with 5s timeout
   - Return validation result to setup screen

### Acceptance Criteria
- First-run screen prompts for URL
- URL validated before saving
- Persisted across restarts
- Changeable from system tray / app menu

---

## ip.08 — WASAPI Exclusive Mode Toggle

**Goal**: Settings toggle for exclusive audio mode on Windows.

### Steps

1. Add IPC command (already defined in ip.04):
   - `native_audio_set_exclusive { enabled: boolean }` → stop output → reopen device → resume

2. Frontend detection:
   - In Tauri + Windows: show toggle in settings page
   - Read current state from `native_audio_get_state().exclusive_mode`
   - Toggle invokes `native_audio_set_exclusive`

3. Persistence:
   - Rust side: read `wasapi_exclusive_enabled` from store on startup
   - Apply setting when opening audio output
   - Save on toggle

4. Error handling:
   - If exclusive mode fails: revert to shared, show toast notification
   - Toast text: "Exclusive mode unavailable — another application may have the audio device"

### Acceptance Criteria
- Toggle visible only on Windows
- Switching mode gracefully stops and resumes playback
- Setting persisted across restarts
- Failure falls back with user notification

---

## ip.09 — Platform Build Configurations

**Goal**: Build configs for all 5 target platforms.

### Steps

1. Desktop builds (in `tauri.conf.json` → `bundle`):
   - **Linux**: AppImage and .deb targets
   - **macOS**: .dmg and .app bundle, code signing config placeholder
   - **Windows**: NSIS installer, code signing config placeholder

2. Mobile builds:
   - **iOS**: `cargo tauri ios init` → generates Xcode project in `src-tauri/gen/apple`
   - **Android**: `cargo tauri android init` → generates Gradle project in `src-tauri/gen/android`
   - Android: add foreground service for background audio playback
   - iOS: add audio background mode capability

3. CI/CD considerations (documented, not necessarily implemented):
   - GitHub Actions workflow for cross-platform builds
   - Platform-specific signing requirements
   - Auto-update configuration (Tauri updater plugin)

4. Tauri config for each platform:
   - App icon assets (placeholder)
   - Bundle identifiers per platform
   - Minimum OS version requirements

### Acceptance Criteria
- Linux: AppImage/deb build configuration present
- macOS: DMG/app bundle configuration present
- Windows: NSIS installer configuration present
- iOS: Xcode project initializable
- Android: Gradle project initializable

---

## ip.10 — GitHub Actions Release CI

**Goal**: Automated cross-platform builds with binary artifacts on GitHub release pages.

### Steps

1. Create `.github/workflows/release-desktop.yml`:
   - **Trigger**: on push of tag matching `v*` (e.g., `v0.1.0`)
   - **Matrix strategy**:
     | Runner           | Target Platform | Artifact                        |
     |------------------|-----------------|---------------------------------|
     | `ubuntu-latest`  | Linux           | AppImage, .deb                  |
     | `macos-latest`   | macOS           | .dmg (universal binary arm64+x86_64) |
     | `windows-latest` | Windows         | NSIS installer (.exe)           |
   - Use `tauri-apps/tauri-action@v0` — handles Rust toolchain, build, and artifact upload
   - Configure action with `tagName: v__VERSION__`, `releaseName: soundspan v__VERSION__`

2. Create `.github/workflows/release-mobile.yml`:
   - **Android**:
     - Runner: `ubuntu-latest`
     - Install Android SDK/NDK, Rust Android targets
     - `cargo tauri android build --apk`
     - Upload APK as release asset
   - **iOS**:
     - Runner: `macos-latest`
     - `cargo tauri ios build`
     - Upload IPA as release asset (unsigned for sideloading, or with signing secrets)

3. Code signing placeholders:
   - macOS: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID` as repository secrets
   - Windows: `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` as repository secrets
   - Workflows reference these secrets but gracefully skip signing if not configured

4. Release page format:
   ```
   soundspan v0.1.0
   ├── soundspan_0.1.0_amd64.AppImage
   ├── soundspan_0.1.0_amd64.deb
   ├── soundspan_0.1.0_universal.dmg
   ├── soundspan_0.1.0_x64-setup.exe
   ├── soundspan_0.1.0.apk
   └── soundspan_0.1.0.ipa
   ```

5. Optional: Add Tauri updater plugin config so the app can check for updates from GitHub releases

### Acceptance Criteria
- Workflow triggers on release tag push
- Matrix builds across Linux, macOS, Windows
- Separate mobile workflow for Android and iOS
- All binaries uploaded as release assets automatically
- Code signing secrets stubbed with graceful skip if unconfigured
- Uses official `tauri-apps/tauri-action` for desktop targets

---

## Task Execution Order (Suggested)

| Phase | Tasks (parallelizable within phase) | Depends On |
|-------|-------------------------------------|------------|
| 1     | ip.01, ip.02, ip.03                 | —          |
| 2     | ip.04, ip.07                        | ip.01 + ip.02 + ip.03 |
| 3     | ip.05                               | ip.04      |
| 4     | ip.06, ip.08                        | ip.05, ip.04 |
| 5     | ip.09                               | ip.04 + ip.07 |
| 6     | ip.10                               | ip.09      |
| 7     | verify:tests                        | ip.06      |

Phase 1 tasks are fully independent and should be tackled in parallel. Phase 2 can also partially parallelize (ip.07 only needs ip.01).
