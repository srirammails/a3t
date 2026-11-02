<script lang="ts">
  /// The step palette (left region, flows): workflow tasks to drag onto the flow
  /// — or click, to add at its end. A new step opens in the Form panel. Explorer
  /// items (resources, flows, agents, triggers) drop onto the flow too.
  import { PALETTE, STEP_MIME, type Doc } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  let filter = $state('');
  const groups = $derived(PALETTE.map((g) => ({ ...g, steps: g.steps.filter((s) => !filter.trim() || s.label.toLowerCase().includes(filter.trim().toLowerCase())) })).filter((g) => g.steps.length));
  const flow = $derived(doc?.kind === 'flow' ? doc : null);

  function drag(e: DragEvent, id: string) {
    e.dataTransfer?.setData(STEP_MIME, id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
  }
  /// A click adds it too: the flow's view listens for the request.
  function add(id: string) { if (flow) flow.ui.addStep = { id, n: Date.now() }; }
</script>

<div class="sp">
  <input class="filter" placeholder="Filter steps" bind:value={filter} />
  {#if !flow}<p class="hint">Open a flow, then drag steps onto it.</p>{:else}<p class="hint">Drag onto the flow, or click to add at the end. Explorer items drop on it too.</p>{/if}
  {#each groups as g (g.title)}
    <h4>{g.title}</h4>
    <div class="grid">
      {#each g.steps as s (s.id)}
        <button class="step" draggable="true" ondragstart={(e) => drag(e, s.id)} onclick={() => add(s.id)} disabled={!flow} title="Drag onto the flow, or click to add {s.label}">
          <span class="ic">{s.icon}</span><span class="lb">{s.label}</span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .sp { height: 100%; overflow-y: auto; padding: 8px 8px 20px; display: flex; flex-direction: column; gap: 6px; }
  .filter { padding: 4px 8px; font-size: 12px; }
  .hint { margin: 0; font-size: 11px; color: var(--text-4); }
  h4 { margin: 6px 2px 0; font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-3); font-weight: 600; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 4px; }
  .step { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); cursor: grab; font-size: 12px; color: var(--text-1); text-align: left; }
  .step:hover:not(:disabled) { border-color: var(--spec); background: var(--spec-bg); }
  .step:disabled { opacity: 0.5; cursor: default; }
  .ic { width: 16px; text-align: center; color: var(--spec); font-family: var(--font-mono); font-size: 11px; }
  .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
