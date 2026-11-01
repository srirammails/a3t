<script lang="ts">
  /// One run, live (console spec §6–§8): its status and what it did step by
  /// step, the journal, a timeline, resume points to re-run from — and, for a
  /// debug run paused at a breakpoint, the controls to continue, step, or edit
  /// the next step's input. Reads the engine's run tools; polls while live.
  import { untrack } from 'svelte';
  import { runsApi, type DebugCommand } from '$lib/services/api';
  import { nodeKey, type FlowDebug } from './debugState.svelte.ts';

  /// `shared`: the flow's state shared with the editor graph — this view
  /// publishes the run's position to it and shows the step picked there.
  let { runId, flowSteps = [], onrerun, shared = null }:
    { runId: string; flowSteps?: string[]; onrerun?: (id: string) => void; shared?: FlowDebug | null } = $props();

  type Step = { step_key: string; status: string; class?: string; duration_us?: number; attempts?: number; edited?: boolean;
                started_at?: string; completed_at?: string; seq?: number; error?: any; fire_at?: string };
  type Entry = { seq: number; at: string; epoch: number; record: any };

  let run = $state<any>(null);
  let entries = $state<Entry[]>([]);
  let error = $state('');
  let message = $state('');
  let tab = $state<'steps' | 'timeline' | 'journal' | 'checkpoints' | 'io'>('steps');
  let selected = $state<string | null>(null);
  let detailTab = $state<'input' | 'output' | 'context' | 'records'>('output');
  let editing = $state(false);
  let editText = $state('');
  let busy = $state(false);
  let runTo = $state('');

  const LIVE = ['pending', 'running', 'waiting', 'suspended'];
  const live = $derived(!!run && LIVE.includes(run.phase));

  async function load() {
    const [r, j] = await Promise.all([runsApi.get(runId), runsApi.journal(runId, entries.length ? entries[entries.length - 1].seq : 0)]);
    if (!r.ok) { error = r.error ?? 'Could not load the run'; return; }
    error = '';
    run = r.data;
    shared?.show(r.data);
    if (j.ok && Array.isArray(j.data?.entries) && j.data.entries.length) entries = [...entries, ...j.data.entries];
  }

  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const id = runId;
    run = null; entries = []; selected = null; message = ''; editing = false;
    let stop = false;
    const tick = async () => {
      if (stop || id !== runId) return;
      await load();
      if (stop) return;
      const wait = !run ? 1500 : run.phase === 'suspended' ? 3000 : LIVE.includes(run.phase) ? 1000 : 0;
      if (wait) timer = setTimeout(tick, wait);
    };
    // Untracked: the poll reads and writes `run`/`entries`, which must not
    // re-trigger this effect — only a different run does.
    untrack(() => void tick());
    return () => { stop = true; if (timer) clearTimeout(timer); };
  });

  // ── Steps ──
  const steps = $derived<Step[]>(run?.steps ?? []);
  const paused = $derived(run?.phase === 'suspended' ? run?.paused : null);
  const rows = $derived.by(() => {
    const out = [...steps];
    if (paused && !out.some((s) => s.step_key === paused.step_key)) out.push({ step_key: paused.step_key, status: 'paused' });
    return out;
  });
  // A step picked on the graph: show its latest iteration here.
  $effect(() => {
    const want = shared?.focus;
    if (!want) return;
    const hit = [...rows].reverse().find((s) => nodeKey(s.step_key) === want);
    untrack(() => {
      tab = 'steps';
      selected = hit?.step_key ?? null;
      message = hit ? '' : `${want} has not run in this run.`;
      shared!.focus = null;
    });
  });

  function recordsOf(key: string): Entry[] {
    return entries.filter((e) => e.record?.step_key === key || e.record?.wait_id === key);
  }
  const last = (key: string) => [...recordsOf(key)].reverse().find((e) => ['step_completed', 'step_failed'].includes(e.record.kind))?.record;
  /// The context in effect after a step: its own, or the last one recorded before it.
  function contextAfter(key: string): any {
    let ctx: any = {};
    for (const e of entries) {
      if (e.record.kind === 'step_completed' && e.record.context !== null && e.record.context !== undefined) ctx = e.record.context;
      if (e.record.step_key === key && e.record.kind === 'step_completed') return ctx;
    }
    return null;
  }
  function icon(s: Step): string {
    if (paused?.step_key === s.step_key) return '⏸';
    return { completed: '✓', failed: '✗', started: '●', waiting: '⏳', paused: '⏸' }[s.status] ?? '·';
  }

  // ── Debug ──
  async function command(c: DebugCommand, stepKey?: string) {
    busy = true; message = '';
    const r = await runsApi.debug(runId, c, stepKey);
    busy = false;
    if (!r.ok) message = r.error ?? 'Command failed';
    await load();
  }
  function openEdit() {
    editText = JSON.stringify(paused?.input ?? {}, null, 2);
    editing = true;
  }
  async function applyEdit() {
    let input: any;
    try { input = JSON.parse(editText); } catch (e) { message = `Not valid JSON: ${(e as Error).message}`; return; }
    busy = true;
    const r = await runsApi.editInput(runId, paused.step_key, input);
    busy = false;
    if (!r.ok) { message = r.error ?? 'Edit failed'; return; }
    editing = false;
    message = `Input for ${paused.step_key} replaced — Continue or Step to run it.`;
    await load();
  }
  async function cancel() {
    busy = true;
    const r = await runsApi.cancel(runId, 'cancelled from the console');
    busy = false;
    if (!r.ok) message = r.error ?? 'Cancel failed';
    await load();
  }
  async function rerun(debug: boolean, fromSeq?: number) {
    busy = true; message = '';
    const r = await runsApi.rerun(runId, { debug, ...(fromSeq ? { from_seq: fromSeq } : {}) });
    busy = false;
    if (!r.ok) { message = r.error ?? 'Re-run failed'; return; }
    onrerun?.(r.data.run_id);
  }

  // ── Timeline ──
  const span = $derived.by(() => {
    const ts = steps.flatMap((s) => [s.started_at, s.completed_at, s.fire_at]).filter(Boolean).map((t) => Date.parse(t as string));
    if (!ts.length) return null;
    const start = Math.min(...ts);
    const end = Math.max(...ts, live ? Date.now() : 0);
    return { start, total: Math.max(end - start, 1) };
  });
  function bar(s: Step) {
    if (!span || !s.started_at) return 'display:none';
    const a = Date.parse(s.started_at);
    const b = s.completed_at ? Date.parse(s.completed_at) : s.fire_at ? Date.parse(s.fire_at) : Date.now();
    return `left:${((a - span.start) / span.total) * 100}%;width:${Math.max(((b - a) / span.total) * 100, 0.6)}%`;
  }

  // ── Formatting ──
  const pretty = (v: any) => (v === undefined || v === null ? '—' : typeof v === 'string' ? v : JSON.stringify(v, null, 2));
  function dur(us?: number | null): string {
    if (us === undefined || us === null) return '';
    const ms = us / 1000;
    return ms < 1 ? `${us} µs` : ms < 1000 ? `${ms.toFixed(ms < 10 ? 1 : 0)} ms` : `${(ms / 1000).toFixed(2)} s`;
  }
  const when = (t?: string) => (t ? new Date(t).toLocaleTimeString() : '');
  const phaseClass = (p: string) => (p === 'completed' ? 'ok' : p === 'faulted' ? 'err' : p === 'cancelled' ? 'mut' : p === 'suspended' ? 'warn' : 'live');
