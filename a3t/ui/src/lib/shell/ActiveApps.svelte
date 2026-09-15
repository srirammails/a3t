<script lang="ts">
  /// The open-app list — taskbar semantics: an app stays here until closed.
  ///
  /// It is also the way to reach an unpinned context list: "Open list pane" reveals
  /// the current app's list without pinning it, which is the path for keeping the
  /// app rail pinned while the conversation list stays out of the way.
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import {
    getOpenApps, closeApp, trackOpen,
    hasContext, contextLabel, isContextPinned, openContextPane,
  } from '$lib/stores/apps.svelte.ts';
  import { appIcon } from './appIcons';
  import { menuGroups as filteredMenuGroups } from '$lib/stores/auth';

  let { activeType = '' }: { activeType?: string } = $props();

  let open = $state(false);
  let apps = $derived(getOpenApps());
  let menuGroups = $derived($filteredMenuGroups);

  /// Icon name for an app type, from the same role-filtered menu config the rail
  /// uses — the open-app list stores only type+title, not presentation.
  function iconFor(type: string): string {
    for (const g of menuGroups) {
      for (const it of g.items) if (it.type === type) return it.icon;
    }
    return 'dashboard';
  }

  function go(type: string, title: string) {
    trackOpen(type, title);
    navigateTo(type, title);
    open = false;
  }

  function close(e: MouseEvent, type: string) {
    // Never let the close button also switch to the app it is removing.
    e.stopPropagation();
    const next = closeApp(type);
    if (type === activeType) {
      if (next) navigateTo(next.type, next.title);
      else navigateTo('chat', 'AI Chat');
    }
  }

  function revealList() {
    openContextPane();
    open = false;
  }

  $effect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!(e.target as HTMLElement)?.closest?.('.aa-wrap')) open = false;
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') open = false; };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="aa-wrap">
  <button
    class="hdr-btn wide"
    class:active={open}
    onclick={() => (open = !open)}
    title="Active apps"
    aria-expanded={open}
  >
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
      <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" /><path d="M1.5 6h13" />
    </svg>
    <span class="hdr-btn-label">Active Apps</span>
    {#if apps.length > 0}<span class="aa-count">{apps.length}</span>{/if}
  </button>

  {#if open}
    <div class="aa-menu" role="menu">
      {#if apps.length === 0}
        <div class="aa-empty">No apps open — use the Applications palette.</div>
      {:else}
        {#each apps as app}
          <div class="aa-row" class:current={app.type === activeType}>
            <button class="aa-go" onclick={() => go(app.type, app.title)}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
                {@html appIcon(iconFor(app.type))}
              </svg>
              <span class="aa-title">{app.title}</span>
            </button>
            <button class="aa-x" onclick={(e) => close(e, app.type)} title="Close {app.title}" aria-label="Close {app.title}">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M12 4L4 12M4 4l8 8" /></svg>
            </button>
          </div>
        {/each}
      {/if}

      {#if hasContext(activeType) && !isContextPinned()}
        <div class="aa-sep"></div>
        <button class="aa-action" onclick={revealList}>
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
            <rect x="1.5" y="2" width="5" height="12" rx="1" /><path d="M8.5 5h6M8.5 8h6M8.5 11h4" />
          </svg>
          Open list pane — {contextLabel(activeType)}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .aa-wrap { position: relative; }

  .hdr-btn {
    height: 30px;
    display: inline-flex; align-items: center; gap: 7px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    color: var(--text-3);
    cursor: pointer;
    font-size: 11.5px;
  }
  .hdr-btn:hover, .hdr-btn.active { color: var(--text-1); border-color: var(--border-strong); }
  .hdr-btn-label { white-space: nowrap; }
  .aa-count {
    min-width: 16px; height: 16px; padding: 0 4px;
    display: grid; place-items: center;
    border-radius: 8px; background: var(--bg-subtle);
    border: 1px solid var(--border);
    font-size: 9.5px; font-variant-numeric: tabular-nums;
  }

  .aa-menu {
    position: absolute; top: calc(100% + 8px); left: 0;
    min-width: 250px; max-height: 60vh; overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.14);
    padding: 5px;
    z-index: 200;
  }
  .aa-empty { padding: 12px 10px; font-size: 11.5px; color: var(--text-4); }

  .aa-row {
    display: flex; align-items: center;
    border-radius: var(--r-sm);
  }
  .aa-row:hover { background: var(--bg-subtle); }
  .aa-row.current { background: var(--bg-subtle); }
  .aa-row.current .aa-title { color: var(--text-1); font-weight: 500; }
  .aa-go {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 9px;
    padding: 7px 8px;
    border: none; background: transparent; color: var(--text-2);
    cursor: pointer; text-align: left; font-size: 12px;
  }
  .aa-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .aa-x {
    width: 22px; height: 22px; margin-right: 4px;
    display: grid; place-items: center;
    border: none; border-radius: 4px;
    background: transparent; color: var(--text-4); cursor: pointer;
    opacity: 0;
  }
  .aa-row:hover .aa-x { opacity: 1; }
  .aa-x:hover { color: var(--text-1); background: var(--bg-surface); }

  .aa-sep { height: 1px; background: var(--border); margin: 5px 2px; }
  .aa-action {
    width: 100%;
    display: flex; align-items: center; gap: 8px;
    padding: 7px 8px;
    border: none; border-radius: var(--r-sm);
    background: transparent; color: var(--text-2);
    cursor: pointer; text-align: left; font-size: 11.5px;
  }
  .aa-action:hover { background: var(--bg-subtle); color: var(--text-1); }
</style>
