<script lang="ts">
  /// Switch a document between its own view and its code (config as JSON,
  /// edited beside the Form panel). Shown in view headers.
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();
  const kind = $derived(workspace.kindOf(doc));
  const asCode = $derived(doc.ui.view === 'code');
  /// The name of the kind's own view, for the way back.
  const ACTION: Record<string, string> = { query: 'Query', chat: 'Chat', tools: 'Tools', trigger: 'Designer', canvas: 'Canvas', runtime: 'Runtime', template: 'Spec' };
</script>

{#if kind.views.code && kind.views.code !== kind.views.center}
  {#if asCode}
    <button class="btn ghost sm" onclick={() => workspace.view(doc, 'action')} title="Back to its own view">← {ACTION[kind.views.center] ?? kind.label}</button>
  {:else}
    <button class="btn ghost sm" onclick={() => workspace.view(doc, 'code')} title="Edit it as code (double-click in the Explorer does the same)">&lt;/&gt; Edit</button>
  {/if}
{/if}
