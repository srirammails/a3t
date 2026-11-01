<script lang="ts">
  /// Run and debug a flow from the editor (console spec §4–§5): an input dialog
  /// (a form for a flat `input.schema`, JSON otherwise), a Debug mode with
  /// breakpoints, and the flow's runs — newest first — with the selected run
  /// shown live in [`RunView`]. Breakpoints are yours: kept per flow in this
  /// browser.
  import { untrack } from 'svelte';
  import RunView from './RunView.svelte';
  import { flows, runsApi, executions } from '$lib/services/api';
  import { flowDebug } from './debugState.svelte.ts';

  let { ns, name, schema = null, steps = [], dialogOpen = $bindable(false), debugMode = $bindable(false) }:
    { ns: string; name: string; schema?: any; steps?: string[]; dialogOpen?: boolean; debugMode?: boolean } = $props();

  type Row = { run_id: string; phase: string; status?: string; created_at?: string; duration_us?: number | null; debug?: boolean; principal_id?: string | null };

  let runs = $state<Row[]>([]);
  let next = $state<string | null>(null);
  let selected = $state<string | null>(null);
  let message = $state('');
  let starting = $state(false);
  /// Servers before the run store answer with the execution list instead.
  let legacy = $state(false);

  // ── Input ──
  const storeKey = $derived(`a3t.run.input.${ns}/${name}`);
  /// Breakpoints and the shown run, shared with the editor graph.
  const shared = $derived(flowDebug(ns, name));
  const fields = $derived.by(() => {
    const props = schema?.document?.properties ?? schema?.properties;
    if (!props || typeof props !== 'object') return null;
    const required: string[] = schema?.document?.required ?? schema?.required ?? [];
    const out: { key: string; type: string; required: boolean; description?: string; enum?: any[] }[] = [];
    for (const [key, p] of Object.entries<any>(props)) {
      if (!['string', 'number', 'integer', 'boolean'].includes(p?.type)) return null;
      out.push({ key, type: p.type, required: required.includes(key), description: p.description, enum: p.enum });
    }
    return out;
  });
  let mode = $state<'form' | 'json'>('json');
  let inputText = $state('{}');
  let formValues = $state<Record<string, any>>({});
  let inputError = $state('');
  let pauseOnStart = $state(false);

  function read<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback; } catch { return fallback; }
  }
  function write(key: string, v: any) {
    try { localStorage.setItem(key, JSON.stringify(v)); } catch { /* private window */ }
  }
  function seed(input: any) {
    inputText = JSON.stringify(input ?? {}, null, 2);
    formValues = { ...(input ?? {}) };
    inputError = '';
  }

  $effect(() => {
    if (!dialogOpen) return;
    const f = fields, sk = storeKey;
    untrack(() => {
      mode = f ? 'form' : 'json';
      seed(read(sk, {}));
    });
  });

  function collectInput(): any | undefined {
    inputError = '';
    if (mode === 'form' && fields) {
      const out: Record<string, any> = {};
      for (const f of fields) {
        const v = formValues[f.key];
        if (v === undefined || v === '') {
          if (f.required) { inputError = `${f.key} is required`; return undefined; }
          continue;
        }
        out[f.key] = f.type === 'number' || f.type === 'integer' ? Number(v) : v;
      }
      return out;
    }
    try { return JSON.parse(inputText || '{}'); } catch (e) { inputError = `Input is not valid JSON: ${(e as Error).message}`; return undefined; }
  }

  // ── Breakpoints ──
  const isSet = (k: string) => shared.has(k);
  const toggle = (k: string) => void shared.toggle(k);

  // ── Runs ──
  async function loadRuns(more = false) {
    const r = await runsApi.list({ namespace: ns, name, limit: 25, ...(more && next ? { cursor: next } : {}) });
    if (r.ok && Array.isArray(r.data?.runs)) {
      legacy = false;
      runs = more ? [...runs, ...r.data.runs] : r.data.runs;
      next = r.data.next ?? null;
    } else {
      // An older server: the execution list (no step views).
      legacy = true;
      const e = await executions.list({ namespace: ns, name, limit: 25 });
      const rows = e.ok ? (Array.isArray(e.data) ? e.data : e.data?.rows ?? []) : [];
      runs = rows.map((x: any) => ({ run_id: x.instance_id, phase: (x.status ?? '').toLowerCase(), created_at: x.started_at, duration_us: x.duration_us, principal_id: x.principal_id }));
      next = null;
    }
    if (!more && !selected) selected = runs[0]?.run_id ?? null;
  }

  let poll: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    if (!(ns && name)) return;
    untrack(() => {
      selected = null;
      void loadRuns();
    });
    poll = setInterval(() => void loadRuns(), 5000);
    return () => { if (poll) clearInterval(poll); };
  });

  // No run shown (or an older server without step views): nothing on the graph.
  $effect(() => {
    const fd = shared;
    if (!selected || legacy) untrack(() => fd.show(null));
  });

  async function run() {
    const input = collectInput();
    if (input === undefined) return;
    starting = true; message = '';
    write(storeKey, input);
    if (debugMode) shared.save();
    const breakpoints = $state.snapshot(shared.breakpoints);
    let id: string | undefined;
    if (!legacy) {
      const r = await runsApi.start({ namespace: ns, name, input, debug: debugMode, ...(debugMode ? { breakpoints, pause_on_start: pauseOnStart } : {}) });
      if (r.ok) id = r.data?.run_id;
      else if (!/unknown tool|not found|Unknown/i.test(r.error ?? '')) { starting = false; message = r.error ?? 'Run failed to start'; return; }
    }
    if (!id) {
      const res = await flows.execute(name, ns, input);
      if (!res.ok) { starting = false; message = res.error ?? 'Run failed to start'; return; }
      const text = typeof res.data === 'string' ? res.data : JSON.stringify(res.data ?? '');
      id = text.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/)?.[0];
    }
    starting = false;
    dialogOpen = false;
    if (id) selected = id;
    await loadRuns();
  }

  const when = (t?: string) => (t ? new Date(t).toLocaleString() : '—');
  const dot = (p: string) => (p === 'completed' ? 'ok' : p === 'faulted' || p === 'failed' ? 'err' : p === 'cancelled' ? 'mut' : p === 'suspended' ? 'warn' : 'live');
  function dur(us?: number | null) {
    if (us == null) return '';
    const ms = us / 1000;
    return ms < 1000 ? `${ms.toFixed(ms < 10 ? 1 : 0)} ms` : `${(ms / 1000).toFixed(2)} s`;
  }
