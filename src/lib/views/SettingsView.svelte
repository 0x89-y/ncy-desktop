<script lang="ts">
  import { api, type AppSettings, type DesktopPrefs, type WhisperStatus, type TranscriptionEventPayload } from "../ipc";
  import {
    settings,
    refreshSettings,
    identity,
    refreshIdentity,
    refreshChats,
    settingsTab,
    activeView,
    quickReactions,
    messageDensity,
    setQuickReactions,
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
  import { Monitor, Sun, Moon, Mic, ArrowLeft, Save } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import DevicesPane from "../components/DevicesPane.svelte";

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "sync", label: "Sync" },
    { id: "interface", label: "Interface" },
    { id: "transcription", label: "Transcription" },
    { id: "backup", label: "Backup" },
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
  let desktopPrefs = $state<DesktopPrefs>({
    startMinimized: false,
    closeToTray: true,
    launchOnStartup: false,
    askBeforeDeleting: true,
    autoCheckUpdates: true,
  });
  let desktopPrefsError = $state<string | null>(null);
  let saving = $state(false);
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
      dirty = false;
    }
  });

  function markDirty() {
    dirty = true;
  }

  function settingsPayload(): AppSettings {
    return { port, deviceName, theme, discoverable, accent, transcriptionEnabled };
  }

  async function persistSettings() {
    await api.updateSettings(settingsPayload());
    await refreshSettings();
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

  async function downloadWhisperModel() {
    if (whisperBusy) return;
    whisperBusy = true;
    whisperError = null;
    downloadProgress = { written: 0, total: 0 };
    try {
      await api.transcriptionDownloadModel();
    } catch (e: any) {
      whisperError = typeof e === "string" ? e : (e?.message ?? "download failed");
    } finally {
      whisperBusy = false;
      downloadProgress = null;
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
      } else if (ev.kind === "download-complete") {
        downloadProgress = null;
        void refreshWhisperStatus();
      } else if (ev.kind === "download-error") {
        whisperError = ev.message;
        downloadProgress = null;
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
</script>

<div class="page-scroll scroll settings">
 <div class="page">
  <div class="page-head">
    <button class="back-btn" onclick={() => activeView.set("chats")} title="Back to chats" aria-label="Back to chats">
      <ArrowLeft size={18} strokeWidth={2.5} />
    </button>
    <h1>Settings</h1>
    {#if dirty || saving}
      <button class="save-btn" onclick={save} disabled={saving} title="Save changes">
        <Save size={16} strokeWidth={2.25} />
        <span>{saving ? "Saving…" : "Save"}</span>
      </button>
    {/if}
  </div>

  <div class="settings-layout">
   <nav class="side" aria-label="Settings sections">
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
        <span class="opt-label">Close to tray</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={desktopPrefs.closeToTray}
            onchange={(e) => updateDesktopPref("closeToTray", (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>
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
        <span class="opt-label">Ask before deleting</span>
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
    <h2>This device</h2>
    <div class="opt-list">
      <label class="field">
        <span class="field-label">Device name</span>
        <input
          type="text"
          bind:value={deviceName}
          oninput={markDirty}
        />
      </label>
      {#if $identity}
        <div class="opt-row">
          <span class="opt-label">Device ID</span>
          <code class="id">{$identity.deviceId}</code>
        </div>
      {/if}
    </div>
  </section>

  <section class="group">
    <h2>Nearby discovery</h2>
    <div class="opt-list">
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
      <label class="opt-row">
        <span class="opt-label">
          Visible to nearby devices
          <span class="inline-hint">QR and JSON still work when off.</span>
        </span>
        <span class="switch">
          <input
            type="checkbox"
            bind:checked={discoverable}
            onchange={markDirty}
          />
          <span class="slider"></span>
        </span>
      </label>
    </div>
  </section>

  <DevicesPane />
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
    </div>
  </section>
  {:else if $settingsTab === "transcription"}
  <section class="group">
    <h2><Mic size={16} strokeWidth={2.25} /> Voice transcription</h2>
    <div class="opt-list">
      <label class="opt-row">
        <span class="opt-label">Auto-transcribe voice messages</span>
        <span class="switch">
          <input
            type="checkbox"
            checked={transcriptionEnabled}
            onchange={(e) => applyTranscriptionToggle((e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="slider"></span>
        </span>
      </label>

    <div class="field">
      <span class="field-label">Whisper install folder</span>
      <div class="row" style="gap:.5rem; align-items:center;">
        <code class="id" style="word-break:break-all">{whisperStatus?.dir ?? "…"}</code>
        <button class="ghost" onclick={openWhisperFolder}>Open folder</button>
      </div>
    </div>

    <div class="field">
      <span class="field-label">Whisper CLI (<code>{cliFileName}</code>)</span>
      {#if whisperStatus?.cliPresent}
        <span class="tiny" style="color:var(--success);">✓ Installed</span>
      {:else}
        <span class="tiny" style="color:var(--warning);">
          Not installed. Extract a
          <a href="https://github.com/ggml-org/whisper.cpp/releases" target="_blank" rel="noreferrer">whisper.cpp release</a>
          into the folder above — <code>{cliFileName}</code> plus its sibling
          <code>{libExt}</code> libraries.{#if isMac || isLinux} Then <code>chmod +x {cliFileName}</code>.{/if}
        </span>
      {/if}
    </div>

    <div class="field">
      <span class="field-label">ffmpeg (optional)</span>
      {#if whisperStatus?.ffmpegPresent}
        <span class="tiny" style="color:var(--success);">
          ✓ Found (next to whisper-cli or on PATH).
        </span>
      {:else}
        <span class="tiny muted">
          Not found. Only needed to decode old webm/opus voice notes — drop
          <code>{ffmpegFileName}</code> in the folder above or on PATH.
        </span>
      {/if}
    </div>

    <div class="field">
      <span class="field-label">Model (<code>ggml-tiny.bin</code>)</span>
      {#if whisperStatus?.modelPresent}
        <div class="row" style="gap:.5rem; align-items:center;">
          <span class="tiny" style="color:var(--success);">
            ✓ Downloaded ({fmtMB(whisperStatus.modelSizeBytes)})
          </span>
          <button class="ghost" onclick={deleteWhisperModel} disabled={whisperBusy}>
            Delete model
          </button>
        </div>
      {:else if downloadProgress}
        {@const pct = downloadProgress.total
          ? Math.floor((downloadProgress.written / downloadProgress.total) * 100)
          : null}
        <span class="tiny muted">
          Downloading… {fmtMB(downloadProgress.written)}{downloadProgress.total ? ` / ${fmtMB(downloadProgress.total)} (${pct}%)` : ""}
        </span>
        <div class="progress">
          <div class="progress-bar" style={`width:${pct ?? 0}%`}></div>
        </div>
      {:else}
        <div class="row" style="gap:.5rem; align-items:center;">
          <span class="tiny muted">Not downloaded.</span>
          <button onclick={downloadWhisperModel} disabled={whisperBusy}>
            Download (~74 MB)
          </button>
        </div>
      {/if}
    </div>
    </div>

    {#if whisperError}
      <span class="tiny" style="color:var(--danger);">{whisperError}</span>
    {/if}
  </section>
  {:else if $settingsTab === "backup"}
  <section class="group">
    <h2>Backup</h2>
    <div class="opt-list">
      <div class="field">
        <div class="action-group">
          <button onclick={doExport} disabled={backupBusy}>
            {backupBusy ? "Working…" : "Export"}
          </button>
          <button onclick={doImport} disabled={backupBusy}>
            {backupBusy ? "Working…" : "Import"}
          </button>
        </div>
        {#if backupMsg}
          <span class="muted tiny">{backupMsg}</span>
        {/if}
      </div>
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

  {#if CHANGELOG.length}
  <section class="group">
    <h2>Changelog</h2>
    <div class="changelog">
      {#each CHANGELOG as rel (rel.version)}
        <div class="rel">
          <div class="rel-head">
            <span class="mono rel-ver">v{rel.version}</span>
            <span class="mono muted rel-date">{rel.date}</span>
          </div>
          <ul class="rel-list">
            {#each rel.changes as c (c)}
              <li>{c}</li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>
  </section>
  {/if}
  {/if}
   </div>
  </div>
 </div>
</div>

<style>
  /* Full-width scroller so the scrollbar hugs the window's right edge, while
     the content itself stays centered inside a narrower column. */
  .page-scroll {
    height: 100%;
    width: 100%;
  }
  .page {
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 880px;
    margin: 0 auto;
  }
  h1 { margin: 0; font-size: 1.4rem; }
  .page-head {
    position: sticky;
    top: 0;
    z-index: 6;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: .75rem;
    margin: -1.5rem -1.5rem 0;
    padding: 1.5rem 1.5rem .75rem;
    background: var(--bg);
  }
  /* Round accent back button — the primary way out of settings. Background /
     hover color come from the global button default (accent + accent-hover). */
  .back-btn {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: var(--radius);
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
    margin-left: auto;
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
  .action-group {
    display: inline-flex;
    align-items: stretch;
    width: fit-content;
    border-radius: var(--radius);
    overflow: hidden;
  }
  .action-group button {
    border-radius: 0;
  }
  .action-group button + button {
    border-left: 1px solid color-mix(in srgb, var(--accent-contrast) 30%, transparent);
  }

  /* Master-detail panel: one bordered frame holding the category rail (left,
     with a divider) and the active category's content (right). */
  .settings-layout {
    display: flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--surface);
  }
  .side {
    flex: 0 0 160px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--surface-2);
  }
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
    padding: 1.5rem 0;
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
