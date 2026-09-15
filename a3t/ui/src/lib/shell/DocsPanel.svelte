<script lang="ts">
  import { mockDocs, type PageDocs } from '$lib/mock/docs';

  let { open = false, pageKey = '', onClose }: {
    open: boolean;
    pageKey: string;
    onClose: () => void;
  } = $props();

  let docs = $derived(mockDocs[pageKey] || null);
</script>

{#if open}
  <button class="docs-backdrop" onclick={onClose} aria-label="Close docs panel"></button>
{/if}
<div class="docs-panel" class:open>
  {#if docs}
    <div class="docs-header">
      <h2>{docs.title}</h2>
      <button class="docs-close" onclick={onClose} title="Close">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>
    <div class="docs-body">
      {#each docs.sections as section}
        <h3>{section.heading}</h3>
        <div class="doc-section">{@html section.body}</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .docs-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.15);
    z-index: 200;
    border: none;
    cursor: default;
  }
  .docs-panel {
    position: fixed;
    top: 6px;
    bottom: 0;
    right: 6px;
    width: calc(100vw - 240px - 50px);
    background: var(--bg-surface);
    border-left: 1px solid var(--border);
    border-top: 1px solid var(--border);
    border-radius: 14px 14px 0 0;
    box-shadow: -4px 0 24px rgba(0,0,0,0.1);
    z-index: 210;
    display: flex;
    flex-direction: column;
    transform: translateX(105%);
    transition: transform 0.35s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .docs-panel.open {
    transform: translateX(0);
  }

  .docs-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 28px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .docs-header h2 {
    font-family: var(--font-serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
  }
  .docs-close {
    width: 40px;
    height: 40px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg-surface);
    color: var(--text-3);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 0.15s;
  }
  .docs-close:hover {
    background: var(--err-bg);
    border-color: var(--err);
    color: var(--err);
  }

  .docs-body {
    flex: 1;
    overflow-y: auto;
    padding: 28px;
  }
  .docs-body h3 {
    font-family: var(--font-serif);
    font-size: 17px;
    font-weight: 500;
    margin: 24px 0 10px;
  }
  .docs-body h3:first-child { margin-top: 0; }

  .doc-section {
    font-size: 14px;
    color: var(--text-2);
    line-height: 1.65;
    margin-bottom: 10px;
  }
  .doc-section :global(ol) { padding-left: 20px; margin-bottom: 14px; }
  .doc-section :global(li) { margin-bottom: 6px; }
  .doc-section :global(dl) { display: grid; grid-template-columns: 140px 1fr; gap: 8px 16px; margin-bottom: 14px; }
  .doc-section :global(dt) { font-size: 13px; }
  .doc-section :global(dd) { font-size: 13px; line-height: 1.5; }
  .doc-section :global(code) { font-family: var(--font-mono); font-size: 12px; background: var(--bg-subtle); padding: 1px 5px; border-radius: 3px; }
</style>
