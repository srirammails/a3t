<script lang="ts">
  /// The Form panel (right region) of a workflow document — a flow, or an agent
  /// template, whose form is the agent template editor: the same text as the
  /// graph / code view, edited through FlowEditorPane's form.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import type { Doc } from '$lib/studio-core';

  let { id, doc }: { id: string; doc: Doc | null } = $props();
  /// Only kinds whose documents are workflow YAML list this panel.
  const flow = $derived(doc && (doc.loaded || doc.isNew) && !doc.error ? doc : null);
</script>

{#if flow}
  {#key `${flow.address}:${id}`}
    <FlowEditorPane part={id === 'ai' ? 'ai' : 'form'} aiKind="flow" aiName={`${flow.ref.project}/${flow.ref.name}`}
      initialYaml={flow.text ?? ''} onchange={(v: string) => (flow.text = v)} focusStep={flow.ui.focusStep ?? null} />
  {/key}
{:else}
  <div class="note">{doc ? `The ${id === 'ai' ? 'AI assistant' : 'form'} works on flows.` : 'Open a flow to edit it here.'}</div>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
