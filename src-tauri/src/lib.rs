use std::process::Command;
use serde::Serialize;

// ── Device Management ─────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
struct DeviceInfo {
    serial: String,
    model: String,
    state: String,
}

#[tauri::command]
async fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    let output = Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to list devices: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<DeviceInfo> = stdout
        .lines()
        .skip(1) // "List of devices attached" header
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts
                .find_map(|p| p.strip_prefix("model:"))
                .unwrap_or("unknown")
                .to_string();
            Some(DeviceInfo { serial, model, state })
        })
        .collect();

    Ok(devices)
}

// Builds an `adb` command, optionally targeting a specific device via `-s`.
// With no device_id (or exactly one device connected) adb picks it
// automatically; with 2+ devices connected and no device_id, adb itself
// fails clearly ("more than one device/emulator") rather than silently
// picking the wrong one.
fn adb_base(device_id: &Option<String>) -> Command {
    let mut cmd = Command::new("adb");
    if let Some(id) = device_id {
        if !id.is_empty() {
            cmd.args(["-s", id]);
        }
    }
    cmd
}

// ── Play Store Integration ──────────────────────────────────────────────────

// Verification uses `apkeep --list-versions`, NOT `apksearch`. apksearch (a
// Python HTML-scraper) was found to be unreliable specifically when invoked
// as a subprocess of a compiled binary (this app, and a `cargo test` harness):
// it consistently returned zero results across ALL 7 mirrors it scrapes, for
// a query that succeeded 100% of the time run directly in an interactive
// shell. PATH resolution, duplicate binaries, TTY-of-stdout formatting, proxy
// env vars, silent exceptions (checked via `--log_err`), and stdin handling
// were all individually tested and ruled out as the cause — the difference
// is internal to apksearch's own HTTP/scraping behavior.
//
// `apkeep` (a compiled Rust tool from the EFF that hits app-store APIs
// directly rather than scraping HTML) was verified reliable in the exact same
// subprocess context apksearch failed in, and it's already the tool this app
// uses for the actual download step — so verification now reuses it via
// `--list-versions`, dropping the apksearch dependency entirely.
//
// Deliberately stays outside Google Play, matching this app's sideload-only
// design. huawei-app-gallery is deliberately excluded: it can't enumerate
// historical versions at all and reports success with an explanatory
// sentence instead of a real version list for EVERY query, existing package
// or not — making `--list-versions` structurally unable to answer "does this
// exist" for that source.
const SIDELOAD_SOURCES: [&str; 2] = ["apk-pure", "f-droid"];

#[derive(Serialize, Clone, Debug)]
struct PlayStoreResult {
    source: String,
    name: String,
    link: String,
}

// Runs `apkeep -a <query> -l -d <source>` once and parses its "Versions
// available for X on Y:\n| v1, v2, ..." output. Returns Ok(None) (not an
// error) when the source simply doesn't have this package, so the caller can
// distinguish "checked, not found here" from "the apkeep call itself failed".
fn check_source_for_versions(query: &str, source: &str) -> Result<Option<PlayStoreResult>, String> {
    let output = Command::new("apkeep")
        .args(["-a", query, "-l", "-d", source, "/tmp"])
        .output()
        .map_err(|e| format!("Failed to run apkeep: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() || !stdout.contains("Versions available") {
        return Ok(None);
    }

    let versions: Vec<String> = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('|'))
        .map(|l| {
            l.trim_start_matches('|')
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // A real version string starts with a digit. Guards against sources
    // that report success with an empty list (package not actually present,
    // e.g. proprietary apps on F-Droid) or a descriptive sentence instead of
    // versions — neither of which means the package was actually found.
    let has_real_versions = versions
        .first()
        .is_some_and(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()));

    if !has_real_versions {
        return Ok(None);
    }

    Ok(Some(PlayStoreResult {
        source: source.to_string(),
        name: format!(
            "{} version(s) available (e.g. {})",
            versions.len(),
            versions.last().map(|s| s.as_str()).unwrap_or("unknown")
        ),
        link: String::new(),
    }))
}

#[tauri::command]
async fn search_play_store(query: String) -> Result<Vec<PlayStoreResult>, String> {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for source in SIDELOAD_SOURCES {
        match check_source_for_versions(&query, source) {
            Ok(Some(r)) => results.push(r),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }

    if results.is_empty() {
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        return Err(format!(
            "No sideload source has \"{}\". Note: this requires the EXACT package ID (e.g. com.whatsapp), not a keyword.",
            query
        ));
    }

    Ok(results)
}

#[tauri::command]
async fn download_apk(package_id: String, folder: String) -> Result<String, String> {
    let _ = std::fs::create_dir_all(&folder);

    // apkeep -a <PACKAGE_ID> -d apk-pure <FOLDER_PATH>
    let output = Command::new("apkeep")
        .args(["-a", &package_id, "-d", "apk-pure", &folder])
        .output()
        .map_err(|e| format!("Failed to run apkeep: {}", e))?;

    if output.status.success() {
        Ok(format!("Downloaded {} to {}", package_id, folder))
    } else {
        Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn execute_stream_pipeline(package_id: String, device_id: Option<String>) -> Result<String, String> {
    let tmp_dir = "/tmp/gplay_stream_cache";
    let _ = std::fs::create_dir_all(tmp_dir);

    // Download APK to staging cache via apkeep
    let dl = Command::new("apkeep")
        .args(["-a", &package_id, "-d", "apk-pure", tmp_dir])
        .output()
        .map_err(|e| format!("Download error: {}", e))?;

    if !dl.status.success() {
        return Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&dl.stderr)
        ));
    }

    // Locate the downloaded APK
    let apk_path = std::fs::read_dir(tmp_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .find(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".apk") && name.contains(&package_id)
        })
        .map(|e| e.path())
        .ok_or("Downloaded APK not found in cache")?;

    // Stream-install to device via ADB
    let install = adb_base(&device_id)
        .args(["install", "-r", "-g", apk_path.to_str().unwrap_or("")])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    let _ = std::fs::remove_file(&apk_path);

    if install.status.success() {
        Ok(format!("Installed {} on device", package_id))
    } else {
        Err(format!(
            "Install failed: {}",
            String::from_utf8_lossy(&install.stderr)
        ))
    }
}

