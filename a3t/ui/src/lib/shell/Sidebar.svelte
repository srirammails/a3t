<script lang="ts">
  import { navigateTo, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { menuGroups as filteredMenuGroups } from '$lib/stores/auth';
  import { appIcon } from './appIcons';

  let menuGroups = $derived($filteredMenuGroups);
  let activeType = $derived(getActivePanel()?.type ?? 'dashboard');

  // Tooltip state
  let tooltipText = $state('');
  let tooltipY = $state(0);
  let tooltipX = $state(0);
  let showTooltip = $state(false);
  let tooltipTimeout: ReturnType<typeof setTimeout> | null = null;

  function handleClick(type: string, label: string) {
    activeType = type;
    navigateTo(type, label);
  }

  function onEnter(e: MouseEvent, label: string) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    tooltipText = label;
    tooltipX = rect.right + 10;
    tooltipY = rect.top + rect.height / 2;
    tooltipTimeout = setTimeout(() => { showTooltip = true; }, 200);
  }

  function onLeave() {
    if (tooltipTimeout) clearTimeout(tooltipTimeout);
    showTooltip = false;
  }


  function getIcon(name: string): string {
    return appIcon(name);
  }
</script>

<nav class="sidebar">
  <div class="sidebar-top">
    {#each menuGroups as group}
      <div class="nav-group">
        <div class="group-divider"></div>
        {#each group.items as item}
          <button
            class="nav-btn"
            class:active={activeType === item.type}
            onclick={() => handleClick(item.type, item.label)}
            onmouseenter={(e) => onEnter(e, item.label)}
            onmouseleave={onLeave}
          >
            <svg class="nav-ico" width="20" height="20" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
              {@html getIcon(item.icon)}
            </svg>
          </button>
        {/each}
      </div>
    {/each}
  </div>

  <div class="sidebar-bottom">
    <div class="version-pill">v0.5</div>
  </div>
</nav>

<!-- Tooltip rendered at document level via fixed positioning — never clipped -->
{#if showTooltip}
  <div class="sidebar-tooltip" style="top: {tooltipY}px; left: {tooltipX}px;">
    {tooltipText}
  </div>
{/if}

<style>
  .sidebar {
    grid-area: sidebar;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    width: 100%;
  }

  .sidebar-top {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 8px 0;
    gap: 2px;
  }

  .nav-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .group-divider {
    height: 1px;
    background: var(--divider);
    margin: 6px 14px;
  }
  .nav-group:first-child .group-divider { display: none; }

  .nav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px 0;
    margin: 0 8px;
    border-radius: var(--r-sm);
    border: none;
    background: none;
    cursor: pointer;
    color: var(--text-3);
    position: relative;
    transition: all 0.12s;
  }
  .nav-btn:hover {
    background: var(--bg-subtle);
    color: var(--text-1);
  }
  .nav-btn.active {
    background: var(--bg-subtle);
    color: var(--text-1);
  }
  .nav-btn.active::before {
    content: '';
    position: absolute;
    left: -8px;
    top: 6px;
    bottom: 6px;
    width: 3px;
    background: var(--ok);
    border-radius: 0 2px 2px 0;
  }

  .nav-ico { flex-shrink: 0; }
  .nav-btn.active .nav-ico { color: var(--ephemeral); }

  .sidebar-bottom {
    padding: 8px 0 12px;
    display: flex;
    justify-content: center;
    border-top: 1px solid var(--divider);
  }
  .version-pill {
    font-family: var(--font-mono);
    font-size: 8px;
    color: var(--text-5);
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--bg-subtle);
  }

  /* Fixed-position tooltip — renders above everything */
  .sidebar-tooltip {
    position: fixed;
    transform: translateY(-50%);
    padding: 5px 12px;
    background: var(--text-1, #0f0f0f);
    color: var(--bg-page, #faf9f6);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    border-radius: 5px;
    pointer-events: none;
    z-index: 9999;
    box-shadow: 0 2px 8px rgba(0,0,0,0.15);
  }
  .sidebar-tooltip::before {
    content: '';
    position: absolute;
    right: 100%;
    top: 50%;
    transform: translateY(-50%);
    border: 5px solid transparent;
    border-right-color: var(--text-1, #0f0f0f);
  }
</style>
