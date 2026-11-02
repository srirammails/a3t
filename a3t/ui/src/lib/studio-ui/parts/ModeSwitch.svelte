<script lang="ts">
  /// The views a document switches between (its kind's `views.modes`), shown in
  /// the view's own header; nothing when the kind has one view.
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();
  const modes = $derived(workspace.kindOf(doc).views.modes ?? []);
  const current = $derived(doc.ui.mode ?? modes[0]?.id);
</script>

{#if modes.length > 1}
  <div class="modes" role="tablist" aria-label="View">
    {#each modes as m (m.id)}
      <button role="tab" aria-selected={current === m.id} class:on={current === m.id} onclick={() => (doc.ui.mode = m.id)}>{m.title}</button>
    {/each}
  </div>
{/if}

<style>
  .modes { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .modes button { border: 0; background: transparent; padding: 3px 10px; border-radius: 3px; font-size: 12px; color: var(--text-3); cursor: pointer; }
  .modes button.on { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
</style>