// ── File Transfer ───────────────────────────────────────────────────────────

#[tauri::command]
async fn list_android_files(remote_path: String, device_id: Option<String>) -> Result<Vec<String>, String> {
    let output = adb_base(&device_id)
        .args(["shell", "ls", "-1", &remote_path])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to list files: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}

#[tauri::command]
async fn push_file(local_path: String, remote_path: String, device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["push", &local_path, &remote_path])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok(format!("Pushed to {}", remote_path))
    } else {
        Err(format!(
            "Push failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn pull_file(remote_path: String, local_path: String, device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["pull", &remote_path, &local_path])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok(format!("Pulled {} to {}", remote_path, local_path))
    } else {
        Err(format!(
            "Pull failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// ── APK Management ──────────────────────────────────────────────────────────

#[tauri::command]
async fn install_apk(path: String, device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["install", "-r", "-g", &path])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        let filename = std::path::Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        Ok(format!("Installed {}", filename))
    } else {
        Err(format!(
            "Install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn batch_install_apks(folder: String, device_id: Option<String>) -> Result<String, String> {
    let entries =
        std::fs::read_dir(&folder).map_err(|e| format!("Cannot read folder: {}", e))?;

    let mut installed = 0u32;
    let mut failed = 0u32;
    let mut errors: Vec<String> = Vec::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e == "apk").unwrap_or(false) {
            match adb_base(&device_id)
                .args(["install", "-r", "-g", path.to_str().unwrap_or("")])
                .output()
            {
                Ok(o) if o.status.success() => installed += 1,
                Ok(o) => {
                    failed += 1;
                    errors.push(format!(
                        "{}: {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        String::from_utf8_lossy(&o.stderr).trim()
                    ));
                }
                Err(e) => {
                    failed += 1;
                    errors.push(format!(
                        "{}: {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        e
                    ));
                }
            }
        }
    }

    if installed == 0 && failed == 0 {
        Err("No APK files found in the selected folder.".into())
    } else if failed > 0 {
        Err(format!(
            "Installed {}, failed {}:\n{}",
            installed,
            failed,
            errors.join("\n")
        ))
    } else {
        Ok(format!("Successfully installed {} APK(s).", installed))
    }
}

// ── Package Management ──────────────────────────────────────────────────────

#[tauri::command]
async fn list_packages(device_id: Option<String>) -> Result<Vec<String>, String> {
    let output = adb_base(&device_id)
        .args(["shell", "pm", "list", "packages"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut packages: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .collect();
    packages.sort();
    Ok(packages)
}

#[tauri::command]
async fn purge_app_cache(package_id: String, device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["shell", "pm", "clear", &package_id])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok(format!("Cleared data for {}", package_id))
    } else {
        Err(format!(
            "Clear failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// ── Diagnostics & Interaction ───────────────────────────────────────────────

#[tauri::command]
async fn inject_text(text: String, device_id: Option<String>) -> Result<String, String> {
    // adb shell input text requires %s for spaces
    let escaped = text.replace(' ', "%s");
    let output = adb_base(&device_id)
        .args(["shell", "input", "text", &escaped])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok(format!("Injected: {}", text))
    } else {
        Err(format!(
            "Injection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn capture_logcat(device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["logcat", "-d", "-t", "200"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "Logcat failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn capture_screenshot(save_path: String, device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["exec-out", "screencap", "-p"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Screenshot failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    std::fs::write(&save_path, &output.stdout)
        .map_err(|e| format!("Failed to save: {}", e))?;

    Ok(format!("Screenshot saved to {}", save_path))
}
#[tauri::command]
async fn record_screen(save_path: String, device_id: Option<String>) -> Result<String, String> {
    let device_path = "/sdcard/adb_toolbox_record.mp4";

    // Record for 10 seconds on the device
    let record = adb_base(&device_id)
        .args(["shell", "screenrecord", "--time-limit", "10", device_path])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if !record.status.success() {
        return Err(format!(
            "Recording failed: {}",
            String::from_utf8_lossy(&record.stderr)
        ));
    }

    // Pull the recording to local filesystem
    let pull = adb_base(&device_id)
        .args(["pull", device_path, &save_path])
        .output()
        .map_err(|e| format!("Pull error: {}", e))?;

    // Clean up device file
    let _ = adb_base(&device_id)
        .args(["shell", "rm", device_path])
        .output();

    if pull.status.success() {
        Ok(format!("Recording saved to {}", save_path))
    } else {
        Err(format!(
            "Pull failed: {}",
            String::from_utf8_lossy(&pull.stderr)
        ))
    }
}

// ── Host Storage ────────────────────────────────────────────────────────────

#[tauri::command]
async fn copy_to_mount(source: String, mount_point: String) -> Result<String, String> {
    let src = std::path::Path::new(&source);
    let filename = src
        .file_name()
        .ok_or("Invalid source path")?
        .to_str()
        .ok_or("Invalid filename encoding")?;
    let dest = std::path::Path::new(&mount_point).join(filename);

    std::fs::copy(&source, &dest).map_err(|e| format!("Copy failed: {}", e))?;

    Ok(format!("Copied {} to {}", filename, dest.display()))
}

// ── Device Power Control ────────────────────────────────────────────────────

#[tauri::command]
async fn restart_framework(device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["shell", "su", "-c", "stop; sleep 1; start"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok("UI framework restarted.".into())
    } else {
        Err(format!(
            "Restart failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn reboot_bootloader(device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["reboot", "bootloader"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok("Rebooting to bootloader...".into())
    } else {
        Err(format!(
            "Reboot failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[tauri::command]
async fn reboot_recovery(device_id: Option<String>) -> Result<String, String> {
    let output = adb_base(&device_id)
        .args(["reboot", "recovery"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;

    if output.status.success() {
        Ok("Rebooting to recovery...".into())
    } else {
        Err(format!(
            "Reboot failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────
// These exercise the real `search_play_store` function against the live
// `apkeep` tool (no mocking). apkeep was chosen over apksearch specifically
// because it was verified reliable as a compiled-binary subprocess, so these
// are NOT marked #[ignore] — unlike the apksearch-based version this
// replaced, they're expected to pass consistently.

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_play_store_finds_real_package_via_apkeep() {
        let results = search_play_store("com.whatsapp".to_string())
            .await
            .expect("apkeep should find com.whatsapp on at least one sideload source");

        assert!(!results.is_empty(), "expected at least one source result");
        for r in &results {
            assert!(!r.source.is_empty(), "source must not be empty");
            assert!(
                r.name.contains("version"),
                "name should mention version count, got: {}",
                r.name
            );
        }
        println!("Found {} source(s) for com.whatsapp:", results.len());
        for r in &results {
            println!("  {} -> {}", r.source, r.name);
        }
    }

    #[tokio::test]
    async fn search_play_store_rejects_free_text_keyword_with_clear_error() {
        let err = search_play_store("gmail".to_string())
            .await
            .expect_err("a free-text keyword should not resolve to any exact package");
        assert!(
            err.contains("EXACT package ID"),
            "error should explain the exact-package-id requirement, got: {}",
            err
        );
    }
}

// ── App Entry Point ─────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_devices,
            search_play_store,
            download_apk,
            execute_stream_pipeline,
            list_android_files,
            push_file,
            pull_file,
            install_apk,
            batch_install_apks,
            list_packages,
            purge_app_cache,
            inject_text,
            capture_logcat,
            capture_screenshot,
            record_screen,
            copy_to_mount,
            restart_framework,
            reboot_bootloader,
            reboot_recovery,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}