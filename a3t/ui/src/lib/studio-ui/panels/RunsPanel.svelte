<script lang="ts">
  /// A flow's runs (bottom region). The run dialog's state lives on the
  /// document (`doc.ui.runDialog` / `runDebug`), so the view's Run button and
  /// this panel share it.
  import FlowRunPanel from '$lib/components/run/FlowRunPanel.svelte';
  import { parse, stepKeys } from '$lib/studio-core/flow/model';
  import type { Doc } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  const flow = $derived(doc?.kind === 'flow' ? doc : null);
  const parsed = $derived(flow ? parse(flow.text).doc : null);
</script>

{#if flow && !flow.isNew}
  <FlowRunPanel ns={flow.ref.project} name={flow.ref.name} schema={parsed?.input?.schema ?? null} steps={stepKeys(parsed?.do)}
    bind:dialogOpen={flow.ui.runDialog} bind:debugMode={flow.ui.runDebug} />
{:else}
  <div class="note">{flow ? 'Deploy this flow to run it.' : 'Open a flow to see its runs.'}</div>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
