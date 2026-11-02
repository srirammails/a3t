<script lang="ts">
  /// The center view for items with no action of their own yet: their config
  /// as code (JSON), beside the Form panel's form of the same document.
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import { fromText, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const parsed = $derived(fromText(doc.text));

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
</script>

<div class="cd">
  <div class="head">
    <span class="kind">{kind.label}{doc.type ? ` · ${doc.type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    <span class="grow"></span>
    {#if kind.console}
      {@const c = kind.console}
      <button class="btn ghost sm" onclick={() => workspace.host.openConsole(c.section, c.title)}>Open in {c.title}</button>
    {/if}
    <SaveBar {doc} {workspace} valid={!parsed.error} />
  </div>
  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else}
    {#if parsed.error}<div class="err">Not valid JSON: {parsed.error}</div>{/if}
    <div class="code"><YamlEditor value={doc.text ?? ''} readOnly={!kind.save} onchange={(v: string) => (doc.text = v)} /></div>
  {/if}
</div>

<style>
  .cd { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .grow { flex: 1; }
  .code { flex: 1; min-height: 0; display: flex; }
  .code > :global(*) { flex: 1; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 8px 12px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
