<script lang="ts">
  import {
    listConversations,
    createConversation,
    getSettings,
    setSettings,
    syncNow,
    type SyncStatus,
    checkFirefly,
    checkOnDevice,
    pullOnDeviceModel,
    type Conversation,
    type Settings,
    type OnDeviceStatus,
    listProfiles,
    registerProfile,
    switchProfile,
    refreshActiveProfile,
    type Profile,
  } from "$lib/api";
  import ConversationList from "$lib/ConversationList.svelte";
  import Chat from "$lib/Chat.svelte";

  let conversations = $state<Conversation[]>([]);
  let selectedId = $state<string | null>(null);
  let settings = $state<Settings>({
    fireflyEndpoint: "",
    onDeviceEndpoint: "",
    onDeviceModel: "",
    modelCode: "",
    modelChatHeavy: "",
    modelFrontier: "",
    syncEndpoint: "",
    deviceName: "",
    memoryEnabled: true,
  });
  let reachable = $state<boolean | null>(null);
  let onDevice = $state<OnDeviceStatus | null>(null);
  let pulling = $state(false);
  let pullPct = $state(0);

  async function pullModel() {
    pulling = true;
    pullPct = 0;
    try {
      await pullOnDeviceModel((p) => {
        if (p.total) pullPct = Math.round((100 * (p.completed ?? 0)) / p.total);
      });
      onDevice = await checkOnDevice();
    } finally {
      pulling = false;
    }
  }
  let syncing = $state(false);
  let syncStatus = $state<SyncStatus | null>(null);
  let syncTick = $state(0);

  let profiles = $state<Profile[]>([]);
  let newProfileName = $state("");
  let registering = $state(false);
  let error = $state<string | null>(null);

  const active = $derived(profiles.find((p) => p.active) ?? null);

  async function runSync() {
    if (syncing) return;
    syncing = true;
    try {
      syncStatus = await syncNow();
      if (syncStatus.pulled > 0) {
        await refresh();
        syncTick += 1; // nudge Chat to refetch the open conversation
      }
    } finally {
      syncing = false;
    }
  }
  let showSettings = $state(false);
  let savedNote = $state("");
  let saveError = $state("");

  async function refresh() {
    conversations = await listConversations();
    if (!selectedId && conversations.length > 0) {
      selectedId = conversations[0].id;
    }
  }

  $effect(() => {
    (async () => {
      settings = await getSettings();
      profiles = await listProfiles();
      if (profiles.length > 0) {
        refreshActiveProfile().then((p) => (profiles = p));
      }
      checkFirefly().then((r) => (reachable = r));
      checkOnDevice().then((r) => (onDevice = r));
      await refresh();
      runSync();
    })();
  });

  async function newConversation() {
    const c = await createConversation("New conversation");
    await refresh();
    selectedId = c.id;
  }

  async function saveSettings() {
    await setSettings({
      fireflyEndpoint: settings.fireflyEndpoint,
      onDeviceEndpoint: settings.onDeviceEndpoint,
      onDeviceModel: settings.onDeviceModel,
      modelCode: settings.modelCode,
      modelChatHeavy: settings.modelChatHeavy,
      modelFrontier: settings.modelFrontier,
      syncEndpoint: settings.syncEndpoint,
      deviceName: settings.deviceName,
      memoryEnabled: settings.memoryEnabled,
    });
    saveError = "";
    reachable = await checkFirefly();
    savedNote = "Saved";
    setTimeout(() => (savedNote = ""), 1500);
  }
</script>

