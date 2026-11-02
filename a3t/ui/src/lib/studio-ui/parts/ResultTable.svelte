<script lang="ts">
  /// A query's answer: a table when it is rows, the raw text otherwise.
  /// Columns that look like secrets are hidden until shown.
  import type { QueryResult } from '$lib/studio-core';

  let { result }: { result: QueryResult } = $props();

  const SECRET = /pass|hash|secret|token|api_?key|salt/i;
  let mode = $state<'table' | 'raw'>('table');
  let showSecrets = $state(false);
  const hidden = $derived(showSecrets ? [] : result.columns.filter((c) => SECRET.test(c)));
  const columns = $derived(result.columns.filter((c) => !hidden.includes(c)));
  const tabular = $derived(result.columns.length > 0);
  const cell = (v: unknown) => (v === null || v === undefined ? 'NULL' : typeof v === 'object' ? JSON.stringify(v) : String(v));
</script>

<div class="rt">
  <div class="bar">
    {#if tabular}
      <div class="seg">
        <button class:on={mode === 'table'} onclick={() => (mode = 'table')}>Table</button>
        <button class:on={mode === 'raw'} onclick={() => (mode = 'raw')}>Raw</button>
      </div>
      <span class="meta">{result.rows.length} row{result.rows.length === 1 ? '' : 's'}</span>
      {#if result.columns.some((c) => SECRET.test(c))}
        <label class="meta chk"><input type="checkbox" bind:checked={showSecrets} /> Show secrets{hidden.length ? ` (${hidden.length} hidden)` : ''}</label>
      {/if}
    {/if}
    <span class="meta">{result.ms} ms</span>
  </div>
  {#if tabular && mode === 'table'}
    <div class="wrap">
      <table>
        <thead><tr>{#each columns as c (c)}<th>{c}</th>{/each}</tr></thead>
        <tbody>
          {#each result.rows as row, i (i)}
            <tr>{#each columns as c (c)}<td class:null={row[c] === null || row[c] === undefined}>{cell(row[c])}</td>{/each}</tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <pre>{result.raw}</pre>
  {/if}
</div>

<style>
  .rt { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { display: flex; align-items: center; gap: 10px; padding: 5px 12px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .seg { display: flex; gap: 2px; background: var(--bg-subtle); border-radius: var(--r-sm); padding: 2px; }
  .seg button { border: 0; background: transparent; padding: 2px 9px; border-radius: 3px; font-size: 11.5px; color: var(--text-3); cursor: pointer; }
  .seg button.on { background: var(--bg-surface); color: var(--text-1); box-shadow: var(--shadow-sm); }
  .meta { font-size: 11.5px; color: var(--text-3); font-family: var(--font-mono); }
  .chk { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; text-transform: none; letter-spacing: 0; font-weight: 400; }
  .wrap { flex: 1; min-height: 0; overflow: auto; }
  table { border-collapse: collapse; font-family: var(--font-mono); font-size: 12px; }
  th, td { padding: 4px 10px; border-bottom: 1px solid var(--border); border-right: 1px solid var(--border); text-align: left; white-space: nowrap; max-width: 420px; overflow: hidden; text-overflow: ellipsis; }
  th { position: sticky; top: 0; background: var(--bg-page); font-weight: 600; color: var(--text-2); }
  td.null { color: var(--text-4); font-style: italic; }
  pre { flex: 1; min-height: 0; overflow: auto; margin: 0; padding: 10px 12px; font-family: var(--font-mono); font-size: 12px; white-space: pre-wrap; word-break: break-word; }
</style>
