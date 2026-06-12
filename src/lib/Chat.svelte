<script lang="ts">
  import {
    getMessages,
    sendMessage,
    type Message,
    type TaskHint,
  } from "./api";

  // `reasoning` is ephemeral; tier/servedModel/degraded drive the badge.
  type ChatMessage = Message & { reasoning?: string; degraded?: boolean };

  let {
    conversationId,
    refreshSignal = 0,
    profile = "adult",
  }: { conversationId: string | null; refreshSignal?: number; profile?: "kid" | "adult" } = $props();

  const isKid = $derived(profile === "kid");

  let messages = $state<ChatMessage[]>([]);
  let draft = $state("");
  let task = $state<TaskHint>("agentic");
  let sending = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (isKid && !["agentic", "quick", "private"].includes(task)) {
      task = "agentic";
    }
  });

  $effect(() => {
    const id = conversationId;
    refreshSignal; // re-run when a sync pulls new rows
    error = null;
    if (id) {
      getMessages(id).then((m) => {
        if (conversationId === id) messages = m;
      });
    } else {
      messages = [];
    }
  });

  async function submit(event: Event) {
    event.preventDefault();
    const id = conversationId;
    const content = draft.trim();
    if (!id || !content || sending) return;

    draft = "";
    error = null;
    sending = true;

    // Optimistic local rows; the Rust core persists the canonical versions.
    messages.push({
      id: `local-user-${Date.now()}`,
      conversationId: id,
      role: "user",
      content,
      createdAt: new Date().toISOString(),
    });
    const assistantIdx =
      messages.push({
        id: `local-assistant-${Date.now()}`,
        conversationId: id,
        role: "assistant",
        content: "",
        reasoning: "",
        createdAt: new Date().toISOString(),
      }) - 1;

    try {
      await sendMessage(id, content, task, (e) => {
        if (e.type === "token") {
          messages[assistantIdx].content += e.text;
        } else if (e.type === "reasoning") {
          messages[assistantIdx].reasoning =
            (messages[assistantIdx].reasoning ?? "") + e.text;
        } else if (e.type === "routed") {
          messages[assistantIdx].tier = e.tier;
          messages[assistantIdx].servedModel = e.model;
          messages[assistantIdx].degraded = e.degraded;
        } else if (e.type === "error") {
          error = e.message;
        }
      });
    } catch (err) {
      error = String(err);
    } finally {
      sending = false;
    }
  }
</script>

<section class="chat">
  {#if !conversationId}
    <div class="placeholder">Select or create a conversation to begin.</div>
  {:else}
    <div class="messages">
      {#each messages as m (m.id)}
        <div class="msg {m.role}">
          <div class="role">{m.role}</div>
          {#if m.role === "assistant" && m.tier}
            <div class="badge" class:degraded={m.degraded}>
              {m.tier}{m.servedModel ? ` · ${m.servedModel}` : ""}{m.degraded ? " · degraded" : ""}
            </div>
          {/if}
          {#if m.reasoning}
            <details class="thinking">
              <summary>thinking</summary>
              <div class="thinking-body">{m.reasoning}</div>
            </details>
          {/if}
          {#if m.content}
            <div class="bubble">{m.content}</div>
          {:else if m.role === "assistant" && m.reasoning}
            <div class="bubble pending">…</div>
          {/if}
        </div>
      {/each}
    </div>

    {#if error}
      <div class="error">{error}</div>
    {/if}

    <form class="composer" onsubmit={submit}>
      <select bind:value={task} class="task" title="Task tier">
        <option value="agentic">agentic</option>
        <option value="quick">quick</option>
        <option value="private">private (on-device)</option>
        {#if !isKid}
          <option value="write">write</option>
          <option value="explain-file">explain-file</option>
          <option value="code-complete">code-complete</option>
          <option value="best">best (cloud)</option>
        {/if}
      </select>
      <textarea
        placeholder="Message Firefly…"
        bind:value={draft}
        rows="2"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit(e);
          }
        }}
      ></textarea>
      <button type="submit" disabled={sending || !draft.trim()}>
        {sending ? "…" : "Send"}
      </button>
    </form>
  {/if}
</section>

<style>
  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .placeholder {
    margin: auto;
    color: var(--muted);
  }
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 1rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }
  .msg {
    max-width: 80%;
  }
  .msg.user {
    align-self: flex-end;
    text-align: right;
  }
  .role {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
    margin-bottom: 0.2rem;
  }
  .bubble {
    display: inline-block;
    padding: 0.6rem 0.8rem;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--border);
    white-space: pre-wrap;
    text-align: left;
    word-break: break-word;
  }
  .msg.user .bubble {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .bubble.pending {
    color: var(--muted);
  }
  .badge {
    display: inline-block;
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.05rem 0.5rem;
    margin-bottom: 0.3rem;
  }
  .badge.degraded {
    color: #ffd7a8;
    border-color: #7a5a2a;
  }
  .task {
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--text);
    padding: 0 0.4rem;
    font-family: inherit;
  }
  .thinking {
    margin-bottom: 0.35rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .thinking summary {
    cursor: pointer;
  }
  .thinking-body {
    margin-top: 0.3rem;
    padding: 0.5rem 0.7rem;
    border-left: 2px solid var(--border);
    white-space: pre-wrap;
    color: var(--muted);
  }
  .error {
    margin: 0 1.25rem;
    padding: 0.6rem 0.8rem;
    border-radius: 8px;
    background: #5b1d1d;
    color: #ffd7d7;
    font-size: 0.85rem;
  }
  .composer {
    display: flex;
    gap: 0.5rem;
    padding: 0.75rem 1rem 1rem;
    border-top: 1px solid var(--border);
  }
  textarea {
    flex: 1;
    resize: none;
    padding: 0.6rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--text);
    font-family: inherit;
    font-size: 0.95rem;
  }
  .composer button {
    padding: 0 1.1rem;
    border-radius: 8px;
    border: none;
    background: var(--accent);
    color: white;
    font-weight: 600;
    cursor: pointer;
  }
  .composer button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
