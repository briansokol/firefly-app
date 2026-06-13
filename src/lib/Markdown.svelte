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
  /* Styling added in Task 8. */
</style>
