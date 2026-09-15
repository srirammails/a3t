<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, active = false, compressed = false, layerColor, onClose, children }: {
    title: string;
    active?: boolean;
    compressed?: boolean;
    layerColor?: string;
    onClose?: () => void;
    children: Snippet;
  } = $props();
</script>

<div class="panel" class:active class:compressed>
  {#if compressed}
    <button class="compressed-strip" onclick={onClose} title="Click to expand">
      <span class="compressed-title">{title}</span>
    </button>
  {:else}
    <div class="panel-header">
      {#if layerColor}
        <span class="panel-layer-dot" style="background: {layerColor}"></span>
      {/if}
      <h2 class="panel-title">{title}</h2>
      {#if onClose}
        <button class="close-btn" onclick={onClose} title="Close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      {/if}
    </div>
    <div class="panel-content">
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .panel { display: flex; flex-direction: column; background: var(--bg-page); border-right: 1px solid var(--border-light); height: 100%; transition: width 0.2s ease; overflow: hidden; }
  .panel.active { flex: 1; min-width: 400px; }
  .panel.compressed { width: var(--panel-compressed); min-width: var(--panel-compressed); max-width: var(--panel-compressed); }

  .compressed-strip { display: flex; align-items: center; justify-content: center; width: 100%; height: 100%; border: none; background: #f8f7f5; cursor: pointer; writing-mode: vertical-rl; text-orientation: mixed; padding: 12px 0; }
  .compressed-strip:hover { background: #f0eeeb; }
  .compressed-title { font-size: 12px; color: var(--text-secondary); font-weight: 500; letter-spacing: 0.5px; }

  .panel-header { display: flex; align-items: center; gap: 8px; padding: 14px 24px; border-bottom: 1px solid var(--border-light); background: white; flex-shrink: 0; }
  .panel-layer-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .panel-title { flex: 1; font-size: 15px; font-weight: 600; font-family: var(--font-sans); }
  .close-btn { width: 28px; height: 28px; border: none; background: none; color: var(--text-muted); border-radius: var(--radius-sm); display: flex; align-items: center; justify-content: center; }
  .close-btn:hover { background: #f0f0f0; color: var(--text); }

  .panel-content { flex: 1; overflow-y: auto; padding: 24px; }
</style>
