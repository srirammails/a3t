<script lang="ts">
  /// A non-flow item opened in a studio tab: what it is, as listed (secrets are
  /// masked by the server), and a way to its own app for editing.
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import type { Doc } from './flow';

  let { doc }: { doc: Doc } = $props();

  const APPS: Record<string, [string, string]> = {
    resource: ['resources', 'Resources'],
    trigger: ['triggers', 'Triggers'],
    template: ['templates', 'Templates'],
    canvas: ['canvas', 'Canvas'],
  };
  const app = $derived(APPS[doc.kind]);
  const pretty = (v: any) => JSON.stringify(v ?? {}, null, 2);
  const type = $derived(doc.data?.type ?? doc.data?.type_ref ?? doc.data?.config?.type ?? '');
</script>

<div class="ip">
  <div class="ip-head">
    <span class="kind">{doc.kind}{type ? ` · ${type}` : ''}</span>
    <span class="mono">{doc.ns}/{doc.name}</span>
    <span class="grow"></span>
    {#if app}<button class="btn ghost sm" onclick={() => navigateTo(app[0], app[1])}>Open in {app[1]}</button>{/if}
  </div>
  <pre>{pretty(doc.kind === 'template' ? { ...doc.data, spec_yaml: undefined } : doc.data)}</pre>
  {#if doc.kind === 'template' && doc.data?.spec_yaml}
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
