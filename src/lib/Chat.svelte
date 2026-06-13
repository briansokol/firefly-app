<script lang="ts">
  import {
    getMessages,
    sendMessage,
    generateConversationTitle,
    type Message,
    type TaskHint,
  } from "./api";
  import Markdown from "./Markdown.svelte";

  // `reasoning` is ephemeral; tier/servedModel/degraded drive the badge.
  type ChatMessage = Message & { reasoning?: string; degraded?: boolean };

  let {
    conversationId,
    refreshSignal = 0,
    profile = "adult",
    isMobile = false,
    ontitled,
  }: {
    conversationId: string | null;
    refreshSignal?: number;
    profile?: "kid" | "adult";
    isMobile?: boolean;
    ontitled?: () => void;
  } = $props();

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
    if (isMobile && (task === "quick" || task === "private")) {
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

  // Best-effort: name a brand-new conversation from its first message.
  // Never let a naming failure disrupt the chat.
  async function nameConversation(id: string, firstMessage: string) {
    try {
      await generateConversationTitle(id, firstMessage);
      ontitled?.();
    } catch {
      /* on-device model may be unreachable; leave the default title */
    }
  }

  async function submit(event: Event) {
    event.preventDefault();
    const id = conversationId;
    const content = draft.trim();
    if (!id || !content || sending) return;

    const isFirst = messages.length === 0;
    const taskHint = task;

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
      const sendPromise = sendMessage(id, content, taskHint, (e) => {
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

      // Auto-name the first message. Skip `private` (its content must not leave
      // the device, and the title row syncs). For `quick`, let the user's request
      // run first; for other tiers, name in parallel so the reply isn't delayed.
      if (isFirst && taskHint !== "private") {
        if (taskHint === "quick") {
          await sendPromise;
          await nameConversation(id, content);
        } else {
          nameConversation(id, content);
          await sendPromise;
        }
      } else {
        await sendPromise;
      }
    } catch (err) {
      error = String(err);
    } finally {
      sending = false;
    }
  }
</script>

<section class="chat-pane">
  {#if !conversationId}
    <div class="placeholder">
      <span class="ff-spark"></span>
      Select or create a conversation to begin.
    </div>
  {:else}
    <div class="messages">
      {#each messages as m (m.id)}
        {#if m.role === "user"}
          <div class="msg-user">
            <div class="ff-bubble-user"><Markdown source={m.content} /></div>
          </div>
        {:else}
          <div class="msg-ai">
            <div class="ai-avatar"><span></span></div>
            <div class="ai-body">
              {#if m.tier}
                <span class="ff-badge mode" class:ff-badge--warm={m.degraded}>
                  {m.tier}{m.servedModel ? ` · ${m.servedModel}` : ""}{m.degraded
                    ? " · degraded"
                    : ""}
                </span>
              {/if}
              {#if m.reasoning}
                <details class="thinking">
                  <summary>Thinking</summary>
                  <div class="thinking-body">{m.reasoning}</div>
                </details>
              {/if}
              {#if m.content}
                <div class="ff-bubble-ai"><Markdown source={m.content} /></div>
              {:else}
                <div class="ff-bubble-ai">
                  <span class="typing"><i></i><i></i><i></i></span>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </div>

    {#if error}
      <div class="error">{error}</div>
    {/if}

    <footer class="composer">
      <form class="pillbar" onsubmit={submit}>
        <select bind:value={task} class="model" title="Task tier" aria-label="Task tier">
          <option value="agentic">Agentic</option>
          {#if !isMobile}
            <option value="quick">Quick</option>
            <option value="private">Private (on-device)</option>
          {/if}
          {#if !isKid}
            <option value="write">Write</option>
            <option value="explain-file">Explain file</option>
            <option value="code-complete">Code complete</option>
            <option value="best">Best (cloud)</option>
          {/if}
        </select>
        <textarea
          class="msg-input"
          placeholder="Message Firefly…"
          bind:value={draft}
          rows="1"
          onkeydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit(e);
            }
          }}
        ></textarea>
        <button class="send" type="submit" disabled={sending || !draft.trim()} title="Send">
          {#if sending}
            <span class="send-dots"><i></i><i></i><i></i></span>
          {:else}
            <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true"
              ><path
                d="M2.2 8.4L15 2.5a.5.5 0 01.66.66L9.8 16a.5.5 0 01-.93.02L7 11.2a1 1 0 00-.5-.5L2.18 9.33a.5.5 0 01.02-.93z"
                fill="#fff"
              /></svg
            >
          {/if}
        </button>
      </form>
    </footer>
  {/if}
</section>

<style>
  .chat-pane {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .placeholder {
    margin: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--ff-text-muted);
    font-weight: 700;
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    background:
      radial-gradient(800px 400px at 30% 110%, rgba(139, 92, 246, 0.1), transparent 60%),
      var(--ff-surface-app);
  }

  .msg-user {
    align-self: flex-end;
    max-width: 62%;
  }
  .ff-bubble-user {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .msg-ai {
    align-self: flex-start;
    max-width: 74%;
    display: flex;
    gap: 12px;
  }
  .ai-avatar {
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--ff-surface-control);
    display: grid;
    place-items: center;
    margin-top: 2px;
    box-shadow: 0 0 0 1.5px var(--ff-line-2);
  }
  .ai-avatar span {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--ff-firefly);
    box-shadow: var(--ff-glow-firefly);
  }
  .ai-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-width: 0;
  }
  .mode {
    align-self: flex-start;
  }
  .ff-bubble-ai {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .typing {
    display: inline-flex;
    gap: 5px;
  }
  .typing i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ff-text-dim);
    animation: blink 1.2s infinite;
  }
  .typing i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .typing i:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes blink {
    0%,
    60%,
    100% {
      opacity: 0.3;
    }
    30% {
      opacity: 1;
    }
  }

  .thinking {
    font-size: var(--ff-text-sm);
    color: var(--ff-text-muted);
    font-weight: 700;
  }
  .thinking summary {
    cursor: pointer;
    color: var(--ff-text-dim);
  }
  .thinking-body {
    margin-top: 6px;
    padding: 8px 12px;
    border-left: 2px solid var(--ff-line-2);
    white-space: pre-wrap;
    color: var(--ff-text-muted);
  }

  .error {
    margin: 0 22px;
    padding: 12px 16px;
    border-radius: var(--ff-radius-md);
    background: rgba(255, 141, 154, 0.1);
    color: var(--ff-red);
    font-size: var(--ff-text-base);
    font-weight: 700;
  }

  .composer {
    display: flex;
    align-items: flex-end;
    padding: 16px 22px 22px;
    flex: none;
  }
  .pillbar {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 56px;
    padding: 6px 6px 6px 9px;
    border-radius: 28px;
    background: var(--ff-surface-panel);
    box-shadow: 0 0 0 1.5px var(--ff-line-1);
    transition: box-shadow var(--ff-dur-fast) var(--ff-ease);
  }
  .pillbar:focus-within {
    box-shadow:
      0 0 0 2px var(--ff-accent),
      var(--ff-glow-accent);
  }
  .model {
    flex: none;
    height: 44px;
    padding: 0 38px 0 16px;
    border-radius: 22px;
    border: none;
    background: var(--ff-surface-control)
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8'%3E%3Cpath d='M1 1.5l5 5 5-5' stroke='%239b93b8' stroke-width='2.2' fill='none' stroke-linecap='round'/%3E%3C/svg%3E")
      no-repeat right 14px center;
    font: 600 13px var(--ff-font-display);
    color: var(--ff-text-muted);
    cursor: pointer;
    appearance: none;
    outline: none;
  }
  .model:focus-visible {
    box-shadow: var(--ff-ring-focus);
  }
  .msg-input {
    flex: 1;
    align-self: center;
    background: transparent;
    border: none;
    outline: none;
    resize: none;
    color: var(--ff-text-body);
    font: 700 14.5px var(--ff-font-body);
    line-height: 1.45;
    padding: 0 8px;
    max-height: 120px;
  }
  .msg-input::placeholder {
    color: var(--ff-text-dim);
  }
  .send {
    flex: none;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--ff-grad-accent);
    border: none;
    cursor: pointer;
    box-shadow: var(--ff-glow-accent-strong);
    transition:
      transform var(--ff-dur-fast) var(--ff-ease-bounce),
      filter var(--ff-dur-fast) var(--ff-ease);
  }
  .send:hover:not(:disabled) {
    transform: scale(1.06);
    filter: brightness(1.08);
  }
  .send:active:not(:disabled) {
    transform: scale(0.95);
  }
  .send:disabled {
    opacity: 0.5;
    cursor: default;
    box-shadow: none;
  }
  .send-dots {
    display: inline-flex;
    gap: 3px;
  }
  .send-dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #fff;
    animation: blink 1.2s infinite;
  }
  .send-dots i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .send-dots i:nth-child(3) {
    animation-delay: 0.4s;
  }
</style>
