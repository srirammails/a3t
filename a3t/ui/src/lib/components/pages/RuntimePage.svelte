<script lang="ts">
  /// The Runtime console — where live systems are operated.
  ///
  /// Same shell as Agents and Templates: the side list is the drill-down
  /// (host → runtime → deployed agents and flows) and this page shows whatever is
  /// selected there. Configuration lives in Canvas; this is operations — whether
  /// each thing is serving, and pause / resume / restart / undeploy. Runtime state
  /// (hibernate / pause / resume) is an admin action because a runtime hosts other
  /// teams' work too.
  import { onMount, onDestroy } from 'svelte';
  import { opsApi, executions, canvasApi } from '$lib/services/api';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import { hasRole } from '$lib/stores/auth';
  import { navigateTo, navigateToDetail, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { bumpContext } from '$lib/stores/apps.svelte.ts';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';

  interface Stats {
    runs: number; inFlight: number; failed: number; refused: number; completed: number;
    runsLastHour: number; errorRate: number; p50Ms?: number; p95Ms?: number;
    lastRunAt?: string; lastError?: string;
  }
  interface Deployment {
    id: string; kind: 'flow' | 'agent'; target: string; flowRef: string; agentRef: string;
    canvas: string; runtime: string; namespace: string; mode: string; state: 'active' | 'paused';
    pausedBy?: string; pausedAt?: string; pauseReason?: string; deployedBy: string; deployedAt: string;
  }
  interface Row { deployment: Deployment; serving: string; callersGetWhenPaused: number; stats: Stats }
  interface Host {
    host: string; note: string;
    memory: { usedBytes?: number; limitBytes?: number };
    cpu: { coresUsed?: number; limitCores?: number };
    disk?: { path: string; totalBytes: number; freeBytes: number; usedBytes: number };
  }
  interface Rt {
    runtime: { name: string; namespace: string; labels: Record<string, string>; url?: string; default?: boolean };
    state: 'active' | 'hibernated' | 'paused'; pausedResponse: number; deployments: Row[];
  }

  // ── What is selected in the side tree ──
  const selectedId: string = getActivePanel()?.props?.selectedId ?? '';
  const view: 'host' | 'runtime' | 'deployment' =
    selectedId.startsWith('dep:') ? 'deployment' : selectedId.startsWith('rt:') ? 'runtime' : 'host';
  const selKey = selectedId.replace(/^(host|rt|dep):/, '');

  let runtimes = $state<Rt[]>([]);
  let unplaced = $state<Row[]>([]);
  let host = $state<Host | null>(null);
  let controller = $state<'none' | 'reachable' | 'unreachable'>('none');
  let loading = $state(true);
  let error = $state('');
  let busy = $state<string>('');
  let toast = $state<{ kind: 'ok' | 'err'; text: string } | null>(null);
  let probe = $state<any>(null);
  const isAdmin = hasRole('admin');

  // Expanded runs (input / output), kept across the 5s refresh by instance id.
  let openRuns = $state<Record<string, any>>({});
  async function toggleRun(id: string) {
    if (openRuns[id]) { const { [id]: _, ...rest } = openRuns; openRuns = rest; return; }
    openRuns = { ...openRuns, [id]: { loading: true } };
    const res = await executions.get(id);
    openRuns = { ...openRuns, [id]: res.ok && typeof res.data === 'object' ? res.data : { error: res.error || 'Could not load this run' } };
  }
  function pretty(v: any): string {
    if (v == null) return '';
    if (typeof v === 'string') { try { return JSON.stringify(JSON.parse(v), null, 2); } catch { return v; } }
    try { return JSON.stringify(v, null, 2); } catch { return String(v); }
  }
  async function copy(text: string) { try { await navigator.clipboard.writeText(text); flash('ok', 'Copied'); } catch {} }

  // Terminate-runtime dialog: the plan first, then a typed confirmation.
  let terminating = $state<{ rt: Rt; plan: any | null; error?: string } | null>(null);
  let terminateConfirm = $state('');
  async function openTerminate(rt: Rt) {
    terminating = { rt, plan: null };
    terminateConfirm = '';
    const res = await opsApi.terminatePlan(rt.runtime.name);
    terminating = res.ok ? { rt, plan: res.data.impact } : { rt, plan: null, error: res.error || 'Could not plan the termination' };
  }
  async function confirmTerminate() {
    if (!terminating || terminateConfirm !== terminating.rt.runtime.name) return;
    const name = terminating.rt.runtime.name;
    busy = `rt:${name}:terminate`;
    const res = await opsApi.terminate(name);
    busy = '';
    terminating = null;
    if (res.ok) flash('ok', res.data?.note || `Runtime ${name} terminated`);
    else flash('err', res.status === 403 ? 'Only an admin can terminate a runtime.' : res.error || 'Failed');
    bumpContext();
    if (res.ok && !res.data?.recreated) { navigateTo('runtime', 'Runtimes'); return; }
    await load();
  }

  // New runtime dialog
  let newRt = $state<{ name: string; namespace: string; labels: string } | null>(null);
  let newRtErr = $state('');
  async function createRuntime() {
    if (!newRt) return;
    const name = newRt.name.trim();
    if (!/^[A-Za-z0-9_-]+$/.test(name)) { newRtErr = "Name: letters, digits, '-' or '_'"; return; }
    const labels: Record<string, string> = {};
    for (const part of newRt.labels.split(',').map((x) => x.trim()).filter(Boolean)) {
      const [k, v] = part.split('=').map((x) => x?.trim());
      if (!k || !v) { newRtErr = `Label "${part}" must be key=value`; return; }
      labels[k] = v;
    }
    busy = 'rt:create';
    const res = await canvasApi.createRuntime(newRt.namespace.trim() || 'default', name, { labels });
    busy = '';
    if (!res.ok) { newRtErr = res.error || 'Could not create the runtime'; return; }
    newRt = null;
    flash('ok', `Runtime ${name} created`);
    bumpContext();
    await load();
  }

  // Pause dialog
  let pausing = $state<Row | null>(null);
  let pauseReason = $state('');

  const allRows = $derived([...runtimes.flatMap((r) => r.deployments), ...unplaced]);
  const currentRt = $derived(
    view === 'runtime'
      ? selKey === '__unplaced'
        ? ({ runtime: { name: 'Unplaced', namespace: '', labels: {} }, state: 'active', pausedResponse: 503, deployments: unplaced } as Rt)
        : runtimes.find((r) => r.runtime.name === selKey) ?? null
      : null,
  );
  const currentRow = $derived(view === 'deployment' ? allRows.find((r) => r.deployment.id === selKey) ?? null : null);
  const totals = $derived({
    deployments: allRows.length,
    notServing: allRows.filter((d) => d.serving !== 'serving' && d.serving !== 'hibernated').length,
    inFlight: allRows.reduce((n, d) => n + d.stats.inFlight, 0),
  });

  async function load() {
    const res = await opsApi.overview();
    loading = false;
    if (!res.ok) { error = res.error || 'Could not load runtimes'; return; }
    error = '';
    runtimes = res.data.runtimes ?? [];
    unplaced = res.data.unplaced ?? [];
    host = res.data.host ?? null;
    controller = res.data.controller ?? 'none';
    if (view === 'deployment') {
      const p = await opsApi.probe(selKey);
      probe = p.ok ? p.data : null;
    }
  }

  // Live: refresh while the tab is visible. CPU needs two samples, so the second
  // refresh is also what fills in the CPU figure.
  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    load();
    timer = setInterval(() => { if (!document.hidden && !busy && !pausing && !terminating && !newRt) load(); }, 5000);
  });
  onDestroy(() => clearInterval(timer));

  function open(id: string) { navigateToDetail('runtime', 'Runtimes', id); }

  function flash(kind: 'ok' | 'err', text: string) {
    toast = { kind, text };
    setTimeout(() => { if (toast?.text === text) toast = null; }, 5000);
  }

  async function act(row: Row, action: 'pause' | 'resume' | 'restart' | 'undeploy' | 'terminate', reason?: string) {
    if (action === 'undeploy' && !confirm(`Undeploy ${row.deployment.target}? Runs in progress finish; the flow and its canvas are kept.`)) return;
    if (action === 'terminate' && !confirm(`Terminate ${row.deployment.target} now? Runs in progress (${row.stats.inFlight}) are cancelled and it is undeployed. The flow and its canvas are kept.`)) return;
    busy = `${row.deployment.id}:${action}`;
    const res = await opsApi.deployment(row.deployment.id, action, reason);
    busy = '';
    if (res.ok) flash('ok', res.data?.note || `${action} done`);
    else flash('err', res.status === 403 ? `Not allowed: ${res.error}` : res.error || 'Failed');
    bumpContext(); // the side tree's status dots
    if (res.ok && (action === 'undeploy' || action === 'terminate') && view === 'deployment') { open(`rt:${row.deployment.runtime}`); return; }
    await load();
  }

  async function confirmPause() {
    if (!pausing) return;
    const row = pausing;
    pausing = null;
    await act(row, 'pause', pauseReason.trim() || undefined);
    pauseReason = '';
  }

  async function runtimeAct(rt: Rt, action: 'hibernate' | 'pause' | 'resume') {
    if (action === 'pause' && !confirm(`Pause runtime ${rt.runtime.name}? Everything deployed on it (${rt.deployments.length}) stops serving until it is resumed.`)) return;
    busy = `rt:${rt.runtime.name}:${action}`;
    const res = await opsApi.runtime(rt.runtime.name, action);
    busy = '';
    if (res.ok) flash('ok', res.data?.note || 'Done');
    else flash('err', res.status === 403 ? 'Only an admin can change a runtime’s state.' : res.error || 'Failed');
    bumpContext();
    await load();
  }

  async function setPausedResponse(rt: Rt, status: 503 | 404) {
    const res = await opsApi.pausedResponse(rt.runtime.name, status);
    if (!res.ok) flash('err', res.error || 'Failed');
    await load();
  }

  function editCanvas(d: Deployment) {
    const name = d.canvas.split('/')[1];
    if (name) navigateTo('canvas', 'Canvas', { editCanvas: name });
  }

  // ── formatting ──
  const SERVING: Record<string, { label: string; cls: string; hint: string }> = {
    serving: { label: 'Serving', cls: 'ok', hint: 'Taking requests' },
    paused: { label: 'Paused', cls: 'bad', hint: 'Refuses every request until resumed' },
    'runtime-paused': { label: 'Runtime paused', cls: 'bad', hint: 'Its runtime is paused by an admin' },
    hibernated: { label: 'Hibernated', cls: 'idle', hint: 'Asleep — the next request wakes the runtime' },
    unplaced: { label: 'Unplaced', cls: 'warn', hint: 'Its runtime no longer exists' },
  };
  const RT_STATE: Record<string, string> = { active: 'ok', hibernated: 'idle', paused: 'bad' };

  function bytes(n?: number) {
    if (n == null) return '—';
    const u = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0; let v = n;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${u[i]}`;
  }
  function pct(a?: number, b?: number) { return a != null && b ? Math.min(100, (a / b) * 100) : null; }
  function ago(iso?: string) {
    if (!iso) return '—';
    const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
    if (s < 60) return `${Math.round(s)}s ago`;
    if (s < 3600) return `${Math.round(s / 60)}m ago`;
    if (s < 86400) return `${Math.round(s / 3600)}h ago`;
    return `${Math.round(s / 86400)}d ago`;
  }
  function ms(v?: number) { return v == null ? '—' : v < 1000 ? `${v} ms` : `${(v / 1000).toFixed(1)} s`; }
</script>

<!-- Page Header — same shape as Agents / Templates -->
<div class="screen-header">
  <div class="breadcrumb">
    <button class="crumb" onclick={() => navigateTo('runtime', 'Runtimes')}>Runtimes</button>
    {#if view !== 'host'}
      {@const rtName = view === 'runtime' ? selKey : currentRow?.deployment.runtime}
      <span class="sep">/</span>
      {#if view === 'deployment' && rtName}
        <button class="crumb" onclick={() => open(`rt:${rtName}`)}>{rtName}</button>
        <span class="sep">/</span> {currentRow?.deployment.target ?? selKey}
      {:else}
        {rtName === '__unplaced' ? 'Unplaced' : rtName}
      {/if}
    {:else}
      <span class="sep">/</span> {host?.host ?? 'Host'}
    {/if}
  </div>
  <div class="screen-title-row">
    <div class="title-block">
      <h1 class="screen-title">
        {#if view === 'host'}Hosts &amp; Runtimes
        {:else if view === 'runtime'}{currentRt?.runtime.name ?? selKey}
          {#if currentRt && selKey !== '__unplaced'}<span class="chip {RT_STATE[currentRt.state]}">{currentRt.state}</span>{/if}
        {:else}<span class="kind {currentRow?.deployment.kind ?? 'flow'}">{currentRow?.deployment.kind ?? ''}</span> {currentRow?.deployment.target ?? selKey}
          {#if currentRow}{@const sv = SERVING[currentRow.serving] ?? SERVING.serving}<span class="chip {sv.cls}" title={sv.hint}>{sv.label}</span>{/if}
        {/if}
      </h1>
    </div>
    <div class="screen-actions">
      {#if view === 'host' && isAdmin}
        <NewButton label="New runtime" accent="runtime" onclick={() => { newRt = { name: '', namespace: 'default', labels: '' }; newRtErr = ''; }} />
      {:else if view === 'runtime' && currentRt && selKey !== '__unplaced'}
        {@render runtimeActions(currentRt)}
      {:else if view === 'deployment' && currentRow}
        {@render depActions(currentRow, true)}
      {/if}
    </div>
  </div>
</div>

<div class="screen-body">
  {#if toast}<div class="toast {toast.kind}">{toast.text}</div>{/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if error}
    <p class="err">{error}</p>

  <!-- ═══ HOST ═══ -->
  {:else if view === 'host'}
    <PageBanner
      headline="Operate what is running, where it runs."
      highlightWord="running"
      subtitle="Hosts → runtimes → the agents and flows deployed on them."
      description="Pick a runtime or a deployment in the list. Pause, resume, restart, undeploy or probe it here; change how it deploys in its canvas."
      accent="runtime"
    />
    {#if host}{@render hostCard(host)}{/if}
    <h3 class="sec">Runtimes on this host</h3>
    {#if runtimes.length === 0}
      <div class="empty"><p>No runtimes are registered yet.</p>
        <button class="btn" onclick={() => navigateTo('canvas', 'Canvas')}>Create one in Canvas</button></div>
    {:else}
      <div class="rt-grid">
        {#each runtimes as rt (rt.runtime.name)}
          {@const notServing = rt.deployments.filter((d) => d.serving !== 'serving' && d.serving !== 'hibernated').length}
          <button class="rt-card" onclick={() => open(`rt:${rt.runtime.name}`)}>
            <div class="rt-top">
              <span class="dot {RT_STATE[rt.state]}"></span>
              <span class="rt-name">{rt.runtime.name}</span>
              <span class="chip {RT_STATE[rt.state]}">{rt.state}</span>
            </div>
            <div class="labels">
              <span class="ns">{rt.runtime.namespace}</span>
              {#each Object.entries(rt.runtime.labels ?? {}) as [k, v]}<span class="lbl">{k}={v}</span>{/each}
            </div>
            <div class="rt-sub">
              {rt.deployments.length} deployed · {rt.deployments.reduce((n, d) => n + d.stats.runsLastHour, 0)} runs/h
              {#if notServing} · <span class="alert">{notServing} not serving</span>{/if}
            </div>
          </button>
        {/each}
      </div>
    {/if}

  <!-- ═══ RUNTIME ═══ -->
  {:else if view === 'runtime'}
    {#if !currentRt}
      <p class="muted">Runtime “{selKey}” is not registered any more.</p>
    {:else}
      <p class="lede">
        {#if selKey === '__unplaced'}Deployments whose runtime was removed after they were placed. Restart re-places them from their canvas; undeploy removes them.
        {:else if currentRt.state === 'hibernated'}Asleep — the first request for anything here wakes it.
        {:else if currentRt.state === 'paused'}Paused by an admin — everything here refuses requests until the runtime is resumed.
        {:else}Serving.{/if}
        {#if selKey !== '__unplaced'}
          {#if controller === 'reachable'}<span class="tag">EE controller connected</span>
          {:else if controller === 'unreachable'}<span class="tag warn">controller unreachable · logical placement</span>
          {:else}<span class="tag">CE · logical placement</span>{/if}
          <span class="labels inline"><span class="ns">ns {currentRt.runtime.namespace}</span>{#each Object.entries(currentRt.runtime.labels ?? {}) as [k, v]}<span class="lbl">{k}={v}</span>{/each}</span>
        {/if}
      </p>
      {@render table(currentRt.deployments)}
    {/if}

  <!-- ═══ DEPLOYMENT ═══ -->
  {:else}
    {#if !currentRow}
      <p class="muted">“{selKey}” is not deployed any more.</p>
    {:else}
      {@const d = currentRow.deployment}
      <p class="lede">
        On <button class="link" onclick={() => open(`rt:${d.runtime}`)}>{d.runtime}</button> · {d.mode} · ns {d.namespace}
        · {d.canvas ? `canvas ${d.canvas}` : 'default canvas'}{#if d.kind === 'agent'} · runs {d.flowRef}{/if}
        · deployed {ago(d.deployedAt)}
      </p>
      {#if d.state === 'paused'}
        <div class="paused-note">
          Paused {ago(d.pausedAt)}{d.pauseReason ? ` — “${d.pauseReason}”` : ''}. Trigger callers get <b>HTTP {currentRow.callersGetWhenPaused}</b>; every other request is refused until resumed.
        </div>
      {/if}

      <div class="kpis">
        <div><b>{currentRow.stats.runsLastHour}</b><span>runs / h</span></div>
        <div><b>{currentRow.stats.inFlight}</b><span>in flight</span></div>
        <div><b class:alert={currentRow.stats.failed > 0}>{Math.round(currentRow.stats.errorRate * 100)}%</b><span>errors</span></div>
        <div><b>{ms(currentRow.stats.p50Ms)}</b><span>p50</span></div>
        <div><b>{ms(currentRow.stats.p95Ms)}</b><span>p95</span></div>
        <div><b>{currentRow.stats.refused}</b><span>refused</span></div>
      </div>

      {#if probe}
        <div class="two">
          <section class="card">
            <h4>Health</h4>
            <ul class="checks">
              {#each probe.checks as c}
                <li class:bad={!c.ok}><span class="dot {c.ok ? 'ok' : 'bad'}"></span>{c.check}{#if c.target}<span class="mono muted tgt-ref">{c.target}</span>{/if}{#if c.detail && !c.ok} — {c.detail}{/if}</li>
              {/each}
            </ul>
          </section>
          <section class="card">
            <h4>Triggers</h4>
            {#if probe.triggers.length === 0}
              <p class="muted small">No triggers feed this deployment — it runs on demand or from chat.</p>
            {:else}
              <ul class="trig">
                {#each probe.triggers as t}
                  <li><span class="chip {t.enabled ? 'ok' : 'idle'}">{t.config?.type}</span>
                    <span class="mono">{t.config?.path ?? t.config?.expression ?? t.config?.source ?? ''}</span>
                    <span class="muted">{t.name}</span></li>
                {/each}
              </ul>
            {/if}
          </section>
        </div>

        <section class="card">
          <h4>Recent runs</h4>
          {#if probe.recentRuns.length === 0}
            <p class="muted small">No runs yet.</p>
          {:else}
            <table class="runs">
              <tbody>
                {#each probe.recentRuns as r (r.instanceId)}
                  {@const refused = (r.error ?? '').startsWith('refused: ')}
                  {@const ex = openRuns[r.instanceId]}
                  <tr class="run-row" class:open={!!ex} onclick={() => toggleRun(r.instanceId)} title="Show input and output">
                    <td><svg class="twist" class:open={!!ex} width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M6 3l5 5-5 5" /></svg><span class="dot {refused ? 'idle' : r.status === 'completed' ? 'ok' : r.status === 'failed' ? 'bad' : 'run'}"></span>{refused ? 'refused' : r.status}</td>
                    <td class="muted">{ago(r.startedAt)}</td>
                    <td class="num">{ms(r.durationMs)}</td>
                    <td class="small" class:err={!refused} class:muted={refused}>{refused ? r.error.slice(9) : (r.error ?? '')}</td>
                  </tr>
                  {#if ex}
                    <tr class="run-detail"><td colspan="4">
                      {#if ex.loading}
                        <p class="muted small">Loading…</p>
                      {:else}
                        <div class="io-meta mono muted">{r.instanceId}{#if ex.current_step} · step {ex.current_step}{/if}</div>
                        {#if ex.error && !refused}<div class="io-err">{ex.error}</div>{/if}
                        <div class="io">
                          <div>
                            <div class="io-h">Input <button class="io-copy" onclick={(e) => { e.stopPropagation(); copy(pretty(ex.input)); }}>copy</button></div>
                            <pre>{pretty(ex.input) || '—'}</pre>
                          </div>
                          <div>
                            <div class="io-h">Output <button class="io-copy" onclick={(e) => { e.stopPropagation(); copy(pretty(ex.output)); }}>copy</button></div>
                            <pre>{pretty(ex.output) || (refused ? 'refused — never ran' : '—')}</pre>
                          </div>
                        </div>
                      {/if}
                    </td></tr>
                  {/if}
                {/each}
              </tbody>
            </table>
            <button class="btn sm ghost" onclick={() => navigateTo('executions', 'Executions')}>All executions →</button>
          {/if}
        </section>
      {/if}
    {/if}
  {/if}
</div>

{#snippet hostCard(h: Host)}
  {@const mem = pct(h.memory.usedBytes, h.memory.limitBytes)}
  {@const cpu = pct(h.cpu.coresUsed, h.cpu.limitCores)}
  {@const disk = h.disk ? pct(h.disk.usedBytes, h.disk.totalBytes) : null}
  <section class="host" title={h.note}>
    <div class="host-id">
      <span class="k">Host</span>
      <span class="mono">{h.host || 'flowr'}</span>
      <span class="shared">shared by all runtimes</span>
    </div>
    <div class="meters">
      <div class="meter"><span class="k">CPU</span><div class="bar"><i style="width:{cpu ?? 0}%" class:hot={(cpu ?? 0) > 80}></i></div>
        <span class="v">{h.cpu.coresUsed != null ? h.cpu.coresUsed.toFixed(2) : '…'} / {h.cpu.limitCores ?? '—'} cores</span></div>
      <div class="meter"><span class="k">Memory</span><div class="bar"><i style="width:{mem ?? 0}%" class:hot={(mem ?? 0) > 80}></i></div>
        <span class="v">{bytes(h.memory.usedBytes)} / {bytes(h.memory.limitBytes)}</span></div>
      {#if h.disk}
        <div class="meter"><span class="k">Disk</span><div class="bar"><i style="width:{disk ?? 0}%" class:hot={(disk ?? 0) > 85}></i></div>
          <span class="v">{bytes(h.disk.usedBytes)} / {bytes(h.disk.totalBytes)} <em>({bytes(h.disk.freeBytes)} free)</em></span></div>
      {/if}
    </div>
    <div class="totals">
      <span><b>{runtimes.length}</b> runtimes</span>
      <span><b>{totals.deployments}</b> deployed</span>
      <span><b>{totals.inFlight}</b> in flight</span>
      <span class:alert={totals.notServing > 0}><b>{totals.notServing}</b> not serving</span>
    </div>
  </section>
{/snippet}

{#snippet runtimeActions(rt: Rt)}
  <label class="pr" title="What a trigger caller gets while a deployment here is paused. A canvas can override it.">
    While paused, callers get
    <select disabled={!isAdmin} value={rt.pausedResponse}
      onchange={(e) => setPausedResponse(rt, Number((e.target as HTMLSelectElement).value) as 503 | 404)}>
      <option value={503}>503 Unavailable</option>
      <option value={404}>404 Not found</option>
    </select>
  </label>
  {#if isAdmin}
    {#if rt.state !== 'active'}<button class="btn sm runtime" disabled={!!busy} onclick={() => runtimeAct(rt, 'resume')}>Resume runtime</button>{/if}
    {#if rt.state === 'active'}<button class="btn sm" disabled={!!busy} onclick={() => runtimeAct(rt, 'hibernate')}>Hibernate</button>{/if}
    {#if rt.state !== 'paused'}<button class="btn sm outline-danger" disabled={!!busy} onclick={() => runtimeAct(rt, 'pause')}>Pause runtime</button>{/if}
    <button class="btn sm ghost text-danger" disabled={!!busy} onclick={() => openTerminate(rt)} title="Kill this runtime's container and reschedule what runs on it">Terminate</button>
  {:else}
    <span class="muted small">Runtime state is changed by an admin.</span>
  {/if}
{/snippet}

{#snippet depActions(row: Row, full: boolean)}
  {@const d = row.deployment}
  {#if d.state === 'paused'}
    <button class="btn sm runtime" disabled={!!busy} onclick={() => act(row, 'resume')}>Resume</button>
  {:else}
    <button class="btn sm" disabled={!!busy} onclick={() => { pausing = row; pauseReason = ''; }}>Pause</button>
  {/if}
  <button class="btn sm ghost" disabled={!!busy || d.state === 'paused'} title={d.state === 'paused' ? 'Resume before restarting' : 'Re-place from the current canvas'} onclick={() => act(row, 'restart')}>Restart</button>
  {#if !full}<button class="btn sm ghost" onclick={() => open(`dep:${d.id}`)}>Probe</button>{/if}
  {#if d.canvas}<button class="btn sm ghost" onclick={() => editCanvas(d)}>Edit canvas</button>{/if}
  <button class="btn sm ghost" disabled={!!busy} title="Remove the deployment; runs in progress finish" onclick={() => act(row, 'undeploy')}>Undeploy</button>
  <button class="btn sm ghost text-danger" disabled={!!busy} title="Cancel runs in progress now, then undeploy" onclick={() => act(row, 'terminate')}>Terminate</button>
{/snippet}

{#snippet table(rows: Row[])}
  {#if rows.length === 0}
    <div class="empty">
      <p>Nothing is deployed on this runtime.</p>
      <p class="muted small">Deploy from a canvas, or ask in AI Chat: “deploy my flow to {selKey}”.</p>
    </div>
  {:else}
    <div class="card flush">
      <table class="deps">
        <thead>
          <tr><th>Deployed</th><th>State</th><th class="num">Runs/h</th><th class="num">In flight</th><th class="num">Errors</th><th class="num">p50 / p95</th><th>Last run</th><th></th></tr>
        </thead>
        <tbody>
          {#each rows as row (row.deployment.id)}
            {@const d = row.deployment}
            {@const sv = SERVING[row.serving] ?? SERVING.serving}
            <tr class:dim={row.serving !== 'serving' && row.serving !== 'hibernated'}>
              <td>
                <div class="tgt">
                  <span class="kind {d.kind}">{d.kind}</span>
                  <button class="link mono" onclick={() => open(`dep:${d.id}`)}>{d.target}</button>
                </div>
                <div class="muted small">{d.canvas ? `canvas ${d.canvas}` : 'default canvas'} · {d.mode}</div>
              </td>
              <td>
                <span class="chip {sv.cls}" title={sv.hint}>{sv.label}</span>
                {#if d.state === 'paused'}<div class="muted small">{d.pauseReason ? `“${d.pauseReason}” · ` : ''}{ago(d.pausedAt)}</div>{/if}
              </td>
              <td class="num">{row.stats.runsLastHour}</td>
              <td class="num">{row.stats.inFlight || '—'}</td>
              <td class="num" title={row.stats.lastError ?? ''}>
                {#if row.stats.failed}<span class="alert">{Math.round(row.stats.errorRate * 100)}%</span>{:else}—{/if}
                {#if row.stats.refused}<div class="muted small">{row.stats.refused} refused</div>{/if}
              </td>
              <td class="num">{ms(row.stats.p50Ms)} / {ms(row.stats.p95Ms)}</td>
              <td>{ago(row.stats.lastRunAt)}</td>
              <td class="acts">{@render depActions(row, false)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/snippet}

<!-- Terminate runtime: show the reschedule plan, warn on isolated, confirm by name -->
{#if terminating}
  {@const pl = terminating.plan}
  <div class="modal-bg" role="presentation" onclick={() => (terminating = null)}></div>
  <div class="modal wide" role="dialog" aria-label="Terminate runtime">
    <h3>Terminate runtime <span class="mono">{terminating.rt.runtime.name}</span></h3>
    <p>Like killing its container: runs on it are cancelled and the runtime is removed. What is deployed here is <b>rescheduled</b>, not lost.</p>
    {#if terminating.error}
      <p class="err">{terminating.error}</p>
    {:else if !pl}
      <p class="muted">Working out the plan…</p>
    {:else}
      {#if pl.warnings.length}
        <div class="warn-box">
          {#each pl.warnings as w}<div>⚠ {w}</div>{/each}
        </div>
      {/if}
      {#if pl.plan.length === 0}
        <p class="callers">Nothing is deployed here — the runtime is simply removed.</p>
      {:else}
        <table class="plan">
          <thead><tr><th>Deployed</th><th>Mode</th><th>Goes to</th></tr></thead>
          <tbody>
            {#each pl.plan as r}
              <tr class:isolated={r.mode === 'isolated'}>
                <td><span class="kind {r.kind}">{r.kind}</span> <span class="mono">{r.target}</span>
                  {#if r.runsInFlight}<div class="small err">{r.runsInFlight} run(s) cancelled</div>{/if}</td>
                <td>{r.mode} · {r.session}</td>
                <td>{#if r.action === 'move'}→ <b>{r.to}</b>{:else}↻ fresh <b>{r.to}</b>{/if}
                  {#if r.note}<div class="small muted">{r.note}</div>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if pl.recreatesRuntime}<p class="small muted">↻ = the runtime is recreated fresh (same name, labels and namespace) because something can only run there.</p>{/if}
      {/if}
      <label>Type <b class="mono">{terminating.rt.runtime.name}</b> to confirm
        <input bind:value={terminateConfirm} placeholder={terminating.rt.runtime.name} onkeydown={(e) => e.key === 'Enter' && confirmTerminate()} />
      </label>
    {/if}
    <div class="modal-acts">
      <button class="btn" onclick={() => (terminating = null)}>Cancel</button>
      <button class="btn danger" disabled={!pl || terminateConfirm !== terminating.rt.runtime.name || !!busy} onclick={confirmTerminate}>Terminate</button>
    </div>
  </div>
{/if}

<!-- New runtime -->
{#if newRt}
  <div class="modal-bg" role="presentation" onclick={() => (newRt = null)}></div>
  <div class="modal" role="dialog" aria-label="New runtime">
    <h3>New runtime</h3>
    <p>A pool canvases place work into. Its labels are what placement selects on.</p>
    <label>Name <input bind:value={newRt.name} placeholder="rt-staging" /></label>
    <label>Namespace <input bind:value={newRt.namespace} placeholder="default" /></label>
    <label>Labels <input bind:value={newRt.labels} placeholder="tier=test, class=cpu" onkeydown={(e) => e.key === 'Enter' && createRuntime()} /></label>
    {#if newRtErr}<p class="err">{newRtErr}</p>{/if}
    <div class="modal-acts">
      <button class="btn" onclick={() => (newRt = null)}>Cancel</button>
      <button class="btn runtime" disabled={!!busy || !newRt.name.trim()} onclick={createRuntime}>Create runtime</button>
    </div>
  </div>
{/if}

<!-- Pause dialog: says exactly what callers will get -->
{#if pausing}
  <div class="modal-bg" role="presentation" onclick={() => (pausing = null)}></div>
  <div class="modal" role="dialog" aria-label="Pause deployment">
    <h3>Pause {pausing.deployment.kind} <span class="mono">{pausing.deployment.target}</span></h3>
    <p>Every new request is refused until someone resumes it — triggers, manual runs, chat and other agents. Runs already in progress finish.</p>
    <p class="callers">Trigger callers will get <b>HTTP {pausing.callersGetWhenPaused}</b>
      <span class="muted">({pausing.callersGetWhenPaused === 404 ? 'endpoint hidden' : 'unavailable'} — set on the runtime{#if pausing.deployment.canvas}, overridable in the canvas{/if})</span></p>
    <label>Reason <span class="muted">(shown to whoever finds it paused)</span>
      <input bind:value={pauseReason} placeholder="e.g. investigating bad outputs" onkeydown={(e) => e.key === 'Enter' && confirmPause()} />
    </label>
    <div class="modal-acts">
      <button class="btn" onclick={() => (pausing = null)}>Cancel</button>
      <button class="btn danger" onclick={confirmPause}>Pause</button>
    </div>
  </div>
{/if}

<style>
  .muted { color: var(--text-4); }
  .small { font-size: 12px; }
  .err { color: var(--err); }
  .alert { color: var(--err); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  /* Header / body — the same shape as Agents and Templates. */
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); margin: 0 2px; }
  .screen-title-row { display: flex; align-items: center; gap: 18px; flex-wrap: wrap; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; min-width: 0; }
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }
  .crumb { background: none; border: 0; padding: 0; color: inherit; cursor: pointer; font: inherit; }
  .crumb:hover { text-decoration: underline; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; display: flex; align-items: center; gap: 10px; margin: 0; }
  .screen-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .lede { font-size: 13px; color: var(--text-3); margin: 0 0 14px; display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .sec { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); font-weight: 500; margin: 20px 0 8px; }
  .tag { font-size: 11px; padding: 1px 7px; border-radius: 999px; background: var(--bg-subtle); color: var(--text-3); border: 1px solid var(--border); }
  .tag.warn { color: var(--warn); border-color: var(--warn); }
  .card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 14px 16px; margin-bottom: 14px; }
  .card.flush { padding: 4px 8px; overflow-x: auto; }
  .card h4 { margin: 0 0 8px; font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); font-weight: 500; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  @media (max-width: 900px) { .two { grid-template-columns: 1fr; } }
  .paused-note { padding: 10px 12px; border-radius: var(--r-sm); background: var(--err-bg); color: var(--err); font-size: 13px; margin-bottom: 14px; }

  /* Host */
  .host { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); }
  .host-id { display: flex; align-items: baseline; gap: 8px; }
  .host .k { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); min-width: 52px; }
  .shared { font-size: 11px; color: var(--text-4); font-style: italic; }
  .meters { display: flex; flex-wrap: wrap; gap: 10px 32px; }
  .meter { display: flex; align-items: center; gap: 8px; }
  .bar { width: 120px; height: 6px; background: var(--bg-subtle); border-radius: 3px; overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--runtime, #0f766e); transition: width .4s; }
  .bar i.hot { background: var(--err); }
  .meter .v { font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .meter em { color: var(--text-4); font-style: normal; }
  .totals { display: flex; gap: 18px; font-size: 12.5px; color: var(--text-3); }
  .totals b { color: var(--text-1); font-variant-numeric: tabular-nums; }

  .rt-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 10px; }
  .rt-card { text-align: left; padding: 12px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); cursor: pointer; display: flex; flex-direction: column; gap: 8px; }
  .rt-card:hover { border-color: var(--runtime, #0f766e); }
  .rt-top { display: flex; align-items: center; gap: 8px; }
  .rt-name { font-weight: 600; color: var(--text-1); font-size: 13.5px; flex: 1; }
  .rt-sub { font-size: 12px; color: var(--text-3); }
  .labels { display: flex; flex-wrap: wrap; gap: 4px; }
  .labels.inline { display: inline-flex; }
  .lbl, .ns { font-family: var(--font-mono); font-size: 10.5px; padding: 1px 6px; border-radius: 4px; background: var(--bg-subtle); color: var(--text-3); }
  .ns { background: var(--runtime-bg, #e6f4f1); color: var(--runtime, #0f766e); }

  .toast { margin: 0 0 12px; padding: 8px 12px; border-radius: var(--r-sm); font-size: 13px; }
  .toast.ok { background: var(--ok-bg, #e7f6ec); color: var(--ok); }
  .toast.err { background: var(--err-bg); color: var(--err); }

  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex: none; margin-right: 6px; background: var(--text-5); }
  .dot.ok { background: var(--ok); } .dot.bad { background: var(--err); } .dot.warn { background: var(--warn); }
  .dot.run { background: var(--signal, #2563eb); } .dot.idle { background: var(--text-5, #bbb); }
  .chip { font-size: 11px; padding: 1px 8px; border-radius: 999px; border: 1px solid currentColor; white-space: nowrap; font-family: var(--font-sans); font-weight: 400; }
  .chip.ok { color: var(--ok); } .chip.bad { color: var(--err); } .chip.warn { color: var(--warn); } .chip.idle { color: var(--text-4); }
  .kind { font-size: 10px; text-transform: uppercase; letter-spacing: .05em; padding: 1px 6px; border-radius: 4px; background: var(--spec-bg); color: var(--spec); font-family: var(--font-sans); font-weight: 500; }
  .kind.agent { background: var(--ephemeral-bg, #fff4e5); color: var(--ephemeral, #b45309); }

  .pr { font-size: 12px; color: var(--text-3); display: flex; align-items: center; gap: 6px; }
  .pr select { font-size: 12px; padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); }
  .btn.runtime { background: var(--runtime, #0f766e); color: white; border-color: var(--runtime, #0f766e); }
  .btn.outline-danger { color: var(--err); border-color: var(--err); background: var(--bg-surface); }
  .btn.outline-danger:hover { background: var(--err-bg); }
  .btn.text-danger { color: var(--err); background: transparent; border-color: transparent; box-shadow: none; }
  .btn.text-danger:hover { background: var(--err-bg); }

  table.deps { width: 100%; border-collapse: collapse; font-size: 13px; }
  .deps th { text-align: left; font-size: 11px; font-weight: 500; text-transform: uppercase; letter-spacing: .05em; color: var(--text-4); padding: 8px; border-bottom: 1px solid var(--border); }
  .deps td { padding: 10px 8px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: top; }
  .deps tr:last-child td { border-bottom: 0; }
  .deps tr.dim td:not(.acts) { opacity: .75; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .tgt { display: flex; align-items: center; gap: 6px; }
  .link { background: none; border: 0; padding: 0; color: var(--text-1); cursor: pointer; text-decoration: underline dotted; font: inherit; }
  .acts { white-space: nowrap; text-align: right; }

  .kpis { display: grid; grid-template-columns: repeat(6, 1fr); gap: 8px; margin-bottom: 14px; }
  @media (max-width: 700px) { .kpis { grid-template-columns: repeat(3, 1fr); } }
  .kpis div { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); padding: 10px; display: flex; flex-direction: column; align-items: center; }
  .kpis b { font-size: 16px; font-variant-numeric: tabular-nums; }
  .kpis span { font-size: 10.5px; color: var(--text-4); }
  .checks, .trig { list-style: none; padding: 0; margin: 0; font-size: 13px; display: flex; flex-direction: column; gap: 5px; }
  .checks li.bad { color: var(--err); }
  .tgt-ref { margin-left: 6px; }
  .trig li { display: flex; gap: 8px; align-items: center; }
  table.runs { width: 100%; font-size: 12.5px; border-collapse: collapse; margin-bottom: 6px; }
  .runs td { padding: 5px 6px; border-bottom: 1px solid var(--divider, var(--border)); }
  .run-row { cursor: pointer; }
  .run-row:hover td, .run-row.open td { background: var(--bg-subtle); }
  .twist { margin-right: 6px; color: var(--text-4); transition: transform .12s; vertical-align: middle; }
  .twist.open { transform: rotate(90deg); }
  .run-detail td { background: var(--bg-subtle); padding: 4px 10px 12px 24px; }
  .io-meta { font-size: 11px; margin-bottom: 6px; }
  .io-err { color: var(--err); font-size: 12.5px; margin-bottom: 8px; white-space: pre-wrap; }
  .io { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  @media (max-width: 900px) { .io { grid-template-columns: 1fr; } }
  .io-h { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); margin-bottom: 4px; display: flex; justify-content: space-between; }
  .io-copy { background: none; border: 0; color: var(--text-4); cursor: pointer; font-size: 11px; text-transform: none; letter-spacing: 0; }
  .io-copy:hover { color: var(--text-1); }
  .io pre { margin: 0; padding: 8px 10px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); font-family: var(--font-mono); font-size: 12px; max-height: 320px; overflow: auto; white-space: pre-wrap; word-break: break-word; }

  .empty { padding: 32px; text-align: center; color: var(--text-3); }

  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.25); z-index: 50; }
  .modal { position: fixed; z-index: 51; top: 20vh; left: 50%; transform: translateX(-50%); width: min(480px, calc(100vw - 32px)); background: var(--bg-surface); border-radius: var(--r); box-shadow: var(--shadow-lg); padding: 20px; }
  .modal h3 { margin: 0 0 8px; font-size: 16px; }
  .modal p { font-size: 13px; color: var(--text-2); margin: 0 0 10px; }
  .modal .callers { padding: 8px 10px; background: var(--bg-subtle); border-radius: var(--r-sm); }
  .modal label + label { margin-top: 10px; }
  .modal label { display: flex; flex-direction: column; gap: 4px; font-size: 12.5px; color: var(--text-2); text-transform: none; letter-spacing: normal; font-weight: 400; }
  .modal input { padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 13px; }
  .modal.wide { width: min(640px, calc(100vw - 32px)); top: 12vh; }
  .warn-box { background: var(--warn-bg, #fff8e6); color: var(--warn, #b45309); border: 1px solid var(--warn, #b45309); border-radius: var(--r-sm); padding: 8px 10px; font-size: 12.5px; margin-bottom: 10px; display: flex; flex-direction: column; gap: 4px; }
  table.plan { width: 100%; border-collapse: collapse; font-size: 12.5px; margin-bottom: 10px; }
  .plan th { text-align: left; font-size: 10.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-4); font-weight: 500; padding: 4px 6px; border-bottom: 1px solid var(--border); }
  .plan td { padding: 6px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: top; }
  .plan tr.isolated td { background: var(--warn-bg, #fff8e6); }
  .modal-acts { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
</style>
