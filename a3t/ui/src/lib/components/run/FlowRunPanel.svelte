<script lang="ts">
  /// Run a flow from the editor and see its runs (console spec U1): an input
  /// dialog (a form for a flat `input.schema`, JSON otherwise), then a panel
  /// with the flow's recent runs and the selected run's input, output and error.
  /// Works on today's engine — execute_flow, get_execution_status and the
  /// per-flow execution list; step views come with the journal (U2).
  import { flows, executions } from '$lib/services/api';

  let { ns, name, schema = null, dialogOpen = $bindable(false) }:
    { ns: string; name: string; schema?: any; dialogOpen?: boolean } = $props();

  type Run = {
    instance_id: string; status: string; started_at?: string; completed_at?: string | null;
    duration_us?: number | null; input?: any; output?: any; error?: string | null; principal_id?: string | null;
  };

  let runs = $state<Run[]>([]);
  let selected = $state<Run | null>(null);
  let message = $state('');
  let starting = $state(false);
  let timer: ReturnType<typeof setInterval> | null = null;

  // ── Input ──
  const storeKey = $derived(`a3t.run.input.${ns}/${name}`);
  /// Flat object schemas get a form; anything else is edited as JSON.
  const fields = $derived.by(() => {
    const props = schema?.document?.properties ?? schema?.properties;
    if (!props || typeof props !== 'object') return null;
    const out: { key: string; type: string; required: boolean; description?: string; enum?: any[] }[] = [];
    const required: string[] = schema?.document?.required ?? schema?.required ?? [];
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

  function lastInput(): any {
    try { const v = localStorage.getItem(storeKey); return v ? JSON.parse(v) : null; } catch { return null; }
  }
  function remember(input: any) {
    try { localStorage.setItem(storeKey, JSON.stringify(input)); } catch { /* private window */ }
  }
  function seed(input: any) {
    inputText = JSON.stringify(input ?? {}, null, 2);
    formValues = { ...(input ?? {}) };
    inputError = '';
  }

  $effect(() => {
    if (!dialogOpen) return;
    mode = fields ? 'form' : 'json';
    seed(lastInput() ?? {});
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

  // ── Runs ──
  async function loadRuns(keepSelection = true) {
    const res = await executions.list({ namespace: ns, name, limit: 15 });
    const rows: Run[] = res.ok ? (Array.isArray(res.data) ? res.data : res.data?.rows ?? []) : [];
    runs = rows;
    if (keepSelection && selected) selected = rows.find((r) => r.instance_id === selected!.instance_id) ?? selected;
    else selected = rows[0] ?? null;
  }

  async function refreshSelected() {
    if (!selected) return;
    const res = await executions.get(selected.instance_id);
    if (res.ok && res.data && typeof res.data === 'object') {
      selected = { ...selected, ...res.data } as Run;
      runs = runs.map((r) => (r.instance_id === selected!.instance_id ? { ...r, ...selected } : r));
    }
  }

  const live = (r: Run | null) => !!r && ['Queued', 'Running', 'Suspended'].includes(r.status);
  $effect(() => {
    if (live(selected) && !timer) timer = setInterval(refreshSelected, 1000);
    if (!live(selected) && timer) { clearInterval(timer); timer = null; }
    return () => { if (timer) { clearInterval(timer); timer = null; } };
  });

  $effect(() => { if (ns && name) void loadRuns(false); });

  async function run() {
    const input = collectInput();
    if (input === undefined) return;
    starting = true; message = '';
    remember(input);
    const res = await flows.execute(name, ns, input);
    starting = false;
    if (!res.ok) { message = res.error ?? 'Run failed to start'; return; }
    const text = typeof res.data === 'string' ? res.data : JSON.stringify(res.data ?? '');
    const id = text.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/)?.[0];
    dialogOpen = false;
    if (!id) { message = text.slice(0, 300); return; }
    selected = { instance_id: id, status: 'Running', input };
    await refreshSelected();
    await loadRuns(true);
  }

  // ── Formatting ──
  function duration(r: Run): string {
    if (r.duration_us != null) {
      const ms = r.duration_us / 1000;
      return ms < 1000 ? `${ms.toFixed(ms < 10 ? 1 : 0)} ms` : `${(ms / 1000).toFixed(2)} s`;
    }
    if (live(r) && r.started_at) return `${((Date.now() - Date.parse(r.started_at)) / 1000).toFixed(0)} s…`;
    return '—';
  }
  const when = (t?: string) => (t ? new Date(t).toLocaleString() : '—');
  const pretty = (v: any) => (v === undefined || v === null ? '—' : typeof v === 'string' ? v : JSON.stringify(v, null, 2));
  const statusClass = (s: string) => (s === 'Completed' ? 'ok' : s === 'Failed' ? 'err' : s === 'Cancelled' ? 'mut' : 'live');
</script>

<section class="rp">
  <div class="rp-list">
    <div class="rp-head">
      <span>Runs</span>
      <button class="rp-link" onclick={() => loadRuns(true)} title="Refresh">↻</button>
    </div>
    {#if runs.length === 0}
      <div class="rp-empty">No runs yet. ▶ Run to start one.</div>
    {/if}
    {#each runs as r (r.instance_id)}
      <button class="rp-row" class:sel={selected?.instance_id === r.instance_id} onclick={() => { selected = r; void refreshSelected(); }}>
        <span class="dot {statusClass(r.status)}"></span>
        <span class="rp-when">{when(r.started_at)}</span>
        <span class="rp-dur">{duration(r)}</span>
      </button>
    {/each}
  </div>

  <div class="rp-detail">
    {#if message}<div class="rp-msg">{message}</div>{/if}
    {#if selected}
      <div class="rp-meta">
        <span class="pill {statusClass(selected.status)}">{selected.status}</span>
        <span class="mono">{selected.instance_id}</span>
        <span>{duration(selected)}</span>
        {#if selected.principal_id}<span>by {selected.principal_id}</span>{/if}
        <span style="flex:1"></span>
        <button class="rp-link" onclick={() => { seed(selected?.input); dialogOpen = true; mode = 'json'; }}>Run again with this input</button>
      </div>
      <div class="rp-cols">
        <div><h4>Input</h4><pre>{pretty(selected.input)}</pre></div>
        <div>
          {#if selected.error}<h4 class="err-h">Error</h4><pre class="err">{selected.error}</pre>{/if}
          <h4>Output</h4><pre>{pretty(selected.output)}</pre>
        </div>
      </div>
      <p class="rp-note">Step-by-step views (inputs and outputs per step, breakpoints) arrive with the execution journal.</p>
    {:else}
      <div class="rp-empty">Select a run.</div>
    {/if}
  </div>
</section>

{#if dialogOpen}
  <div class="rd-overlay" role="presentation" onclick={() => (dialogOpen = false)}></div>
  <div class="rd" role="dialog" aria-label="Run flow">
    <h3>Run <span class="mono">{ns}/{name}</span></h3>
    <div class="rd-tabs">
      {#if fields}<button class:on={mode === 'form'} onclick={() => (mode = 'form')}>Form</button>{/if}
      <button class:on={mode === 'json'} onclick={() => { if (mode === 'form') { const v = collectInput(); if (v !== undefined) seed(v); } mode = 'json'; }}>JSON</button>
      <span style="flex:1"></span>
      {#if runs.length}
        <select onchange={(e) => { const r = runs.find((x) => x.instance_id === (e.currentTarget as HTMLSelectElement).value); if (r) seed(r.input); }}>
          <option value="">Use input of a run…</option>
          {#each runs as r (r.instance_id)}<option value={r.instance_id}>{when(r.started_at)} · {r.status}</option>{/each}
        </select>
      {/if}
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
      <textarea rows="12" spellcheck="false" bind:value={inputText}></textarea>
    {/if}
    {#if inputError}<div class="rp-msg">{inputError}</div>{/if}
    <div class="rd-row">
      <button class="btn ghost sm" onclick={() => (dialogOpen = false)}>Cancel</button>
      <button class="btn spec sm" disabled={starting} onclick={run}>{starting ? 'Starting…' : '▶ Run'}</button>
    </div>
  </div>
{/if}

<style>
  .rp { display: flex; height: 100%; min-height: 0; border-top: 1px solid var(--border); background: var(--bg-surface); font-size: 12.5px; }
  .rp-list { width: 260px; flex-shrink: 0; border-right: 1px solid var(--border); overflow-y: auto; }
  .rp-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; font-weight: 600; color: var(--text-2); }
  .rp-row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; color: var(--text-2); }
  .rp-row:hover { background: var(--bg-subtle); }
  .rp-row.sel { background: var(--bg-subtle); color: var(--text-1); }
  .rp-when { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rp-dur { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--text-4); }
  .dot.ok { background: var(--ok, #15803d); } .dot.err { background: var(--err, #b42318); } .dot.live { background: var(--spec); animation: pulse 1s infinite; }
  @keyframes pulse { 50% { opacity: 0.4; } }
  .rp-detail { flex: 1; min-width: 0; overflow-y: auto; padding: 10px 14px; }
  .rp-meta { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; color: var(--text-3); margin-bottom: 8px; }
  .pill { padding: 2px 8px; border-radius: 10px; font-size: 11px; background: var(--bg-subtle); color: var(--text-2); }
  .pill.ok { background: var(--ok-bg, #e7f6ec); color: var(--ok, #15803d); } .pill.err { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); } .pill.live { background: var(--spec-bg); color: var(--spec); }
  .rp-cols { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  @media (max-width: 800px) { .rp-cols { grid-template-columns: 1fr; } .rp-list { width: 180px; } }
  h4 { margin: 6px 0 4px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .err-h { color: var(--err, #b42318); }
  pre { margin: 0; padding: 8px 10px; background: var(--bg-page); border: 1px solid var(--border); border-radius: var(--r-sm); font-family: var(--font-mono); font-size: 11.5px; white-space: pre-wrap; word-break: break-word; max-height: 260px; overflow: auto; }
  pre.err { color: var(--err, #b42318); }
  .rp-note { margin: 10px 0 0; font-size: 11.5px; color: var(--text-4); }
  .rp-empty { padding: 12px; color: var(--text-4); }
  .rp-msg { padding: 6px 10px; margin-bottom: 8px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .rp-link { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; }
  .mono { font-family: var(--font-mono); }
  .rd-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.15); z-index: 200; }
  .rd { position: fixed; top: 12vh; left: 50%; transform: translateX(-50%); width: min(560px, calc(100vw - 32px)); z-index: 201;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); box-shadow: var(--shadow-lg);
        padding: 18px 20px; display: flex; flex-direction: column; gap: 10px; }
  .rd h3 { margin: 0; font-family: var(--font-serif); font-size: 18px; font-weight: 500; }
  .rd-tabs { display: flex; align-items: center; gap: 6px; }
  .rd-tabs button { border: 1px solid var(--border); background: var(--bg-surface); border-radius: var(--r-sm); padding: 4px 10px; font-size: 12px; cursor: pointer; }
  .rd-tabs button.on { background: var(--spec-bg); color: var(--spec); border-color: var(--spec); }
  .rd select, .rd input, .rd textarea { padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 12.5px; }
  .rd textarea { font-family: var(--font-mono); resize: vertical; }
  .rd-form { display: flex; flex-direction: column; gap: 8px; max-height: 50vh; overflow-y: auto; }
  .rd-form label { display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-3); }
  .rd-form small { color: var(--text-4); }
  .rd-row { display: flex; justify-content: flex-end; gap: 8px; }
</style>
