<script lang="ts">
  /// One flow tab: its toolbar (Run/Debug split button, Deploy) and the graph /
  /// code editor (the studio shows its Form and AI in the right panels). The text lives in the studio's document, so switching
  /// tabs never loses an edit.
  import { flows as flowsMcp, mcpCall } from '$lib/services/api';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { parse, type Doc } from './flow';

  let { doc, onrun, ondeployed }: {
    doc: Doc;
    onrun: (debug: boolean) => void;
    ondeployed: (ns: string, name: string) => void;
  } = $props();

  let busy = $state(false);
  let message = $state('');
  const dirty = $derived((doc.yaml ?? '') !== (doc.saved ?? ''));

  $effect(() => {
    if (doc.loaded || doc.isNew) return;
    const d = doc;
    void (async () => {
      const res = await flowsMcp.get(d.name, d.ns);
      const text = res.ok ? (typeof res.data === 'string' ? res.data : res.data?.text ?? '') : '';
      const yaml = (text.match(/```yaml\n([\s\S]*?)```/)?.[1] ?? text).trim() + '\n';
      d.yaml = yaml;
      d.saved = yaml;
      d.loaded = true;
      if (!res.ok) message = res.error ?? 'Could not load the flow';
    })();
  });

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
    const { doc: parsed, error } = parse(doc.yaml);
    if (error || !parsed) { message = `Not valid YAML: ${error}`; return; }
    busy = true; message = '';
    const res = await mcpCall('create_flow', { yaml: doc.yaml });
    busy = false;
    if (!res.ok) { message = res.error ?? 'Deploy failed'; return; }
    doc.saved = doc.yaml;
    const ns = parsed.document?.namespace ?? doc.ns;
    const name = parsed.document?.name ?? doc.name;
    ondeployed(ns, name);
  }
</script>

<div class="fd">
  <div class="fd-bar">
    <span class="fd-title mono">{doc.ns}/{doc.name}</span>
    {#if dirty}<span class="dirty">unsaved</span>{/if}
    {#if message}<span class="msg" title={message}>{message}</span>{/if}
    <span class="grow"></span>
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
  </div>
  <div class="fd-body">
    {#if doc.loaded || doc.isNew}
      <FlowEditorPane part="main" aiKind="flow" aiName={`${doc.ns}/${doc.name}`} initialYaml={doc.yaml ?? ''} onchange={(v: string) => (doc.yaml = v)} />
    {:else}
      <div class="loading">Loading {doc.ns}/{doc.name}…</div>
    {/if}
  </div>
</div>

<style>
  .fd { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .fd-bar { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .fd-title { font-size: 12px; color: var(--text-2); }
  .mono { font-family: var(--font-mono); }
  .dirty { font-family: var(--font-mono); font-size: 10px; color: var(--warn); background: var(--warn-bg); padding: 2px 6px; border-radius: 3px; }
  .msg { font-size: 11.5px; color: var(--err, #b42318); max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
