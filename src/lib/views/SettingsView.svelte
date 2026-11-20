<script lang="ts">
  import { ACCENTS, DEFAULT_ACCENT, normalizeAccent } from "../accent";
  import { api, type AppSettings, type BlobGcReport, type DesktopPrefs, type WhisperStatus, type TranscriptionEventPayload } from "../ipc";
  import { formatFileSize } from "../format";
  import {
    settings,
    refreshSettings,
    refreshPairings,
    identity,
    refreshIdentity,
    refreshChats,
    settingsTab,
    activeView,
    quickReactions,
    messageDensity,
    chatListDensity,
    setQuickReactions,
    serverStatus,
    serverAddr,
    lanIps,
    advertisedHost,
    refreshLanIps,
    REACTION_CHOICES,
    DEFAULT_QUICK_REACTIONS,
    type SettingsTab,
  } from "../stores/app";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { getVersion } from "@tauri-apps/api/app";
  import {
    updaterPhase,
    updaterVersion,
    updaterError,
    checkForUpdate,
    installUpdate,
  } from "../stores/updater";
  import { CHANGELOG } from "../changelog";
  import { Monitor, Sun, Moon, ArrowLeft, ChevronDown, ChevronRight, Copy, Download, Upload, RefreshCw } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import DevicesPane from "../components/DevicesPane.svelte";
  import { requestConfirmation, showNotice } from "../stores/confirm";

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "sync", label: "Devices & sync" },
    { id: "interface", label: "Interface" },
    { id: "transcription", label: "Transcription" },
    { id: "backup", label: "Backup" },
    { id: "changelog", label: "Changelog" },
    { id: "about", label: "About" },
  ];

  let appVersion = $state("");
  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch {
      /* dev/browser fallback — leave blank */
    }
  });

  let port = $state(8765);
  let deviceName = $state("");
  let theme = $state<"system" | "light" | "dark">("system");
  let accent = $state<string>(DEFAULT_ACCENT);
  let discoverable = $state(false);
  let transcriptionEnabled = $state(false);
  let automaticFileDownloadLimitBytes = $state(100 * 1024 * 1024);
  let automaticFileDownloadMode = $state("104857600");
  let desktopPrefs = $state<DesktopPrefs>({
    startMinimized: false,
    closeToTray: false,
    askCloseToTray: true,
    launchOnStartup: false,
    askBeforeDeleting: true,
    autoCheckUpdates: true,
    autoBackupEnabled: false,
    autoBackupIntervalHours: 24,
    autoBackupFolder: null,
    autoBackupRetention: 10,
    lastAutoBackupAt: null,
    lastAutoBackupError: null,
    syncServerEnabled: false,
  });
  let desktopPrefsError = $state<string | null>(null);
  let saving = $state(false);
  let showGeneralAdvanced = $state(false);
  let showSyncAdvanced = $state(false);
  let showTranscriptionAdvanced = $state(false);
  let showReactionPicker = $state(false);
  let copiedDeviceId = $state(false);

  // Mirrors the titlebar dot, spelled out. "starting" is the brief window
  // between flipping the toggle on and the listener binding its port.
  const serverStatusLabel = $derived(
    $serverStatus === "running"
      ? "Ready"
      : $serverStatus === "error"
        ? "Could not start"
        : $serverStatus === "starting"
          ? "Starting…"
          : "Off",
  );
  let dirty = $state(false);

  // Platform-aware bits — whisper.cpp releases + install hints differ across
  // Windows/macOS/Linux (.exe + .dll vs. binary + .dylib/.so). Detect once
  // from the UA so the Voice-transcription card names the right files.
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  const isMac = /Mac|Darwin/.test(ua);
  const isLinux = /Linux/.test(ua) && !/Android/.test(ua);
  const cliFileName = isMac || isLinux ? "whisper-cli" : "whisper-cli.exe";
  const libExt = isMac ? ".dylib" : isLinux ? ".so" : ".dll";
  const ffmpegFileName = isMac || isLinux ? "ffmpeg" : "ffmpeg.exe";

  /** The last `$settings` we mirrored into the fields below. Every save round-trips
   *  through `refreshSettings()`, so without this the effect would re-run on an
   *  unrelated toggle and stomp whatever the user is currently typing. Free-text
   *  fields are therefore only adopted when the *stored* value actually moved —
   *  i.e. the change came from elsewhere, not from our own save. */
  let syncedSettings: AppSettings | null = null;

  $effect(() => {
    if ($settings) {
      const editable = syncedSettings;
      if (!editable || editable.port !== $settings.port) {
        port = $settings.port;
        dirty = false;
      }
      if (!editable || editable.deviceName !== $settings.deviceName) {
        deviceName = $settings.deviceName;
      }
      syncedSettings = $settings;
      theme = $settings.theme as "system" | "light" | "dark";
      accent = normalizeAccent($settings.accent);
      discoverable = $settings.discoverable;
      transcriptionEnabled = $settings.transcriptionEnabled ?? false;
      automaticFileDownloadLimitBytes =
        $settings.automaticFileDownloadLimitBytes ?? 100 * 1024 * 1024;
      cutoffMs = $settings.mediaBackfillCutoffMs ?? 0;
      automaticFileDownloadMode = [0, 25, 100, 250, 500, 1024]
        .map((mib) => String(mib * 1024 * 1024))
        .includes(String(automaticFileDownloadLimitBytes))
          ? String(automaticFileDownloadLimitBytes)
          : "custom";
    }
  });

  async function applyDiscoverable(value: boolean) {
    const previous = discoverable;
    discoverable = value;
    try {
      await persistSettings({ discoverable: value });
      await refreshLanIps();
    } catch {
      discoverable = previous;
    }
  }

  async function pickAdvertisedHost(ip: string) {
    await api.setAdvertisedHost(ip);
    await refreshLanIps();
    await refreshPairings();
  }

  function markDirty() {
    dirty = true;
  }

  function settingsPayload(): AppSettings {
    return {
      port,
      deviceName,
      theme,
      discoverable,
      accent,
      transcriptionEnabled,
      automaticFileDownloadLimitBytes,
      mediaBackfillCutoffMs: cutoffMs,
    };
  }

  async function persistSettings(changes: Partial<AppSettings>) {
    const current = $settings ?? settingsPayload();
    await api.updateSettings({ ...current, ...changes });
    await refreshSettings();
  }

  // Theme and accent apply and persist immediately.
  async function applyThemeNow(value: "system" | "light" | "dark") {
    if (theme === value) return;
    theme = value;
    markDirty();
    try {
      await persistSettings({ theme: value });
    } catch {
      theme = $settings?.theme as typeof theme ?? "system";
    }
  }
  async function applyAccentNow(value: string) {
    if (accent === value) return;
    accent = value;
    markDirty();
    try {
      await persistSettings({ accent: value });
    } catch {
      accent = normalizeAccent($settings?.accent);
    }
  }

  // --- Voice transcription (whisper.cpp) ---
  let whisperStatus = $state<WhisperStatus | null>(null);
  let whisperError = $state<string | null>(null);
  let whisperBusy = $state(false);
  let downloadProgress = $state<{ written: number; total: number } | null>(null);
  let downloadStage = $state<"runtime" | "model" | null>(null);
  let pendingModelId = $state<string | null>(null);
  let transcriptionUnlisten: UnlistenFn | null = null;

  async function refreshWhisperStatus() {
    try {
      whisperStatus = await api.transcriptionStatus();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "status probe failed");
    }
  }

  async function applyTranscriptionToggle(v: boolean) {
    if (transcriptionEnabled === v) return;
    transcriptionEnabled = v;
    try {
      await persistSettings({ transcriptionEnabled: v });
    } catch {
      /* revert on failure */
      transcriptionEnabled = !v;
    }
  }

  async function setupWhisper() {
    if (whisperBusy) return;
    whisperBusy = true;
    whisperError = null;
    downloadProgress = { written: 0, total: 0 };
    try {
      await api.transcriptionSetup();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "setup failed");
    } finally {
      whisperBusy = false;
      downloadProgress = null;
      downloadStage = null;
      await refreshWhisperStatus();
    }
  }
  async function deleteWhisperModel() {
    if (whisperBusy) return;
    if (!await requestConfirmation("You will have to download this model again to use it.", { title: "Delete speech model", confirmLabel: "Delete", dangerous: true })) return;
    whisperBusy = true;
    try {
      await api.transcriptionDeleteModel();
      await refreshWhisperStatus();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "delete failed");
    } finally {
      whisperBusy = false;
    }
  }
  async function openWhisperFolder() {
    try {
      await api.transcriptionOpenFolder();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "open folder failed");
    }
  }

  function fmtMB(bytes: number | null | undefined): string {
    if (!bytes) return "";
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(async () => {
    await refreshWhisperStatus();
    transcriptionUnlisten = await api.onTranscriptionEvent((ev: TranscriptionEventPayload) => {
      if (ev.kind === "download-progress") {
        downloadProgress = { written: ev.written, total: ev.total };
        downloadStage = ev.stage ?? "model";
      } else if (ev.kind === "download-complete") {
        downloadProgress = null;
        downloadStage = null;
        void refreshWhisperStatus();
      } else if (ev.kind === "download-error") {
        whisperError = ev.message;
        downloadProgress = null;
        downloadStage = null;
      }
    });
  });
  onDestroy(() => {
    transcriptionUnlisten?.();
  });

  onMount(async () => {
    try {
      desktopPrefs = await api.getDesktopPrefs();
    } catch (e: any) {
      desktopPrefsError = typeof e === "string" ? e : (e?.message ?? "could not load desktop preferences");
    }
  });

  const themeModes: { value: "system" | "light" | "dark"; label: string; icon: any }[] = [
    { value: "system", label: "System", icon: Monitor },
    { value: "light", label: "Light", icon: Sun },
    { value: "dark", label: "Dark", icon: Moon },
  ];
  // Swatches show each accent as it renders in the current theme.
  const accents = $derived(ACCENTS.map((a) => ({
    name: a.name,
    label: a.label,
    color: document.documentElement.dataset.theme === "dark" ? a.dark : a.light,
  })));
  let changelogSeries = $state("all");
  const changelogSeriesOptions = $derived([...new Set(CHANGELOG.map((entry) => entry.version.split(".").slice(0, 2).join(".")))]);
  const visibleChangelog = $derived(changelogSeries === "all" ? CHANGELOG : CHANGELOG.filter((entry) => entry.version.startsWith(`${changelogSeries}.`)));

  function toggleQuickReaction(emoji: string) {
    if ($quickReactions.includes(emoji)) {
      if ($quickReactions.length <= 1) return;
      setQuickReactions($quickReactions.filter((x) => x !== emoji));
    } else {
      setQuickReactions([...$quickReactions, emoji]);
    }
  }

  function resetQuickReactions() {
    setQuickReactions(DEFAULT_QUICK_REACTIONS);
  }

  async function updateDesktopPref<K extends keyof DesktopPrefs>(key: K, value: DesktopPrefs[K]) {
    const previous = desktopPrefs;
    desktopPrefs = { ...desktopPrefs, [key]: value };
    desktopPrefsError = null;
    try {
      desktopPrefs = await api.updateDesktopPrefs(desktopPrefs);
    } catch (e: any) {
      desktopPrefs = previous;
      desktopPrefsError = typeof e === "string" ? e : (e?.message ?? "could not save desktop preference");
    }
  }

  async function copyDeviceId() {
    if (!$identity?.deviceId) return;
    await writeText($identity.deviceId);
    copiedDeviceId = true;
    setTimeout(() => copiedDeviceId = false, 1200);
  }

  async function saveDeviceName() {
    if (saving) return;
    saving = true;
    try {
      await persistSettings({ deviceName });
      await refreshIdentity();
    } finally {
      saving = false;
    }
  }

  async function applyPort() {
    if (saving || !dirty) return;
    saving = true;
    try {
      await persistSettings({ port });
      dirty = false;
    } finally {
      saving = false;
    }
  }

  // --- Historical media. ---
  // Pairing into an established mesh replays the whole history at once, so without
  // a cutoff this device would try to pull every attachment ever sent. Older media
  // is listed with a Download button instead. Turning this on clears the cutoff and
  // lets the sweep collect the backlog; it cannot be re-armed, because the stamp
  // records when this device joined and moving it forward would re-classify media
  // that has already arrived.
  let cutoffMs = $state(0);
  let backfillEverything = $derived(cutoffMs === 0);
  let backfillHint = $derived(
    cutoffMs === 0
      ? "Fetching everything, including media sent before this device was paired."
      : "Media from before " +
        new Date(cutoffMs).toLocaleDateString() +
        " is listed with a Download button instead.",
  );
  async function applyBackfill(everything: boolean) {
    // Turning it off stamps now, so only media from here on arrives by itself.
    cutoffMs = everything ? 0 : Date.now();
    await persistSettings({ mediaBackfillCutoffMs: cutoffMs });
  }

  // --- Force a full replay to every paired device. ---
  // The outbound watermark can end up ahead of what a device actually stored, and
  // everything under it then becomes unreachable — a deletion stranded that way
  // never arrives, while a manual sync truthfully reports nothing to send.
  let resyncing = $state(false);
  let resyncNote = $state("Re-send the whole history to every device. Use if a device is missing messages or still shows ones you deleted.");
  async function forceResync() {
    if (resyncing) return;
    resyncing = true;
    try {
      const cleared = await api.forceFullResync();
      resyncNote = cleared > 0
        ? `Cleared ${cleared} sync marker${cleared === 1 ? "" : "s"}. Each device replays on its next connection.`
        : "Nothing to clear — no device has recorded sync progress yet.";
    } catch (e) {
      resyncNote = `Could not clear sync markers: ${e}`;
    } finally {
      resyncing = false;
    }
  }

  // --- Backup / restore (this desktop's own notes). ---
  let backupBusy = $state(false);

  // --- Media storage ---
  // The blob store is content-addressed and append-only, so deleting a message never
  // removed its attachment. Scan first and show what would go before removing it.
  let storageReport = $state<BlobGcReport | null>(null);
  let storageBusy = $state(false);
  let storageError = $state<string | null>(null);

  async function runStorageScan(dryRun: boolean) {
    if (storageBusy) return;
    storageBusy = true;
    storageError = null;
    try {
      storageReport = await api.collectUnusedBlobs(dryRun);
    } catch (e: any) {
      storageError = typeof e === "string" ? e : (e?.message ?? "storage scan failed");
    } finally {
      storageBusy = false;
    }
  }

  const measureStorage = () => runStorageScan(true);

  async function cleanUpStorage() {
    const freeing = formatFileSize(storageReport?.reclaimedBytes ?? 0);
    if (
      !(await requestConfirmation(
        `Delete ${freeing} of media that no message refers to any more? This cannot be undone, though anything still on a paired device can be downloaded again.`,
        { title: "Free up space", confirmLabel: "Delete", dangerous: true },
      ))
    ) {
      return;
    }
    await runStorageScan(false);
    // Re-measure so the panel reflects the store as it now stands.
    if (!storageError) await runStorageScan(true);
  }
  let backupMsg = $state<string | null>(null);

  async function doExport() {
    if (backupBusy) return;
    backupMsg = null;
    let dest: string | null;
    try {
      const stamp = new Date().toISOString().slice(0, 10);
      dest = await saveDialog({
        title: "Save Ncy backup",
        defaultPath: `ncy-desktop-backup-${stamp}.zip`,
        filters: [{ name: "Zip", extensions: ["zip"] }],
      });
    } catch (e: any) {
      backupMsg = typeof e === "string" ? e : (e?.message ?? "could not open save dialog");
      return;
    }
    if (!dest) return;
    backupBusy = true;
    try {
      await api.exportBackup(dest);
      backupMsg = "Backup saved.";
    } catch (e: any) {
      backupMsg = typeof e === "string" ? e : (e?.message ?? "export failed");
    } finally {
      backupBusy = false;
    }
  }

  async function doImport() {
    if (backupBusy) return;
    backupMsg = null;
    let src: string | string[] | null;
    try {
      src = await openDialog({
        title: "Choose an Ncy backup",
        multiple: false,
        directory: false,
        filters: [{ name: "Zip", extensions: ["zip"] }],
      });
    } catch (e: any) {
      backupMsg = typeof e === "string" ? e : (e?.message ?? "could not open file dialog");
      return;
    }
    const path = Array.isArray(src) ? src[0] : src;
    if (!path) return;
    if (!await requestConfirmation("Ncy will merge new events and media into your local chats.", { title: "Import backup", confirmLabel: "Import" })) return;
    backupBusy = true;
    try {
      const r = await api.importBackup(path);
      await refreshChats();
      backupMsg = `Imported ${r.events} new event${r.events === 1 ? "" : "s"} across ${r.chats} chat${r.chats === 1 ? "" : "s"}, ${r.blobs} media file${r.blobs === 1 ? "" : "s"}.`;
    } catch (e: any) {
      backupMsg = typeof e === "string" ? e : (e?.message ?? "import failed");
    } finally {
      backupBusy = false;
    }
  }
  function chosenModelId(): string {
    return pendingModelId ?? whisperStatus?.modelId ?? "tiny";
  }
  function stageModelQuality(quality: "tiny" | "base" | "small") {
    pendingModelId = `${quality}${chosenModelId().endsWith(".en") ? ".en" : ""}`;
  }
  function stageModelLanguage(englishOnly: boolean) {
    pendingModelId = `${chosenModelId().split(".")[0]}${englishOnly ? ".en" : ""}`;
  }
  async function applyWhisperModel() {
    const modelId = chosenModelId();
    if (modelId === whisperStatus?.modelId || whisperBusy) return;
    const sizes = { tiny: "75 MiB", base: "142 MiB", small: "466 MiB" };
    const quality = modelId.split(".")[0] as "tiny" | "base" | "small";
    const cached = whisperStatus?.cachedModelIds.includes(modelId) ?? false;
    if (!cached && !await requestConfirmation(`Download ${modelId} (~${sizes[quality]}) and use it for transcription?`, { title: "Download speech model", confirmLabel: "Download" })) return;
    whisperBusy = true;
    whisperError = null;
    downloadStage = "model";
    downloadProgress = { written: 0, total: 0 };
    try {
      await api.transcriptionSelectModel(modelId);
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "model download failed");
    } finally {
      whisperBusy = false;
      downloadProgress = null;
      downloadStage = null;
      await refreshWhisperStatus();
      if (!whisperError) pendingModelId = null;
    }
  }
  async function removeUnusedWhisperModels() {
    const bytes = whisperStatus?.unusedModelBytes ?? 0;
    if (!bytes || whisperBusy) return;
    if (!await requestConfirmation(`Remove unused speech models and recover ${fmtMB(bytes)}? The active model will be kept.`, { title: "Remove unused models", confirmLabel: "Remove", dangerous: true })) return;
    whisperBusy = true;
    whisperError = null;
    try {
      await api.transcriptionRemoveUnusedModels();
      await refreshWhisperStatus();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "cleanup failed");
    } finally {
      whisperBusy = false;
    }
  }

  async function chooseAutoBackupFolder() {
    const picked = await openDialog({ title: "Choose automatic backup folder", directory: true, multiple: false });
    const folder = Array.isArray(picked) ? picked[0] : picked;
    if (folder) await updateDesktopPref("autoBackupFolder", folder);
  }

  async function runAutoBackupNow() {
    backupBusy = true; backupMsg = null;
    try {
      await api.runAutomaticBackup();
      const now = Date.now();
      await updateDesktopPref("lastAutoBackupAt", now);
      await updateDesktopPref("lastAutoBackupError", null);
      backupMsg = "Automatic backup saved.";
    } catch (e: any) {
      backupMsg = typeof e === "string" ? e : (e?.message ?? "automatic backup failed");
    } finally { backupBusy = false; }
  }

  async function purgeTombstones() {
    if (!await requestConfirmation(
      "Only continue if every old device and backup containing deleted chats or messages is gone. " +
      "Stale history paired later can restore them. This cannot be undone.",
      { title: "Forget deletion records", confirmLabel: "Forget", dangerous: true },
    )) return;
    const removed = await api.purgeChatTombstones();
    backupMsg = `Forgot ${removed} deletion record${removed === 1 ? "" : "s"}.`;
  }
