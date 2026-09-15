<script lang="ts">
  import type { Snippet } from 'svelte';

  interface ContextItem {
    id: string;
    label: string;
    icon?: string;
    meta?: string;
  }

  let {
    open = false,
    items = [],
    selectedId = '',
    title = '',
    onSelect,
    onClose,
    children,
  }: {
    open: boolean;
    items: ContextItem[];
    selectedId: string;
    title: string;
    onSelect: (id: string) => void;
    onClose: () => void;
    children: Snippet;
  } = $props();

  let dropdownOpen = $state(false);
  let selectedItem = $derived(items.find(i => i.id === selectedId));
</script>

<div class="context-panel" class:open>
  <!-- Header with combo selector -->
  <div class="cp-header">
    <button class="cp-close" onclick={onClose} title="Close">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18M6 6l12 12"/></svg>
    </button>
    <span class="cp-title">{title}</span>
  </div>

  <!-- Combo selector -->
  <div class="cp-combo">
    <button class="combo-trigger" onclick={() => dropdownOpen = !dropdownOpen}>
      <span class="combo-label">{selectedItem?.label || 'Select...'}</span>
      {#if selectedItem?.meta}
        <span class="combo-meta">{selectedItem.meta}</span>
      {/if}
      <svg class="combo-chevron" class:flipped={dropdownOpen} width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6l4 4 4-4"/></svg>
    </button>
    {#if dropdownOpen}
      <div class="combo-dropdown">
        {#each items as item}
          <button
            class="combo-option"
            class:active={item.id === selectedId}
            onclick={() => { onSelect(item.id); dropdownOpen = false; }}
          >
            <span class="co-label">{item.label}</span>
            {#if item.meta}<span class="co-meta">{item.meta}</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Tree content (provided by page) -->
  <div class="cp-body">
    {@render children()}
  </div>
</div>

<style>
  .context-panel {
    width: 0;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: width 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .context-panel.open {
    width: 300px;
  }

  .cp-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .cp-close {
    width: 28px; height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    color: var(--text-3);
    display: grid; place-items: center;
    cursor: pointer;
  }
  .cp-close:hover { background: var(--bg-subtle); color: var(--text-1); }
  .cp-title {
    font-family: var(--font-serif);
    font-size: 15px;
    font-weight: 500;
    color: var(--text-1);
  }

  /* Combo selector */
  .cp-combo {
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    position: relative;
  }
  .combo-trigger {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    cursor: pointer;
    gap: 8px;
    transition: border-color 0.15s;
  }
  .combo-trigger:hover { border-color: var(--border-strong); }
  .combo-label {
    flex: 1;
    text-align: left;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .combo-meta {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-3);
  }
  .combo-chevron { flex-shrink: 0; color: var(--text-3); transition: transform 0.2s; }
  .combo-chevron.flipped { transform: rotate(180deg); }

  .combo-dropdown {
    position: absolute;
    top: 100%;
    left: 14px;
    right: 14px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-lg);
    z-index: 50;
    max-height: 240px;
    overflow-y: auto;
    margin-top: 4px;
  }
  .combo-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: none;
    cursor: pointer;
    text-align: left;
    font-size: 13px;
    color: var(--text-2);
  }
  .combo-option:hover { background: var(--bg-subtle); }
  .combo-option.active { background: var(--bg-subtle); color: var(--text-1); font-weight: 500; }
  .co-label { flex: 1; }
  .co-meta { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); }

  /* Body */
  .cp-body {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
  }
</style>
