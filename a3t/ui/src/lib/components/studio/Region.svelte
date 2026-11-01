<script lang="ts">
  /// A studio region: tabs of panels with minimize (to a stack on its edge) and
  /// maximize (fill the studio). Double-click a tab to maximize it.
  import type { Snippet } from 'svelte';
  import type { Panel, RegionId, Studio } from './layout.svelte.ts';

  let { studio, region, panels, panel, badge }: {
    studio: Studio;
    region: RegionId;
    panels: Panel[];
    panel: Snippet<[string]>;
    badge?: (id: string) => string | number | null;
  } = $props();

  const rs = $derived(studio.state[region]);
  const active = $derived(panels.some((p) => p.id === rs.active) ? rs.active : panels[0]?.id ?? '');
  const maximized = $derived(studio.state.maximized !== null && panels.some((p) => p.id === studio.state.maximized));
</script>

{#if rs.minimized && !maximized}
  <div class="stack {region}" role="tablist" aria-label="Minimized panels">
    {#each panels as p (p.id)}
      <button class="stack-tab" onclick={() => studio.reveal(region, p.id)} title="Show {p.title}">
        {p.title}{#if badge?.(p.id)}<span class="b">{badge(p.id)}</span>{/if}
      </button>
    {/each}
  </div>
{:else}
  <section class="region {region}" class:max={maximized}>
    <header class="bar" role="tablist">
      {#each panels as p (p.id)}
        <button class="tab" class:on={active === p.id} role="tab" aria-selected={active === p.id}
          onclick={() => studio.reveal(region, p.id)} ondblclick={() => studio.toggleMaximize(p.id)} title="Double-click to maximize">
          {p.title}{#if badge?.(p.id)}<span class="b">{badge(p.id)}</span>{/if}
        </button>
      {/each}
      <span class="grow"></span>
      <button class="ctl" onclick={() => studio.toggleMaximize(active)} title={maximized ? 'Restore (Esc)' : 'Maximize'}>{maximized ? '⤡' : '⤢'}</button>
      <button class="ctl" onclick={() => studio.minimize(region)} title="Minimize to the edge">{region === 'left' ? '⇤' : region === 'right' ? '⇥' : '⤓'}</button>
    </header>
    <div class="body">
      {@render panel(active)}
    </div>
  </section>
{/if}

<style>
  .region { display: flex; flex-direction: column; min-width: 0; min-height: 0; height: 100%; width: 100%; background: var(--bg-surface); }
  .region.max { position: absolute; inset: 0; z-index: 20; }
  .bar { display: flex; align-items: center; gap: 2px; height: 32px; flex-shrink: 0; padding: 0 4px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .tab { border: 0; background: transparent; padding: 0 10px; height: 100%; font-size: 11.5px; letter-spacing: 0.03em; text-transform: uppercase; color: var(--text-3); cursor: pointer; border-bottom: 2px solid transparent; white-space: nowrap; }
  .tab.on { color: var(--text-1); border-bottom-color: var(--spec); }
  .grow { flex: 1; }
  .ctl { border: 0; background: transparent; width: 26px; height: 26px; border-radius: var(--r-sm); color: var(--text-3); cursor: pointer; font-size: 13px; flex-shrink: 0; }
  .ctl:hover { background: var(--bg-subtle); color: var(--text-1); }
  .b { margin-left: 5px; font-size: 10px; padding: 0 5px; border-radius: 8px; background: var(--bg-subtle); color: var(--text-2); text-transform: none; }
  .body { flex: 1; min-height: 0; overflow: auto; }
  .stack { display: flex; gap: 2px; background: var(--bg-page); flex-shrink: 0; }
  .stack.left, .stack.right { flex-direction: column; width: 28px; height: 100%; padding-top: 6px; }
  .stack.left { border-right: 1px solid var(--border); }
  .stack.right { border-left: 1px solid var(--border); }
  .stack.left .stack-tab, .stack.right .stack-tab { writing-mode: vertical-rl; padding: 10px 0; }
  .stack.left .stack-tab { transform: rotate(180deg); }
  .stack.bottom { flex-direction: row; height: 28px; border-top: 1px solid var(--border); padding-left: 6px; align-items: center; }
  .stack.bottom .stack-tab { padding: 0 10px; height: 100%; }
  .stack-tab { border: 0; background: transparent; font-size: 11px; letter-spacing: 0.03em; text-transform: uppercase; color: var(--text-3); cursor: pointer; }
  .stack-tab:hover { color: var(--text-1); background: var(--bg-subtle); }
</style>
