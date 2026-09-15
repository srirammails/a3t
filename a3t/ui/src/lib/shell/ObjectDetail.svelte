<script lang="ts">
  import type { Snippet } from 'svelte';
  import TreeMenu from './TreeMenu.svelte';
  import type { TreeNode } from './TreeMenu.svelte';

  interface ComboItem {
    id: string;
    label: string;
    meta?: string;
  }

  let {
    items = [],
    selectedId = '',
    activeSection = '',
    treeNodes = [],
    objectKind = '',
    onSelect,
    onSectionChange,
    onBack,
    children,
  }: {
    items: ComboItem[];
    selectedId: string;
    activeSection: string;
    treeNodes: TreeNode[];
    objectKind: string;
    onSelect: (id: string) => void;
    onSectionChange: (id: string) => void;
    onBack: () => void;
    children: Snippet;
  } = $props();

  let dropdownOpen = $state(false);
  let selectedItem = $derived(items.find(i => i.id === selectedId));

  function scrollToSection(id: string) {
    onSectionChange(id);
    // Scroll the detail page to the section
    const el = document.getElementById(`section-${id}`);
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  }
</script>

<div class="object-detail">
  <!-- LEFT: context tree -->
  <div class="ctx-tree">
    <button class="ctx-back" onclick={onBack}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 2L4 8l6 6"/></svg>
      <span>Back to {objectKind}s</span>
    </button>

    <div class="ctx-combo">
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
            <button class="combo-option" class:active={item.id === selectedId} onclick={() => { onSelect(item.id); dropdownOpen = false; }}>
              <span class="co-label">{item.label}</span>
              {#if item.meta}<span class="co-meta">{item.meta}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="ctx-menu">
      <TreeMenu nodes={treeNodes} selectedId={activeSection} onSelect={scrollToSection} />
    </div>
  </div>

  <!-- RIGHT: single scrollable detail page -->
  <div class="detail-page">
    {@render children()}
  </div>
</div>

<style>
  .object-detail { display: flex; height: 100%; overflow: hidden; }

  .ctx-tree {
    width: 260px; flex-shrink: 0; background: var(--bg-surface);
    border-right: 1px solid var(--border); display: flex; flex-direction: column; overflow-y: auto;
  }
  .ctx-back {
    display: flex; align-items: center; gap: 8px; padding: 14px 16px;
    border: none; background: none; color: var(--text-3); font-size: 12px;
    cursor: pointer; border-bottom: 1px solid var(--border); transition: color 0.12s;
  }
  .ctx-back:hover { color: var(--text-1); }

  .ctx-combo { padding: 12px 14px; border-bottom: 1px solid var(--border); position: relative; }
  .combo-trigger {
    display: flex; align-items: center; width: 100%; padding: 8px 12px;
    border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); cursor: pointer; gap: 6px;
  }
  .combo-trigger:hover { border-color: var(--border-strong); }
  .combo-label { flex: 1; text-align: left; font-size: 13px; font-weight: 500; color: var(--text-1); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .combo-meta { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); }
  .combo-chevron { flex-shrink: 0; color: var(--text-3); transition: transform 0.2s; }
  .combo-chevron.flipped { transform: rotate(180deg); }
  .combo-dropdown {
    position: absolute; top: 100%; left: 14px; right: 14px;
    background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm);
    box-shadow: var(--shadow-lg); z-index: 50; max-height: 240px; overflow-y: auto; margin-top: 4px;
  }
  .combo-option {
    display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 12px;
    border: none; background: none; cursor: pointer; text-align: left; font-size: 13px; color: var(--text-2);
  }
  .combo-option:hover { background: var(--bg-subtle); }
  .combo-option.active { background: var(--bg-subtle); color: var(--text-1); font-weight: 500; }
  .co-label { flex: 1; }
  .co-meta { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); }

  .ctx-menu { flex: 1; padding: 10px; }

  .detail-page { flex: 1; overflow-y: auto; background: var(--bg-page); scroll-behavior: smooth; }
</style>
