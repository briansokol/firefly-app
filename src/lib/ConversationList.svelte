<script lang="ts">
  import type { Conversation } from "./api";

  let {
    conversations,
    selectedId,
    onselect,
    onnew,
    userName = "",
    syncNote = "",
  }: {
    conversations: Conversation[];
    selectedId: string | null;
    onselect: (id: string) => void;
    onnew: () => void;
    userName?: string;
    syncNote?: string;
  } = $props();

  const initial = $derived((userName.trim()[0] ?? "·").toUpperCase());
</script>

<aside class="side">
  <button class="new-conv" onclick={onnew}>＋ New conversation</button>

  <nav class="convs">
    <div class="ff-overline conv-label">Conversations</div>
    {#each conversations as c (c.id)}
      <button
        class="ff-conv conv"
        class:is-active={c.id === selectedId}
        onclick={() => onselect(c.id)}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
          ><path
            d="M2 3.5A1.5 1.5 0 013.5 2h9A1.5 1.5 0 0114 3.5v7a1.5 1.5 0 01-1.5 1.5H8l-3.5 3v-3h-1A1.5 1.5 0 012 10.5v-7z"
            fill="currentColor"
          /></svg
        >
        <span class="conv-title">{c.title}</span>
      </button>
    {/each}
    {#if conversations.length === 0}
      <div class="empty">No conversations yet</div>
    {/if}
  </nav>

  {#if userName}
    <div class="side-foot">
      <span class="ff-avatar">{initial}</span>
      <div>
        <b>{userName}</b>
        <small>{syncNote}</small>
      </div>
    </div>
  {/if}
</aside>

<style>
  .side {
    width: 264px;
    flex: none;
    background: var(--ff-surface-panel);
    display: flex;
    flex-direction: column;
    padding: var(--ff-space-4);
    gap: var(--ff-space-4);
    overflow: hidden;
  }
  .new-conv {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: var(--ff-control-h);
    border: none;
    cursor: pointer;
    border-radius: var(--ff-radius-pill);
    background: var(--ff-grad-accent);
    color: #fff;
    font: 600 15px var(--ff-font-display);
    box-shadow: var(--ff-glow-accent-soft);
    transition:
      filter var(--ff-dur-fast) var(--ff-ease),
      transform var(--ff-dur-fast) var(--ff-ease-bounce);
  }
  .new-conv:hover {
    filter: brightness(1.1);
    transform: translateY(-1px);
  }
  .new-conv:active {
    transform: translateY(0) scale(0.98);
  }
  .convs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    min-height: 0;
  }
  .conv-label {
    padding: 6px 12px 2px;
    letter-spacing: 0.06em;
  }
  /* .ff-conv is global; reset the button chrome and let the design class style it. */
  .conv {
    width: 100%;
    border: none;
    background: transparent;
    font-family: inherit;
    text-align: left;
  }
  .conv svg {
    flex: none;
    opacity: 0.75;
  }
  .conv-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    padding: 10px 14px;
    color: var(--ff-text-dim);
    font-size: var(--ff-text-sm);
    font-weight: 700;
  }
  .side-foot {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 18px;
    background: var(--ff-surface-control);
  }
  .side-foot b {
    font-size: 13.5px;
    color: var(--ff-text-body);
  }
  .side-foot small {
    display: block;
    font-size: 11px;
    color: var(--ff-text-dim);
    font-weight: 700;
  }
</style>