<div class="app">
  {#if profiles.length === 0}
    <div class="onboard">
      <h2>Create a profile</h2>
      <p>New profiles start as a kid profile. An adult can upgrade it later on the server.</p>
      {#if error}
        <p class="onboard-error">{error}</p>
      {/if}
      <input placeholder="Display name" bind:value={newProfileName} />
      <button
        disabled={registering || !newProfileName.trim()}
        onclick={async () => {
          registering = true;
          error = null;
          try {
            profiles = await registerProfile(newProfileName.trim());
            newProfileName = "";
            await refresh();
            runSync();
          } catch (e) {
            error = String(e);
          } finally {
            registering = false;
          }
        }}
      >{registering ? "Creating…" : "Create"}</button>
    </div>
  {:else}
    <ConversationList
      {conversations}
      {selectedId}
      onselect={(id) => (selectedId = id)}
      onnew={newConversation}
    />

    <main>
      <header>
        <span class="title">Firefly</span>
        <span class="conn" class:down={reachable === false}>
          {reachable === null ? "…" : reachable ? "Firefly online" : "Firefly offline"}
        </span>
        <span class="conn" class:down={syncStatus?.ok === false}>
          {syncing
            ? "syncing…"
            : syncStatus == null
              ? "sync idle"
              : syncStatus.ok
                ? "synced"
                : syncStatus.message ?? "offline"}
        </span>
        <select
          value={active?.userId ?? ""}
          onchange={async (e) => {
            profiles = await switchProfile((e.currentTarget as HTMLSelectElement).value);
            selectedId = null;
            await refresh();
            runSync();
          }}
        >
          {#each profiles as p (p.userId)}
            <option value={p.userId}>{p.displayName}</option>
          {/each}
        </select>
        {#if active}
          <span class="profile-badge">{active.profile}</span>
        {/if}
        <button class="gear" onclick={runSync} disabled={syncing}>⟳ Sync now</button>
        <button class="gear" onclick={() => (showSettings = !showSettings)}>
          ⚙ Settings
        </button>
      </header>

      {#if showSettings}
        <div class="settings">
          <label>
            Firefly endpoint
            <input bind:value={settings.fireflyEndpoint} spellcheck="false" />
          </label>
          <label>Sync endpoint
            <input bind:value={settings.syncEndpoint} spellcheck="false" />
          </label>
          <label>Device name
            <input bind:value={settings.deviceName} spellcheck="false" />
          </label>
          <label>On-device endpoint
            <input bind:value={settings.onDeviceEndpoint} spellcheck="false" />
          </label>
          <label>On-device model
            <input bind:value={settings.onDeviceModel} spellcheck="false" />
          </label>
          {#if onDevice}
            <div class="ready" class:down={onDevice.state === "unreachable"}>
              {#if onDevice.state === "ready"}
                on-device ready · {onDevice.model}
              {:else if onDevice.state === "modelMissing"}
                model not installed
                <button type="button" onclick={pullModel} disabled={pulling}>
                  {pulling ? `pulling… ${pullPct}%` : `Pull ${onDevice.model}`}
                </button>
              {:else}
                server unreachable: install &amp; start Ollama, then pull the model:
                <code>ollama serve</code> · <code>ollama pull {settings.onDeviceModel}</code>
                <br />(Framework NPU: run <code>flm serve</code> + <code>flm pull {settings.onDeviceModel}</code> instead)
              {/if}
            </div>
          {/if}
          <label>Home-base model: code/write
            <input bind:value={settings.modelCode} spellcheck="false" />
          </label>
          <label>Home-base model: agentic
            <input bind:value={settings.modelChatHeavy} spellcheck="false" />
          </label>
          <label class="toggle">
            <input type="checkbox" bind:checked={settings.memoryEnabled} />
            Inject memories on home-base requests
          </label>
          <label>Cloud model: best
            <input bind:value={settings.modelFrontier} spellcheck="false" />
          </label>
          <div class="actions">
            <button onclick={saveSettings}>Save</button>
            <span class="note">{savedNote}</span>
          </div>
          {#if saveError}
            <p class="save-error">{saveError}</p>
          {/if}
        </div>
      {/if}

      <Chat conversationId={selectedId} refreshSignal={syncTick} profile={active?.profile ?? "adult"} />
    </main>
  {/if}
</div>

<style>
  :root {
    --bg: #1c1c20;
    --panel: #26262c;
    --hover: #33333b;
    --border: #3a3a42;
    --text: #ececf0;
    --muted: #9a9aa5;
    --accent: #d9622b;
    color-scheme: dark;
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: Inter, system-ui, Avenir, Helvetica, Arial, sans-serif;
  }
  .app {
    display: flex;
    height: 100vh;
  }
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  .title {
    font-weight: 700;
  }
  .conn {
    font-size: 0.72rem;
    color: #9fe0a0;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
  }
  .conn.down {
    color: #ffb3b3;
  }
  .ready {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .ready.down {
    color: #ffb3b3;
  }
  .ready code {
    background: var(--bg);
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
  }
  .ready button {
    margin-left: 0.5rem;
    padding: 0.2rem 0.6rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--accent);
    color: white;
    cursor: pointer;
  }
  .gear {
    margin-left: auto;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 8px;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  .settings label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .settings input {
    padding: 0.5rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font-family: inherit;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .actions button {
    padding: 0.45rem 1rem;
    border-radius: 8px;
    border: none;
    background: var(--accent);
    color: white;
    font-weight: 600;
    cursor: pointer;
  }
  .note {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .save-error {
    margin: 0.5rem 0 0;
    color: #ffb3b3;
    font-size: 0.85rem;
  }
  .onboard {
    margin: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 2rem;
    max-width: 360px;
    width: 100%;
  }
  .onboard h2 {
    margin: 0;
    font-size: 1.2rem;
  }
  .onboard p {
    margin: 0;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .onboard input {
    padding: 0.5rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font-family: inherit;
    font-size: 0.95rem;
  }
  .onboard button {
    padding: 0.5rem 1rem;
    border-radius: 8px;
    border: none;
    background: var(--accent);
    color: white;
    font-weight: 600;
    cursor: pointer;
    align-self: flex-start;
  }
  .onboard button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .onboard-error {
    margin: 0;
    color: #ffb3b3;
    font-size: 0.85rem;
  }
  .profile-badge {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
    color: var(--muted);
  }
  header select {
    background: var(--panel);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 8px;
    padding: 0.25rem 0.5rem;
    font-family: inherit;
    font-size: 0.85rem;
  }
</style>
