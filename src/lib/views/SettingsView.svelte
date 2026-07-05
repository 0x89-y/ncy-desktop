<script lang="ts">
  import { api } from "../ipc";
  import { settings, refreshSettings, identity, refreshIdentity, refreshChats } from "../stores/app";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Monitor, Sun, Moon } from "@lucide/svelte";

  let port = $state(8765);
  let deviceName = $state("");
  let theme = $state<"system" | "light" | "dark">("system");
  let accent = $state("purple");
  let discoverable = $state(false);
  let saving = $state(false);
  let dirty = $state(false);

  $effect(() => {
    if ($settings) {
      port = $settings.port;
      deviceName = $settings.deviceName;
      theme = $settings.theme as "system" | "light" | "dark";
      accent = $settings.accent ?? "purple";
      discoverable = $settings.discoverable;
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
      await api.updateSettings({ port, deviceName, theme, discoverable, accent });
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
      await api.updateSettings({ port, deviceName, theme, discoverable, accent });
      await refreshSettings();
      dirty = false;
    } catch {
      /* leave dirty so user can retry via Save */
    }
  }

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
      await api.updateSettings({ port, deviceName, theme, discoverable, accent });
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

<div class="page scroll">
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

<style>
  .page {
    height: 100%;
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
</style>
