# [1.0.1] Release Notes - 2026-03-13

## Release Summary

soundspan-app 1.0.1 is a maintenance release focused on release metadata alignment and Windows setup reliability. It corrects the packaged version metadata, fixes local-origin detection for the setup shell on Windows, and improves the error details shown when instance bootstrap fails.

## Fixed

- Fixed the Windows setup-shell bootstrap flow by recognizing Tauri-hosted WebView2 content as local app content during initial instance connection.
- Improved setup-shell error reporting so failed connection attempts show the underlying connection or permission failure instead of only a generic connect error.

## Added

None.

## Changed

- Allowed setup-only commands from the configured Tauri `devUrl` origin during development while preserving the configured-instance origin boundary for remote IPC.
- Corrected release metadata and package versioning across Cargo, Tauri bundle configuration, and the AUR package definition so published artifacts align on `1.0.1`.

## Install

- **Linux**: Download `.AppImage`, `.deb`, or `.rpm` from the [Releases page](https://github.com/soundspan/soundspan-app/releases/tag/v1.0.1)
- **macOS**: Download `.dmg` (universal binary)
- **Windows**: Download `.exe` (NSIS installer)
- **Android**: Download `.apk`
- **Arch Linux**: `yay -S soundspan-bin`

## Breaking Changes

None.

## Known Issues

None at this time.

## Full Changelog

- Compare changes since 1.0.0: https://github.com/soundspan/soundspan-app/compare/1.0.0...1.0.1
- Full changelog: https://github.com/soundspan/soundspan-app/blob/main/CHANGELOG.md
