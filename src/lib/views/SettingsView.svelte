<script lang="ts">
  import { api, type AppSettings, type DesktopPrefs, type WhisperStatus, type TranscriptionEventPayload } from "../ipc";
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
    REACTION_CHOICES,
    DEFAULT_QUICK_REACTIONS,
    type SettingsTab,
  } from "../stores/app";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import {
    updaterPhase,
    updaterVersion,
    updaterError,
    checkForUpdate,
    installUpdate,
  } from "../stores/updater";
  import { CHANGELOG } from "../changelog";
  import { Monitor, Sun, Moon, ArrowLeft, Save, Download, Upload } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import DevicesPane from "../components/DevicesPane.svelte";

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "sync", label: "Sync" },
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
  let accent = $state("purple");
  let discoverable = $state(false);
  let transcriptionEnabled = $state(false);
  let syncTransportMode = $state<AppSettings["syncTransportMode"]>("localOnly");
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
  let showSyncAdvanced = $state(false);
  let showTranscriptionAdvanced = $state(false);

  // Mirrors the titlebar dot, spelled out. "starting" is the brief window
  // between flipping the toggle on and the listener binding its port.
  const serverStatusLabel = $derived(
    $serverStatus === "running"
      ? `Listening on ${$serverAddr || "…"}`
      : $serverStatus === "error"
        ? "Failed to start — is the port already in use?"
        : $serverStatus === "starting"
          ? "Starting…"
          : "Not running",
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

  $effect(() => {
    if ($settings) {
      port = $settings.port;
      deviceName = $settings.deviceName;
      theme = $settings.theme as "system" | "light" | "dark";
      accent = $settings.accent ?? "purple";
      discoverable = $settings.discoverable;
      transcriptionEnabled = $settings.transcriptionEnabled ?? false;
      syncTransportMode = $settings.syncTransportMode ?? "localOnly";
      dirty = false;
    }
  });

  function markDirty() {
    dirty = true;
  }

  function settingsPayload(): AppSettings {
    return { port, deviceName, theme, discoverable, accent, transcriptionEnabled, syncTransportMode };
  }

  async function changeTransportMode(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const next = select.value as AppSettings["syncTransportMode"];
    if (next !== "localOnly" && syncTransportMode === "localOnly") {
      const accepted = confirm(
        "Enable Pear networking? Ncy will start a bundled peer-to-peer networking process and contact the Pear/Holepunch discovery infrastructure so your paired devices can connect over the internet. Chat contents remain end-to-end protected by Ncy's existing pairing credentials.",
      );
      if (!accepted) {
        select.value = syncTransportMode;
        return;
      }
    }
    syncTransportMode = next;
    markDirty();
    try {
      await persistSettings();
      dirty = false;
    } catch (error) {
      // The backend persists the requested policy before attempting to start
      // Pear, so reload that durable choice and expose the startup failure.
      await refreshSettings();
      const message = typeof error === "string"
        ? error
        : error instanceof Error
          ? error.message
          : "unknown error";
      alert(`The connection method was saved, but Pear could not start: ${message}`);
    }
  }

  async function persistSettings() {
    await api.updateSettings(settingsPayload());
    await Promise.all([refreshSettings(), refreshPairings()]);
  }

  // Theme + accent both live-apply so the picker feels immediate. The Save
  // button still writes them to disk along with everything else dirty.
  async function applyThemeNow(value: "system" | "light" | "dark") {
    if (theme === value) return;
    theme = value;
    markDirty();
    try {
      await persistSettings();
      dirty = false;
    } catch {
      /* leave dirty so user can retry via Save */
    }
  }
  async function applyAccentNow(value: string) {
    if (accent === value) return;
    accent = value;
    markDirty();
    try {
      await persistSettings();
      dirty = false;
    } catch {
      /* leave dirty so user can retry via Save */
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
      await persistSettings();
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
    if (!confirm("Delete the whisper model file? You'll have to re-download to transcribe again.")) return;
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
  const accents: { name: string; color: string; label: string }[] = [
    { name: "purple", color: "#7c3aed", label: "Purple" },
    { name: "blue", color: "#2563eb", label: "Blue" },
    { name: "green", color: "#059669", label: "Green" },
    { name: "pink", color: "#db2777", label: "Pink" },
    { name: "orange", color: "#ea580c", label: "Orange" },
    { name: "teal", color: "#0d9488", label: "Teal" },
  ];
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

  async function save() {
    if (saving) return;
    saving = true;
    try {
      await persistSettings();
      await refreshIdentity();
    } finally {
      saving = false;
    }
  }

  // --- Backup / restore (this desktop's own notes). ---
  let backupBusy = $state(false);
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
    if (!confirm("Import this backup? Ncy will merge any new events and media into your local chats.")) return;
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
    if (!cached && !confirm(`Download ${modelId} (~${sizes[quality]}) and use it for transcription?`)) return;
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
    if (!confirm(`Remove unused speech models and recover ${fmtMB(bytes)}? The active model will be kept.`)) return;
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
    if (!confirm(
      "Only continue if every old device and backup containing deleted chats or messages is gone. " +
      "Stale history paired later can restore them. This cannot be undone."
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

   {#if dirty || saving}
     <button class="save-btn" onclick={save} disabled={saving} title="Save changes"><Save size={16} strokeWidth={2.25} /><span>{saving ? "Saving…" : "Save"}</span></button>
   {/if}

   <div class="panes">
  {#if $settingsTab === "general"}
  <section class="group">
    <h2>General</h2>
    <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">Start minimized</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.startMinimized}
            onchange={(e) => updateDesktopPref("startMinimized", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      <label class="opt-row">
        <span class="opt-label">
          Close to tray
          <span class="inline-hint">Closing the window hides Ncy instead of quitting it.</span>
        </span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.closeToTray}
            onchange={(e) => updateDesktopPref("closeToTray", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      {#if !desktopPrefs.closeToTray}
        <label class="opt-row">
          <span class="opt-label">
            Ask when closing
            <span class="inline-hint">Offer to minimize to the tray instead of quitting.</span>
          </span>
          <span class="switch">
            <input
              type="checkbox"
              checked={desktopPrefs.askCloseToTray}
              onchange={(e) => updateDesktopPref("askCloseToTray", (e.currentTarget as HTMLInputElement).checked)}
            />
            <span class="slider"></span>
          </span>
        </label>
      {/if}
      <label class="opt-row">
        <span class="opt-label">Launch on startup</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.launchOnStartup}
            onchange={(e) => updateDesktopPref("launchOnStartup", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      <label class="opt-row">
        <span class="opt-label">Confirm before deleting</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.askBeforeDeleting}
            onchange={(e) => updateDesktopPref("askBeforeDeleting", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
      {#if desktopPrefsError}
        <div class="field">
          <span class="tiny" style="color:var(--danger);">{desktopPrefsError}</span>
        </div>
      {/if}
    </div>
  </section>
  {:else if $settingsTab === "sync"}
  <section class="group">
    <h2>Paired-device connections</h2>
    <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">
          Enable paired-device connections
          <span class="inline-hint">{serverStatusLabel}</span>
        </span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.syncServerEnabled}
            onchange={(e) => updateDesktopPref("syncServerEnabled", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
    </div>
  </section>

  <section class="group">
    <h2>This device</h2>
    <div class="opt-list">
      <label class="field">
        <span class="field-label">Connection mode</span>
        <select value={syncTransportMode} onchange={changeTransportMode}>
          <option value="localOnly">Local only</option>
          <option value="pearOnly">Pear only</option>
          <option value="localPreferred">Local preferred</option>
          <option value="pearPreferred">Pear preferred</option>
        </select>
        <span class="tiny muted">
          Local stays on your current network. Pear can connect already-paired devices across networks; it never discovers or pairs unknown devices.
        </span>
      </label>
      <label class="field">
        <span class="field-label">Device name</span>
        <div class="row device-name-row">
          <input
            type="text"
            bind:value={deviceName}
            oninput={markDirty}
          />
          <button type="button" class="ghost" onclick={() => showSyncAdvanced = !showSyncAdvanced}>
            {showSyncAdvanced ? "Hide advanced" : "Advanced"}
          </button>
        </div>
      </label>
      {#if showSyncAdvanced}
      {#if $identity}
        <div class="opt-row">
          <span class="opt-label">Device ID</span>
          <code class="id">{$identity.deviceId}</code>
        </div>
      {/if}
        <label class="field">
          <span class="field-label">Sync port · restart to apply</span>
          <input
            type="number"
            min="1"
            max="65535"
            bind:value={port}
            oninput={markDirty}
          />
        </label>
        <div class="opt-row backup-action-row">
          <span class="opt-label backup-action-label"><span>Forget deletion records</span><span class="inline-hint">Data maintenance: stale devices or backups may restore deleted content afterward.</span></span>
          <button class="ghost backup-row-button danger" onclick={purgeTombstones}>Forget…</button>
        </div>
      {/if}
    </div>
  </section>

  <!-- Discovery and the paired-device list are inert without a running server,
       so they collapse rather than sit there disabled. -->
  {#if desktopPrefs.syncServerEnabled}
    <DevicesPane
      discoverable={discoverable}
      connectionMode={syncTransportMode}
      onDiscoverableChange={(value) => {
        discoverable = value;
        markDirty();
      }}
    />
  {/if}
  {:else if $settingsTab === "interface"}
  <section class="group">
    <h2>Interface</h2>
    <div class="opt-list">
      <div class="opt-row">
        <span class="opt-label">Theme</span>
        <div class="seg">
          {#each themeModes as m (m.value)}
            {@const Icon = m.icon}
            <button
              type="button"
              class="seg-btn"
              class:on={theme === m.value}
              onclick={() => applyThemeNow(m.value)}
            >
              <Icon size={16} />
              {m.label}
            </button>
          {/each}
        </div>
      </div>
      <div class="opt-row">
        <span class="opt-label">Accent</span>
        <div class="accents">
          {#each accents as a (a.name)}
            <button
              type="button"
              class="swatch"
              class:on={accent === a.name}
              style={`--sw:${a.color};`}
              title={a.label}
              aria-label={a.label}
              onclick={() => applyAccentNow(a.name)}
            ></button>
          {/each}
        </div>
      </div>
      <div class="opt-row reaction-row">
        <span class="opt-label">Quick reactions</span>
        <div class="reaction-settings">
          <div class="reaction-choice-grid">
            {#each REACTION_CHOICES as emoji (emoji)}
              {@const selected = $quickReactions.includes(emoji)}
              <button
                type="button"
                class="reaction-choice"
                class:on={selected}
                title={selected ? `Hide ${emoji}` : `Show ${emoji}`}
                aria-label={selected ? `Hide ${emoji}` : `Show ${emoji}`}
                onclick={() => toggleQuickReaction(emoji)}
              >{emoji}</button>
            {/each}
          </div>
          <button type="button" class="subtle reset-reactions" onclick={resetQuickReactions}>
            Reset
          </button>
        </div>
      </div>
      <div class="opt-row">
        <span class="opt-label">Message density</span>
        <div class="seg">
          <button
            type="button"
            class="seg-btn"
            class:on={$messageDensity === "comfortable"}
            onclick={() => messageDensity.set("comfortable")}
          >Comfortable</button>
          <button
            type="button"
            class="seg-btn"
            class:on={$messageDensity === "compact"}
            onclick={() => messageDensity.set("compact")}
          >Compact</button>
        </div>
      </div>
      <div class="opt-row">
        <span class="opt-label">Chat list density</span>
        <div class="seg">
          <button
            type="button"
            class="seg-btn"
            class:on={$chatListDensity === "comfortable"}
            onclick={() => chatListDensity.set("comfortable")}
          >Comfortable</button>
          <button
            type="button"
            class="seg-btn"
            class:on={$chatListDensity === "compact"}
            onclick={() => chatListDensity.set("compact")}
          >Compact</button>
        </div>
      </div>
    </div>
  </section>
  {:else if $settingsTab === "transcription"}
  <section class="group">
    <h2>Voice transcription</h2>
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

    {#if whisperError}
      <div class="panel">
        <span class="tiny" style="color:var(--danger);">{whisperError}</span>
      </div>
    {/if}
  </section>
  {:else if $settingsTab === "backup"}
  <section class="group">
    <h2>Backup</h2>
    <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label"><strong>Automatic backups</strong><span class="inline-hint">Runs while Ncy is open or in the tray.</span></span>
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
        <div class="opt-row backup-action-row"><span class="opt-label">{desktopPrefs.lastAutoBackupAt ? `Last backup: ${new Date(desktopPrefs.lastAutoBackupAt).toLocaleString()}` : "No automatic backup yet"}{#if desktopPrefs.lastAutoBackupError}<span class="inline-hint" style="color:var(--danger)">{desktopPrefs.lastAutoBackupError}</span>{/if}</span><button onclick={runAutoBackupNow} disabled={backupBusy || !desktopPrefs.autoBackupFolder}>Back up now</button></div>
      {/if}
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
  </section>
  {:else if $settingsTab === "changelog"}
  <section class="group changelog-view">
    <div class="changelog-head">
      <h2>Changelog</h2>
    </div>
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
    <h2>About</h2>
    <div class="opt-list">
      <div class="opt-row">
        <span class="opt-label">Ncy</span>
        <span class="muted mono">v{appVersion || "…"}</span>
      </div>
      <div class="opt-row">
        <span class="opt-label">Source</span>
        <a href="https://github.com/0x89-y" target="_blank" rel="noreferrer noopener">github.com/0x89-y</a>
      </div>
      <div class="opt-row">
        <span class="opt-label">
          Pear open-source components
          <span class="inline-hint">Bare/Bare Kit and UDX · Apache-2.0; Hyperswarm, HyperDHT, and sodium-native · MIT.</span>
        </span>
      </div>
      <div class="opt-row">
        <span class="opt-label">
          Third-party notices
          <span class="inline-hint">A detailed Apache-2.0, MIT, and ISC dependency inventory and the supplied license and notice texts are installed with the Pear runtime.</span>
        </span>
      </div>
    </div>
  </section>

  <section class="group">
    <h2>Updates</h2>
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
  </section>

  {/if}
   </div>
  </div>
 </div>
</div>

<style>
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
    min-height: 49px;
    padding: .45rem .6rem;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
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
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: .8rem;
    background: var(--surface-2);
    padding: .15rem .4rem;
    border-radius: 6px;
    user-select: all;
  }
  /* Save action lives top-right in the header, appearing only when there are
     unsaved changes. */
  .save-btn {
    position: absolute;
    top: .55rem;
    right: .75rem;
    z-index: 5;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: .35rem;
    padding: .4rem .7rem;
    font-size: .88rem;
    font-weight: 600;
    line-height: 1;
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
  .device-name-row {
    gap: .5rem;
    flex-wrap: wrap;
  }
  .device-name-row input {
    flex: 1 1 220px;
  }
  .device-name-row .ghost {
    flex-shrink: 0;
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
    flex: 0 0 160px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--surface-2);
  }
  .side h1 { font-size:.95rem; font-weight:600; }
  .changelog-series { margin:0 var(--settings-pad) 1rem; max-width:calc(100% - 2 * var(--settings-pad)); overflow-x:auto; }
  .panes:has(.changelog-view) { padding-top:0; }
  .changelog-head { display:flex; align-items:center; gap:.55rem; padding:.45rem var(--settings-pad); border-bottom:1px solid var(--border); margin-bottom:.55rem; }
  .changelog-head h2 { margin:0; padding:0; font-size:1rem; font-weight:600; }
  .side-link {
    text-align: left;
    padding: .55rem .7rem;
    border: none;
    border-top: 1px solid var(--border);
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--text-muted);
    border-radius: 0;
    font-size: .92rem;
    font-weight: 500;
    cursor: pointer;
  }
  .side-link:first-child { border-top: none; }
  /* Close off the list with a divider under the last category, since the nav
     column stretches taller than the links. */
  .side-link:last-child { border-bottom: 1px solid var(--border); }
  .side-link:hover { background: var(--surface-hover); color: var(--text); filter: none; }
  .side-link.active {
    background: var(--surface);
    color: var(--text);
    border-left-color: var(--accent);
  }
  .panes {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: .65rem 0 1.5rem;
    overflow-y: auto;
  }

  /* Sliding switch, sized to sit on the right of an `.opt-row`. */
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
    background: var(--accent);
    color: var(--accent-contrast);
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
    border-radius: 5px;
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
    transition: background-color .15s ease, color .15s ease;
  }
  .model-tab:hover:not(.on) { background: var(--surface-hover); color: var(--text); filter: none; }
  .model-tab.on { background: var(--accent); color: var(--accent-contrast); }
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
    border-color: var(--text);
    transform: scale(1.08);
  }
  .reaction-row {
    align-items: flex-start;
  }
  .reaction-settings {
    display: flex;
    align-items: flex-start;
    gap: .5rem;
    justify-content: flex-end;
    min-width: 0;
  }
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
  @media (max-width: 720px) {
    .reaction-row,
    .reaction-settings {
      flex-direction: column;
      align-items: stretch;
    }
    .reaction-choice-grid {
      grid-template-columns: repeat(auto-fill, minmax(2rem, 1fr));
    }
    .reset-reactions {
      width: fit-content;
    }
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
