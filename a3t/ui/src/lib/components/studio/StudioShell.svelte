<script lang="ts">
  /// A reusable IDE-style shell for A3T apps (Flow Studio, DB Console, Logs,
  /// Runtime…): an optional left and right region, the editor area in the
  /// middle, and an optional bottom region under it. Regions are tabbed panel
  /// groups that resize by dragging, minimize to a stack on their edge, and
  /// maximize to fill the studio — so does the editor area. Esc restores.
  ///
  /// The app supplies the panels (and what they render) and the editor area;
  /// the layout is kept per app (`new Studio('<app>')`).
  import type { Snippet } from 'svelte';
  import Region from './Region.svelte';
  import type { Panel, RegionId, Studio } from './layout.svelte.ts';

  type Side = { panels: Panel[]; panel: Snippet<[string]>; badge?: (id: string) => string | number | null };

  let { studio, left, right, bottom, center }: {
    studio: Studio;
    left?: Side;
    right?: Side;
    bottom?: Side;
    center: Snippet;
  } = $props();

  const s = $derived(studio.state);
  const editorMax = $derived(s.maximized === 'editor');

  function drag(region: RegionId, e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const start = { x: e.clientX, y: e.clientY, size: studio.state[region].size };
    const move = (ev: PointerEvent) => {
      const d = region === 'bottom' ? start.y - ev.clientY : region === 'left' ? ev.clientX - start.x : start.x - ev.clientX;
      studio.resize(region, start.size + d);
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      studio.save();
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape' && studio.state.maximized) studio.state.maximized = null;
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="studio">
  {#if left}
    <div class="side" style={s.left.minimized ? '' : `width:${s.left.size}px`}>
      <Region {studio} region="left" panels={left.panels} panel={left.panel} badge={left.badge} />
    </div>
    {#if !s.left.minimized}<div class="split v" role="separator" aria-orientation="vertical" onpointerdown={(e) => drag('left', e)}></div>{/if}
  {/if}

  <div class="middle">
    <div class="center" class:max={editorMax}>
      {@render center()}
    </div>
    {#if bottom}
      {#if !s.bottom.minimized}<div class="split h" role="separator" aria-orientation="horizontal" onpointerdown={(e) => drag('bottom', e)}></div>{/if}
      <div class="bottom" style={s.bottom.minimized ? '' : `height:${s.bottom.size}px`}>
        <Region {studio} region="bottom" panels={bottom.panels} panel={bottom.panel} badge={bottom.badge} />
      </div>
    {/if}
  </div>

  {#if right}
    {#if !s.right.minimized}<div class="split v" role="separator" aria-orientation="vertical" onpointerdown={(e) => drag('right', e)}></div>{/if}
    <div class="side" style={s.right.minimized ? '' : `width:${s.right.size}px`}>
      <Region {studio} region="right" panels={right.panels} panel={right.panel} badge={right.badge} />
    </div>
  {/if}
</div>

<style>
  .studio { position: relative; display: flex; height: 100%; min-height: 0; overflow: hidden; background: var(--bg-page); }
  .side { display: flex; flex-shrink: 0; min-height: 0; }
  .middle { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .center { flex: 1; min-height: 0; display: flex; flex-direction: column; background: var(--bg-surface); }
  .center.max { position: absolute; inset: 0; z-index: 20; }
  .bottom { flex-shrink: 0; display: flex; min-height: 0; }
  .split { flex-shrink: 0; background: var(--border); }
  .split.v { width: 1px; cursor: col-resize; position: relative; }
  .split.h { height: 1px; cursor: row-resize; position: relative; }
  .split.v::after { content: ''; position: absolute; inset: 0 -3px; }
  .split.h::after { content: ''; position: absolute; inset: -3px 0; }
  .split:hover, .split:active { background: var(--spec); }
</style>
