<script lang="ts">
  /// Queries kept by name (bottom region); this database's, or all.
  import type { Doc, Workspace } from '$lib/studio-core';
  import { loadQueryLists, queryLists, removeSaved, reuse } from '../queryLists.svelte.ts';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();
  let all = $state(false);
  $effect(() => loadQueryLists(workspace.host));
  const here = $derived(doc && workspace.kindOf(doc).query ? `${doc.ref.project}/${doc.ref.name}` : null);
  const list = $derived(queryLists.saved.filter((s) => all || !here || s.resource === here));
</script>

<div class="ql">
  {#if here}
    <label class="scope"><input type="checkbox" bind:checked={all} /> All databases</label>
  {/if}
  {#if !list.length}<div class="note">No saved queries{here && !all ? ' for this database' : ''}. Use “Save query” in the Query view.</div>{/if}
  <ul>
    {#each list as s (`${s.resource}/${s.name}/${s.query}`)}
      <li>
        <button class="main" onclick={() => reuse(workspace, s)} title={s.query}>
          <span class="nm">{s.name}</span>
          <span class="q">{s.query}</span>
          {#if all || !here}<span class="res">{s.resource}</span>{/if}
        </button>
        <button class="x" title="Forget {s.name}" onclick={() => removeSaved(workspace.host, s)}>×</button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .ql { height: 100%; overflow: auto; font-size: 12.5px; }
  .scope { display: flex; align-items: center; gap: 6px; padding: 6px 12px; font-size: 12px; color: var(--text-3); text-transform: none; letter-spacing: 0; font-weight: 400; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; }
  li:hover { background: var(--bg-subtle); }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 5px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; color: var(--text-1); }
  .nm { font-weight: 500; white-space: nowrap; }
  .q { flex: 1; min-width: 0; font-family: var(--font-mono); font-size: 12px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .res { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .x { border: 0; background: transparent; color: var(--text-4); cursor: pointer; font-size: 14px; padding: 0 12px; }
  .x:hover { color: var(--err, #b42318); }
  .note { padding: 14px; color: var(--text-4); }
</style>
