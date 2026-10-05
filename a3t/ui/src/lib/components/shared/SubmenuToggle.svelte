<script lang="ts">
  /// The ≡ in a page heading: opens the page's submenu (its context list).
  ///
  /// Click reveals the list over the page; click again hides it. The pin in the
  /// list's own header docks it so it stays — and while it is pinned, this
  /// button unpins it. Renders nothing for pages that have no submenu.
  import {
    hasContext, contextLabel, isContextPinned, isContextOpen,
    openContextPane, closeContextPane, toggleContextPinned,
  } from '$lib/stores/apps.svelte.ts';

  let { appType, floating = false }: { appType: string; floating?: boolean } = $props();

  const shown = $derived(isContextPinned() || isContextOpen());

  function toggle() {
    if (isContextPinned()) { toggleContextPinned(); closeContextPane(); return; }
    if (isContextOpen()) closeContextPane(); else openContextPane();
  }
</script>

{#if hasContext(appType)}
  <button class="smt" class:on={shown} class:floating onclick={toggle}
    title={shown ? `Hide ${contextLabel(appType).toLowerCase()}` : `Show ${contextLabel(appType).toLowerCase()}`}
    aria-label="Toggle submenu" aria-expanded={shown}>
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
