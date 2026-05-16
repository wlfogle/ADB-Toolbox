use std::path::PathBuf;
use std::process::Command;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn run_adb(args: &[&str]) -> Result<String, String> {
    let output = Command::new("adb")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to launch adb: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        })
    } else {
        Err(if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        })
    }
}

fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to launch {}: {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        })
    } else {
        Err(if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        })
    }
}

// ── Device Detection ─────────────────────────────────────────────────────────

#[tauri::command]
async fn get_connected_devices() -> Result<Vec<String>, String> {
    let raw = run_adb(&["devices", "-l"])?;
    let devices: Vec<String> = raw
        .lines()
        .skip(1) // skip "List of devices attached"
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    Ok(devices)
}

// ── App & Payload Control ────────────────────────────────────────────────────

#[tauri::command]
async fn push_file(local_path: String, remote_path: String) -> Result<String, String> {
    run_adb(&["push", &local_path, &remote_path])
}

#[tauri::command]
async fn pull_file(remote_path: String, local_path: String) -> Result<String, String> {
    run_adb(&["pull", &remote_path, &local_path])
}

#[tauri::command]
async fn install_apk(apk_path: String) -> Result<String, String> {
    run_adb(&["install", "-r", "-g", &apk_path])
}

#[tauri::command]
async fn batch_install_apks(directory: String) -> Result<Vec<String>, String> {
    let dir = std::fs::read_dir(&directory)
        .map_err(|e| format!("Cannot read directory '{}': {}", directory, e))?;

    let mut results = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "apk") {
            let path_str = path.to_string_lossy().to_string();
            match run_adb(&["install", "-r", "-g", &path_str]) {
                Ok(msg) => results.push(format!("✓ {} — {}", path_str, msg.trim())),
                Err(msg) => results.push(format!("✗ {} — {}", path_str, msg.trim())),
            }
        }
    }
    if results.is_empty() {
        return Err(format!("No .apk files found in '{}'", directory));
    }
    Ok(results)
}

#[tauri::command]
async fn purge_app_cache(package_name: String) -> Result<String, String> {
    run_adb(&["shell", "pm", "clear", &package_name])
}

// ── Google Play Store Pipeline ───────────────────────────────────────────────

