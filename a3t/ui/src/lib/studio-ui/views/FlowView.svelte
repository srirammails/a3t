<script lang="ts">
  /// A flow's center view: the graph / code editor, with the flow's name and its
  /// life on the editor's own Graph | Code bar — Save (a draft here), Sync ▾ (Push /
  /// Merge with the registry) and Run ▾ (Deploy onto a runtime, Run, Debug).
  /// Form and AI are right-region panels on the same document; the text lives
  /// on the document, so switching tabs never loses an edit.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import { flowDebug } from '$lib/components/run/debugState.svelte.ts';
  import SaveBar from '../parts/SaveBar.svelte';
  import { addTask, getTrigger, ITEM_MIME, ops, paletteStep, putTrigger, STEP_MIME, taskForItem, type Doc, type Item, type Workspace } from '$lib/studio-core';
  import type { Studio } from '../layout.svelte.ts';

  let { doc, workspace, studio }: { doc: Doc; workspace: Workspace; studio: Studio } = $props();

  /// Breakpoints and the run shown below, marked on the graph (deployed flows).
  const debug = $derived(doc.isNew ? null : flowDebug(doc.ref.project, doc.ref.name));

  let busy = $state(false);
  let message = $state('');
  /// Not yet in the registry as edited: runs and deploys use the registry's copy.
  const unpushed = $derived(workspace.isUnpushed(doc));

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

  // ── Run ▾: Deploy / Run / Debug; the main button does the last one picked ──
  type Mode = 'deploy' | 'run' | 'debug';
  let runMode = $state<Mode>((() => { try { const m = localStorage.getItem('a3t.flow.runMode'); return m === 'debug' || m === 'deploy' ? m : 'run'; } catch { return 'run'; } })());
  let runMenu = $state(false);
  const blocked = $derived(doc.isNew ? 'Push the flow first — a run uses the registry’s copy' : unpushed ? 'Push your changes first — a run uses the registry’s copy' : '');
  function go(m: Mode) {
    if (m === 'deploy') void deploy(); else onrun(m === 'debug');
  }
  function pick(m: Mode) {
    runMode = m; runMenu = false;
    try { localStorage.setItem('a3t.flow.runMode', m); } catch { /* private window */ }
    go(m);
  }
  const MODES: Record<Mode, { label: string; hint: string }> = {
    deploy: { label: '⇪ Deploy', hint: 'Put the flow on a runtime, where its canvas places it' },
    run: { label: '▶ Run', hint: 'Run with inputs' },
    debug: { label: '🐞 Debug', hint: 'Breakpoints, stepping, step inputs and outputs' },
  };

  // ── Compose: drop a palette step or an Explorer item on the flow ──
  let dropping = $state(false);
  function ondragover(e: DragEvent) {
    const t = e.dataTransfer?.types ?? [];
    if (t.includes(STEP_MIME) || t.includes(ITEM_MIME)) { e.preventDefault(); dropping = true; }
  }
  /// Add a task and open it in the Form panel.
  function addStep(base: string, task: Record<string, unknown>) {
    const r = addTask(doc.text ?? '', base, task);
    doc.text = r.yaml;
    doc.ui.focusStep = { name: r.name, n: Date.now() };
    studio.reveal('right', 'form');
    message = '';
  }
  async function ondrop(e: DragEvent) {
    dropping = false;
    const step = e.dataTransfer?.getData(STEP_MIME);
    const raw = e.dataTransfer?.getData(ITEM_MIME);
    if (!step && !raw) return;
    e.preventDefault();
    if (step) {
      const s = paletteStep(step);
      if (s) addStep(s.base, s.task());
      return;
    }
    const it = JSON.parse(raw!) as Item;
    if (it.kind === 'trigger') { await wire(it); return; }
    if (it.kind === 'flow' && it.project === doc.ref.project && it.name === doc.ref.name) { message = 'A flow cannot run itself as a sub-flow.'; return; }
    const made = taskForItem(it);
    if (made) addStep(made.base, made.task);
    else message = `${it.name} is not something a flow step uses.`;
  }
  /// A trigger is not a step: it starts flows. Dropping one adds this flow to its targets.
  async function wire(it: Item) {
    if (doc.isNew) { message = 'Push the flow first — a trigger starts the registry’s copy.'; return; }
    if (!it.id) return;
    if (!(await workspace.host.confirm(`Start ${doc.ref.project}/${doc.ref.name} from trigger ${it.name} too?`))) return;
    const t = await getTrigger(it.id);
    if (!t.ok) { message = t.error; return; }
    const { id: _id, status: _s, ...rest } = t.data;
    if ((rest.targets ?? []).some((x: any) => x.type === 'workflow' && x.name === doc.ref.name && (x.namespace ?? rest.namespace) === doc.ref.project)) { message = `${it.name} already starts this flow.`; return; }
    const r = await putTrigger(it.id, { ...rest, targets: [...(rest.targets ?? []), { type: 'workflow', namespace: doc.ref.project, name: doc.ref.name }] });
    message = r.ok ? `Trigger ${it.name} now starts this flow too.` : r.error;
  }
  // A click in the step palette asks for a step.
  $effect(() => {
    const req = doc.ui.addStep as { id: string; n: number } | undefined;
    if (!req) return;
    doc.ui.addStep = undefined;
    const s = paletteStep(req.id);
    if (s) addStep(s.base, s.task());
  });

  /// Deploy: the pushed flow onto a runtime, where its canvas places it.
  let deployNote = $state('');
  async function deploy() {
    busy = true; message = ''; deployNote = '';
    const r = await ops.deploy(doc.ref.project, doc.ref.name);
    busy = false;
    if (!r.ok) { message = r.error; return; }
    const d: any = r.data ?? {};
    if (d.deployed === false) message = d.error ?? 'Not deployed.';
    else deployNote = `Deployed on ${d.placedRuntime ?? d.placement?.runtime ?? 'its runtime'}.`;
  }
