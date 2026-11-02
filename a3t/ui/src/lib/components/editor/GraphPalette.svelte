<script lang="ts">
  /// The designer's own palette: a rail on the graph's left edge with the CNCF
  /// DSL's building blocks — control (switch, fork, for, try…), calls, run,
  /// events, state — and a Library of the flow's own functions (`use.functions`).
  /// Open a group, then drag a block onto the flow, or click it to add it at the
  /// end; the new step opens in the form.
  import { PALETTE, STEP_MIME, libraryOf, type PaletteGroup } from '$lib/studio-core/flow/compose';

  let { yaml, onadd }: { yaml: string; onadd: (id: string) => void } = $props();

  const ICONS: Record<string, string> = { Control: '⑂', Calls: '✦', Run: 'ƒ', Events: '↯', State: '⇅', Library: '▤' };
  const groups = $derived<PaletteGroup[]>([...PALETTE, { title: 'Library', steps: libraryOf(yaml) }]);
  let open = $state<string | null>(null);
  const current = $derived(groups.find((g) => g.title === open) ?? null);

  function drag(e: DragEvent, id: string) {
    e.dataTransfer?.setData(STEP_MIME, id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
    // Out of the way of the drop.
    setTimeout(() => (open = null), 0);
  }
</script>

<div class="gp" role="toolbar" aria-label="Steps">
  {#each groups as g (g.title)}
    <button class="gi" class:on={open === g.title} onclick={() => (open = open === g.title ? null : g.title)} title={g.title}>
      <span>{ICONS[g.title] ?? '•'}</span><small>{g.title}</small>
    </button>
  {/each}
</div>
{#if current}
  <div class="fly" role="menu" aria-label={current.title}>
    <div class="fh">{current.title}<button class="x" onclick={() => (open = null)} aria-label="Close">×</button></div>
    {#if !current.steps.length}
      <p class="empty">{current.title === 'Library' ? 'The flow declares no functions. Add them under use → functions; they show here to drop as steps.' : 'Nothing here.'}</p>
    {/if}
    {#each current.steps as s (s.id)}
      <button class="st" role="menuitem" draggable="true" ondragstart={(e) => drag(e, s.id)} onclick={() => { onadd(s.id); open = null; }} title="Drag onto the flow, or click to add {s.label} at the end">
        <span class="si">{s.icon}</span>{s.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .gp { position: absolute; left: 8px; top: 50%; transform: translateY(-50%); z-index: 6; display: flex; flex-direction: column; gap: 2px; padding: 4px;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: 8px; box-shadow: var(--shadow-sm); }
  .gi { display: flex; flex-direction: column; align-items: center; gap: 1px; width: 46px; padding: 5px 2px; border: 0; border-radius: 6px; background: none; color: var(--text-2); cursor: pointer; }
  .gi span { font-size: 15px; line-height: 1; color: var(--spec, #5530a8); }
  .gi small { font-size: 9px; color: var(--text-3); }
  .gi:hover { background: var(--bg-subtle); }
  .gi.on { background: var(--spec-bg, #f3effc); }
  .fly { position: absolute; left: 66px; top: 50%; transform: translateY(-50%); z-index: 7; width: 190px; max-height: 80%; overflow: auto; padding: 4px;
         background: var(--bg-surface); border: 1px solid var(--border); border-radius: 8px; box-shadow: var(--shadow-lg); display: flex; flex-direction: column; }
  .fh { display: flex; align-items: center; justify-content: space-between; padding: 4px 6px 6px; font-size: 10.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-3); }
  .x { border: 0; background: none; color: var(--text-4); cursor: pointer; font-size: 14px; line-height: 1; }
  .st { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border: 1px solid transparent; border-radius: 6px; background: none; font-size: 12px; color: var(--text-1); cursor: grab; text-align: left; }
  .st:hover { border-color: var(--spec); background: var(--spec-bg); }
  .si { width: 18px; text-align: center; color: var(--spec, #5530a8); font-family: var(--font-mono); }
  .empty { margin: 4px 6px 8px; font-size: 11.5px; color: var(--text-4); line-height: 1.4; }
</style>
