<script lang="ts">
  import { api } from "../ipc";
  import { settings, refreshSettings, identity, refreshIdentity } from "../stores/app";

  let port = $state(8765);
  let deviceName = $state("");
  let theme = $state("system");
  let discoverable = $state(false);
  let saving = $state(false);
  let dirty = $state(false);

  $effect(() => {
    if ($settings) {
      port = $settings.port;
      deviceName = $settings.deviceName;
      theme = $settings.theme;
      discoverable = $settings.discoverable;
      dirty = false;
    }
  });

  function markDirty() {
    dirty = true;
  }

  async function save() {
    if (saving) return;
    saving = true;
    try {
      await api.updateSettings({ port, deviceName, theme, discoverable });
      await refreshSettings();
      await refreshIdentity();
    } finally {
      saving = false;
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
    <label class="col field">
      <span class="muted tiny">Theme</span>
      <select bind:value={theme} onchange={markDirty}>
        <option value="system">Match system</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
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
</style>
