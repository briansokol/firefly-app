<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { renderMarkdown } from "./markdown";

  let { source }: { source: string } = $props();
  const html = $derived(renderMarkdown(source));

  function handleClick(e: MouseEvent) {
    const anchor = (e.target as HTMLElement).closest("a[href]");
    if (!anchor) return;
    e.preventDefault();
    const href = anchor.getAttribute("href");
    if (href) openUrl(href);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="ff-md" onclick={handleClick}>{@html html}</div>

<style>
  /* {@html} content is unscoped, so target it with :global rooted at .ff-md */
  .ff-md :global(p) { margin: 0 0 0.6em; }
  .ff-md :global(p:last-child) { margin-bottom: 0; }
  .ff-md :global(p:first-child) { margin-top: 0; }

  .ff-md :global(h1),
  .ff-md :global(h2),
  .ff-md :global(h3),
  .ff-md :global(h4),
  .ff-md :global(h5),
  .ff-md :global(h6) {
    font-family: var(--ff-font-display);
    line-height: var(--ff-leading-tight);
    margin: 0.6em 0 0.4em;
  }
  .ff-md :global(h1) { font-size: var(--ff-text-lg); }
  .ff-md :global(h2) { font-size: var(--ff-text-md); }
  .ff-md :global(h3) { font-size: var(--ff-text-base); }

  .ff-md :global(ul),
  .ff-md :global(ol) { margin: 0 0 0.6em; padding-left: 1.4em; }
  .ff-md :global(li) { margin: 0.15em 0; }

  .ff-md :global(a) { color: var(--ff-accent-light); text-decoration: underline; cursor: pointer; }

  .ff-md :global(code) {
    font-family: var(--ff-font-mono);
    font-size: 0.9em;
    background: var(--ff-surface-control);
    padding: 0.1em 0.35em;
    border-radius: var(--ff-radius-sm);
    word-break: break-word;
  }
  .ff-md :global(pre) {
    background: var(--ff-surface-app);
    padding: 12px 14px;
    border-radius: var(--ff-radius-md);
    overflow-x: auto;
    margin: 0 0 0.6em;
  }
  .ff-md :global(pre code) {
    background: none;
    padding: 0;
    font-size: 0.86em;
    line-height: 1.5;
  }

  .ff-md :global(blockquote) {
    margin: 0 0 0.6em;
    padding-left: 12px;
    border-left: 2px solid var(--ff-line-2);
    color: var(--ff-text-muted);
  }

  .ff-md :global(table) { border-collapse: collapse; margin: 0 0 0.6em; display: block; overflow-x: auto; }
  .ff-md :global(th),
  .ff-md :global(td) { border: 1px solid var(--ff-line-1); padding: 6px 10px; }
  .ff-md :global(th) { font-weight: 800; }

  .ff-md :global(hr) { border: none; border-top: 1px solid var(--ff-line-2); margin: 0.8em 0; }

  /* User bubble: violet gradient bg + white text — override code/link contrast */
  :global(.ff-bubble-user) .ff-md :global(code),
  :global(.ff-bubble-user) .ff-md :global(pre) { background: rgba(255, 255, 255, 0.15); }
  :global(.ff-bubble-user) .ff-md :global(a) { color: #fff; }
  :global(.ff-bubble-user) .ff-md :global(blockquote) { border-left-color: rgba(255, 255, 255, 0.4); color: inherit; }
</style>
