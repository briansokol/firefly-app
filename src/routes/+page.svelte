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
        // pick up server-side kid->adult upgrades for the active profile
        profiles = await refreshActiveProfile();
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

  const syncNote = $derived(
    syncing
      ? "Syncing…"
      : syncStatus == null
        ? "Sync idle"
        : syncStatus.ok
          ? "Synced · just now"
          : (syncStatus.message ?? "Offline"),
  );
</script>

<div class="app">
  {#if profiles.length === 0}
    <div class="onboard-screen">
      <div class="onboard ff-card">
        <div class="brand brand--lg"><span class="ff-spark"></span> Firefly</div>
        <h2>Create a profile</h2>
        <p>New profiles start as a kid profile. An adult can upgrade it later on the server.</p>
        {#if error}
          <p class="onboard-error">{error}</p>
        {/if}
        <input
          class="ff-input"
          aria-label="Display name"
          placeholder="Display name"
          bind:value={newProfileName}
        />
        <button
          class="ff-btn ff-btn--primary"
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
    </div>
  {:else}
    <ConversationList
      {conversations}
      {selectedId}
      onselect={(id) => (selectedId = id)}
      onnew={newConversation}
      userName={active?.displayName ?? ""}
      {syncNote}
    />

    <main>
      <header class="head">
        <div class="brand"><span class="ff-spark"></span> Firefly</div>
        <span class="ff-chip">
          {#if reachable}<span class="ff-dot-online"></span>{/if}
          {reachable === null ? "Connecting…" : reachable ? "Online" : "Offline"}
        </span>
        <span class="ff-chip">{syncNote}</span>

        <div class="spacer"></div>

        <div class="profile">
          <select
            class="profile-select"
            aria-label="Switch profile"
            value={active?.userId ?? ""}
            onchange={async (e) => {
              const target = e.currentTarget as HTMLSelectElement;
              try {
                profiles = await switchProfile(target.value);
                selectedId = null;
                await refresh();
                runSync();
              } catch (err) {
                error = String(err);
                // re-sync the dropdown to the actual active profile
                profiles = await listProfiles();
              }
            }}
          >
            {#each profiles as p (p.userId)}
              <option value={p.userId}>{p.displayName}</option>
            {/each}
          </select>
          {#if active}
            <span class="ff-badge" class:ff-badge--warm={active.profile === "adult"}
              >{active.profile}</span
            >
          {/if}
        </div>

        <button class="icon-btn" onclick={runSync} disabled={syncing} title="Sync now">
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true"
            ><path
              d="M13.5 8a5.5 5.5 0 11-1.6-3.9M13.5 1.5v3h-3"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            /></svg
          >
        </button>
        <button
          class="icon-btn"
          class:is-active={showSettings}
          onclick={() => (showSettings = !showSettings)}
          title="Settings"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true"
            ><path d="M8 10.2a2.2 2.2 0 100-4.4 2.2 2.2 0 000 4.4z" stroke="currentColor" stroke-width="1.4" /><path
              d="M13 8c0-.4.5-1.1.4-1.5-.2-.4-1-.5-1.3-.8-.2-.3-.1-1.2-.4-1.4-.4-.3-1.1.2-1.5.1C9.8 4.3 9.4 3.5 9 3.5h-2c-.4 0-.8.8-1.2.9-.4.1-1.1-.4-1.5-.1-.3.2-.2 1.1-.4 1.4-.3.3-1.1.4-1.3.8-.1.4.4 1.1.4 1.5s-.5 1.1-.4 1.5c.2.4 1 .5 1.3.8.2.3.1 1.2.4 1.4.4.3 1.1-.2 1.5-.1.4.1.8.9 1.2.9h2c.4 0 .8-.8 1.2-.9.4-.1 1.1.4 1.5.1.3-.2.2-1.1.4-1.4.3-.3 1.1-.4 1.3-.8.1-.4-.4-1.1-.4-1.5z"
              stroke="currentColor"
              stroke-width="1.2"
            /></svg
          >
        </button>
      </header>

      <div class="content">
        {#if showSettings}
          <div class="settings-scroll">
            <div class="settings-page">
            <section class="ff-card sec">
              <h3>Endpoints</h3>
              <div class="ff-field">
                <span class="ff-label">Firefly endpoint</span>
                <input class="ff-input" bind:value={settings.fireflyEndpoint} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">Sync endpoint</span>
                <input class="ff-input" bind:value={settings.syncEndpoint} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">Device name</span>
                <input class="ff-input" bind:value={settings.deviceName} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">On-device endpoint</span>
                <input class="ff-input" bind:value={settings.onDeviceEndpoint} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">On-device model</span>
                <input class="ff-input" bind:value={settings.onDeviceModel} spellcheck="false" />
              </div>
              {#if onDevice}
                <div class="ready" class:down={onDevice.state === "unreachable"}>
                  {#if onDevice.state === "ready"}
                    On-device ready · {onDevice.model}
                  {:else if onDevice.state === "modelMissing"}
                    Model not installed
                    <button
                      type="button"
                      class="ff-btn ff-btn--sm"
                      onclick={pullModel}
                      disabled={pulling}
                    >
                      {pulling ? `Pulling… ${pullPct}%` : `Pull ${onDevice.model}`}
                    </button>
                  {:else}
                    Server unreachable: install &amp; start Ollama, then pull the model:
                    <code>ollama serve</code> · <code>ollama pull {settings.onDeviceModel}</code>
                    <br />(Framework NPU: run <code>flm serve</code> +
                    <code>flm pull {settings.onDeviceModel}</code> instead)
                  {/if}
                </div>
              {/if}
            </section>

            <section class="ff-card sec">
              <h3>Models</h3>
              <div class="ff-field">
                <span class="ff-label">Home-base model: code/write</span>
                <input class="ff-input" bind:value={settings.modelCode} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">Home-base model: agentic</span>
                <input class="ff-input" bind:value={settings.modelChatHeavy} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">Cloud model: best</span>
                <input class="ff-input" bind:value={settings.modelFrontier} spellcheck="false" />
              </div>
              <div class="ff-field">
                <span class="ff-label">Options</span>
                <label class="ff-check">
                  <input type="checkbox" bind:checked={settings.memoryEnabled} />
                  <span>Inject memories on home-base requests</span>
                </label>
              </div>
            </section>

            <div class="btn-row">
              <button class="ff-btn ff-btn--primary" onclick={saveSettings}>Save changes</button>
              <span class="note">{savedNote}</span>
            </div>
            {#if saveError}
              <p class="save-error">{saveError}</p>
            {/if}
          </div>
        </div>
        {/if}

        <Chat conversationId={selectedId} refreshSignal={syncTick} profile={active?.profile ?? "adult"} />
      </div>
    </main>
  {/if}
</div>

<style>
  .app {
    display: flex;
    height: 100vh;
    background: var(--ff-surface-app);
  }
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* ---------- Header ---------- */
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 22px;
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-right: 8px;
    font: 600 20px var(--ff-font-display);
    color: var(--ff-text-body);
  }
  .spacer {
    flex: 1;
  }
  .profile {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px 0 6px;
    border-radius: var(--ff-radius-pill);
    background: var(--ff-surface-control);
  }
  .profile-select {
    height: 32px;
    border: none;
    background: transparent;
    color: var(--ff-text-body);
    font: 700 13.5px var(--ff-font-body);
    cursor: pointer;
    outline: none;
    padding: 0 6px;
    max-width: 160px;
  }
  .profile-select:focus-visible {
    border-radius: var(--ff-radius-pill);
    box-shadow: var(--ff-ring-focus);
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: var(--ff-control-h-sm);
    height: var(--ff-control-h-sm);
    border-radius: 50%;
    background: var(--ff-surface-control);
    border: none;
    color: var(--ff-text-muted);
    cursor: pointer;
    transition: filter var(--ff-dur-fast) var(--ff-ease);
  }
  .icon-btn:hover:not(:disabled) {
    filter: brightness(1.2);
  }
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .icon-btn.is-active {
    color: var(--ff-text-body);
    box-shadow: inset 0 0 0 1.5px var(--ff-line-2);
  }

  /* ---------- Content area + settings overlay ---------- */
  .content {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .settings-scroll {
    position: absolute;
    inset: 0;
    z-index: 2;
    overflow-y: auto;
    display: flex;
    justify-content: center;
    background:
      radial-gradient(800px 400px at 70% -10%, rgba(139, 92, 246, 0.08), transparent 60%),
      var(--ff-surface-app);
  }
  .settings-page {
    width: 620px;
    max-width: 100%;
    padding: 24px 24px 48px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .sec {
    display: flex;
    flex-direction: column;
  }
  .sec h3 {
    margin: 0 0 18px;
    font: 600 13px var(--ff-font-display);
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--ff-accent-light);
  }
  .sec :global(.ff-field + .ff-field) {
    margin-top: 18px;
  }
  .ready {
    margin-top: 14px;
    font-size: var(--ff-text-sm);
    font-weight: 700;
    color: var(--ff-text-muted);
    line-height: 1.7;
  }
  .ready.down {
    color: var(--ff-red);
  }
  .ready code {
    background: var(--ff-surface-control);
    padding: 2px 7px;
    border-radius: var(--ff-radius-sm);
    font-size: 12px;
  }
  .ready :global(.ff-btn) {
    margin-left: 8px;
  }
  .btn-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .note {
    color: var(--ff-green);
    font-size: var(--ff-text-sm);
    font-weight: 700;
  }
  .save-error {
    margin: 8px 0 0;
    color: var(--ff-red);
    font-size: var(--ff-text-base);
    font-weight: 700;
  }

  /* ---------- Onboarding ---------- */
  .onboard-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background:
      radial-gradient(800px 400px at 50% 120%, rgba(139, 92, 246, 0.1), transparent 60%),
      var(--ff-surface-app);
  }
  .onboard {
    width: 380px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: var(--ff-shadow-pop);
  }
  .brand--lg {
    font-size: 22px;
  }
  .onboard h2 {
    margin: 0;
    font: 600 24px var(--ff-font-display);
    color: var(--ff-text-body);
  }
  .onboard p {
    margin: 0;
    font-size: var(--ff-text-base);
    color: var(--ff-text-muted);
    line-height: var(--ff-leading-body);
  }
  .onboard :global(.ff-btn) {
    align-self: flex-start;
  }
  .onboard-error {
    margin: 0;
    color: var(--ff-red);
    font-size: var(--ff-text-base);
    font-weight: 700;
  }
</style>
