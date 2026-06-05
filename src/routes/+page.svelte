<script lang="ts">
  import {
    listConversations,
    createConversation,
    getSettings,
    setSettings,
    setToken,
    hasToken,
    checkFirefly,
    type Conversation,
    type Settings,
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
  let tokenPresent = $state(true);
  let showSettings = $state(false);
  let tokenInput = $state("");
  let savedNote = $state("");

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
    if (tokenInput.trim()) {
      await setToken(tokenInput.trim());
      tokenInput = "";
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
</style>
