<script lang="ts">
  /// Queries run lately, newest first; this database's, or all (bottom region).
  /// Click one to put it back in its database's Query view.
  import type { Doc, Workspace } from '$lib/studio-core';
  import { loadQueryLists, queryLists, reuse } from '../queryLists.svelte.ts';

  let { doc, workspace }: { doc: Doc | null; workspace: Workspace } = $props();
  let all = $state(false);
  $effect(() => loadQueryLists(workspace.host));
  const here = $derived(doc && workspace.kindOf(doc).query ? `${doc.ref.project}/${doc.ref.name}` : null);
  const list = $derived(queryLists.history.filter((h) => all || !here || h.resource === here));
  const when = (ts: number) => new Date(ts).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
</script>

<div class="ql">
  {#if here}
    <label class="scope"><input type="checkbox" bind:checked={all} /> All databases</label>
  {/if}
  {#if !list.length}<div class="note">No queries yet.</div>{/if}
  <ul>
    {#each list.slice(0, 60) as h (h.ts)}
      <li>
        <button onclick={() => reuse(workspace, h)} title={h.query}>
          <span class="dot" class:ok={h.ok}></span>
          <span class="q">{h.query}</span>
          {#if all || !here}<span class="res">{h.resource}</span>{/if}
          <span class="meta">{h.lang.toUpperCase()} · {h.ok ? `${h.ms} ms` : 'failed'} · {when(h.ts)}</span>
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .ql { height: 100%; overflow: auto; font-size: 12.5px; }
  .scope { display: flex; align-items: center; gap: 6px; padding: 6px 12px; font-size: 12px; color: var(--text-3); text-transform: none; letter-spacing: 0; font-weight: 400; }
  ul { list-style: none; margin: 0; padding: 0; }
  button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 5px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; color: var(--text-1); }
  button:hover { background: var(--bg-subtle); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--err, #b42318); flex-shrink: 0; }
  .dot.ok { background: var(--ok, #15803d); }
  .q { flex: 1; min-width: 0; font-family: var(--font-mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .res { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .meta { font-size: 11px; color: var(--text-4); white-space: nowrap; }
  .note { padding: 14px; color: var(--text-4); }
</style>
