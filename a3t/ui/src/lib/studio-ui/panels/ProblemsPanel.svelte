<script lang="ts">
  /// What is wrong with a flow before it is deployed (bottom region).
  import { problems } from '$lib/studio-core/flow/model';
  import type { Doc } from '$lib/studio-core';

  let { doc }: { doc: Doc | null } = $props();
  const flow = $derived(doc?.kind === 'flow' ? doc : null);
  const issues = $derived(flow && (flow.loaded || flow.isNew) ? problems(flow.text) : []);
</script>

{#if !flow}
  <div class="note">Open a flow to check it.</div>
{:else if !issues.length}
  <div class="note ok">No problems in {flow.ref.name}.</div>
{:else}
  <ul class="problems">
    {#each issues as p}<li class={p.level}><span>{p.level === 'error' ? '✗' : '!'}</span>{p.message}</li>{/each}
  </ul>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .note.ok { color: var(--ok, #15803d); }
  .problems { list-style: none; margin: 0; padding: 8px 12px; font-size: 12.5px; }
  .problems li { display: flex; gap: 8px; padding: 3px 0; }
  .problems li.error span { color: var(--err, #b42318); }
  .problems li.warning span { color: var(--warn, #b45309); }
</style>
