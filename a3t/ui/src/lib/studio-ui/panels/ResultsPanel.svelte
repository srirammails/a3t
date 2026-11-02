<script lang="ts">
  /// The active database document's last answer (bottom region).
  import ResultTable from '../parts/ResultTable.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();
  const db = $derived(doc && workspace.kindOf(doc).query ? doc : null);
</script>

{#if !db}
  <div class="note">Open a database to run queries.</div>
{:else if db.ui.running}
  <div class="note">Running…</div>
{:else if db.ui.error}
  <div class="err">{db.ui.error}</div>
{:else if db.ui.result}
  <ResultTable result={db.ui.result} />
{:else}
  <div class="note">Run a query (Ctrl/⌘+Enter) to see its answer here.</div>
{/if}

<style>
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 10px 12px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; font-family: var(--font-mono); white-space: pre-wrap; }
</style>
