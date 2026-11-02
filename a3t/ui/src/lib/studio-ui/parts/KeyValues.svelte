<script lang="ts">
  /// A record shown as key → value (status, metrics, stats). Empty values are
  /// left out; nested objects show compactly.
  let { value }: { value: Record<string, unknown> | null | undefined } = $props();
  const entries = $derived(Object.entries(value ?? {}).filter(([, v]) => v !== undefined && v !== null && v !== '' && !(typeof v === 'object' && !Array.isArray(v) && !Object.keys(v as object).length)));
  const show = (v: unknown) => (typeof v === 'object' ? JSON.stringify(v) : String(v));
</script>

{#if entries.length}
  <dl>
    {#each entries as [k, v] (k)}<dt>{k}</dt><dd>{show(v)}</dd>{/each}
  </dl>
{:else}
  <p class="dim">—</p>
{/if}

<style>
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 3px 12px; margin: 0; font-size: 12.5px; }
  dt { color: var(--text-3); font-family: var(--font-mono); font-size: 11.5px; }
  dd { margin: 0; font-family: var(--font-mono); font-size: 12px; color: var(--text-1); word-break: break-word; }
  .dim { margin: 0; color: var(--text-4); font-size: 12.5px; }
</style>