</script>

{#if error}
  <div class="rv-msg">{error}</div>
{:else if !run}
  <div class="rv-empty">Loading run…</div>
{:else}
  <div class="rv">
    <div class="rv-head">
      <span class="pill {phaseClass(run.phase)}">{run.phase}</span>
      {#if run.debug}<span class="pill mut">debug</span>{/if}
      <span class="mono dim" title="Run id">{run.run_id}</span>
      {#if run.duration_us}<span>{dur(run.duration_us)}</span>{/if}
      {#if run.principal_id}<span class="dim">by {run.principal_id}</span>{/if}
      {#if run.resume_count > 0}<span class="dim" title="Resumed after an interruption, a timer or a pause">resumed {run.resume_count}×</span>{/if}
      {#if run.current_step}<span class="dim">at <span class="mono">{run.current_step}</span></span>{/if}
      <span class="grow"></span>
      {#if live && run.can_steer}<button class="lnk" disabled={busy} onclick={cancel}>Cancel</button>{/if}
      {#if !live && run.can_steer}
        <button class="lnk" disabled={busy} onclick={() => rerun(false)}>Re-run</button>
        <button class="lnk" disabled={busy} onclick={() => rerun(true)}>Re-run in debug</button>
      {/if}
    </div>

    {#if paused}
      <div class="rv-pause">
        <div>
          ⏸ Paused {paused.when === 'before' ? 'before' : paused.when === 'after' ? 'after' : 'on an error in'}
          <span class="mono">{paused.step_key}</span> <span class="dim">({paused.reason})</span>
        </div>
        {#if run.can_steer && run.debug}
          <div class="rv-ctl">
            <button class="btn spec sm" disabled={busy} onclick={() => command('continue')}>▶ Continue</button>
            <button class="btn ghost sm" disabled={busy} onclick={() => command('step_over')} title="Run this step, pause before the next">⤼ Step over</button>
            <button class="btn ghost sm" disabled={busy} onclick={() => command('step_into')} title="Pause before the very next step, inside this one if it has steps">⤵ Step into</button>
            <button class="btn ghost sm" disabled={busy} onclick={() => command('step_out')} title="Finish the enclosing block, pause after it">⤴ Step out</button>
            {#if flowSteps.length}
              <select bind:value={runTo} title="Run until this step">
                <option value="">Run to…</option>
                {#each flowSteps as k (k)}<option value={k}>{k}</option>{/each}
              </select>
              <button class="btn ghost sm" disabled={busy || !runTo} onclick={() => command('run_to', runTo)}>⇥</button>
            {/if}
            {#if paused.when === 'before'}<button class="btn ghost sm" disabled={busy} onclick={openEdit}>✎ Edit input</button>{/if}
            <button class="btn ghost sm" disabled={busy} onclick={() => command('stop')}>■ Stop</button>
          </div>
        {/if}
      </div>
    {/if}
    {#if run.phase === 'waiting'}<div class="rv-note">⏳ Waiting on a timer — no worker is held; it wakes on time, on any worker.</div>{/if}
    {#if message}<div class="rv-msg">{message}</div>{/if}
    {#if run.error}<div class="rv-msg">{run.error}</div>{/if}

    {#if editing}
      <div class="rv-edit">
        <div class="dim">Input for <span class="mono">{paused?.step_key}</span> — the step runs with this instead. The change is recorded with your name.</div>
        <textarea rows="8" spellcheck="false" bind:value={editText}></textarea>
        <div class="rv-row">
          <button class="btn ghost sm" onclick={() => (editing = false)}>Cancel</button>
          <button class="btn spec sm" disabled={busy} onclick={applyEdit}>Apply</button>
        </div>
      </div>
    {/if}

    <div class="rv-tabs">
      {#each [['steps', `Steps (${rows.length})`], ['timeline', 'Timeline'], ['journal', `Journal (${entries.length})`], ['checkpoints', 'Checkpoints'], ['io', 'Input / Output']] as [k, label] (k)}
        <button class:on={tab === k} onclick={() => (tab = k as typeof tab)}>{label}</button>
      {/each}
    </div>

    {#if tab === 'steps'}
      <div class="rv-steps">
        <div class="rv-list">
          {#if rows.length === 0}<div class="rv-empty">No steps recorded{run.durability === 'ephemeral' ? ' — an ephemeral flow keeps no journal' : ''}.</div>{/if}
          {#each rows as s (s.step_key)}
            <button class="rv-step" class:sel={selected === s.step_key} class:paused={paused?.step_key === s.step_key}
              onclick={() => { selected = s.step_key; detailTab = s.status === 'failed' ? 'output' : detailTab; }}>
              <span class="ic {s.status}">{icon(s)}</span>
              <span class="mono key">{s.step_key}</span>
              {#if s.edited}<span title="Input edited while paused">✎</span>{/if}
              {#if (s.attempts ?? 0) > 1}<span class="badge" title="Attempts">×{s.attempts}</span>{/if}
              <span class="dur">{dur(s.duration_us)}</span>
            </button>
          {/each}
        </div>
        <div class="rv-detail">
          {#if selected}
            {@const rec = last(selected)}
            <div class="rv-dtabs">
              {#each ['input', 'output', 'context', 'records'] as k (k)}
                <button class:on={detailTab === k} onclick={() => (detailTab = k as typeof detailTab)}>{k}</button>
              {/each}
              {#if rec?.class}<span class="dim small">class: {rec.class}</span>{/if}
            </div>
            {#if detailTab === 'input'}
              <pre>{pretty(rec?.input ?? (paused?.step_key === selected ? paused?.input : undefined))}</pre>
            {:else if detailTab === 'output'}
              {#if rec?.kind === 'step_failed'}<pre class="err">{pretty(rec.error)}</pre>{:else}<pre>{pretty(rec?.output)}</pre>{/if}
            {:else if detailTab === 'context'}
              <pre>{pretty(contextAfter(selected))}</pre>
            {:else}
              <table class="rv-table"><tbody>
                {#each recordsOf(selected) as e (e.seq)}
                  <tr><td class="mono">{e.seq}</td><td>{when(e.at)}</td><td>{e.record.kind}</td><td class="dim">{e.record.attempt !== undefined ? `attempt ${e.record.attempt}` : ''} {e.record.idem_key ? `· idem ${e.record.idem_key.slice(-12)}` : ''}</td></tr>
                {/each}
              </tbody></table>
            {/if}
          {:else}
            <div class="rv-empty">Select a step to see its input, output and context.</div>
          {/if}
        </div>
      </div>
    {:else if tab === 'timeline'}
      <div class="rv-timeline">
        {#each steps as s (s.step_key)}
          <div class="tl-row">
            <span class="mono tl-key">{s.step_key}</span>
            <span class="tl-track"><span class="tl-bar {s.status}" style={bar(s)} title="{dur(s.duration_us)}"></span></span>
          </div>
        {/each}
        {#if !steps.length}<div class="rv-empty">Nothing to show yet.</div>{/if}
      </div>
    {:else if tab === 'journal'}
      <table class="rv-table"><tbody>
        <tr><th>seq</th><th>time</th><th>record</th><th>step</th><th>owner epoch</th></tr>
        {#each entries as e (e.seq)}
          <tr><td class="mono">{e.seq}</td><td>{when(e.at)}</td><td>{e.record.kind}</td><td class="mono">{e.record.step_key ?? e.record.wait_id ?? ''}</td><td>{e.epoch}</td></tr>
        {/each}
      </tbody></table>
    {:else if tab === 'checkpoints'}
      <p class="dim small">Each completed step is a point the run can resume from. Re-run from one starts a new debug run: the steps up to it replay from this journal, the steps after it run again.</p>
      <table class="rv-table"><tbody>
        {#each entries.filter((e) => e.record.kind === 'step_completed') as e (e.seq)}
          <tr>
            <td class="mono">{e.seq}</td><td class="mono">{e.record.step_key}</td><td>{when(e.at)}</td>
            <td class="dim">{e.record.context === null ? 'context unchanged' : `context ${JSON.stringify(e.record.context).length} B`}</td>
            <td>{#if run.can_steer && !live}<button class="lnk" disabled={busy} onclick={() => rerun(true, e.seq)}>Re-run from here</button>{/if}</td>
          </tr>
        {/each}
      </tbody></table>
    {:else}
      <div class="rv-io">
        <div><h4>Input</h4><pre>{pretty(run.input)}</pre></div>
        <div><h4>Output</h4><pre>{pretty(run.output)}</pre></div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .rv { display: flex; flex-direction: column; gap: 8px; font-size: 12.5px; }
  .rv-head { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; color: var(--text-2); }
  .grow { flex: 1; }
  .dim { color: var(--text-3); }
  .small { font-size: 11.5px; }
  .mono { font-family: var(--font-mono); font-size: 11.5px; }
  .pill { padding: 2px 8px; border-radius: 10px; font-size: 11px; background: var(--bg-subtle); color: var(--text-2); }
  .pill.ok { background: var(--ok-bg, #e7f6ec); color: var(--ok, #15803d); }
  .pill.err { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .pill.live { background: var(--spec-bg); color: var(--spec); }
  .pill.warn { background: var(--warn-bg, #fef3c7); color: var(--warn, #b45309); }
  .lnk { border: 0; background: transparent; color: var(--spec); cursor: pointer; font-size: 12px; }
  .lnk:disabled { opacity: 0.5; }
  .rv-pause { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; border: 1px solid var(--warn, #b45309); border-radius: var(--r-sm); background: var(--warn-bg, #fef3c7); }
  .rv-ctl { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .rv-ctl select { padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 12px; background: var(--bg-surface); }
  .rv-note { color: var(--text-3); }
  .rv-msg { padding: 6px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); white-space: pre-wrap; }
  .rv-edit { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--border); border-radius: var(--r-sm); }
  .rv-edit textarea { font-family: var(--font-mono); font-size: 12px; padding: 6px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); }
  .rv-row { display: flex; justify-content: flex-end; gap: 6px; }
  .rv-tabs, .rv-dtabs { display: flex; gap: 4px; align-items: center; border-bottom: 1px solid var(--border); }
  .rv-tabs button, .rv-dtabs button { border: 0; background: transparent; padding: 5px 8px; font-size: 12px; color: var(--text-3); cursor: pointer; border-bottom: 2px solid transparent; }
  .rv-tabs button.on, .rv-dtabs button.on { color: var(--text-1); border-bottom-color: var(--spec); }
  .rv-steps { display: grid; grid-template-columns: minmax(200px, 38%) 1fr; gap: 10px; min-height: 160px; }
  @media (max-width: 800px) { .rv-steps { grid-template-columns: 1fr; } }
  .rv-list { display: flex; flex-direction: column; overflow-y: auto; max-height: 340px; }
  .rv-step { display: flex; align-items: center; gap: 6px; padding: 4px 6px; border: 0; background: transparent; cursor: pointer; text-align: left; border-radius: var(--r-sm); color: var(--text-2); }
  .rv-step:hover, .rv-step.sel { background: var(--bg-subtle); }
  .rv-step.paused { outline: 1px solid var(--warn, #b45309); }
  .key { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ic { width: 14px; text-align: center; }
  .ic.completed { color: var(--ok, #15803d); } .ic.failed { color: var(--err, #b42318); } .ic.started { color: var(--spec); }
  .badge { font-size: 10px; padding: 0 4px; border-radius: 6px; background: var(--bg-subtle); }
  .dur { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .rv-detail { min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  pre { margin: 0; padding: 8px 10px; background: var(--bg-page); border: 1px solid var(--border); border-radius: var(--r-sm); font-family: var(--font-mono); font-size: 11.5px; white-space: pre-wrap; word-break: break-word; max-height: 300px; overflow: auto; }
  pre.err { color: var(--err, #b42318); }
  .rv-empty { padding: 10px; color: var(--text-4); }
  .rv-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .rv-table th { text-align: left; font-weight: 500; color: var(--text-3); padding: 3px 6px; border-bottom: 1px solid var(--border); }
  .rv-table td { padding: 3px 6px; border-bottom: 1px solid var(--border); }
  .rv-timeline { display: flex; flex-direction: column; gap: 3px; }
  .tl-row { display: grid; grid-template-columns: minmax(120px, 30%) 1fr; gap: 8px; align-items: center; }
  .tl-key { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tl-track { position: relative; height: 12px; background: var(--bg-subtle); border-radius: 3px; }
  .tl-bar { position: absolute; top: 0; bottom: 0; border-radius: 3px; background: var(--ok, #15803d); }
  .tl-bar.failed { background: var(--err, #b42318); } .tl-bar.started { background: var(--spec); }
  .tl-bar.waiting { background: repeating-linear-gradient(45deg, var(--text-4), var(--text-4) 3px, transparent 3px, transparent 6px); }
  .rv-io { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  h4 { margin: 4px 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
</style>
