# ADB Toolbox

Desktop GUI for managing Android devices via ADB, built with [Tauri v2](https://tauri.app/) + TypeScript.

## Features

- **Device Management** — Auto-detects connected devices/emulators; a device picker
  targets ADB commands via `-s <serial>` whenever more than one device is connected
- **App & Payload Control** — Push/pull files, install APKs (single or batch), purge app data
- **Sideload APK Search & Install** — Verify an exact package ID exists on `apk-pure` /
  `f-droid`, then download or stream-install it, via [`apkeep`](https://github.com/EFForg/apkeep).
  Deliberately does not use Google Play.
- **Diagnostics** — Logcat viewer, screenshots, screen recording, text injection
- **Device Control** — Copy to mounted SD images, restart framework, reboot to bootloader/recovery

## Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (stable)
- ADB — `sudo nala install adb`
- [`apkeep`](https://github.com/EFForg/apkeep) — `cargo install apkeep` (required for the
  sideload search/download/stream-install features; must be on `PATH`)

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

> **Do not** build the backend with a bare `cargo build --release` inside `src-tauri/`.
> Tauri only embeds the production frontend (`dist/`) when built via the `tauri` CLI
> (`npm run tauri build` or `npm run tauri dev`); a bare `cargo build` produces a binary
> that tries to connect to the dev server at `localhost:1420` and fails with
> "Could not connect to localhost: Connection refused".

To build the release binary without also producing installer packages (`.deb`/`.rpm`/`.AppImage`):

```bash
npm run tauri build -- --no-bundle
```

## Notes

- **Sideload search requires an exact package ID** (e.g. `com.whatsapp`), not a keyword
  search — `apkeep` verifies a known ID against each source rather than searching by name.
- **Multiple devices**: if more than one device/emulator is connected, select the target
  device from the dropdown in the header before running any command — without a selection,
  `adb` itself will fail with "more than one device/emulator" once 2+ are attached.
