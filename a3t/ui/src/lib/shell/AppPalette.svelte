<script lang="ts">
  /// The Squares launcher: every app the signed-in user may open, grouped exactly
  /// as the rail groups them. This is the way in when the rail is unpinned, so it
  /// must list the same set the rail would — it reads the same role-filtered
  /// config rather than a second copy of the catalogue.
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import { menuGroups as filteredMenuGroups } from '$lib/stores/auth';
  import { trackOpen, isOpen, isRailPinned, toggleRailPinned } from '$lib/stores/apps.svelte.ts';
  import { APP_ICONS } from './appIcons';

  let { activeType = '' }: { activeType?: string } = $props();

  let open = $state(false);
  let menuGroups = $derived($filteredMenuGroups);

  function launch(type: string, label: string) {
    trackOpen(type, label);
    navigateTo(type, label);
    open = false;
  }

  /// Close on outside click. Registered only while open so the shell is not
  /// listening on every document click for a menu nobody has opened.
  $effect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!(e.target as HTMLElement)?.closest?.('.palette-wrap')) open = false;
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

<div class="palette-wrap">
  <button
    class="hdr-btn"
    class:active={open}
    onclick={() => (open = !open)}
    title="Applications"
    aria-label="Applications"
    aria-expanded={open}
  >
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
      <rect x="1.5" y="1.5" width="4" height="4" rx="1" />
      <rect x="10.5" y="1.5" width="4" height="4" rx="1" />
      <rect x="1.5" y="10.5" width="4" height="4" rx="1" />
      <rect x="10.5" y="10.5" width="4" height="4" rx="1" />
    </svg>
  </button>

  {#if open}
    <div class="palette" role="menu">
      <div class="palette-head">
        <span class="palette-title">Applications</span>
        <button
          class="pin-btn"
          class:pinned={isRailPinned()}
          onclick={() => toggleRailPinned()}
          title={isRailPinned() ? 'Unpin the app menu (frees the column)' : 'Pin the app menu as a side rail'}
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M8 10v4M5 3h6l-1 6H6L5 3z" />
          </svg>
          {isRailPinned() ? 'Pinned' : 'Pin'}
        </button>
      </div>

      <div class="palette-body">
        {#each menuGroups as group}
          <div class="pg">
            <div class="pg-label">{group.label}</div>
            <div class="pg-grid">
              {#each group.items as item}
                <button
                  class="app-tile"
                  class:current={item.type === activeType}
                  onclick={() => launch(item.type, item.label)}
                  title={item.label}
                >
                  <span class="app-ico" class:dot={isOpen(item.type)}>
                    <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
                      {@html APP_ICONS[item.icon] ?? APP_ICONS.dashboard}
                    </svg>
                  </span>
                  <span class="app-name">{item.label}</span>
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .palette-wrap { position: relative; }

  .hdr-btn {
    width: 30px; height: 30px;
    display: grid; place-items: center;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    color: var(--text-3);
    cursor: pointer;
  }
  .hdr-btn:hover, .hdr-btn.active { color: var(--text-1); border-color: var(--border-strong); }

  .palette {
    position: absolute; top: calc(100% + 8px); left: 0;
    width: 420px; max-height: 68vh; overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.14);
    z-index: 200;
  }
  .palette-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    position: sticky; top: 0; background: var(--bg-surface);
  }
  .palette-title {
    font-family: var(--font-serif); font-size: 13px; font-weight: 500;
  }
  .pin-btn {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: 10.5px; padding: 4px 8px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-3); cursor: pointer;
  }
  .pin-btn:hover { color: var(--text-1); border-color: var(--border-strong); }
  .pin-btn.pinned { color: var(--text-1); border-color: var(--border-strong); background: var(--bg-subtle); }

  .palette-body { padding: 6px 10px 12px; }
  .pg + .pg { margin-top: 4px; }
  .pg-label {
    font-size: 9.5px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.06em; color: var(--text-4);
    padding: 10px 2px 6px;
  }
  .pg-grid {
    display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px;
  }
  .app-tile {
    display: flex; flex-direction: column; align-items: center; gap: 6px;
    padding: 10px 4px;
    border: 1px solid transparent; border-radius: var(--r-sm);
    background: transparent; color: var(--text-2);
    cursor: pointer; text-align: center;
  }
  .app-tile:hover { background: var(--bg-subtle); border-color: var(--border); color: var(--text-1); }
  .app-tile.current { border-color: var(--border-strong); background: var(--bg-subtle); color: var(--text-1); }
  .app-ico { position: relative; display: grid; place-items: center; }
  /* A dot marks an app that is already open, so the palette doubles as a hint
     about what is in the Active Apps list. */
  .app-ico.dot::after {
    content: ''; position: absolute; right: -5px; top: -3px;
    width: 5px; height: 5px; border-radius: 50%;
    background: var(--ephemeral);
  }
  .app-name {
    font-size: 10.5px; line-height: 1.25;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%;
  }
</style>
