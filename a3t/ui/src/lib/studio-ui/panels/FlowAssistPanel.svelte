<script lang="ts">
  /// A flow's Form or AI panel (right region): the same document as the graph /
  /// code view, edited through FlowEditorPane's form or designer chat.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import type { Doc } from '$lib/studio-core';

  let { id, doc }: { id: string; doc: Doc | null } = $props();
  const flow = $derived(doc?.kind === 'flow' && (doc.loaded || doc.isNew) ? doc : null);
</script>

{#if flow}
  {#key `${flow.address}:${id}`}
    <FlowEditorPane part={id === 'ai' ? 'ai' : 'form'} aiKind="flow" aiName={`${flow.ref.project}/${flow.ref.name}`}
      initialYaml={flow.text ?? ''} onchange={(v: string) => (flow.text = v)} />
  {/key}
{:else}
  <div class="note">{doc ? `The ${id === 'ai' ? 'AI assistant' : 'form'} works on flows.` : 'Open a flow to edit it here.'}</div>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
