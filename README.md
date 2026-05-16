# ADB Toolbox

A desktop ADB control center built with Tauri v2 + TypeScript. Provides a GUI for common ADB operations, Google Play Store integration via gplaycli, and host storage management.

## Features

- **App & Payload Control** — Push/pull files, install APKs (single or batch), purge app cache
- **Google Play Store Pipeline** — Search, download, and stream-install apps via gplaycli
- **Diagnostics** — Logcat snapshots, screenshots, screen recording, text macro injection
- **Host Storage** — Copy files directly to mounted SD card images
- **Device Control** — Restart UI framework, reboot to bootloader/recovery
- **Live Status** — Auto-detects connected ADB devices with status indicator
- **Output Log** — Terminal-style log panel with color-coded output

## Prerequisites

- **ADB** — `sudo nala install android-tools-adb`
- **gplaycli** — `pip install gplaycli` (requires Google account token setup for Play Store features)
- **Rust** — https://rustup.rs
- **Node.js** — v18+

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

## Note on gplaycli

gplaycli requires Google authentication. The public token dispenser may be unreliable. You may need to configure credentials or a custom token URL in `~/.config/gplaycli/gplaycli.conf`.
