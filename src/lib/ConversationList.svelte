<script lang="ts">
  import type { Conversation } from "./api";

  let {
    conversations,
    selectedId,
    onselect,
    onnew,
  }: {
    conversations: Conversation[];
    selectedId: string | null;
    onselect: (id: string) => void;
    onnew: () => void;
  } = $props();
</script>

<aside class="sidebar">
  <button class="new" onclick={onnew}>+ New conversation</button>
  <ul>
    {#each conversations as c (c.id)}
      <li>
        <button
          class="item"
          class:active={c.id === selectedId}
          onclick={() => onselect(c.id)}
        >
          {c.title}
        </button>
      </li>
    {/each}
    {#if conversations.length === 0}
      <li class="empty">No conversations yet</li>
    {/if}
  </ul>
</aside>

<style>
  .sidebar {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--panel);
  }
  .new {
    margin: 0.75rem;
    padding: 0.55rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--accent);
    color: white;
    cursor: pointer;
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 0.5rem;
  }
  .item {
    width: 100%;
    text-align: left;
    padding: 0.55rem 0.6rem;
    margin-bottom: 0.25rem;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.active {
    background: var(--hover);
    font-weight: 600;
  }
  .empty {
    padding: 0.6rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
