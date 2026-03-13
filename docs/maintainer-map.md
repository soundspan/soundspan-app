# Maintainer Map

Use this file when a task is specific to `soundspan-app` and you need to know where to start.

## Architecture In One Screen

- `app-shell/index.html` is the local setup shell shown before an instance URL is configured.
- `src-tauri/src/lib.rs` and `src-tauri/src/main.rs` bootstrap Tauri, register commands, load persisted settings, and navigate to the configured remote instance.
- `src-tauri/src/audio/**` owns the native playback path used to bypass Chromium's 48 kHz mixer limits on platforms that need it.
- `src-tauri/src/config/security.rs` is the local-versus-remote IPC gate.
- `src-tauri/src/config/store.rs` is the canonical map of persisted `config.json` keys.
- `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, and `aur/**` hold packaging, capability, and release metadata.

## Routing Guide

| If you are changing... | Start here | Also keep aligned |
|---|---|---|
| Local setup and instance bootstrap UX | `app-shell/index.html` | `src-tauri/src/lib.rs`, `README.md` |
| Instance URL persistence or startup navigation | `src-tauri/src/lib.rs` | `src-tauri/src/config/store.rs`, `app-shell/index.html` |
| Native playback commands exposed to the webview | `src-tauri/src/audio/commands.rs` | `src-tauri/src/lib.rs`, `src-tauri/src/audio/player.rs`, `src-tauri/src/audio/state.rs` |
| Decoder, preload, output, or exclusive-mode behavior | `src-tauri/src/audio/**` | tests under the same module tree, any affected IPC commands |
| Local-only versus remote-instance IPC authorization | `src-tauri/src/config/security.rs` | command handlers in `src-tauri/src/lib.rs` and `src-tauri/src/audio/commands.rs` |
| Persisted settings or new config keys | `src-tauri/src/config/store.rs` | `src-tauri/src/lib.rs`, any code that reads or writes `config.json` |
| Bundle targets, capabilities, or app identity | `src-tauri/tauri.conf.json` | `src-tauri/capabilities/**`, `aur/**`, `README.md` |
| Linux release packaging | `aur/PKGBUILD`, `aur/soundspan-bin.install` | release assumptions in `README.md` |
| Feature-plan process or ACM governance | `AGENTS.md`, `CLAUDE.md`, `.acm/**` | `.claude/acm-broker/**`, `.codex/acm-broker/**`, `docs/feature-plans.md`, `scripts/acm-cross-review.sh`, `scripts/acm-tdd-guard.py` |

## Repo Invariants

- The product UI lives in the configured remote `soundspan` instance. The local shell is bootstrap-only unless a task explicitly changes that model.
- Local-only commands should stay restricted to local app content.
- Remote playback IPC should stay limited to the configured `soundspan` instance origin.
- Store keys should remain documented in `src-tauri/src/config/store.rs`.
- Packaging and capability changes should keep Tauri metadata and AUR metadata aligned.
- Behavior-changing Rust work under `src-tauri/src/**` should start with a failing Rust test, record either `tdd:red` or `tdd:exemption` through ACM work before implementation, and keep a Rust test-bearing file change in the same task unless the exemption explains why that is not practical.
- Workflow-selected implementation and governance changes should satisfy `acm review --run` before final completion.
- Changes to routing surfaces listed above should keep `docs/maintainer-map.md` aligned in the same task.
- User-facing onboarding, packaging, or permission changes should keep `README.md` aligned in the same task.

## Verification Defaults

- ACM/governance changes: `acm sync --mode working_tree --insert-new-candidates` then `acm health --include-details`
- Rust/Tauri changes: `cargo test --manifest-path src-tauri/Cargo.toml`
- Rust/Tauri compile confirmation: `cargo check --manifest-path src-tauri/Cargo.toml`
- Workflow-selected review gate: `acm review --run --receipt-id <receipt-id>`
- Maintainer-map drift guard: `python3 scripts/acm-doc-drift-guard.py --mode maintainer-map`
- README drift guard: `python3 scripts/acm-doc-drift-guard.py --mode readme`
- AUR packaging edits: `bash -n aur/PKGBUILD aur/soundspan-bin.install`

## Design References

- `docs/designs/tauri-native-app-spec-outline.md`
- `docs/designs/tauri-native-app-refined-spec.md`
- `docs/designs/tauri-native-app-implementation-plan.md`
