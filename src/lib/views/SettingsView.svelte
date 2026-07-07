<script lang="ts">
  import { api, type WhisperStatus, type TranscriptionEventPayload } from "../ipc";
  import { settings, refreshSettings, identity, refreshIdentity, refreshChats } from "../stores/app";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Monitor, Sun, Moon, Mic } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  let port = $state(8765);
  let deviceName = $state("");
  let theme = $state<"system" | "light" | "dark">("system");
  let accent = $state("purple");
  let discoverable = $state(false);
  let transcriptionEnabled = $state(false);
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

  // Theme + accent both live-apply so the picker feels immediate. The Save
  // button still writes them to disk along with everything else dirty.
  async function applyThemeNow(value: "system" | "light" | "dark") {
    if (theme === value) return;
    theme = value;
    markDirty();
    try {
      await api.updateSettings({ port, deviceName, theme, discoverable, accent, transcriptionEnabled });
      await refreshSettings();
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
      await api.updateSettings({ port, deviceName, theme, discoverable, accent, transcriptionEnabled });
      await refreshSettings();
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
      await api.updateSettings({ port, deviceName, theme, discoverable, accent, transcriptionEnabled });
      await refreshSettings();
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

  async function save() {
    if (saving) return;
    saving = true;
    try {
      await api.updateSettings({ port, deviceName, theme, discoverable, accent, transcriptionEnabled });
      await refreshSettings();
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

<div class="page-scroll scroll">
 <div class="page">
  <h1>Settings</h1>

  <section class="card col">
    <h2>This device</h2>
    {#if $identity}
      <div class="row">
        <span class="muted">Device ID</span>
        <code class="id">{$identity.deviceId}</code>
      </div>
    {/if}
    <label class="col field">
      <span class="muted tiny">Display name (shown to the phone)</span>
      <input
        type="text"
        bind:value={deviceName}
        oninput={markDirty}
      />
    </label>
  </section>

  <section class="card col">
    <h2>Sync server</h2>
    <label class="col field">
      <span class="muted tiny">Port</span>
      <input
        type="number"
        min="1"
        max="65535"
        bind:value={port}
        oninput={markDirty}
      />
      <span class="muted tiny">
        Changes take effect after restart.
      </span>
    </label>

    <label class="row toggle-field">
      <input
        type="checkbox"
        bind:checked={discoverable}
        onchange={markDirty}
      />
      <span class="col field">
        <span>Discoverable on the network</span>
        <span class="muted tiny">
          When off, this desktop doesn't broadcast itself over mDNS, so it stays
          invisible to other devices scanning the network. You can still pair by
          scanning the QR code or pasting the pairing JSON — those carry the
          address directly. Takes effect immediately.
        </span>
      </span>
    </label>
  </section>

  <section class="card col">
    <h2>Appearance</h2>
    <div class="col field">
      <span class="muted tiny">Theme</span>
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
    <div class="col field">
      <span class="muted tiny">Accent</span>
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
  </section>

  <section class="card col">
    <h2><Mic size={16} strokeWidth={2.25} /> Voice transcription</h2>
    <p class="muted tiny">
      Runs whisper.cpp locally to transcribe voice messages. Transcripts sync to
      paired devices via the same event log as message edits — nothing leaves
      this machine. Auto-transcribes new voice messages when enabled; you can
      also right-click a voice message and choose <strong>Transcribe</strong>.
    </p>
    <label class="row toggle-field">
      <input
        type="checkbox"
        checked={transcriptionEnabled}
        onchange={(e) => applyTranscriptionToggle((e.currentTarget as HTMLInputElement).checked)}
      />
      <span class="col field">
        <span>Auto-transcribe new voice messages</span>
        <span class="muted tiny">
          Uses whisper's tiny multilingual model (~74 MB). CPU-only; a short
          voice note takes 1–3 seconds on a modern laptop.
        </span>
      </span>
    </label>

    <div class="col field">
      <span class="muted tiny">Whisper install folder</span>
      <div class="row" style="gap:.5rem; align-items:center;">
        <code class="id" style="word-break:break-all">{whisperStatus?.dir ?? "…"}</code>
        <button class="ghost" onclick={openWhisperFolder}>Open folder</button>
      </div>
    </div>

    <div class="col field">
      <span class="muted tiny">Whisper CLI (<code>{cliFileName}</code>)</span>
      {#if whisperStatus?.cliPresent}
        <span class="tiny" style="color:var(--success);">
          ✓ Installed in the folder above.
        </span>
      {:else}
        <span class="tiny" style="color:var(--warning);">
          Not installed. Download the latest release from
          <a href="https://github.com/ggml-org/whisper.cpp/releases" target="_blank" rel="noreferrer">whisper.cpp releases</a>
          and extract <strong>the whole archive</strong> into the folder above
          (<code>{cliFileName}</code> plus every sibling <code>{libExt}</code>
          it links against — the CLI won't start without them).
          {#if isMac || isLinux}
            You may also need to <code>chmod +x {cliFileName}</code> after
            extracting.
          {/if}
        </span>
      {/if}
    </div>

    <div class="col field">
      <span class="muted tiny">ffmpeg (optional — legacy webm/opus fallback)</span>
      {#if whisperStatus?.ffmpegPresent}
        <span class="tiny" style="color:var(--success);">
          ✓ Found (bundled next to whisper-cli or on PATH).
        </span>
      {:else}
        <span class="tiny muted">
          Not found. New desktop recordings now use AAC-in-MP4 directly and
          transcribe without ffmpeg. It's only needed to decode old webm/opus
          voice messages recorded before this change, or on browsers that
          don't support AAC MediaRecorder. If you hit an "unsupported codec"
          error, install ffmpeg{#if isMac}
            (<code>brew install ffmpeg</code>)
          {:else if isLinux}
            (via your package manager, e.g. <code>apt install ffmpeg</code>)
          {:else}
            (<a href="https://www.gyan.dev/ffmpeg/builds/" target="_blank" rel="noreferrer">Windows builds</a>)
          {/if} and drop <code>{ffmpegFileName}</code> in the folder above or on PATH.
        </span>
      {/if}
    </div>

    <div class="col field">
      <span class="muted tiny">Model (<code>ggml-tiny.bin</code>)</span>
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

    {#if whisperError}
      <span class="tiny" style="color:var(--danger);">{whisperError}</span>
    {/if}
  </section>

  <section class="card col">
    <h2>Backup</h2>
    <p class="muted tiny">
      A backup is a single <code>.zip</code> of every chat's event log plus its
      media — your notes, portable and restorable. Importing <em>merges</em>
      (it replays events; duplicates are ignored), so it's safe to import onto a
      device that already has data. Your device identity, pairings and settings
      are not included.
    </p>
    <div class="row" style="gap:.5rem; flex-wrap:wrap;">
      <button onclick={doExport} disabled={backupBusy}>
        {backupBusy ? "Working…" : "Export backup…"}
      </button>
      <button onclick={doImport} disabled={backupBusy}>
        {backupBusy ? "Working…" : "Import backup…"}
      </button>
    </div>
    {#if backupMsg}
      <span class="muted tiny">{backupMsg}</span>
    {/if}
  </section>

  <div class="row actions">
    <button onclick={save} disabled={!dirty || saving}>
      {saving ? "Saving…" : "Save changes"}
    </button>
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
    max-width: 760px;
    margin: 0 auto;
  }
  h1 { margin: 0; font-size: 1.4rem; }
  h2 { margin: 0 0 .5rem; font-size: 1.05rem; font-weight: 600; }
  .field { gap: .25rem; }
  .id {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: .8rem;
    background: var(--surface-2);
    padding: .15rem .4rem;
    border-radius: 6px;
    user-select: all;
  }
  .actions { justify-content: flex-end; }
  .toggle-field {
    align-items: flex-start;
    gap: .6rem;
    margin-top: .85rem;
  }
  .toggle-field input {
    margin-top: .2rem;
    flex-shrink: 0;
  }
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
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--card-shadow);
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
