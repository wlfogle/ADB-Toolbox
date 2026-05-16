import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

const MOUNT_POINT = "/home/loufogle/mount_point";

// ── Log Panel ────────────────────────────────────────────────────────────────

const logEl = () => document.getElementById("logOutput")!;

function log(msg: string, cls: string = "") {
  const ts = new Date().toLocaleTimeString();
  const line = document.createElement("div");
  if (cls) line.className = cls;
  line.textContent = `[${ts}] ${msg}`;
  logEl().appendChild(line);
  logEl().scrollTop = logEl().scrollHeight;
}

function logOk(msg: string) { log(msg, "log-ok"); }
function logErr(msg: string) { log(msg, "log-err"); }
function logInfo(msg: string) { log(msg, "log-info"); }
function logWarn(msg: string) { log(msg, "log-warn"); }

// ── Reusable Modal Helpers ───────────────────────────────────────────────────

function showInputModal(
  title: string,
  fields: Array<{ id: string; label: string; placeholder: string; type?: "text" | "textarea" }>,
): Promise<Record<string, string> | null> {
  return new Promise((resolve) => {
    const modal = document.getElementById("inputModal")!;
    const titleEl = document.getElementById("inputModalTitle")!;
    const body = document.getElementById("inputModalBody")!;
    const okBtn = document.getElementById("inputModalOk")!;
    const cancelBtn = document.getElementById("inputModalCancel")!;

    titleEl.textContent = title;
    body.innerHTML = "";

    for (const f of fields) {
      const label = document.createElement("label");
      label.textContent = f.label;
      label.style.cssText = "display:block;font-size:0.78rem;color:#aaa;margin-bottom:3px;";
      body.appendChild(label);

      if (f.type === "textarea") {
        const ta = document.createElement("textarea");
        ta.id = `modal-field-${f.id}`;
        ta.className = "modal-textarea";
        ta.placeholder = f.placeholder;
        body.appendChild(ta);
      } else {
        const inp = document.createElement("input");
        inp.id = `modal-field-${f.id}`;
        inp.className = "modal-input";
        inp.placeholder = f.placeholder;
        body.appendChild(inp);
      }
    }

    modal.classList.remove("invisible");
    const firstField = body.querySelector("input, textarea") as HTMLElement;
    firstField?.focus();

    const cleanup = () => {
      modal.classList.add("invisible");
      okBtn.replaceWith(okBtn.cloneNode(true));
      cancelBtn.replaceWith(cancelBtn.cloneNode(true));
    };

    document.getElementById("inputModalOk")!.addEventListener("click", () => {
      const result: Record<string, string> = {};
      for (const f of fields) {
        const el = document.getElementById(`modal-field-${f.id}`) as HTMLInputElement | HTMLTextAreaElement;
        result[f.id] = el?.value ?? "";
      }
      cleanup();
      resolve(result);
    });

    document.getElementById("inputModalCancel")!.addEventListener("click", () => {
      cleanup();
      resolve(null);
    });
  });
}

function showConfirm(title: string, message: string): Promise<boolean> {
  return new Promise((resolve) => {
    const modal = document.getElementById("confirmModal")!;
    document.getElementById("confirmModalTitle")!.textContent = title;
    document.getElementById("confirmModalMsg")!.textContent = message;
    modal.classList.remove("invisible");

    const cleanup = () => {
      modal.classList.add("invisible");
      okBtn.replaceWith(okBtn.cloneNode(true));
      cancelBtn.replaceWith(cancelBtn.cloneNode(true));
    };

    const okBtn = document.getElementById("confirmModalOk")!;
    const cancelBtn = document.getElementById("confirmModalCancel")!;

    document.getElementById("confirmModalOk")!.addEventListener("click", () => { cleanup(); resolve(true); });
    document.getElementById("confirmModalCancel")!.addEventListener("click", () => { cleanup(); resolve(false); });
  });
}

// ── Device Detection ─────────────────────────────────────────────────────────

