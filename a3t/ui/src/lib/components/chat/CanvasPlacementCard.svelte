<script lang="ts">
  /// Where a flow would land (or did). This is the thing a user agrees to when
  /// they deploy, so it is shown as a fact sheet rather than left in the tool log.
  let { data }: { data: any } = $props();

  const matched = $derived(!!data?.matched);
  const deployed = $derived(!!data?.deployed);
  const scale = $derived.by(() => {
    const s = data?.scaling;
    if (!s) return '1 replica';
    return s.min === s.max ? `${s.min} replica${s.min === 1 ? '' : 's'}` : `${s.min}–${s.max} replicas`;
  });
</script>

<div class="cpl" class:bad={!matched}>
  <div class="cpl-head">
    <span class="cpl-title">{deployed ? 'Placed' : 'Would deploy to'}</span>
    <code>{data?.flow}</code>
  </div>
  {#if matched}
    <dl class="cpl-facts">
      <dt>Runtime</dt><dd><code>{data.placedRuntime}</code></dd>
      <dt>Namespace</dt><dd>{data.namespace || '—'}</dd>
      <dt>Mode</dt><dd>{data.mode === 'isolated' ? 'dedicated runtime' : 'shared pool'}</dd>
      <dt>Scale</dt><dd>{scale}</dd>
    </dl>
  {:else}
    <p class="cpl-miss">
      No runtime matches this placement. Available: {(data?.availableRuntimes ?? []).join(', ') || 'none'}.
    </p>
  {/if}
  {#if data?.note}<p class="cpl-note">{data.note}</p>{/if}
</div>

<style>
  .cpl { border: 1px solid var(--border); border-radius: var(--r, 10px); background: var(--bg-surface); padding: 12px 14px; margin: 10px 0; max-width: 520px; }
  .cpl.bad { border-color: #e7b4b4; }
  .cpl-head { display: flex; align-items: baseline; gap: 8px; margin-bottom: 8px; }
  .cpl-title { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); }
  .cpl-head code, .cpl-facts code { font-family: var(--font-mono, ui-monospace, monospace); font-size: 12.5px; color: var(--text-1); }
  .cpl-facts { display: grid; grid-template-columns: max-content 1fr; gap: 3px 12px; margin: 0; font-size: 12px; }
  .cpl-facts dt { color: var(--text-4); }
  .cpl-facts dd { margin: 0; color: var(--text-1); }
  .cpl-miss { margin: 0; font-size: 12.5px; color: #b91c1c; }
  .cpl-note { margin: 8px 0 0; font-size: 11px; color: var(--text-4); line-height: 1.45; }
</style>
