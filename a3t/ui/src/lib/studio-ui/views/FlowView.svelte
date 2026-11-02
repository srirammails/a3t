<script lang="ts">
  /// A flow's center view: the graph / code editor, with the flow's name, its
  /// Run/Debug split button and Deploy on the editor's own Graph | Code bar.
  /// Form and AI are right-region panels on the same document; the text lives
  /// on the document, so switching tabs never loses an edit.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { flowDebug } from '$lib/components/run/debugState.svelte.ts';
  import type { Doc, Workspace } from '$lib/studio-core';
  import type { Studio } from '../layout.svelte.ts';

  let { doc, workspace, studio }: { doc: Doc; workspace: Workspace; studio: Studio } = $props();

  /// Breakpoints and the run shown below, marked on the graph (deployed flows).
  const debug = $derived(doc.isNew ? null : flowDebug(doc.ref.project, doc.ref.name));

  let busy = $state(false);
  let message = $state('');
  const dirty = $derived((doc.text ?? '') !== (doc.saved ?? ''));

  $effect(() => {
    if (doc.loaded || doc.isNew) return;
    void workspace.load(doc).then(() => { if (doc.error) message = doc.error; });
  });

  /// Open the run dialog in the Runs panel.
  function onrun(debugRun: boolean) {
    doc.ui.runDebug = debugRun;
    studio.reveal('bottom', 'runs');
    doc.ui.runDialog = true;
  }
  const onshowrun = () => studio.reveal('bottom', 'runs');

  // ── Run / Debug split button ──
  let runMode = $state<'run' | 'debug'>((() => { try { return localStorage.getItem('a3t.flow.runMode') === 'debug' ? 'debug' : 'run'; } catch { return 'run'; } })());
  let runMenu = $state(false);
  const blocked = $derived(doc.isNew ? 'Deploy the flow first — a run uses the saved flow' : dirty ? 'Deploy your changes first — a run uses the saved flow' : '');
  function pick(m: 'run' | 'debug') {
    runMode = m; runMenu = false;
    try { localStorage.setItem('a3t.flow.runMode', m); } catch { /* private window */ }
    onrun(m === 'debug');
  }

  async function deploy() {
    busy = true; message = '';
    const r = await workspace.save(doc);
    busy = false;
    if (!r.ok) message = r.error;
  }
</script>

<div class="fd">
  <div class="fd-body">
    {#if doc.loaded || doc.isNew}
      <FlowEditorPane part="main" aiKind="flow" aiName={`${doc.ref.project}/${doc.ref.name}`} initialYaml={doc.text ?? ''} onchange={(v: string) => (doc.text = v)}
        {debug} onrun={blocked ? undefined : onrun} {onshowrun}>
        {#snippet toolbar()}
          <!-- Name centred on the Graph | Code bar, actions at its right end:
               one bar for the document instead of two. -->
          <div class="fd-title">
            <span class="fd-name mono" title={`${doc.ref.project}/${doc.ref.name}`}>{doc.ref.project}/{doc.ref.name}</span>
            {#if dirty}<span class="dirty">unsaved</span>{/if}
          </div>
          <span class="grow"></span>
          {#if message}<span class="msg" title={message}>{message}</span>{/if}
          <div class="split">
            <button class="btn sm split-main" disabled={!!blocked || busy} onclick={() => onrun(runMode === 'debug')} title={blocked || (runMode === 'debug' ? 'Debug: breakpoints, stepping, step inputs and outputs' : 'Run with inputs')}>
              {runMode === 'debug' ? '🐞 Debug' : '▶ Run'}
            </button>
            <button class="btn sm split-caret" disabled={busy} onclick={() => (runMenu = !runMenu)} aria-label="Run or debug" aria-haspopup="menu" aria-expanded={runMenu}>▾</button>
            {#if runMenu}
              <div class="split-overlay" role="presentation" onclick={() => (runMenu = false)}></div>
              <div class="split-menu" role="menu">
                <button role="menuitem" class:on={runMode === 'run'} disabled={!!blocked} onclick={() => pick('run')}><span>▶ Run</span><small>Run with inputs</small></button>
                <button role="menuitem" class:on={runMode === 'debug'} disabled={!!blocked} onclick={() => pick('debug')}><span>🐞 Debug</span><small>Breakpoints, stepping, step inputs and outputs</small></button>
              </div>
            {/if}
          </div>
          <button class="btn spec sm" disabled={busy || (!dirty && !doc.isNew)} onclick={deploy}>{busy ? 'Deploying…' : 'Deploy'}</button>
        {/snippet}
      </FlowEditorPane>
    {:else}
      <div class="loading">Loading {doc.ref.project}/{doc.ref.name}…{#if message}<span class="msg"> {message}</span>{/if}</div>
    {/if}
  </div>
</div>

<style>
  .fd { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  /* Centred on the whole bar, not the gap between tabs and buttons; kept narrow
     enough to clear both, and ellipsised past that. */
  .fd-title { position: absolute; left: 50%; transform: translateX(-50%); max-width: 36%;
              display: flex; align-items: center; gap: 6px; }
  .fd-name { font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mono { font-family: var(--font-mono); }
  .dirty { font-family: var(--font-mono); font-size: 10px; color: var(--warn); background: var(--warn-bg); padding: 2px 6px; border-radius: 3px; }
  .msg { font-size: 11.5px; color: var(--err, #b42318); max-width: 24%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grow { flex: 1; }
  .fd-body { flex: 1; min-height: 0; position: relative; overflow: hidden; }
  .loading { padding: 16px; color: var(--text-4); }
  .split { position: relative; display: inline-flex; }
  .split-main, .split-caret { background: var(--bg-surface); color: var(--spec); border: 1px solid var(--spec); }
  .split-main:hover:not(:disabled), .split-caret:hover:not(:disabled) { background: var(--spec-bg); }
  .split-main { border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .split-caret { border-top-left-radius: 0; border-bottom-left-radius: 0; padding-left: 7px; padding-right: 7px; border-left: 0; }
  .split-overlay { position: fixed; inset: 0; z-index: 50; }
  .split-menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 51; min-width: 250px; display: flex; flex-direction: column;
                background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); padding: 4px; }
  .split-menu button { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; border: 0; background: transparent; padding: 7px 10px; border-radius: var(--r-sm); cursor: pointer; text-align: left; color: var(--text-1); font-size: 13px; }
  .split-menu button:hover, .split-menu button.on { background: var(--bg-subtle); }
  .split-menu button:disabled { opacity: 0.45; cursor: default; }
  .split-menu small { color: var(--text-3); font-size: 11.5px; }
</style>