async function refreshDevices() {
  const titleEl = document.getElementById("deviceTitle")!;
  const dot = document.getElementById("statusDot")!;

  try {
    const devices: string[] = await invoke("get_connected_devices");
    if (devices.length > 0) {
      const first = devices[0];
      // Parse serial and model from "emulator-5554  device product:... model:Pixel_Tablet ..."
      const serial = first.split(/\s+/)[0];
      const modelMatch = first.match(/model:(\S+)/);
      const model = modelMatch ? modelMatch[1].replace(/_/g, " ") : serial;
      titleEl.textContent = `ADB Toolbox — ${model} (${serial})`;
      if (devices.length > 1) {
        titleEl.textContent += ` +${devices.length - 1} more`;
      }
      dot.className = "status-dot connected";
      logOk(`Device detected: ${devices[0].trim()}`);
    } else {
      titleEl.textContent = "ADB Toolbox — No device connected";
      dot.className = "status-dot disconnected";
      logWarn("No ADB devices detected.");
    }
  } catch (err) {
    titleEl.textContent = "ADB Toolbox — ADB error";
    dot.className = "status-dot disconnected";
    logErr(`Device detection failed: ${err}`);
  }
}

// ── Play Store Modal (Search/Stream/Download) ────────────────────────────────

let cachedPackages: string[] = [];
let pipelineMode: "download" | "stream" = "download";

function initPlayStoreModal() {
  const modal = document.getElementById("pipelinePickerModal")!;
  const filter = document.getElementById("modalFilter") as HTMLInputElement;
  const list = document.getElementById("modalDataContainer")!;
  const status = document.getElementById("modalMetaStatus")!;
  let selectedPkg: string | null = null;

  function openModal(mode: "download" | "stream") {
    pipelineMode = mode;
    document.getElementById("modalHeadline")!.textContent =
      mode === "stream" ? "Play Store — Search & Stream to Device" : "Play Store — Search & Fetch APK";
    cachedPackages = [];
    list.innerHTML = "";
    filter.value = "";
    status.textContent = "Type a search term and press Enter.";
    selectedPkg = null;
    modal.classList.remove("invisible");
    filter.focus();
  }

  function renderList(items: string[]) {
    list.innerHTML = "";
    if (items.length === 0) {
      list.innerHTML = "<li class='modal-row' style='color:#888;'>No results.</li>";
      return;
    }
    for (const pkg of items) {
      const row = document.createElement("li");
      row.className = "modal-row";
      row.textContent = pkg;
      row.addEventListener("click", () => {
        list.querySelectorAll(".modal-row").forEach(r => r.classList.remove("selected"));
        row.classList.add("selected");
        selectedPkg = pkg;
      });
      row.addEventListener("dblclick", () => { selectedPkg = pkg; commitSelection(); });
      list.appendChild(row);
    }
  }

  async function commitSelection() {
    const raw = selectedPkg || list.firstElementChild?.textContent;
    if (!raw || raw === "No results.") return;

    const appId = raw.match(/\[(.*?)\]/)?.[1];
    if (!appId) { logErr("Could not parse package ID from selection."); return; }

    modal.classList.add("invisible");
    logInfo(`Play Store pipeline: ${pipelineMode} for [${appId}]...`);

    if (pipelineMode === "stream") {
      try {
        const res: string = await invoke("execute_stream_pipeline", { packageId: appId });
        logOk(res);
      } catch (err) {
        logErr(`Stream pipeline failed: ${err}`);
      }
    } else {
      try {
        const folder = await save({ title: "Choose download folder", defaultPath: `/home/loufogle/Downloads/${appId}.apk` });
        if (!folder) { logWarn("Download cancelled."); return; }
        const dir = folder.substring(0, folder.lastIndexOf("/"));
        const res: string = await invoke("download_apk", { packageId: appId, folder: dir });
        logOk(`Downloaded: ${res}`);
      } catch (err) {
        logErr(`Download failed: ${err}`);
      }
    }
  }

  // Search on Enter
  filter.addEventListener("keydown", async (e) => {
    if (e.key !== "Enter") return;
    const term = filter.value.trim();
    if (!term) return;
    status.textContent = "Searching...";
    list.innerHTML = "<li class='modal-row' style='color:#888;'>Querying gplaycli...</li>";
    try {
      cachedPackages = await invoke("search_play_store", { query: term });
      renderList(cachedPackages);
      status.textContent = `${cachedPackages.length} result(s).`;
    } catch (err) {
      list.innerHTML = `<li class='modal-row' style='color:#f44;'>Error: ${err}</li>`;
      status.textContent = "Search failed.";
    }
  });

  // Live filter cached results
  filter.addEventListener("input", () => {
    if (cachedPackages.length === 0) return;
    const term = filter.value.toLowerCase();
    renderList(cachedPackages.filter(p => p.toLowerCase().includes(term)));
  });

  document.getElementById("btnCancelPicker")!.addEventListener("click", () => modal.classList.add("invisible"));
  document.getElementById("btnConfirmPicker")!.addEventListener("click", commitSelection);

  return openModal;
}

