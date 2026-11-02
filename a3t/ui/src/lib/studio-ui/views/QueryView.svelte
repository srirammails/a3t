<script lang="ts">
  /// A database's Query view: write a statement in the database's language and
  /// run it (Run or Ctrl/⌘+Enter); the answer shows in the Results panel. The
  /// draft is kept per database; runs go to History; Save keeps a query by name.
  import SqlEditor from '$lib/components/editor/SqlEditor.svelte';
  import { languagesOf, type Doc, type Workspace } from '$lib/studio-core';
  import type { Studio } from '../layout.svelte.ts';
  import { addHistory, addSaved, loadQueryLists } from '../queryLists.svelte.ts';

  let { doc, workspace, studio }: { doc: Doc; workspace: Workspace; studio: Studio } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const langs = $derived(languagesOf(doc.type));
  const lang = $derived(langs.find((l) => l.id === doc.ui.lang) ?? langs[0]);
  const draftKey = $derived(`a3t.studio.db.draft.${doc.address}`);

  // The draft survives reloads; the config loads for the Config view and the type.
  $effect(() => {
    loadQueryLists(workspace.host);
    if (!doc.ui.query) doc.ui.query = (workspace.host.storage.get(draftKey) as string) ?? '';
    if (!doc.loaded) void workspace.load(doc);
  });

  function edit(v: string) {
    doc.ui.query = v;
    workspace.host.storage.set(draftKey, v);
  }

  async function run() {
    const text = (doc.ui.query ?? '').trim();
    if (!text || doc.ui.running || !kind.query) return;
    doc.ui.running = true;
    doc.ui.error = '';
    studio.reveal('bottom', 'results');
    const r = await kind.query(doc, text, lang.id);
    doc.ui.running = false;
    doc.ui.result = r.ok ? r.data : null;
    doc.ui.error = r.ok ? '' : r.error;
    addHistory(workspace.host, {
      resource: `${doc.ref.project}/${doc.ref.name}`, type: doc.type, query: text, lang: lang.id,
      ts: Date.now(), ok: r.ok, ms: r.ok ? r.data.ms : 0,
    });
  }

  let naming = $state<string | null>(null);
  function keep() {
    const name = naming?.trim();
    if (!name) return;
    addSaved(workspace.host, { name, query: doc.ui.query ?? '', resource: `${doc.ref.project}/${doc.ref.name}`, type: doc.type, lang: lang.id });
    naming = null;
    studio.reveal('bottom', 'saved');
  }
</script>

<div class="qv">
  <div class="head">
    <span class="kind">{kind.label}{doc.type ? ` · ${doc.type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if langs.length > 1}
      <div class="langs" role="group" aria-label="Language">
        {#each langs as l (l.id)}
          <button class:on={lang.id === l.id} onclick={() => (doc.ui.lang = l.id)}
            title={l.id === 'sql' && (doc.type === 'adb' || doc.type === 'clawdb') ? 'Read-only SQL over ADB\'s tables (superadmin)' : l.label}>{l.label}</button>
        {/each}
      </div>
    {:else}
      <span class="lang">{lang.label}</span>
    {/if}
    <span class="grow"></span>
    {#if naming !== null}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="name" placeholder="name this query" bind:value={naming} autofocus
        onkeydown={(e) => { if (e.key === 'Enter') keep(); if (e.key === 'Escape') naming = null; }} />
      <button class="btn ghost sm" onclick={keep} disabled={!naming.trim()}>Keep</button>
    {:else}
      <button class="btn ghost sm" onclick={() => (naming = '')} disabled={!(doc.ui.query ?? '').trim()}>Save query</button>
    {/if}
    <button class="btn spec sm" onclick={run} disabled={doc.ui.running || !(doc.ui.query ?? '').trim()} title="Run (Ctrl/⌘+Enter)">
      {doc.ui.running ? 'Running…' : '▶ Run'}
    </button>
  </div>
  <div class="editor">
    <SqlEditor value={doc.ui.query ?? ''} dialect={lang.dialect} placeholder={`${lang.label} for ${doc.ref.name} — Ctrl/⌘+Enter runs it`} onchange={edit} onexecute={run} />
  </div>
</div>

<style>
  .qv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .lang { font-family: var(--font-mono); font-size: 10px; font-weight: 600; letter-spacing: 0.05em; color: var(--spec); background: var(--spec-bg); padding: 2px 6px; border-radius: 3px; }
  .langs { display: inline-flex; border: 1px solid var(--border); border-radius: 4px; overflow: hidden; }
  .langs button { font-family: var(--font-mono); font-size: 10px; font-weight: 600; letter-spacing: 0.05em; padding: 2px 8px; background: transparent; color: var(--text-4); border: 0; cursor: pointer; }
  .langs button.on { color: var(--spec); background: var(--spec-bg); }
  .grow { flex: 1; }
  .name { width: 200px; padding: 5px 8px; font-size: 12px; }
  .editor { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .editor > :global(*) { flex: 1; min-height: 0; }
</style>