</script>

<div class="page-scroll settings">
 <div class="page">
  <div class="settings-layout">
   <nav class="side" aria-label="Settings sections">
     <div class="side-head">
       <button class="back-btn" onclick={() => activeView.set("chats")} title="Back to chats" aria-label="Back to chats"><ArrowLeft size={17} strokeWidth={2.5} /></button>
       <h1>Settings</h1>
     </div>
     {#each tabs as t (t.id)}
       <button
         type="button"
         class="side-link"
         class:active={$settingsTab === t.id}
         onclick={() => settingsTab.set(t.id)}
       >{t.label}</button>
     {/each}
   </nav>


   <div class="panes">
  <div class="page-head">
    <h2 class="page-title">{tabs.find((t) => t.id === $settingsTab)?.label ?? "Settings"}</h2>
    {#if $settingsTab === "sync"}<p class="page-desc">Ncy only talks to devices you paired, on the same network.</p>{/if}
  </div>
  {#if $settingsTab === "general"}
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">This computer</div>
      <div class="opt-list">
        <label class="field">
          <span class="field-label">Device name</span>
          <input type="text" bind:value={deviceName} onchange={saveDeviceName} />
        </label>
        <button type="button" class="disclosure-row" onclick={() => showGeneralAdvanced = !showGeneralAdvanced}>
          <span>
            <strong>Installation details</strong>
            <small class="inline-hint">Technical identity for this installation.</small>
          </span>
          {#if showGeneralAdvanced}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}
        </button>
        {#if showGeneralAdvanced && $identity}
          <div class="opt-row nested-row">
            <span class="opt-label">Device ID<span class="inline-hint mono">{showGeneralAdvanced ? $identity.deviceId : `${$identity.deviceId.slice(0, 8)}…`}</span></span>
            <button type="button" class="ghost compact-btn" onclick={copyDeviceId}><Copy size={14} /> {copiedDeviceId ? "Copied" : "Copy"}</button>
          </div>
        {/if}
      </div>
    </div>
    <div class="settings-subsection">
      <div class="subsection-title">When Ncy starts and closes</div>
      <div class="opt-list">
        <label class="opt-row"><span class="opt-label">Launch on startup</span><span class="switch"><input type="checkbox" checked={desktopPrefs.launchOnStartup} onchange={(e) => updateDesktopPref("launchOnStartup", e.currentTarget.checked)} /><span class="slider"></span></span></label>
        <label class="opt-row"><span class="opt-label">Start minimized</span><span class="switch"><input type="checkbox" checked={desktopPrefs.startMinimized} onchange={(e) => updateDesktopPref("startMinimized", e.currentTarget.checked)} /><span class="slider"></span></span></label>
        <label class="opt-row"><span class="opt-label">Close to tray<span class="inline-hint">Closing the window keeps Ncy running in the tray.</span></span><span class="switch"><input type="checkbox" checked={desktopPrefs.closeToTray} onchange={(e) => updateDesktopPref("closeToTray", e.currentTarget.checked)} /><span class="slider"></span></span></label>
        {#if !desktopPrefs.closeToTray}
          <label class="opt-row nested-row"><span class="opt-label">Ask when closing<span class="inline-hint">Offer the tray before quitting.</span></span><span class="switch"><input type="checkbox" checked={desktopPrefs.askCloseToTray} onchange={(e) => updateDesktopPref("askCloseToTray", e.currentTarget.checked)} /><span class="slider"></span></span></label>
        {/if}
      </div>
    </div>
    <div class="settings-subsection">
      <div class="subsection-title">Safety</div>
      <div class="opt-list">
        <label class="opt-row"><span class="opt-label">Confirm before deleting</span><span class="switch"><input type="checkbox" checked={desktopPrefs.askBeforeDeleting} onchange={(e) => updateDesktopPref("askBeforeDeleting", e.currentTarget.checked)} /><span class="slider"></span></span></label>
        {#if desktopPrefsError}<div class="field"><span class="tiny" style="color:var(--danger);">{desktopPrefsError}</span></div>{/if}
      </div>
    </div>
  </section>
  {:else if $settingsTab === "sync"}
  <section class="group">
    <label class="status-card">
      <span class="status-ic"><RefreshCw size={18} strokeWidth={2.25} /></span>
      <span class="status-text">
        <strong>Sync between devices</strong>
        <span class="mono tiny muted" class:status-err={$serverStatus === "error"}>{serverStatusLabel}{$serverAddr ? ` · ${$serverAddr}` : ""} · {discoverable ? "discoverable" : "not discoverable"}</span>
      </span>
      <span class="switch"><input type="checkbox" checked={desktopPrefs.syncServerEnabled} onchange={(e) => updateDesktopPref("syncServerEnabled", e.currentTarget.checked)} /><span class="slider"></span></span>
    </label>
  </section>

  <div class:sync-disabled={!desktopPrefs.syncServerEnabled}>
    <DevicesPane />
  </div>

  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Files</div>
      <div class="opt-list">
        <label class="field">
          <span class="field-label">Automatic file downloads</span>
          <select value={automaticFileDownloadMode} onchange={async (event) => { const value = event.currentTarget.value; automaticFileDownloadMode = value; if (value !== "custom") { automaticFileDownloadLimitBytes = Number(value); await persistSettings({ automaticFileDownloadLimitBytes }); } }}>
            <option value="0">Always ask</option><option value={String(25 * 1024 * 1024)}>25 MB</option><option value={String(100 * 1024 * 1024)}>100 MB</option><option value={String(250 * 1024 * 1024)}>250 MB</option><option value={String(500 * 1024 * 1024)}>500 MB</option><option value={String(1024 * 1024 * 1024)}>1 GB</option><option value="custom">Custom…</option>
          </select>
          {#if automaticFileDownloadMode === "custom"}<input type="number" min="1" max="1024" step="1" value={Math.max(1, Math.round(automaticFileDownloadLimitBytes / (1024 * 1024)))} aria-label="Custom automatic file download limit in megabytes" onchange={async (event) => { const mib = Math.max(1, Math.min(1024, Number(event.currentTarget.value) || 1)); automaticFileDownloadLimitBytes = mib * 1024 * 1024; await persistSettings({ automaticFileDownloadLimitBytes }); }} />{/if}
          <span class="tiny muted">Larger files remain available to download manually.</span>
        </label>
        <label class="opt-row">
          <span class="opt-label">Download media from before pairing<span class="inline-hint">{backfillHint}</span></span>
          <span class="switch"><input type="checkbox" checked={backfillEverything} onchange={(e) => applyBackfill(e.currentTarget.checked)} /><span class="slider"></span></span>
        </label>
      </div>
    </div>
    <div class="settings-subsection">
      <button type="button" class="disclosure-row technical-disclosure" aria-expanded={showSyncAdvanced} onclick={() => showSyncAdvanced = !showSyncAdvanced}>
        <span><strong>Advanced</strong><small class="inline-hint">Troubleshooting, network details and recent pairing requests.</small></span>
        {#if showSyncAdvanced}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}
      </button>
      {#if showSyncAdvanced}
        <div class="opt-list">
        <div class="opt-row">
          <span class="opt-label">Replay everything<span class="inline-hint">{resyncNote}</span></span>
          <button class="ghost" onclick={forceResync} disabled={resyncing}>{resyncing ? "Clearing…" : "Force full resync"}</button>
        </div>
          <label class="opt-row nested-row">
            <span class="opt-label">Discoverable on local network<span class="inline-hint">Helps paired devices find this desktop after its address changes.</span></span>
            <span class="switch"><input type="checkbox" checked={discoverable} onchange={(e) => applyDiscoverable(e.currentTarget.checked)} /><span class="slider"></span></span>
          </label>
          <div class="opt-row nested-row">
            <span class="opt-label">Local sync address<span class="inline-hint">Used for direct connections on this network.</span></span>
            <span class="muted mono tiny">{$serverAddr || "Not listening"}</span>
          </div>
          <div class="field nested-row">
            <span class="field-label">Advertised address</span>
            <div class="row ip-row">
              {#each $lanIps as ip (ip)}
                <button class="chip" class:active={$advertisedHost === ip} onclick={() => pickAdvertisedHost(ip)}>{ip}</button>
              {/each}
              <button class="ghost" onclick={() => refreshLanIps()}>Refresh</button>
            </div>
            <span class="tiny muted">Guest Wi-Fi and virtual adapters may block or misroute local discovery.</span>
          </div>
          <div class="opt-row nested-row">
            <label class="opt-label" for="sync-port">Sync port<span class="inline-hint">Restart Ncy after applying.</span></label>
            <div class="row technical-control"><input id="sync-port" type="number" min="1" max="65535" bind:value={port} oninput={markDirty} /><button onclick={applyPort} disabled={saving || !dirty}>{saving ? "Applying…" : "Apply"}</button></div>
          </div>
        </div>
        <DevicesPane part="requests" />
      {/if}
    </div>
  </section>
  {:else if $settingsTab === "interface"}
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Appearance</div>
      <div class="opt-list">
        <div class="opt-row"><span class="opt-label">Theme</span><div class="seg">{#each themeModes as m (m.value)}{@const Icon = m.icon}<button type="button" class="seg-btn" class:on={theme === m.value} onclick={() => applyThemeNow(m.value)}><Icon size={16} />{m.label}</button>{/each}</div></div>
        <div class="opt-row"><span class="opt-label">Accent color</span><div class="accents">{#each accents as a (a.name)}<span class="swatch-wrap" class:on={accent === a.name}><button type="button" class="swatch" class:on={accent === a.name} style={`--sw:${a.color};`} aria-label={a.label} aria-pressed={accent === a.name} onclick={() => applyAccentNow(a.name)}></button>{a.label}</span>{/each}</div></div>
      </div>
    </div>
    <div class="settings-subsection">
      <div class="subsection-title">Layout</div>
      <div class="opt-list">
        <div class="opt-row"><span class="opt-label">Chat list density</span><div class="seg density-seg"><button class="seg-btn" class:on={$chatListDensity === "comfortable"} onclick={() => chatListDensity.set("comfortable")}>Comfortable</button><button class="seg-btn" class:on={$chatListDensity === "compact"} onclick={() => chatListDensity.set("compact")}>Compact</button></div></div>
        <div class="opt-row"><span class="opt-label">Message density</span><div class="seg density-seg"><button class="seg-btn" class:on={$messageDensity === "comfortable"} onclick={() => messageDensity.set("comfortable")}>Comfortable</button><button class="seg-btn" class:on={$messageDensity === "compact"} onclick={() => messageDensity.set("compact")}>Compact</button></div></div>
      </div>
    </div>
    <div class="settings-subsection">
      <div class="subsection-title">Messages</div>
      <div class="opt-list">
        <div class="opt-row">
          <span class="opt-label">Quick reactions<span class="inline-hint">Choose the reactions shown above messages.</span></span>
          <div class="row reaction-summary">{#each $quickReactions as emoji (emoji)}<span>{emoji}</span>{/each}<button class="ghost compact-btn" onclick={() => showReactionPicker = !showReactionPicker}>{showReactionPicker ? "Done" : "Configure…"}</button></div>
        </div>
        {#if showReactionPicker}
          <div class="reaction-picker">
            <div class="reaction-choice-grid">{#each REACTION_CHOICES as emoji (emoji)}{@const selected = $quickReactions.includes(emoji)}<button class="reaction-choice" class:on={selected} title={selected ? `Hide ${emoji}` : `Show ${emoji}`} onclick={() => toggleQuickReaction(emoji)}>{emoji}</button>{/each}</div>
            <button class="subtle reset-reactions" onclick={resetQuickReactions}>Reset to defaults</button>
          </div>
        {/if}
      </div>
    </div>
  </section>
  {:else if $settingsTab === "transcription"}
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Transcription</div>
      <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">
          Auto-transcribe voice messages
          {#if whisperStatus && (!whisperStatus.cliPresent || !whisperStatus.modelPresent)}
            <span class="inline-hint">Available after setup.</span>
          {/if}
        </span>
        <span class="switch">
          <input
            type="checkbox"
            checked={transcriptionEnabled}
            disabled={!whisperStatus?.cliPresent || !whisperStatus?.modelPresent || whisperBusy}
            onchange={(e) => applyTranscriptionToggle((e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      <div class="opt-row">
        <span class="opt-label">
          Local transcription
          {#if whisperStatus?.cliPresent && whisperStatus?.modelPresent}
            <span class="inline-hint" style="color:var(--success);">Ready · runs entirely on this device</span>
          {:else if downloadProgress}
            <span class="inline-hint">Setting up {downloadStage === "runtime" ? "Whisper" : "speech model"}…</span>
          {:else if isMac || isLinux}
            <span class="inline-hint">Automatic setup is currently available on 64-bit Windows.</span>
          {:else}
            <span class="inline-hint">Downloads Whisper and the multilingual model (about 78 MB).</span>
          {/if}
        </span>
        {#if !whisperStatus?.cliPresent || !whisperStatus?.modelPresent}
          {#if isMac || isLinux}
            <button onclick={() => showTranscriptionAdvanced = true}>Show instructions</button>
          {:else}
            <button onclick={setupWhisper} disabled={whisperBusy || !!downloadProgress}>
              {whisperStatus?.cliPresent || whisperStatus?.modelPresent ? "Repair setup" : "Set up"}
            </button>
          {/if}
        {/if}
      </div>
      {#if downloadProgress}
        {@const pct = downloadProgress.total
          ? Math.floor((downloadProgress.written / downloadProgress.total) * 100)
          : null}
        <div class="panel">
          <span class="tiny muted">
            {downloadStage === "runtime" ? "Whisper runtime" : "Speech model"} · {fmtMB(downloadProgress.written)}{downloadProgress.total ? ` / ${fmtMB(downloadProgress.total)} (${pct}%)` : ""}
          </span>
          <div class="progress">
            <div class="progress-bar" style={`width:${pct ?? 0}%`}></div>
          </div>
        </div>
      {/if}
      <div class="opt-row">
        <span class="opt-label">
          Models & installation
          <span class="inline-hint">Installation details and maintenance.</span>
        </span>
        <button type="button" class="subtle compact-btn" onclick={() => showTranscriptionAdvanced = !showTranscriptionAdvanced}>
          {showTranscriptionAdvanced ? "Hide" : "Show"}
        </button>
      </div>
      {#if showTranscriptionAdvanced}
        <div class="field">
          <span class="field-label">Whisper install folder</span>
          <div class="row wrap-row">
            <code class="id path-id">{whisperStatus?.dir ?? "…"}</code>
            <button class="ghost" onclick={openWhisperFolder}>Open folder</button>
          </div>
        </div>

        <div class="field">
          <span class="field-label">Whisper CLI (<code>{cliFileName}</code>)</span>
          {#if whisperStatus?.cliPresent}
            <span class="tiny" style="color:var(--success);">
              Installed · {whisperStatus.runtimeVersion ? `whisper.cpp ${whisperStatus.runtimeVersion}` : "version unknown (manual installation)"}
            </span>
          {:else}
            <span class="tiny" style="color:var(--warning);">
              Not installed.{#if isMac || isLinux} Extract a
              <a href="https://github.com/ggml-org/whisper.cpp/releases" target="_blank" rel="noreferrer">whisper.cpp release</a>
              into the folder above: <code>{cliFileName}</code> plus its sibling
              <code>{libExt}</code> libraries, then <code>chmod +x {cliFileName}</code>.{/if}
            </span>
          {/if}
        </div>

        <div class="field">
          <span class="field-label">Speech model · {whisperStatus?.modelLabel ?? "Whisper tiny · Fast · Multilingual"}</span>
          <div class="row wrap-row">
            <span class="tiny" style={whisperStatus?.modelPresent ? "color:var(--success);" : "color:var(--warning);"}>
              {whisperStatus?.modelPresent ? `Installed${whisperStatus.modelSizeBytes ? ` · ${fmtMB(whisperStatus.modelSizeBytes)}` : ""}` : "Not installed"}
            </span>
            {#if whisperStatus?.modelPresent}
              <button class="ghost" onclick={deleteWhisperModel} disabled={whisperBusy}>Delete model</button>
            {/if}
          </div>
        </div>

        <div class="field">
          <span class="field-label">Quality</span>
          <div class="model-tabs" aria-label="Transcription model quality">
            <button class="model-tab" class:on={chosenModelId().startsWith("tiny")} disabled={whisperBusy} onclick={() => stageModelQuality("tiny")}>Fast</button>
            <button class="model-tab" class:on={chosenModelId().startsWith("base")} disabled={whisperBusy} onclick={() => stageModelQuality("base")}>Balanced</button>
            <button class="model-tab" class:on={chosenModelId().startsWith("small")} disabled={whisperBusy} onclick={() => stageModelQuality("small")}>Accurate</button>
          </div>
          <span class="tiny muted">
            {chosenModelId().startsWith("tiny")
              ? "Fast uses Whisper Tiny."
              : chosenModelId().startsWith("base")
                ? "Balanced uses Whisper Base."
                : "Accurate uses Whisper Small."}
          </span>
        </div>

        <div class="field">
          <span class="field-label">Language support</span>
          <div class="model-tabs" aria-label="Transcription model language support">
            <button class="model-tab" class:on={!chosenModelId().endsWith(".en")} disabled={whisperBusy} onclick={() => stageModelLanguage(false)}>Multilingual</button>
            <button class="model-tab" class:on={chosenModelId().endsWith(".en")} disabled={whisperBusy} onclick={() => stageModelLanguage(true)}>English only</button>
          </div>
        </div>

        {#if pendingModelId && pendingModelId !== whisperStatus?.modelId}
          {@const pendingQuality = pendingModelId.split(".")[0] as "tiny" | "base" | "small"}
          {@const pendingSize = { tiny: "75 MiB", base: "142 MiB", small: "466 MiB" }[pendingQuality]}
          <div class="opt-row model-apply-row">
            <span class="opt-label">
              Selected: <strong>{pendingModelId}</strong>
              <span class="inline-hint">
                {whisperStatus?.cachedModelIds.includes(pendingModelId) ? "Already installed" : `Download size: ~${pendingSize}`}
              </span>
            </span>
            <button disabled={whisperBusy} onclick={applyWhisperModel}>
              {whisperStatus?.cachedModelIds.includes(pendingModelId) ? "Use model" : "Download and use"}
            </button>
          </div>
        {/if}

        <div class="opt-row">
          <span class="opt-label">
            Cached models
            <span class="inline-hint">Downloaded models are kept for quick switching.</span>
          </span>
          <button class="ghost" disabled={whisperBusy || !(whisperStatus?.unusedModelBytes)} onclick={removeUnusedWhisperModels}>
            Remove unused models{whisperStatus?.unusedModelBytes ? ` · ${fmtMB(whisperStatus.unusedModelBytes)}` : ""}
          </button>
        </div>

        <div class="field">
          <span class="field-label">ffmpeg (optional)</span>
          {#if whisperStatus?.ffmpegPresent}
            <span class="tiny" style="color:var(--success);">
              Found next to whisper-cli or on PATH.
            </span>
          {:else}
            <span class="tiny muted">
              Only needed to decode old webm/opus voice notes. Put
              <code>{ffmpegFileName}</code> in the folder above or on PATH.
            </span>
          {/if}
        </div>
      {/if}
    </div>

      </div>
    {#if whisperError}
      <div class="panel">
        <span class="tiny" style="color:var(--danger);">{whisperError}</span>
      </div>
    {/if}
  </section>
  {:else if $settingsTab === "backup"}
  <section class="group">
    <div class="status-card backup-card">
      <span class="status-ic"><Download size={18} strokeWidth={2.25} /></span>
      <span class="status-text">
        {#if desktopPrefs.autoBackupEnabled}
          <strong>{desktopPrefs.lastAutoBackupAt ? `Last backup ${new Date(desktopPrefs.lastAutoBackupAt).toLocaleString()}` : "No automatic backup yet"}</strong>
          {#if desktopPrefs.lastAutoBackupError}
            <span class="tiny status-err">{desktopPrefs.lastAutoBackupError}</span>
          {:else}
            <span class="mono tiny muted">{desktopPrefs.autoBackupFolder ?? "No folder selected"}</span>
          {/if}
        {:else}
          <strong>Automatic backups are off</strong>
          <span class="tiny muted">Turn them on below, or export everything by hand.</span>
        {/if}
      </span>
      {#if desktopPrefs.autoBackupEnabled}
        <button onclick={runAutoBackupNow} disabled={backupBusy || !desktopPrefs.autoBackupFolder}>Back up now</button>
      {/if}
    </div>
  </section>
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Media storage</div>
      <div class="opt-list">
        <div class="opt-row">
          <span class="opt-label">
            Downloaded media
            <span class="inline-hint">
              {#if storageReport}
                {formatFileSize(storageReport.totalBytes)} across {storageReport.scanned} files
                {#if storageReport.deleted > 0} · {formatFileSize(storageReport.reclaimedBytes)} in {storageReport.deleted} unused {storageReport.deleted === 1 ? "file" : "files"}{/if}
              {:else}
                Photos, voice messages and attachments kept for the chats that use them.
              {/if}
            </span>
          </span>
          <button class="ghost backup-row-button" onclick={measureStorage} disabled={storageBusy}>
            {storageBusy ? "Scanning…" : "Scan"}
          </button>
        </div>
        {#if storageReport && storageReport.lockedChats.length > 0}
          <div class="field">
            <span class="tiny" style="color:var(--warning);">
              {storageReport.lockedChats.length} protected {storageReport.lockedChats.length === 1 ? "chat is" : "chats are"} locked.
              Their media references are still encrypted, so nothing can be removed until they are unlocked.
            </span>
          </div>
        {:else if storageReport && storageReport.deleted > 0}
          <div class="opt-row">
            <span class="opt-label">
              Free up {formatFileSize(storageReport.reclaimedBytes)}
              <span class="inline-hint">Media no message refers to any more — usually from deleted messages.</span>
            </span>
            <button class="danger backup-row-button" onclick={cleanUpStorage} disabled={storageBusy}>Delete</button>
          </div>
        {:else if storageReport}
          <div class="field"><span class="tiny muted">Nothing to clean up — every stored file is still in use.</span></div>
        {/if}
        {#if storageError}
          <div class="field"><span class="tiny" style="color:var(--danger);">{storageError}</span></div>
        {/if}
      </div>
    </div>
    <div class="settings-subsection">
      <div class="subsection-title">Automatic backups</div>
      <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">Automatic backups<span class="inline-hint">Runs while Ncy is open or in the tray.</span></span>
        <span class="switch">
          <input type="checkbox" checked={desktopPrefs.autoBackupEnabled} onchange={(e) => updateDesktopPref("autoBackupEnabled", e.currentTarget.checked)} />
          <span class="slider"></span>
        </span>
      </label>
      {#if desktopPrefs.autoBackupEnabled}
        <div class="opt-row"><span class="opt-label">Backup folder<span class="inline-hint">{desktopPrefs.autoBackupFolder ?? "No folder selected"}</span></span><button class="ghost backup-row-button" onclick={chooseAutoBackupFolder}>Choose…</button></div>
        <label class="opt-row"><span class="opt-label">Frequency</span><select value={[24,168,720].includes(desktopPrefs.autoBackupIntervalHours) ? desktopPrefs.autoBackupIntervalHours : 0} onchange={(e) => updateDesktopPref("autoBackupIntervalHours", Number(e.currentTarget.value))}><option value="24">Daily</option><option value="168">Weekly</option><option value="720">Monthly</option><option value="0">Custom</option></select></label>
        {#if ![24,168,720].includes(desktopPrefs.autoBackupIntervalHours)}
          <label class="opt-row"><span class="opt-label">Custom interval in hours<span class="inline-hint">Minimum 1 hour.</span></span><input type="number" min="1" value={Math.max(1, desktopPrefs.autoBackupIntervalHours)} onchange={(e) => updateDesktopPref("autoBackupIntervalHours", Math.max(1, Number(e.currentTarget.value)))} /></label>
        {/if}
        <label class="opt-row"><span class="opt-label">Retention<span class="inline-hint">Only automatic backups created by Ncy are removed.</span></span><select value={desktopPrefs.autoBackupRetention === null ? "all" : "limited"} onchange={(e) => updateDesktopPref("autoBackupRetention", e.currentTarget.value === "all" ? null : 10)}><option value="limited">Keep latest</option><option value="all">Keep all</option></select></label>
        {#if desktopPrefs.autoBackupRetention !== null}
          <label class="opt-row"><span class="opt-label">Backups to keep</span><input type="number" min="1" max="100" value={desktopPrefs.autoBackupRetention} onchange={(e) => updateDesktopPref("autoBackupRetention", Math.min(100, Math.max(1, Number(e.currentTarget.value))))} /></label>
        {/if}
      {/if}
      </div>
      </div>
      <div class="settings-subsection">
        <div class="subsection-title">Manual backup</div>
        <div class="opt-list">
          <div class="opt-row backup-action-row">
            <span class="opt-label backup-action-label">
              <span class="backup-action-title">
                <Download size={18} strokeWidth={2.25} />
            Export everything
          </span>
        </span>
        <button class="backup-row-button" onclick={doExport} disabled={backupBusy}>
          {backupBusy ? "Working…" : "Export"}
        </button>
      </div>
      <div class="opt-row backup-action-row">
        <span class="opt-label backup-action-label">
          <span class="backup-action-title">
            <Upload size={18} strokeWidth={2.25} />
            Import backup
          </span>
          <span class="inline-hint">Merges new chats, events, and media into this device.</span>
        </span>
        <button class="ghost backup-row-button" onclick={doImport} disabled={backupBusy}>
          {backupBusy ? "Working…" : "Import"}
        </button>
      </div>
      {#if backupMsg}
        <div class="panel backup-status">
          <span class="muted tiny">{backupMsg}</span>
        </div>
      {/if}
    </div>
    </div>
  </section>
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Data maintenance</div>
      <div class="opt-list">
        <div class="opt-row backup-action-row">
          <span class="opt-label">Forget deletion records<span class="inline-hint">Old devices or backups may restore deleted content afterward.</span></span>
          <button class="ghost backup-row-button danger" onclick={purgeTombstones}>Forget…</button>
        </div>
      </div>
    </div>
  </section>
  {:else if $settingsTab === "changelog"}
  <section class="group changelog-view">
    <div class="subsection-title changelog-label">Versions</div>
    <div class="seg changelog-series">
      <button class="seg-btn" class:on={changelogSeries === "all"} onclick={() => changelogSeries = "all"}>All</button>
      {#each changelogSeriesOptions as series (series)}<button class="seg-btn" class:on={changelogSeries === series} onclick={() => changelogSeries = series}>{series}.x</button>{/each}
    </div>
    <div class="changelog">
      {#each visibleChangelog as rel (rel.version)}
        <div class="rel"><div class="rel-head"><span class="mono rel-ver">v{rel.version}</span><span class="mono muted rel-date">{rel.date}</span></div><ul class="rel-list">{#each rel.changes as c (c)}<li>{c}</li>{/each}</ul></div>
      {/each}
    </div>
  </section>
  {:else if $settingsTab === "about"}
  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Application</div>
      <div class="opt-list">
      <div class="opt-row">
        <span class="opt-label">Ncy</span>
        <span class="muted mono">v{appVersion || "…"}</span>
      </div>
      <div class="opt-row">
        <span class="opt-label">Source</span>
        <a href="https://github.com/0x89-y" target="_blank" rel="noreferrer noopener">github.com/0x89-y</a>
      </div>
      </div>
    </div>
  </section>

  <section class="group">
    <div class="settings-subsection">
      <div class="subsection-title">Updates</div>
      <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">Check for updates automatically</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.autoCheckUpdates}
            onchange={(e) => updateDesktopPref("autoCheckUpdates", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      <div class="opt-row">
        <span class="opt-label">
          {#if $updaterPhase === "checking"}Checking…
          {:else if $updaterPhase === "available"}v{$updaterVersion} is available
          {:else if $updaterPhase === "downloading"}Downloading…
          {:else if $updaterPhase === "uptodate"}You're on the latest version
          {:else if $updaterPhase === "error"}Check failed
          {:else}Check for a new version{/if}
        </span>
        {#if $updaterPhase === "available"}
          <button onclick={installUpdate}>Install &amp; restart</button>
        {:else}
          <button
            onclick={checkForUpdate}
            disabled={$updaterPhase === "checking" || $updaterPhase === "downloading"}
          >Check now</button>
        {/if}
      </div>
      {#if $updaterError}
        <span class="muted tiny">{$updaterError}</span>
      {/if}
    </div>
    </div>
  </section>

  {/if}
   </div>
  </div>
 </div>
</div>

<style>
  .page-head { padding: .35rem var(--settings-pad) .9rem; }
  .page-title { font-size: 1.35rem; font-weight: 600; letter-spacing: -.02em; }
  .page-desc { margin: .15rem 0 0; font-size: .84rem; color: var(--text-muted); }
  .status-card {
    display: flex;
    align-items: center;
    gap: .85rem;
    margin: 0 var(--settings-pad) 1rem;
    padding: .85rem 1rem;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--surface);
    cursor: pointer;
  }
  .status-ic {
    width: 38px; height: 38px; flex: none; border-radius: 11px;
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--accent-soft); color: var(--on-accent-soft);
  }
  .status-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .1rem; }
  .status-text strong { font-size: .95rem; }
  .status-err { color: var(--danger) !important; }
  .backup-card { cursor: default; }
  .page-scroll {
    height: 100%;
    width: 100%;
    overflow: hidden;
    min-width: 0;
  }
  .page {
    height: 100%;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    max-width: none;
    margin: 0;
  }
  h1 { margin: 0; font-size: 1.4rem; }
  .side-head {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .9rem .35rem .8rem;
  }
  /* Round accent back button — the primary way out of settings. Background /
     hover color come from the global button default (accent + accent-hover). */
  .back-btn {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    line-height: 0;
  }
  .id {
    font-family: var(--font-mono);
    font-size: .8rem;
    background: var(--surface-2);
    padding: .15rem .4rem;
    border-radius: 6px;
    user-select: all;
  }
  /* Changelog — each release is a full-width block divided by a hairline, matching
     the edge-to-edge settings rows. */
  .changelog { display: flex; flex-direction: column; }
  .rel {
    padding: .85rem var(--settings-pad);
    border-top: 1px solid var(--border);
  }
  .rel-head {
    display: flex;
    align-items: baseline;
    gap: .5rem;
    margin-bottom: .45rem;
  }
  .rel-ver { font-weight: 600; }
  .rel-date { font-size: .8rem; }
  .rel-list {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: .3rem;
  }
  .rel-list li { font-size: .88rem; line-height: 1.45; }
  .backup-action-row {
    align-items: center;
  }
  .backup-action-label {
    display: flex;
    flex-direction: column;
    gap: .15rem;
  }
  .backup-action-title {
    display: inline-flex;
    align-items: center;
    gap: .55rem;
  }
  .backup-action-title :global(svg) {
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .backup-row-button {
    min-width: 6.5rem;
    flex-shrink: 0;
  }
  .backup-status {
    padding-top: .65rem;
    padding-bottom: .65rem;
  }
  .compact-btn {
    padding: .42rem .7rem;
    min-width: 4.75rem;
    flex-shrink: 0;
    font-size: .85rem;
  }
  .wrap-row {
    gap: .5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .path-id {
    word-break: break-all;
  }

  /* Master-detail panel: one bordered frame holding the category rail (left,
     with a divider) and the active category's content (right). */
  .settings-layout {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: stretch;
    border: none;
    border-radius: 0;
    overflow: hidden;
    width: 100%;
    background: var(--surface);
  }
  .side {
    flex: 0 0 210px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 .6rem;
    border-right: 1px solid var(--border);
    background: var(--bg);
  }
  .side h1 { font-size: 1.45rem; font-weight: 600; letter-spacing: -.03em; }
  .changelog-series { margin:0 var(--settings-pad) 1rem; max-width:calc(100% - 2 * var(--settings-pad)); overflow-x:auto; }
  .panes:has(.changelog-view) { padding-top:0; }
  .side-link {
    text-align: left;
    padding: .5rem .7rem;
    border: none;
    background: transparent;
    color: var(--text);
    border-radius: 9px;
    font-size: .9rem;
    font-weight: 500;
    cursor: pointer;
  }
  .side-link:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  /* The active section takes the accent tint, like the selected filter chip. */
  .side-link.active {
    background: var(--accent-soft);
    color: var(--on-accent-soft);
    font-weight: 600;
  }
  .panes {
    flex: 1;
    min-width: 0;
    /* Held to a readable width, so a label sits near its control on a wide window. */
    max-width: 760px;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: .65rem 0 1.5rem;
    overflow-y: auto;
  }

  .settings-subsection + .settings-subsection { margin-top: 1.15rem; }
  .disclosure-row {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .disclosure-row > span { display: flex; flex-direction: column; }
  .disclosure-row:hover { background: var(--surface-hover); filter: none; }
  .nested-row { padding-left: calc(var(--settings-pad) + .75rem) !important; background: color-mix(in oklab, var(--surface-2) 45%, transparent); }
  .sync-disabled { opacity: .55; }
  .sync-disabled :global(button), .sync-disabled :global(input), .sync-disabled :global(textarea) { pointer-events: none; }
  .technical-disclosure { padding: .75rem var(--settings-pad); border-top: 1px solid var(--border); }
  .technical-control { width: min(100%, 18rem); }
  .technical-control input { flex: 1; min-width: 0; }
  .ip-row { flex-wrap: wrap; gap: .4rem; }
  .changelog-label { margin: 0 var(--settings-pad) .55rem; }
  .density-seg { min-width: 15rem; }
  .density-seg .seg-btn { flex: 1; justify-content: center; }
  .reaction-summary { align-items: center; justify-content: flex-end; flex-wrap: wrap; }
  .reaction-summary > span { font-size: 1.05rem; }
  .reaction-picker { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; }
  /* Sliding switch, sized to sit on the right of an .opt-row. */
  .switch {
    position: relative;
    display: inline-block;
    width: 40px;
    height: 22px;
    flex-shrink: 0;
  }
  .switch input { opacity: 0; width: 0; height: 0; }
  .slider {
    position: absolute;
    inset: 0;
    cursor: pointer;
    background: var(--border-strong);
    border-radius: var(--radius-pill);
    transition: background .15s;
  }
  .slider::before {
    content: "";
    position: absolute;
    height: 16px;
    width: 16px;
    left: 3px;
    top: 3px;
    background: #fff;
    border-radius: 50%;
    transition: transform .15s;
  }
  .switch input:checked + .slider { background: var(--accent); }
  .switch input:checked + .slider::before { transform: translateX(18px); }
  /* Segmented theme picker (System / Light / Dark) */
  .seg {
    display: inline-flex;
    align-items: stretch;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 2px;
    width: fit-content;
    gap: 2px;
  }
  .seg-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    color: var(--text-muted);
    border: none;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: .9rem;
    font-weight: 500;
    cursor: pointer;
    line-height: 1;
  }
  .seg-btn:hover { background: var(--surface-hover); color: var(--text); }
  .seg-btn.on {
    background: var(--accent-soft);
    color: var(--on-accent-soft);
    font-weight: 600;
  }
  /* Matches the Chat / Starred / Reminders selector in the main sidebar. */
  .model-tabs {
    display: flex;
    width: min(100%, 30rem);
    gap: 2px;
    padding: 2px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .model-tab {
    flex: 1;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: .45rem .65rem;
    background: transparent;
    color: var(--text-muted);
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
    transition: background-color .15s ease, color .15s ease;
  }
  .model-tab:hover:not(.on) { background: var(--surface-hover); color: var(--text); filter: none; }
  .model-tab.on { background: var(--accent-soft); color: var(--on-accent-soft); font-weight: 600; }
  .model-apply-row { margin-top: .15rem; }
  /* Accent swatch row */
  .accents {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 999px;
    background: var(--sw);
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
    transition: transform .12s ease, border-color .12s ease;
  }
  .swatch:hover { transform: scale(1.08); }
  .swatch.on {
    border-color: transparent;
    box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--text);
  }
  .swatch-wrap { display: inline-flex; flex-direction: column; align-items: center; gap: .3rem; width: 3.6rem; font-size: .72rem; color: var(--text-muted); }
  .swatch-wrap.on { color: var(--text); font-weight: 600; }
  .reaction-choice-grid {
    display: grid;
    grid-template-columns: repeat(8, 2rem);
    gap: 4px;
  }
  .reaction-choice {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--text);
    font-size: 17px;
    line-height: 1;
  }
  .reaction-choice:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .reaction-choice.on {
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
    border-color: var(--accent);
  }
  .reset-reactions {
    height: 2rem;
    padding: 0 .7rem;
    border-radius: var(--radius-sm);
    font-size: .85rem;
  }
  .progress {
    width: 100%;
    max-width: 400px;
    height: 6px;
    background: var(--surface-2);
    border-radius: 999px;
    overflow: hidden;
  }
  .progress-bar {
    height: 100%;
    background: var(--accent);
    transition: width .15s linear;
  }
</style>
