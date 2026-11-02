<script lang="ts">
  /// An item shown as listed (secrets are masked by the server), with a way to
  /// the Console page where its kind is managed.
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const pretty = (v: any) => JSON.stringify(v ?? {}, null, 2);
  const type = $derived(doc.type ?? doc.data?.type ?? doc.data?.type_ref ?? doc.data?.config?.type ?? '');
</script>

<div class="ip">
  <div class="ip-head">
    <span class="kind">{kind.label}{type ? ` · ${type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    <span class="grow"></span>
    {#if kind.console}{@const c = kind.console}<button class="btn ghost sm" onclick={() => workspace.host.openConsole(c.section, c.title)}>Open in {c.title}</button>{/if}
  </div>
  <pre>{pretty(doc.ref.kind === 'template' ? { ...doc.data, spec_yaml: undefined } : doc.data)}</pre>
  {#if doc.ref.kind === 'template' && doc.data?.spec_yaml}
    <h4>Spec</h4>
    <pre>{doc.data.spec_yaml}</pre>
  {/if}
</div>

<style>
  .ip { padding: 12px 16px; display: flex; flex-direction: column; gap: 10px; overflow: auto; height: 100%; }
  .ip-head { display: flex; align-items: center; gap: 10px; }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .grow { flex: 1; }
  h4 { margin: 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  pre { margin: 0; padding: 10px 12px; background: var(--bg-page); border: 1px solid var(--border); border-radius: var(--r-sm); font-family: var(--font-mono); font-size: 12px; white-space: pre-wrap; word-break: break-word; }
</style>
