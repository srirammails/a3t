<script lang="ts">
  /// One run in its own tab: steps, timeline, journal, checkpoints, and re-run /
  /// re-run in debug (the run view the Runs panels use). A re-run opens as a
  /// new tab.
  import RunView from '$lib/components/run/RunView.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();
  const flow = $derived((doc.data?.flow ?? '') as string);

  function openFlow() {
    const [project, ...name] = flow.split('/');
    workspace.open({ project, kind: 'flow', name: name.join('/') });
  }
</script>

<div class="rd">
  <div class="head">
    <span class="kind">Run</span>
    {#if flow}<button class="chip" onclick={openFlow} title="Open the flow">ƒ {flow}</button>{/if}
    <span class="mono dim" title="Run id">{doc.ref.name}</span>
  </div>
  <div class="body">
    {#key doc.ref.name}
      <RunView runId={doc.ref.name} onrerun={(id) => workspace.open({ project: doc.ref.project, kind: 'run', name: id, data: { flow } })} />
    {/key}
  </div>
</div>

<style>
  .rd { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .chip { font-family: var(--font-mono); font-size: 12px; padding: 2px 8px; border-radius: 4px; border: 0; background: var(--spec-bg); color: var(--spec-ink, var(--spec)); cursor: pointer; }
  .mono { font-family: var(--font-mono); font-size: 11.5px; }
  .dim { color: var(--text-4); }
  .body { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 14px; }
</style>
