<script lang="ts">
  /// Editor-area tabs for a studio: open documents with close buttons; double-
  /// click (or ⤢) maximizes the editor area. `actions` adds buttons on the right.
  import type { Snippet } from 'svelte';
  import type { Studio, Tab } from './layout.svelte.ts';

  let { studio, tabs, active, onselect, onclose, actions }: {
    studio: Studio;
    tabs: Tab[];
    active: string | null;
    onselect: (id: string) => void;
    onclose: (id: string) => void;
    actions?: Snippet;
  } = $props();

  const max = $derived(studio.state.maximized === 'editor');
</script>

<div class="tabs" role="tablist">
  <div class="scroll">
    {#each tabs as t (t.id)}
      <div class="tab" class:on={t.id === active} role="tab" tabindex="0" aria-selected={t.id === active} title={t.hint ?? t.title}
        onclick={() => onselect(t.id)} ondblclick={() => studio.toggleMaximize('editor')}
        onauxclick={(e) => { if (e.button === 1) onclose(t.id); }}
        onkeydown={(e) => { if (e.key === 'Enter') onselect(t.id); }}>
        {#if t.icon}<span class="ic">{t.icon}</span>{/if}
        <span class="nm">{t.title}</span>
        <button class="x" class:dirty={t.dirty} aria-label="Close {t.title}" onclick={(e) => { e.stopPropagation(); onclose(t.id); }}>
          <span class="dot">●</span><span class="cross">×</span>
        </button>
      </div>
    {/each}
  </div>
  {#if actions}{@render actions()}{/if}
  <button class="ctl" onclick={() => studio.toggleMaximize('editor')} title={max ? 'Restore (Esc)' : 'Maximize the editor'}>{max ? '⤡' : '⤢'}</button>
</div>

<style>
  .tabs { display: flex; align-items: stretch; height: 34px; flex-shrink: 0; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .scroll { flex: 1; display: flex; overflow-x: auto; min-width: 0; }
  .tab { display: flex; align-items: center; gap: 6px; padding: 0 6px 0 12px; border-right: 1px solid var(--border); font-size: 12.5px; color: var(--text-3); cursor: pointer; white-space: nowrap; max-width: 220px; }
  .tab.on { background: var(--bg-surface); color: var(--text-1); box-shadow: inset 0 2px 0 var(--spec); }
  .nm { overflow: hidden; text-overflow: ellipsis; }
  .ic { font-size: 11px; color: var(--spec); }
  .x { border: 0; background: transparent; width: 20px; height: 20px; border-radius: var(--r-sm); cursor: pointer; color: var(--text-4); display: grid; place-items: center; }
  .x:hover { background: var(--bg-subtle); color: var(--text-1); }
  .x .dot { display: none; font-size: 9px; }
  .x.dirty .dot { display: block; color: var(--text-2); }
  .x.dirty .cross { display: none; }
  .x.dirty:hover .dot { display: none; }
  .x.dirty:hover .cross { display: block; }
  .ctl { border: 0; background: transparent; width: 30px; color: var(--text-3); cursor: pointer; }
  .ctl:hover { background: var(--bg-subtle); color: var(--text-1); }
</style>
