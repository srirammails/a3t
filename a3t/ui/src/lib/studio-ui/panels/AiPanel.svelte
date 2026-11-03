<script lang="ts">
  /// The AI assistant (right region) for any document: it reads the document's
  /// text (a flow's YAML, an item's config) and proposes a new one; applying it
  /// edits the document, unsaved, like typing would.
  import DesignerChat from '$lib/components/editor/DesignerChat.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();
  const kind = $derived(doc ? workspace.kindOf(doc) : null);
  /// What the assistant is told it edits.
  const AI_KIND: Record<string, string> = { flow: 'flow', template: 'template', agent: 'agent', canvas: 'canvas' };
</script>

{#if !doc || !kind}
  <div class="note">Open an item to ask the AI about it.</div>
{:else if !doc.loaded && !doc.isNew}
  <div class="note">Loading…</div>
{:else}
  {#key doc.address}
    <DesignerChat kind={AI_KIND[kind.id] ?? 'resource'} name={`${doc.ref.project}/${doc.ref.name}`} readonly={!kind.push}
      gettext={() => doc.text ?? ''} onapply={(t: string) => (doc.text = t)} />
  {/key}
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
</style>
