<script lang="ts">
  /// The runs of the flow a trigger starts (bottom region) — the same Runs panel
  /// as a flow's, for its first workflow target.
  import FlowRunPanel from '$lib/components/run/FlowRunPanel.svelte';
  import type { Doc } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  const target = $derived(((doc?.data?.targets ?? []) as any[]).find((x) => x.type === 'workflow' && x.name) ?? null);
  const ns = $derived(target?.namespace ?? doc?.data?.namespace ?? doc?.ref.project ?? '');
</script>

{#if !doc}
  <div class="note">Open a trigger to see the runs it starts.</div>
{:else if !target}
  <div class="note">{doc.data ? 'This trigger starts no flow.' : 'Loading…'}</div>
{:else}
  {#key `${ns}/${target.name}`}
    <FlowRunPanel {ns} name={target.name} schema={null} steps={[]} />
  {/key}
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
