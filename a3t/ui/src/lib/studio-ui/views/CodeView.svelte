<script lang="ts">
  /// An item as code: its config as JSON, edited beside the Form panel's form of
  /// the same document. Items with no action of their own show here; others come
  /// here with Edit (or a double-click) and go back with ←. A new item is named
  /// (and, for resources, typed) here before its first save.
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import EditButton from '../parts/EditButton.svelte';
  import { fromText, startingConfig, toText, typesLike, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const parsed = $derived(fromText(doc.text));
  /// A new resource or canvas is named here (kinds named in their text are not).
  const namedHere = $derived(!!doc.isNew && !kind.nameInText);
  const nameProblem = $derived(namedHere ? kind.checkName?.(doc.ref.name) ?? '' : '');
  const types = $derived(doc.isNew && doc.type ? typesLike(doc.type) : []);

  /// Another type for a new resource; its starting config too, unless edited.
  function retype(t: string) {
    const untouched = (doc.text ?? '') === toText(startingConfig(doc.type ?? ''));
    doc.type = t;
    if (untouched) doc.text = toText(startingConfig(t));
  }

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
</script>

<div class="cd">
  <div class="head">
    <span class="kind">{doc.isNew ? `New ${kind.label.toLowerCase()}` : kind.label}{doc.type && !types.length ? ` · ${doc.type}` : ''}</span>
    {#if types.length > 1}
      <select class="type" value={doc.type} onchange={(e) => retype(e.currentTarget.value)} title="Type">
        {#each types as t (t.type)}<option value={t.type}>{t.label}</option>{/each}
      </select>
    {/if}
    {#if namedHere}
      <span class="mono dim">{doc.ref.project}/</span><input class="name" bind:value={doc.ref.name} spellcheck="false" title="Name" />
      {#if nameProblem}<span class="bad">{nameProblem}</span>{/if}
    {:else}
      <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    {#if kind.console}
      {@const c = kind.console}
      <button class="btn ghost sm" onclick={() => workspace.host.openConsole(c.section, c.title)}>Open in {c.title}</button>
    {/if}
    <SaveBar {doc} {workspace} valid={!parsed.error && !nameProblem} />
  </div>
  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else}
    {#if parsed.error}<div class="err">Not valid JSON: {parsed.error}</div>{/if}
    <div class="code"><YamlEditor value={doc.text ?? ''} readOnly={!kind.push} onchange={(v: string) => (doc.text = v)} /></div>
  {/if}
</div>

<style>
  .cd { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .grow { flex: 1; }
  .dim { color: var(--text-4); }
  .name { font-family: var(--font-mono); font-size: 12.5px; padding: 4px 8px; width: 200px; flex: 0 1 200px; min-width: 120px; }
  .type { font-size: 12px; padding: 3px 24px 3px 6px; width: auto; flex: 0 0 auto; }
  .bad { color: var(--err, #b42318); font-size: 11.5px; }
  .code { flex: 1; min-height: 0; display: flex; }
  .code > :global(*) { flex: 1; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 8px 12px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
