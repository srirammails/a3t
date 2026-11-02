<script lang="ts">
  /// A template's spec, read-only here (its other fields are in the Form
  /// panel). Saving a template changes the agents built from it; until the
  /// Studio has a template editor, that happens in the Templates page.
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
</script>

<div class="tpl">
  <div class="head">
    <span class="kind">{kind.label}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if doc.data?.version}<span class="ver">v{doc.data.version}</span>{/if}
    <span class="grow"></span>
    <span class="ro">read-only</span>
    {#if kind.console}
      {@const c = kind.console}
      <button class="btn ghost sm" onclick={() => workspace.host.openConsole(c.section, c.title)}>Edit in {c.title}</button>
    {/if}
  </div>
  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else if !doc.data?.spec_yaml}
    <div class="note">This template has no spec.</div>
  {:else}
    <div class="code"><YamlEditor value={doc.data.spec_yaml} readOnly /></div>
  {/if}
</div>

<style>
  .tpl { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .ver { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .ro { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--bg-subtle); color: var(--text-3); }
  .grow { flex: 1; }
  .code { flex: 1; min-height: 0; display: flex; }
  .code > :global(*) { flex: 1; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 8px 12px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