// ── Main Init ────────────────────────────────────────────────────────────────

window.addEventListener("DOMContentLoaded", () => {
  const openPlayStoreModal = initPlayStoreModal();

  // Clear log
  document.getElementById("clearLogBtn")!.addEventListener("click", () => {
    logEl().innerHTML = "";
  });

  // Device detection
  document.getElementById("refreshDevicesBtn")!.addEventListener("click", refreshDevices);
  refreshDevices();

  // ── App & Payload Control ────────────────────────────────────────────────

  // Push File
  document.getElementById("push-file-btn")!.addEventListener("click", async () => {
    const file = await open({ title: "Select file to push", multiple: false });
    if (!file) return;

    const input = await showInputModal("Push File to Device", [
      { id: "remote", label: "Remote path on device:", placeholder: "/sdcard/Download/filename" },
    ]);
    if (!input || !input.remote.trim()) return;

    logInfo(`Pushing ${file} → ${input.remote}...`);
    try {
      const res: string = await invoke("push_file", { localPath: file as string, remotePath: input.remote });
      logOk(res.trim());
    } catch (err) {
      logErr(`Push failed: ${err}`);
    }
  });

  // Pull File
  document.getElementById("pull-file-btn")!.addEventListener("click", async () => {
    const input = await showInputModal("Pull File from Device", [
      { id: "remote", label: "Remote path on device:", placeholder: "/sdcard/Download/filename" },
    ]);
    if (!input || !input.remote.trim()) return;

    const localPath = await save({ title: "Save pulled file as", defaultPath: input.remote.split("/").pop() || "pulled_file" });
    if (!localPath) return;

    logInfo(`Pulling ${input.remote} → ${localPath}...`);
    try {
      const res: string = await invoke("pull_file", { remotePath: input.remote, localPath });
      logOk(res.trim());
    } catch (err) {
      logErr(`Pull failed: ${err}`);
    }
  });

  // Install APK
  document.getElementById("install-apk-btn")!.addEventListener("click", async () => {
    const file = await open({
      title: "Select APK to install",
      multiple: false,
      filters: [{ name: "APK", extensions: ["apk"] }],
    });
    if (!file) return;

    logInfo(`Installing ${file}...`);
    try {
      const res: string = await invoke("install_apk", { apkPath: file as string });
      logOk(res.trim());
    } catch (err) {
      logErr(`Install failed: ${err}`);
    }
  });

  // Batch Install APKs
  document.getElementById("batch-apk-btn")!.addEventListener("click", async () => {
    const dir = await open({ title: "Select directory containing APKs", directory: true });
    if (!dir) return;

    logInfo(`Batch installing APKs from ${dir}...`);
    try {
      const results: string[] = await invoke("batch_install_apks", { directory: dir as string });
      for (const line of results) {
        if (line.startsWith("\u2713")) logOk(line); else logErr(line);
      }
    } catch (err) {
      logErr(`Batch install failed: ${err}`);
    }
  });

  // Purge App Cache
  document.getElementById("purge-cache-btn")!.addEventListener("click", async () => {
    const input = await showInputModal("Purge App Cache", [
      { id: "pkg", label: "Package name:", placeholder: "com.example.app" },
    ]);
    if (!input || !input.pkg.trim()) return;

    logInfo(`Clearing cache for ${input.pkg}...`);
    try {
      const res: string = await invoke("purge_app_cache", { packageName: input.pkg });
      logOk(res.trim());
    } catch (err) {
      logErr(`Purge failed: ${err}`);
    }
  });

  // Play Store buttons
  document.getElementById("fetch-apk-btn")!.addEventListener("click", () => openPlayStoreModal("download"));
  document.getElementById("stream-apk-btn")!.addEventListener("click", () => openPlayStoreModal("stream"));

  // ── Diagnostics & Interaction ─────────────────────────────────────────────

  // Inject Macros
  document.getElementById("inject-macro-btn")!.addEventListener("click", async () => {
    const input = await showInputModal("Inject Text Macros", [
      { id: "lines", label: "One string per line:", placeholder: "Hello World\nuser@example.com\npassword123", type: "textarea" },
    ]);
    if (!input || !input.lines.trim()) return;

    const lines = input.lines.split("\n").map(l => l.trim()).filter(Boolean);
    logInfo(`Injecting ${lines.length} macro(s)...`);
    try {
      const results: string[] = await invoke("inject_text_macros", { lines });
      for (const r of results) {
        if (r.startsWith("\u2713")) logOk(r); else logErr(r);
      }
    } catch (err) {
      logErr(`Macro injection failed: ${err}`);
    }
  });

  // Logcat Snapshot
  document.getElementById("open-logcat-btn")!.addEventListener("click", async () => {
    logInfo("Fetching logcat snapshot (last 200 lines)...");
    try {
      const res: string = await invoke("get_logcat_snapshot");
      log("── Logcat Snapshot ──", "log-info");
      for (const line of res.split("\n")) {
        if (line.includes(" E ") || line.includes(" E/")) log(line, "log-err");
        else if (line.includes(" W ") || line.includes(" W/")) log(line, "log-warn");
        else log(line, "log-dim");
      }
      log("── End Logcat ──", "log-info");
    } catch (err) {
      logErr(`Logcat failed: ${err}`);
    }
  });

  // Screenshot
  document.getElementById("capture-screen-btn")!.addEventListener("click", async () => {
    const dest = await save({
      title: "Save screenshot as",
      defaultPath: `screenshot_${Date.now()}.png`,
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (!dest) return;

    logInfo("Capturing screenshot...");
    try {
      const res: string = await invoke("capture_screenshot", { saveTo: dest });
      logOk(res);
    } catch (err) {
      logErr(`Screenshot failed: ${err}`);
    }
  });

  // Record Screen
  document.getElementById("record-screen-btn")!.addEventListener("click", async () => {
    const dest = await save({
      title: "Save recording as",
      defaultPath: `recording_${Date.now()}.mp4`,
      filters: [{ name: "MP4", extensions: ["mp4"] }],
    });
    if (!dest) return;

    logInfo("Recording screen for 10 seconds...");
    try {
      const res: string = await invoke("record_screen", { saveTo: dest, durationSecs: 10 });
      logOk(res);
    } catch (err) {
      logErr(`Recording failed: ${err}`);
    }
  });

  // ── Host Storage & Machine Control ─────────────────────────────────────────

  // Copy to SD Image
  document.getElementById("copy-image-btn")!.addEventListener("click", async () => {
    const file = await open({ title: "Select file to copy to SD image", multiple: false });
    if (!file) return;

    logInfo(`Copying ${file} → ${MOUNT_POINT}...`);
    try {
      const res: string = await invoke("copy_to_sd_image", { sourcePath: file as string, mountPoint: MOUNT_POINT });
      logOk(res);
    } catch (err) {
      logErr(`Copy failed: ${err}`);
    }
  });

  // Restart Framework (dangerous — confirm)
  document.getElementById("restart-framework-btn")!.addEventListener("click", async () => {
    const ok = await showConfirm("Restart UI Framework", "This will stop and restart the Android UI framework via su. The device screen will go black briefly. Continue?");
    if (!ok) return;

    logWarn("Restarting UI framework...");
    try {
      const res: string = await invoke("restart_framework");
      logOk(res.trim() || "Framework restart command sent.");
    } catch (err) {
      logErr(`Framework restart failed: ${err}`);
    }
  });

  // Reboot Bootloader (dangerous — confirm)
  document.getElementById("bootloader-btn")!.addEventListener("click", async () => {
    const ok = await showConfirm("Reboot to Bootloader", "This will reboot the device into bootloader/fastboot mode. You will lose the active ADB connection. Continue?");
    if (!ok) return;

    logWarn("Rebooting to bootloader...");
    try {
      const res: string = await invoke("reboot_bootloader");
      logOk(res);
    } catch (err) {
      logErr(`Reboot failed: ${err}`);
    }
  });

  // Reboot Recovery (dangerous — confirm)
  document.getElementById("recovery-btn")!.addEventListener("click", async () => {
    const ok = await showConfirm("Reboot to Recovery", "This will reboot the device into recovery mode. You will lose the active ADB connection. Continue?");
    if (!ok) return;

    logWarn("Rebooting to recovery...");
    try {
      const res: string = await invoke("reboot_recovery");
      logOk(res);
    } catch (err) {
      logErr(`Reboot failed: ${err}`);
    }
  });

  logInfo("ADB Toolbox initialized.");
});