</script>

<div class="fd" class:dropping {ondragover} ondragleave={(e) => { if (e.currentTarget === e.target) dropping = false; }} {ondrop} role="region" aria-label="Flow">
  {#if dropping}<div class="drop-hint">Drop to add a step</div>{/if}
  <div class="fd-body">
    {#if doc.loaded || doc.isNew}
      <FlowEditorPane part="main" aiKind="flow" aiName={`${doc.ref.project}/${doc.ref.name}`} initialYaml={doc.text ?? ''} onchange={(v: string) => (doc.text = v)} showCode={doc.ui.codeAsked}
        {debug} onrun={blocked ? undefined : onrun} {onshowrun} palette gotoLine={doc.ui.gotoLine ?? null}
        onaddstep={(id: string) => { const s = paletteStep(id); if (s) addStep(s.base, s.task()); }}>
        {#snippet toolbar()}
          <!-- Name centred on the Graph | Code bar, actions at its right end:
               one bar for the document instead of two. -->
          <div class="fd-title">
            <span class="fd-name mono" title={`${doc.ref.project}/${doc.ref.name}`}>{doc.ref.project}/{doc.ref.name}</span>
          </div>
          {#if message}<span class="msg" title={message}>{message}</span>{:else if deployNote}<span class="msg ok" title={deployNote}>{deployNote}</span>{/if}
          <SaveBar {doc} {workspace} quiet />
          <div class="split">
            <button class="btn sm split-main" disabled={!!blocked || busy} onclick={() => go(runMode)} title={blocked || MODES[runMode].hint}>
              {busy && runMode === 'deploy' ? 'Deploying…' : MODES[runMode].label}
            </button>
            <button class="btn sm split-caret" disabled={busy} onclick={() => (runMenu = !runMenu)} aria-label="Deploy, run or debug" aria-haspopup="menu" aria-expanded={runMenu}>▾</button>
            {#if runMenu}
              <div class="split-overlay" role="presentation" onclick={() => (runMenu = false)}></div>
              <div class="split-menu" role="menu">
                {#each ['deploy', 'run', 'debug'] as const as m (m)}
                  <button role="menuitem" class:on={runMode === m} disabled={!!blocked} onclick={() => pick(m)}><span>{MODES[m].label}</span><small>{MODES[m].hint}</small></button>
                {/each}
                {#if blocked}<span class="blocked">{blocked}</span>{/if}
              </div>
            {/if}
          </div>
        {/snippet}
      </FlowEditorPane>
    {:else}
      <div class="loading">Loading {doc.ref.project}/{doc.ref.name}…{#if message}<span class="msg"> {message}</span>{/if}</div>
    {/if}
  </div>
</div>

<style>
  .fd { display: flex; flex-direction: column; height: 100%; min-height: 0; position: relative; }
  .fd.dropping { outline: 2px dashed var(--spec); outline-offset: -4px; }
  .drop-hint { position: absolute; left: 50%; top: 56px; transform: translateX(-50%); z-index: 30; pointer-events: none;
               padding: 4px 12px; border-radius: 12px; background: var(--spec); color: white; font-size: 12px; }
  /* The name takes the room between the tabs and the actions, ellipsised. */
  .fd-title { flex: 1 1 auto; min-width: 60px; display: flex; align-items: center; justify-content: center; gap: 6px; overflow: hidden; }
  .fd-name { font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mono { font-family: var(--font-mono); }
  .msg.ok { color: var(--ok, #15803d); }
  .blocked { padding: 6px 10px 4px; font-size: 11.5px; color: var(--warn, #b45309); border-top: 1px solid var(--border); margin-top: 3px; }
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
