<script lang="ts">
  // A listing of any artifact kind, rendered as openable rows.
  //
  // The row fetches its item on demand via the kind's `get` tool — a read, still
  // Cedar-gated at site A — rather than the server streaming every body up front.
  import { mcpCall } from '$lib/services/api';
  import { ARTIFACTS, LIST_KIND_TO_ITEM, toEditorText, rowMeta, idArgsFor, type ArtifactKind } from './artifacts';

  let { kind, data, onopen }: {
    /// Plural widget kind: flows | agents | templates | resources | triggers
    kind: string;
    data: { namespace?: string; items?: { namespace: string; name: string; id?: string; meta?: any }[] };
    onopen?: (a: { kind: ArtifactKind; namespace: string; name: string; text: string; id?: string }) => void;
  } = $props();

  const itemKind = $derived((LIST_KIND_TO_ITEM[kind] ?? 'flow') as ArtifactKind);
  const spec = $derived(ARTIFACTS[itemKind]);
  const items = $derived(data?.items ?? []);

  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function open(it: { namespace: string; name: string; meta?: any }) {
    busy = `${it.namespace}/${it.name}`; error = null;
    const res = await mcpCall(spec.get, idArgsFor(spec, {
      namespace: it.namespace, name: it.name, id: it.meta?.id,
    }));
    busy = null;
    if (!res.ok) { error = res.error ?? `Could not load ${spec.label}`; return; }
    onopen?.({
      kind: itemKind, namespace: it.namespace, name: it.name, id: it.meta?.id,
      text: toEditorText(res.data, spec.format),
    });
  }
</script>

<div class="widget">
  <div class="widget-head">
    <span class="widget-kind">{kind}</span>
    <span class="widget-count">{items.length}{data?.namespace ? ` in ${data.namespace}` : ''}</span>
  </div>
  {#if error}<div class="widget-err">{error}</div>{/if}
  <div class="rows">
    {#each items as it}
      <div class="row">
        <button class="row-main" onclick={() => open(it)} title="Open {it.namespace}/{it.name}">
          <span class="row-name">{it.name}</span>
          <span class="row-meta">{rowMeta(itemKind, it.meta ?? {})}</span>
        </button>
        <button class="row-btn" onclick={() => open(it)} title="Open in the editor">
          {busy === `${it.namespace}/${it.name}` ? '…' : 'Open'}
        </button>
      </div>
    {/each}
  </div>
</div>

<style>
  .widget { margin-top: 8px; border: 1px solid var(--border); border-radius: 8px;
            background: var(--bg-subtle); overflow: hidden; }
  .widget-head { display: flex; align-items: center; gap: 8px; padding: 7px 10px; }
  .widget-kind { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
                 color: var(--text-3); border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  .widget-count { font-size: 11.5px; color: var(--text-3); }
  .widget-err { padding: 5px 10px; font-size: 11.5px; color: var(--err); }
  .rows { border-top: 1px solid var(--border); max-height: 300px; overflow-y: auto; }
  .row { display: flex; align-items: center; gap: 6px; padding: 0 6px 0 0; }
  .row + .row { border-top: 1px solid var(--border); }
  .row:hover { background: var(--bg-surface); }
  .row-main { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0;
              padding: 7px 10px; border: 0; background: none; cursor: pointer; text-align: left; }
  .row-name { font-family: var(--font-mono); font-size: 12px; color: var(--text-1);
              white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row-meta { font-size: 10.5px; color: var(--text-4); white-space: nowrap;
              overflow: hidden; text-overflow: ellipsis; }
  .row-btn { font-size: 11px; padding: 3px 8px; cursor: pointer; border-radius: 5px;
             border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); }
  .row-btn:hover { color: var(--text-1); border-color: var(--border-strong); }
</style>
