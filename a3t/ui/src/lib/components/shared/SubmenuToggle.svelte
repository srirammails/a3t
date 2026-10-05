<script lang="ts">
  /// The ≡ in a page heading: opens the page's submenu (its context list).
  ///
  /// It only ever OPENS. If the submenu is already there — pinned, already open,
  /// or docked beside an open item — clicking does nothing. A floating submenu
  /// is hidden by clicking the page; a pinned one stays until it is unpinned
  /// from its own header. Renders nothing for pages that have no submenu.
  import { hasContext, contextLabel, contextVisible, openContextPane } from '$lib/stores/apps.svelte.ts';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';

  let { appType, floating = false }: { appType: string; floating?: boolean } = $props();

  const shown = $derived(contextVisible(appType, !!getActivePanel()?.props?.selectedId));

  function open() {
    if (!shown) openContextPane();
  }
</script>

{#if hasContext(appType)}
  <button class="smt" class:on={shown} class:floating onclick={open}
    title={shown ? `${contextLabel(appType)} is open` : `Show ${contextLabel(appType).toLowerCase()}`}
    aria-label="Open submenu" aria-expanded={shown}>
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
      <path d="M2.5 4h11M2.5 8h11M2.5 12h11" />
    </svg>
  </button>
{/if}

<style>
  .smt {
    flex: none; width: 30px; height: 30px; display: grid; place-items: center;
    border: 1px solid transparent; border-radius: var(--r-sm);
    background: transparent; color: var(--text-3); cursor: pointer;
    transition: background 0.1s ease, color 0.1s ease;
  }
  .smt:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border); }
  .smt.on { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border); }
  .smt.floating { position: absolute; top: 12px; left: 12px; z-index: 5; background: var(--bg-surface); border-color: var(--border); box-shadow: var(--shadow-sm); }
</style>