</script>

<section class="rp">
  <div class="rp-list">
    <div class="rp-head"><span>Runs</span><button class="rp-link" onclick={() => loadRuns()} title="Refresh">↻</button></div>
    {#if runs.length === 0}<div class="rp-empty">No runs yet. ▶ Run or 🐞 Debug to start one.</div>{/if}
    {#each runs as r (r.run_id)}
      <button class="rp-row" class:sel={selected === r.run_id} onclick={() => (selected = r.run_id)}>
        <span class="dot {dot(r.phase)}"></span>
        <span class="rp-when">{when(r.created_at)}</span>
        {#if r.debug}<span class="tag">debug</span>{/if}
        <span class="rp-dur">{r.phase === 'suspended' ? 'paused' : r.phase === 'waiting' ? 'waiting' : dur(r.duration_us)}</span>
      </button>
    {/each}
    {#if next}<button class="rp-link more" onclick={() => loadRuns(true)}>Older runs…</button>{/if}
  </div>
  <div class="rp-detail">
    {#if message}<div class="rp-msg">{message}</div>{/if}
    {#if selected && !legacy}
      <RunView runId={selected} flowSteps={steps} {shared} onrerun={(id) => { selected = id; void loadRuns(); }} />
    {:else if selected}
      <div class="rp-empty">This server keeps no step journal yet; step views need the updated FlowR.</div>
    {:else}
      <div class="rp-empty">Select a run.</div>
    {/if}
  </div>
</section>

{#if dialogOpen}
  <div class="rd-overlay" role="presentation" onclick={() => (dialogOpen = false)}></div>
  <div class="rd" role="dialog" aria-label="Run flow">
    <h3>{debugMode ? 'Debug' : 'Run'} <span class="mono">{ns}/{name}</span></h3>
    <div class="rd-tabs">
      {#if fields}<button class:on={mode === 'form'} onclick={() => (mode = 'form')}>Form</button>{/if}
      <button class:on={mode === 'json'} onclick={() => { if (mode === 'form') { const v = collectInput(); if (v !== undefined) seed(v); } mode = 'json'; }}>JSON</button>
      <span style="flex:1"></span>
      <label class="rd-check"><input type="checkbox" bind:checked={debugMode} /> Debug</label>
    </div>
    {#if mode === 'form' && fields}
      <div class="rd-form">
        {#each fields as f (f.key)}
          <label>
            <span>{f.key}{f.required ? ' *' : ''}</span>
            {#if f.type === 'boolean'}
              <input type="checkbox" bind:checked={formValues[f.key]} />
            {:else if f.enum}
              <select bind:value={formValues[f.key]}>{#each f.enum as o}<option value={o}>{o}</option>{/each}</select>
            {:else}
              <input type={f.type === 'string' ? 'text' : 'number'} bind:value={formValues[f.key]} />
            {/if}
            {#if f.description}<small>{f.description}</small>{/if}
          </label>
        {/each}
      </div>
    {:else}
      <textarea rows="9" spellcheck="false" bind:value={inputText}></textarea>
    {/if}
    {#if debugMode}
      <div class="rd-bps">
        <div class="rd-bps-head">
          <span>Breakpoints <span class="dim">— pause before these steps</span></span>
          <label class="rd-check"><input type="checkbox" bind:checked={pauseOnStart} /> Pause on start</label>
        </div>
        {#if steps.length === 0}<div class="dim">This flow has no steps to break on.</div>{/if}
        <div class="rd-bp-list">
          {#each steps as k (k)}
            <div class="rd-bp" class:on={isSet(k)}>
              <button class="bp-dot" onclick={() => toggle(k)} title={isSet(k) ? 'Remove breakpoint' : 'Add breakpoint'}>{isSet(k) ? '●' : '○'}</button>
              <span class="mono">{k}</span>
              {#if isSet(k)}
                {@const bp = shared.breakpoints.find((b) => b.step_key === k)!}
                <select bind:value={bp.when}><option value="before">before</option><option value="after">after</option><option value="on_error">on error</option></select>
                <input class="cond" placeholder="condition (jq on the step input)" bind:value={bp.condition} />
              {/if}
            </div>
          {/each}
        </div>
        <div class="dim small">A debug run executes real effects. It pauses durably — you can close this page and come back.</div>
      </div>
    {/if}
    {#if inputError}<div class="rp-msg">{inputError}</div>{/if}
    <div class="rd-row">
      {#if runs.length}
        <select onchange={async (e) => { const id = (e.currentTarget as HTMLSelectElement).value; if (!id) return; const r = await runsApi.get(id); if (r.ok) seed(r.data.input); }}>
          <option value="">Use input of a run…</option>
          {#each runs as r (r.run_id)}<option value={r.run_id}>{when(r.created_at)} · {r.phase}</option>{/each}
        </select>
      {/if}
      <span style="flex:1"></span>
      <button class="btn ghost sm" onclick={() => (dialogOpen = false)}>Cancel</button>
      <button class="btn spec sm" disabled={starting} onclick={run}>{starting ? 'Starting…' : debugMode ? '🐞 Debug' : '▶ Run'}</button>
    </div>
  </div>
{/if}

<style>
  .rp { display: flex; height: 100%; min-height: 0; border-top: 1px solid var(--border); background: var(--bg-surface); font-size: 12.5px; }
  .rp-list { width: 250px; flex-shrink: 0; border-right: 1px solid var(--border); overflow-y: auto; }
  .rp-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; font-weight: 600; color: var(--text-2); }
  .rp-row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; color: var(--text-2); }
  .rp-row:hover, .rp-row.sel { background: var(--bg-subtle); color: var(--text-1); }
  .rp-when { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rp-dur { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .tag { font-size: 10px; padding: 0 5px; border-radius: 6px; background: var(--bg-subtle); color: var(--text-3); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--text-4); }
  .dot.ok { background: var(--ok, #15803d); } .dot.err { background: var(--err, #b42318); } .dot.warn { background: var(--warn, #b45309); }
  .dot.live { background: var(--spec); animation: pulse 1s infinite; }
  @keyframes pulse { 50% { opacity: 0.4; } }
  .rp-detail { flex: 1; min-width: 0; overflow-y: auto; padding: 10px 14px; }
  .rp-empty { padding: 12px; color: var(--text-4); }
  .rp-msg { padding: 6px 10px; margin-bottom: 8px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .rp-link { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; }
  .rp-link.more { padding: 8px 12px; }
  .mono { font-family: var(--font-mono); }
  .dim { color: var(--text-3); } .small { font-size: 11.5px; }
  @media (max-width: 800px) { .rp-list { width: 170px; } }
  .rd-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.15); z-index: 200; }
  .rd { position: fixed; top: 8vh; left: 50%; transform: translateX(-50%); width: min(640px, calc(100vw - 32px)); max-height: 84vh; overflow-y: auto; z-index: 201;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); box-shadow: var(--shadow-lg);
        padding: 18px 20px; display: flex; flex-direction: column; gap: 10px; }
  .rd h3 { margin: 0; font-family: var(--font-serif); font-size: 18px; font-weight: 500; }
  .rd-tabs { display: flex; align-items: center; gap: 6px; }
  .rd-tabs button { border: 1px solid var(--border); background: var(--bg-surface); border-radius: var(--r-sm); padding: 4px 10px; font-size: 12px; cursor: pointer; }
  .rd-tabs button.on { background: var(--spec-bg); color: var(--spec); border-color: var(--spec); }
  .rd-check { display: flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-2); }
  .rd select, .rd input, .rd textarea { padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 12.5px; }
  .rd textarea { font-family: var(--font-mono); resize: vertical; }
  .rd-form { display: flex; flex-direction: column; gap: 8px; max-height: 40vh; overflow-y: auto; }
  .rd-form label { display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-3); }
  .rd-bps { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--border); border-radius: var(--r-sm); }
  .rd-bps-head { display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .rd-bp-list { display: flex; flex-direction: column; gap: 2px; max-height: 30vh; overflow-y: auto; }
  .rd-bp { display: flex; align-items: center; gap: 6px; }
  .rd-bp .bp-dot { border: 0; background: transparent; cursor: pointer; color: var(--text-4); font-size: 13px; width: 18px; }
  .rd-bp.on .bp-dot { color: var(--err, #b42318); }
  .rd-bp select { padding: 2px 4px; font-size: 11.5px; }
  .rd-bp .cond { flex: 1; padding: 2px 6px; font-size: 11.5px; font-family: var(--font-mono); }
  .rd-row { display: flex; align-items: center; gap: 8px; }
</style>
