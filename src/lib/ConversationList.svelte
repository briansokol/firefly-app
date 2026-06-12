<script lang="ts">
  import type { Conversation } from "./api";

  let {
    conversations,
    selectedId,
    onselect,
    onnew,
    onrename,
    userName = "",
    syncNote = "",
  }: {
    conversations: Conversation[];
    selectedId: string | null;
    onselect: (id: string) => void;
    onnew: () => void;
    onrename: (id: string, title: string) => void;
    userName?: string;
    syncNote?: string;
  } = $props();

  const initial = $derived((userName.trim()[0] ?? "·").toUpperCase());

  let editingId = $state<string | null>(null);
  let editValue = $state("");

  function startEdit(c: Conversation) {
    editingId = c.id;
    editValue = c.title;
  }
  function cancelEdit() {
    editingId = null;
    editValue = "";
  }
  function acceptEdit(id: string) {
    const next = editValue.trim();
    const current = conversations.find((c) => c.id === id)?.title ?? "";
    if (next && next !== current) onrename(id, next);
    editingId = null;
    editValue = "";
  }
</script>

<aside class="side">
  <button class="new-conv" onclick={onnew}>＋ New conversation</button>

  <nav class="convs">
    <div class="ff-overline conv-label">Conversations</div>
    {#each conversations as c (c.id)}
      <div class="ff-conv conv" class:is-active={c.id === selectedId}>
        {#if editingId === c.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="edit-input"
            aria-label="Conversation name"
            bind:value={editValue}
            autofocus
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                acceptEdit(c.id);
              } else if (e.key === "Escape") {
                e.preventDefault();
                cancelEdit();
              }
            }}
          />
          <button class="icon-mini" title="Save" aria-label="Save name" onclick={() => acceptEdit(c.id)}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true"
              ><path d="M3.5 8.5l3 3 6-7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg
            >
          </button>
          <button class="icon-mini" title="Cancel" aria-label="Cancel rename" onclick={cancelEdit}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true"
              ><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg
            >
          </button>
        {:else}
          <button class="conv-open" onclick={() => onselect(c.id)}>
            <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
              ><path
                d="M2 3.5A1.5 1.5 0 013.5 2h9A1.5 1.5 0 0114 3.5v7a1.5 1.5 0 01-1.5 1.5H8l-3.5 3v-3h-1A1.5 1.5 0 012 10.5v-7z"
                fill="currentColor"
              /></svg
            >
            <span class="conv-title">{c.title}</span>
          </button>
          <button class="icon-mini edit-btn" title="Rename" aria-label="Rename conversation" onclick={() => startEdit(c)}>
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true"
              ><path d="M11.5 2.5l2 2L6 12l-2.5.5L4 10l7.5-7.5z" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg
            >
          </button>
        {/if}
      </div>
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
  /* .ff-conv is global; override its padding so .conv-open fills the row. */
  .conv {
    width: 100%;
    padding: 0;
    border: none;
    background: transparent;
  }
  .conv-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
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
  .icon-mini {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    margin-right: 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--ff-text-muted);
    cursor: pointer;
    transition:
      background var(--ff-dur-fast) var(--ff-ease),
      color var(--ff-dur-fast) var(--ff-ease),
      opacity var(--ff-dur-fast) var(--ff-ease);
  }
  .icon-mini:hover {
    background: var(--ff-surface-active);
    color: var(--ff-text-body);
  }
  .edit-btn {
    opacity: 0;
  }
  .conv:hover .edit-btn,
  .conv:focus-within .edit-btn,
  .conv.is-active .edit-btn {
    opacity: 1;
  }
  .edit-input {
    flex: 1;
    min-width: 0;
    height: 34px;
    margin: 6px 0 6px 12px;
    padding: 0 10px;
    border-radius: 10px;
    border: 2px solid var(--ff-accent);
    background: var(--ff-surface-app);
    color: var(--ff-text-body);
    font: 700 13.5px var(--ff-font-body);
    outline: none;
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