#[tauri::command]
async fn search_play_store(query: String) -> Result<Vec<String>, String> {
    let output = Command::new("gplaycli")
        .args(["-s", &query])
        .output()
        .map_err(|e| format!("Failed to launch gplaycli: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gplaycli error: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    // gplaycli outputs a space-padded table:
    //   Title  Creator  Size  Downloads  LastUpdate  AppID  Version  Rating
    // Skip the header row and parse remaining rows by splitting on 2+ spaces.
    for (i, line) in stdout.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line
            .split("  ")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        // Need at least 6 columns (Title, Creator, Size, Downloads, LastUpdate, AppID)
        if cols.len() >= 6 {
            let title = cols[0];
            let creator = cols[1];
            let app_id = cols[5];
            results.push(format!("{} — {} [{}]", title, creator, app_id));
        }
    }

    Ok(results)
}

#[tauri::command]
async fn download_apk(package_id: String, folder: String) -> Result<String, String> {
    let _ = std::fs::create_dir_all(&folder);

    run_cmd("gplaycli", &["-d", &package_id, "-f", &folder])?;

    let dir = std::fs::read_dir(&folder).map_err(|e| e.to_string())?;
    for entry in dir.flatten() {
        let name = entry.file_name().into_string().unwrap_or_default();
        if name.ends_with(".apk") && name.contains(&package_id) {
            return Ok(entry.path().to_string_lossy().to_string());
        }
    }
    Err("Download completed but APK file not found in output directory.".to_string())
}

#[tauri::command]
async fn execute_stream_pipeline(package_id: String) -> Result<String, String> {
    let tmp_dir = "/tmp/gplay_stream_cache";
    let _ = std::fs::create_dir_all(tmp_dir);

    run_cmd("gplaycli", &["-d", &package_id, "-f", tmp_dir])?;

    let dir = std::fs::read_dir(tmp_dir).map_err(|e| e.to_string())?;
    let mut target_apk: Option<PathBuf> = None;
    for entry in dir.flatten() {
        let name = entry.file_name().into_string().unwrap_or_default();
        if name.ends_with(".apk") && name.contains(&package_id) {
            target_apk = Some(entry.path());
            break;
        }
    }
    let apk_path = target_apk.ok_or("APK not found in staging cache after download.")?;
    let apk_str = apk_path.to_string_lossy().to_string();

    let result = run_adb(&["install", "-r", "-g", &apk_str]);
    let _ = std::fs::remove_file(&apk_path);

    match result {
        Ok(msg) => Ok(format!("Installed '{}': {}", package_id, msg.trim())),
        Err(msg) => Err(format!(
            "ADB install failed for '{}': {}",
            package_id,
            msg.trim()
        )),
    }
}

// ── Diagnostics & Interaction ────────────────────────────────────────────────

#[tauri::command]
async fn inject_text_macros(lines: Vec<String>) -> Result<Vec<String>, String> {
    let mut results = Vec::new();
    for line in &lines {
        let escaped = line.replace(' ', "%s");
        match run_adb(&["shell", "input", "text", &escaped]) {
            Ok(_) => results.push(format!("✓ Injected: {}", line)),
            Err(e) => results.push(format!("✗ Failed '{}': {}", line, e.trim())),
        }
    }
    Ok(results)
}

#[tauri::command]
async fn get_logcat_snapshot() -> Result<String, String> {
    run_adb(&["logcat", "-d", "-t", "200"])
}

#[tauri::command]
async fn capture_screenshot(save_to: String) -> Result<String, String> {
    let device_path = "/sdcard/adb_toolbox_screenshot.png";

    run_adb(&["shell", "screencap", "-p", device_path])?;
    run_adb(&["pull", device_path, &save_to])?;
    run_adb(&["shell", "rm", device_path])?;

    Ok(format!("Screenshot saved to {}", save_to))
}

#[tauri::command]
async fn record_screen(save_to: String, duration_secs: u32) -> Result<String, String> {
    let device_path = "/sdcard/adb_toolbox_recording.mp4";
    let limit = duration_secs.min(180).to_string();

    run_adb(&["shell", "screenrecord", "--time-limit", &limit, device_path])?;
    run_adb(&["pull", device_path, &save_to])?;
    run_adb(&["shell", "rm", device_path])?;

    Ok(format!("Recording saved to {} ({}s)", save_to, limit))
}

// ── Host Storage & Machine Control ───────────────────────────────────────────

#[tauri::command]
async fn copy_to_sd_image(source_path: String, mount_point: String) -> Result<String, String> {
    let src = std::path::Path::new(&source_path);
    let filename = src
        .file_name()
        .ok_or("Invalid source file path")?
        .to_string_lossy();
    let dest = std::path::Path::new(&mount_point).join(filename.as_ref());

    if !std::path::Path::new(&mount_point).exists() {
        return Err(format!("Mount point '{}' does not exist", mount_point));
    }

    std::fs::copy(&source_path, &dest).map_err(|e| format!("Copy failed: {}", e))?;

    Ok(format!("Copied '{}' → '{}'", source_path, dest.display()))
}

#[tauri::command]
async fn restart_framework() -> Result<String, String> {
    run_adb(&["shell", "su", "-c", "stop && sleep 1 && start"])
}

#[tauri::command]
async fn reboot_bootloader() -> Result<String, String> {
    run_adb(&["reboot", "bootloader"]).map(|_| "Device rebooting to bootloader...".to_string())
}

#[tauri::command]
async fn reboot_recovery() -> Result<String, String> {
    run_adb(&["reboot", "recovery"]).map(|_| "Device rebooting to recovery...".to_string())
}

// ── Tauri App Entry ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_connected_devices,
            push_file,
            pull_file,
            install_apk,
            batch_install_apks,
            purge_app_cache,
            search_play_store,
            download_apk,
            execute_stream_pipeline,
            inject_text_macros,
            get_logcat_snapshot,
            capture_screenshot,
            record_screen,
            copy_to_sd_image,
            restart_framework,
            reboot_bootloader,
            reboot_recovery,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
