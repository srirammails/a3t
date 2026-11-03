<script lang="ts">
  /// A trigger's designer (adopted from the Triggers page): its identity and
  /// channel (webhook, cron or event), the flows it starts as a graph — drop
  /// flows from the list on the right or from the Explorer, remove them on the
  /// graph — and a Test area that fires it with a payload; the runs it starts
  /// show in the Runs panel below. It edits the trigger document itself, so the
  /// Form panel and the code view stay in step. Test fires the trigger as saved.
  import { SvelteFlow, Background, Controls, Position, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import TargetNode from '$lib/components/editor/TargetNode.svelte';
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import EditButton from '../parts/EditButton.svelte';
  import { flows as flowsApi, fromText, ITEM_MIME, setAt, toText, type Doc, type Workspace } from '$lib/studio-core';
  import type { Studio } from '../layout.svelte.ts';

  let { doc, workspace, studio }: { doc: Doc; workspace: Workspace; studio: Studio } = $props();

  type Target = { type: string; namespace?: string; name?: string; agent_id?: string; url?: string; input_mapping?: unknown };
  const kind = $derived(workspace.kindOf(doc));
  const parsed = $derived(fromText(doc.text));
  const t = $derived((parsed.value ?? {}) as { name?: string; description?: string; enabled?: boolean; config?: any; targets?: Target[] });
  const cfg = $derived(t.config ?? {});
  const targets = $derived(t.targets ?? []);
  const connected = $derived(new Set(targets.filter((x) => x.type === 'workflow').map((x) => `${x.namespace}/${x.name}`)));

  /// Edit one field of the trigger document.
  const set = (path: (string | number)[], v: unknown) => { if (parsed.value) doc.text = toText(setAt(t, path, v)); };
  const CHANNELS = {
    webhook: () => ({ type: 'webhook', path: `/${t.name || 'my-hook'}`, method: 'POST' }),
    cron: () => ({ type: 'cron', expression: '*/5 * * * *', message: '' }),
    event: () => ({ type: 'event', source: '' }),
  } as const;
  function channel(type: keyof typeof CHANNELS) { if (cfg.type !== type) set(['config'], CHANNELS[type]()); }

  function connect(ns: string, name: string) {
    if (connected.has(`${ns}/${name}`)) return;
    set(['targets'], [...targets, { type: 'workflow', namespace: ns, name }]);
  }
  const disconnect = (i: number) => set(['targets'], targets.filter((_, j) => j !== i));

  // ── The flows that can be started: the projects in scope ──
  let available = $state<{ namespace: string; name: string }[]>([]);
  let search = $state('');
  const shown = $derived(available.filter((f) => !search.trim() || `${f.namespace}/${f.name}`.toLowerCase().includes(search.trim().toLowerCase())));
  $effect(() => {
    const projects = workspace.host.scope.projects();
    void Promise.all(projects.map((p) => flowsApi.list(p).then((r) => (r.ok ? r.data.map((f: any) => ({ namespace: f.namespace ?? p, name: f.name })) : []))))
      .then((lists) => { available = lists.flat().sort((a, b) => `${a.namespace}/${a.name}`.localeCompare(`${b.namespace}/${b.name}`)); });
  });

  $effect(() => {
    if (!doc.loaded && !doc.isNew) void workspace.load(doc);
  });

  // ── Graph: the trigger, and a node per target ──
  const nodeTypes = { target: TargetNode };
  const nodes = $derived.by<Node[]>(() => [
    {
      id: 'trigger', position: { x: 60, y: 140 }, type: 'default', sourcePosition: Position.Right,
      data: { label: `${t.name || 'trigger'} · ${cfg.type ?? '?'}` },
      style: 'background: var(--runtime-bg, #e6f4ef); border: 2px solid var(--runtime, #1a7158); border-radius: 10px; padding: 12px 20px; font-weight: 600; font-size: 13px; color: var(--runtime, #1a7158); min-width: 150px; text-align: center;',
    },
    ...targets.map((x, i) => ({
      id: `target-${i}`, type: 'target', position: { x: 400, y: 50 + i * 80 }, targetPosition: Position.Left,
      data: { label: x.type === 'workflow' ? `${x.namespace}/${x.name}` : x.type === 'agent' ? `agent:${x.agent_id}` : x.url ?? x.type, targetIdx: i, onremove: disconnect },
    })),
  ]);
  const edges = $derived<Edge[]>(targets.map((_, i) => ({
    id: `e-${i}`, source: 'trigger', target: `target-${i}`, type: 'smoothstep',
    style: 'stroke: var(--runtime, #1a7158); stroke-width: 2;',
  })));

  let dropping = $state(false);
  function ondragover(e: DragEvent) {
    const types = e.dataTransfer?.types ?? [];
    if (types.includes(ITEM_MIME) || types.includes('application/x-a3t-flow')) { e.preventDefault(); dropping = true; }
  }
  function ondrop(e: DragEvent) {
    dropping = false;
    const raw = e.dataTransfer?.getData(ITEM_MIME) || e.dataTransfer?.getData('application/x-a3t-flow');
    if (!raw) return;
    e.preventDefault();
    const it = JSON.parse(raw);
    if (it.kind && it.kind !== 'flow') return;
    connect(it.project ?? it.namespace, it.name);
  }
  function dragFlow(e: DragEvent, f: { namespace: string; name: string }) {
    e.dataTransfer?.setData('application/x-a3t-flow', JSON.stringify(f));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'link';
  }

  // ── Test: fire it as saved ──
  const payload = $derived(fromText(doc.ui.payload));
  const dirty = $derived(workspace.isDirty(doc));
  async function fire() {
    if (!kind.fire || doc.ui.firing || payload.error) return;
    doc.ui.firing = true; doc.ui.error = ''; doc.ui.fired = null;
    const r = await kind.fire(doc, payload.value);
    doc.ui.firing = false;
    if (r.ok) {
      doc.ui.fired = { how: r.data.how, at: new Date().toLocaleTimeString(), detail: JSON.stringify(r.data.detail, null, 2) };
      studio.reveal('bottom', 'target-runs');
    } else doc.ui.error = r.error;
  }
  const endpoint = $derived(cfg.type === 'webhook' ? `${cfg.method ?? 'POST'} /webhook${String(cfg.path ?? '').startsWith('/') ? '' : '/'}${cfg.path ?? ''}` : '');
</script>

<div class="tv">
  <div class="head">
    <span class="kind">{doc.isNew ? 'New trigger' : kind.label}</span>
    <span class="mono">{doc.ref.project}/{t.name || doc.ref.name}</span>
    {#if t.enabled === false}<span class="paused">paused</span>{/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    <SaveBar {doc} {workspace} valid={!parsed.error} />
  </div>

  {#if !doc.loaded && !doc.isNew}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else if parsed.error}
    <div class="err">The trigger is not valid JSON ({parsed.error}) — fix it with Edit.</div>
  {:else}
    <div class="form">
      <div class="col">
        <div class="sec">Identity</div>
        <label><span>Name</span><input value={t.name ?? ''} oninput={(e) => set(['name'], e.currentTarget.value)} placeholder="my-webhook" /></label>
        <label><span>Description</span><input value={t.description ?? ''} oninput={(e) => set(['description'], e.currentTarget.value)} placeholder="What this trigger does" /></label>
        <label class="chk"><input type="checkbox" checked={t.enabled !== false} onchange={(e) => set(['enabled'], e.currentTarget.checked)} /> Enabled</label>
      </div>
      <div class="col">
        <div class="sec">Channel</div>
        <div class="types">
          <button class:on={cfg.type === 'webhook'} onclick={() => channel('webhook')}><span class="ti w">W</span>Webhook</button>
          <button class:on={cfg.type === 'cron'} onclick={() => channel('cron')}><span class="ti c">C</span>Cron</button>
          <button class:on={cfg.type === 'event'} onclick={() => channel('event')}><span class="ti e">E</span>Event</button>
        </div>
        {#if cfg.type === 'webhook'}
          <div class="row2">
            <label><span>Path</span><input value={cfg.path ?? ''} oninput={(e) => set(['config', 'path'], e.currentTarget.value)} placeholder="/agent" /></label>
            <label><span>Method</span><select value={cfg.method ?? 'POST'} onchange={(e) => set(['config', 'method'], e.currentTarget.value)}><option>POST</option><option>GET</option><option>PUT</option></select></label>
          </div>
          {#if endpoint}<div class="ep"><span>Endpoint</span><code>{endpoint}</code></div>{/if}
        {:else if cfg.type === 'cron'}
          <div class="row2">
            <label><span>Expression</span><input value={cfg.expression ?? ''} oninput={(e) => set(['config', 'expression'], e.currentTarget.value)} placeholder="*/5 * * * *" /></label>
            <label><span>Timezone</span><input value={cfg.timezone ?? ''} oninput={(e) => set(['config', 'timezone'], e.currentTarget.value)} placeholder="UTC" /></label>
          </div>
          <label><span>Message</span><input value={cfg.message ?? ''} oninput={(e) => set(['config', 'message'], e.currentTarget.value)} placeholder="What each tick sends" /></label>
        {:else if cfg.type === 'event'}
          <label><span>Source (event type)</span><input value={cfg.source ?? ''} oninput={(e) => set(['config', 'source'], e.currentTarget.value)} placeholder="system.workflow.completed" /></label>
          <label><span>Filter</span><input value={cfg.filter ?? ''} oninput={(e) => set(['config', 'filter'], e.currentTarget.value || null)} placeholder="optional" /></label>
        {/if}
      </div>
    </div>

    <div class="design">
      <div class="graph" class:dropping {ondragover} ondragleave={() => (dropping = false)} {ondrop} role="application" aria-label="Targets">
        {#key `${targets.length}`}
          <SvelteFlow {nodes} {edges} {nodeTypes} fitView nodesDraggable={false}>
            <Background />
            <Controls showLock={false} />
          </SvelteFlow>
        {/key}
        {#if !targets.length}<div class="hint">Drag flows here — from the list, or from the Explorer</div>{/if}
      </div>
      <aside class="flows">
        <input class="search" placeholder="Filter {available.length} flows" bind:value={search} />
        <ul>
          {#each shown as f (`${f.namespace}/${f.name}`)}
            {@const linked = connected.has(`${f.namespace}/${f.name}`)}
            <li class:linked draggable={!linked} ondragstart={(e) => dragFlow(e, f)}>
              <span class="fi">ƒ</span><span class="fn">{f.name}</span><span class="fns">{f.namespace}</span>
              {#if linked}<span class="lk">linked</span>{:else}<button class="lnk" onclick={() => connect(f.namespace, f.name)} title="Connect">+</button>{/if}
            </li>
          {/each}
        </ul>
      </aside>
    </div>

    <div class="test">
      <div class="tbar">
        <span class="sec">Test</span>
        <span class="dim">{cfg.type === 'event' ? 'the event’s data' : cfg.type === 'webhook' ? 'the request body' : 'the flow’s input'} — fires the trigger as saved{dirty ? ' (save first to test your changes)' : ''}</span>
        <span class="grow"></span>
        <button class="btn spec sm" disabled={doc.isNew || doc.ui.firing || !!payload.error} onclick={fire}>{doc.ui.firing ? 'Firing…' : '⚡ Fire'}</button>
      </div>
      <div class="tbody">
        <div class="payload"><YamlEditor value={doc.ui.payload ?? ''} onchange={(v: string) => (doc.ui.payload = v)} /></div>
        <div class="out">
          {#if payload.error}<div class="err">Payload is not valid JSON: {payload.error}</div>{/if}
          {#if doc.ui.error}<div class="err">{doc.ui.error}</div>{/if}
          {#if doc.ui.fired}
            <div class="ok">Fired at {doc.ui.fired.at}: {doc.ui.fired.how}. Its runs show in Runs below.</div>
            <pre>{doc.ui.fired.detail}</pre>
          {:else if !doc.ui.error}
            <p class="dim">The answer shows here.</p>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .tv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .paused { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--warn-bg); color: var(--warn); }
  .grow { flex: 1; }
  .form { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .col { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .sec { font-size: 10.5px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); }
  label { display: flex; flex-direction: column; gap: 3px; font-size: 11px; color: var(--text-2); text-transform: none; letter-spacing: 0; font-weight: 500; }
  label.chk { flex-direction: row; align-items: center; gap: 6px; font-size: 12px; }
  input:not([type='checkbox']), select { padding: 6px 9px; font-size: 12.5px; font-family: var(--font-mono); }
  .row2 { display: grid; grid-template-columns: 2fr 1fr; gap: 8px; }
  .types { display: flex; gap: 4px; }
  .types button { display: flex; align-items: center; gap: 6px; padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); cursor: pointer; font-size: 12px; color: var(--text-2); }
  .types button.on { border-color: var(--runtime, #1a7158); color: var(--text-1); background: var(--runtime-bg, #e6f4ef); }
  .ti { width: 18px; height: 18px; border-radius: 4px; display: grid; place-items: center; font-size: 10px; font-weight: 700; }
  .ti.w { background: var(--runtime-bg); color: var(--runtime); } .ti.c { background: var(--control-bg); color: var(--control); } .ti.e { background: var(--signal-bg, #f0ebfa); color: var(--signal, #7d4bbb); }
  .ep { display: flex; gap: 8px; align-items: center; font-size: 11.5px; color: var(--text-3); }
  .ep code { font-family: var(--font-mono); color: var(--text-1); background: var(--bg-subtle); padding: 2px 6px; border-radius: 3px; }
  .design { flex: 1; min-height: 200px; overflow: hidden; display: grid; grid-template-columns: 1fr 240px; border-bottom: 1px solid var(--border); }
  .graph { position: relative; min-height: 0; }
  .graph.dropping { outline: 2px dashed var(--spec); outline-offset: -4px; }
  .hint { position: absolute; inset: 0; display: grid; place-items: center; color: var(--text-4); font-size: 13px; pointer-events: none; }
  .flows { border-left: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; }
  .search { margin: 8px; padding: 5px 8px; font-size: 12px; }
  .flows ul { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; }
  .flows li { display: flex; align-items: center; gap: 6px; padding: 5px 10px; font-size: 12px; cursor: grab; }
  .flows li:hover { background: var(--bg-subtle); }
  .flows li.linked { cursor: default; color: var(--text-4); }
  .fi { color: var(--spec); }
  .fn { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); }
  .fns { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-4); }
  .lk { font-size: 10px; color: var(--runtime, #1a7158); }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 14px; padding: 0 2px; }
  .test { flex: 0 0 210px; display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .tbody { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; }
  .payload { display: flex; border-right: 1px solid var(--border); min-height: 0; overflow: hidden; }
  .payload > :global(*) { flex: 1; }
  .out { overflow: auto; padding: 8px 12px; font-size: 12px; }
  .out pre { margin: 6px 0 0; font-family: var(--font-mono); font-size: 11.5px; white-space: pre-wrap; word-break: break-word; }
  .ok { color: var(--ok, #15803d); }
  .dim { color: var(--text-4); font-size: 12px; margin: 0; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { padding: 6px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; margin: 6px 0; }
</style>
