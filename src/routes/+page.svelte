<script lang="ts">
  import {
    listConversations,
    createConversation,
    getSettings,
    setSettings,
    setToken,
    hasToken,
    checkFirefly,
    checkOnDevice,
    pullOnDeviceModel,
    type Conversation,
    type Settings,
    type OnDeviceStatus,
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
  let tokenPresent = $state(true);
  let showSettings = $state(false);
  let tokenInput = $state("");
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
      checkFirefly().then((r) => (reachable = r));
      checkOnDevice().then((r) => (onDevice = r));
      tokenPresent = await hasToken();
      if (!tokenPresent) showSettings = true;
      await refresh();
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
    });
    saveError = "";
    if (tokenInput.trim()) {
      try {
        await setToken(tokenInput.trim());
        tokenInput = "";
      } catch (e) {
        saveError = String(e);
        return;
      }
    }
    tokenPresent = await hasToken();
    reachable = await checkFirefly();
    savedNote = "Saved";
    setTimeout(() => (savedNote = ""), 1500);
  }
</script>

<div class="app">
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
      {#if !tokenPresent}
        <span class="warn">no token set</span>
      {/if}
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
        <label>Cloud model: best
          <input bind:value={settings.modelFrontier} spellcheck="false" />
        </label>
        <label>
          Device token {tokenPresent ? "(stored — leave blank to keep)" : "(required)"}
          <input
            type="password"
            bind:value={tokenInput}
            placeholder="sk-…"
            spellcheck="false"
          />
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

    <Chat conversationId={selectedId} />
  </main>
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
  .warn {
    font-size: 0.75rem;
    color: #ffb3b3;
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
</style>
