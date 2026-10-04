<script lang="ts">
  /// The Runtime console — where live systems are operated.
  ///
  /// Configuration lives in Canvas; this page is operations: every runtime, what
  /// is deployed on it, whether each thing is serving, and the controls to pause,
  /// resume, restart, undeploy or probe it. Runtime state (hibernate / pause /
  /// resume) is an admin action because a runtime hosts other teams' work too.
  import { onMount, onDestroy } from 'svelte';
  import { opsApi } from '$lib/services/api';
  import { hasRole } from '$lib/stores/auth';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';

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

  let runtimes = $state<Rt[]>([]);
  let unplaced = $state<Row[]>([]);
  let host = $state<Host | null>(null);
  let controller = $state<'none' | 'reachable' | 'unreachable'>('none');
  let loading = $state(true);
  let error = $state('');
  let selected = $state<string>('');
  let busy = $state<string>('');
  let toast = $state<{ kind: 'ok' | 'err'; text: string } | null>(null);
  const isAdmin = hasRole('admin');

  // Pause dialog
  let pausing = $state<Row | null>(null);
  let pauseReason = $state('');
  // Probe drawer
  let probing = $state<any>(null);
  let probeLoading = $state(false);

  const current = $derived(runtimes.find((r) => r.runtime.name === selected) ?? null);
  const totals = $derived({
    deployments: runtimes.reduce((n, r) => n + r.deployments.length, 0) + unplaced.length,
    paused: [...runtimes.flatMap((r) => r.deployments), ...unplaced].filter((d) => d.serving !== 'serving' && d.serving !== 'hibernated').length,
    inFlight: [...runtimes.flatMap((r) => r.deployments), ...unplaced].reduce((n, d) => n + d.stats.inFlight, 0),
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
    if (!selected || !runtimes.some((r) => r.runtime.name === selected)) {
      selected = (runtimes.find((r) => r.deployments.length) ?? runtimes[0])?.runtime.name ?? '';
    }
  }

  // Live: refresh while the tab is visible. CPU needs two samples, so the second
  // refresh is also what fills in the CPU figure.
  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    load();
    timer = setInterval(() => { if (!document.hidden && !busy) load(); }, 5000);
  });
  onDestroy(() => clearInterval(timer));

  function flash(kind: 'ok' | 'err', text: string) {
    toast = { kind, text };
    setTimeout(() => { if (toast?.text === text) toast = null; }, 5000);
  }

  async function act(row: Row, action: 'pause' | 'resume' | 'restart' | 'undeploy', reason?: string) {
    if (action === 'undeploy' && !confirm(`Undeploy ${row.deployment.target}? The flow and its canvas are kept.`)) return;
    busy = `${row.deployment.id}:${action}`;
    const res = await opsApi.deployment(row.deployment.id, action, reason);
    busy = '';
    if (res.ok) {
      flash('ok', res.data?.note || `${action} done`);
      if (probing?.deployment?.id === row.deployment.id) openProbe(row);
    } else {
      flash('err', res.status === 403 ? `Not allowed: ${(res as any).data?.reason || res.error}` : res.error || 'Failed');
    }
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
    await load();
  }

  async function setPausedResponse(rt: Rt, status: 503 | 404) {
    const res = await opsApi.pausedResponse(rt.runtime.name, status);
    if (!res.ok) flash('err', res.error || 'Failed');
    await load();
  }

  async function openProbe(row: Row) {
    probeLoading = true;
    probing = { deployment: row.deployment };
    const res = await opsApi.probe(row.deployment.id);
    probeLoading = false;
    probing = res.ok ? res.data : { deployment: row.deployment, error: res.error };
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

<div class="rt-page">
  <PageBanner
    headline="Operate what is running, where it runs."
    highlightWord="running"
    subtitle="Runtimes"
    description="Every runtime, what is deployed on it and whether it is serving. Pause, resume, restart, undeploy or probe a flow or agent. Changing how something deploys is done in its canvas."
    accent="runtime"
  />

  {#if host}
    {@const mem = pct(host.memory.usedBytes, host.memory.limitBytes)}
    {@const cpu = pct(host.cpu.coresUsed, host.cpu.limitCores)}
    {@const disk = host.disk ? pct(host.disk.usedBytes, host.disk.totalBytes) : null}
    <section class="host" title={host.note}>
      <div class="host-id">
        <span class="k">Host</span>
        <span class="mono">{host.host || 'flowr'}</span>
        <span class="shared">shared by all runtimes</span>
      </div>
      <div class="meter">
        <span class="k">CPU</span>
        <div class="bar"><i style="width:{cpu ?? 0}%" class:hot={(cpu ?? 0) > 80}></i></div>
        <span class="v">{host.cpu.coresUsed != null ? host.cpu.coresUsed.toFixed(2) : '…'} / {host.cpu.limitCores ?? '—'} cores</span>
      </div>
      <div class="meter">
        <span class="k">Memory</span>
        <div class="bar"><i style="width:{mem ?? 0}%" class:hot={(mem ?? 0) > 80}></i></div>
        <span class="v">{bytes(host.memory.usedBytes)} / {bytes(host.memory.limitBytes)}</span>
      </div>
      {#if host.disk}
        <div class="meter">
          <span class="k">Disk</span>
          <div class="bar"><i style="width:{disk ?? 0}%" class:hot={(disk ?? 0) > 85}></i></div>
          <span class="v">{bytes(host.disk.usedBytes)} / {bytes(host.disk.totalBytes)} <em>({bytes(host.disk.freeBytes)} free)</em></span>
        </div>
      {/if}
      <div class="totals">
        <span><b>{totals.deployments}</b> deployed</span>
        <span><b>{totals.inFlight}</b> in flight</span>
        <span class:alert={totals.paused > 0}><b>{totals.paused}</b> not serving</span>
      </div>
    </section>
  {/if}

  {#if toast}<div class="toast {toast.kind}">{toast.text}</div>{/if}

  {#if loading}
    <p class="muted pad">Loading runtimes…</p>
  {:else if error}
    <p class="err pad">{error}</p>
  {:else if runtimes.length === 0}
    <div class="empty">
      <p>No runtimes are registered yet.</p>
      <button class="btn" onclick={() => navigateTo('canvas', 'Canvas')}>Create one in Canvas</button>
    </div>
  {:else}
    <div class="split">
      <!-- Runtime list -->
      <aside class="rts">
        {#each runtimes as rt (rt.runtime.name)}
          {@const notServing = rt.deployments.filter((d) => d.serving !== 'serving' && d.serving !== 'hibernated').length}
          <button class="rt" class:active={rt.runtime.name === selected} onclick={() => (selected = rt.runtime.name)}>
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
              {rt.deployments.length} deployed{#if notServing} · <span class="alert">{notServing} not serving</span>{/if}
            </div>
          </button>
        {/each}
        {#if unplaced.length}
          <button class="rt" class:active={selected === '__unplaced'} onclick={() => (selected = '__unplaced')}>
            <div class="rt-top"><span class="dot warn"></span><span class="rt-name">Unplaced</span></div>
            <div class="rt-sub">{unplaced.length} whose runtime was removed</div>
          </button>
        {/if}
      </aside>

      <!-- Selected runtime -->
      <main class="detail">
        {#if current}
          <header class="d-head">
            <div>
              <h2>{current.runtime.name} <span class="chip {RT_STATE[current.state]}">{current.state}</span></h2>
              <p class="muted">
                {#if current.state === 'hibernated'}Asleep — the first request for anything here wakes it.
                {:else if current.state === 'paused'}Paused by an admin — everything here refuses requests until the runtime is resumed.
                {:else}Serving.{/if}
                {#if controller === 'reachable'}<span class="tag">EE controller connected</span>
                {:else if controller === 'unreachable'}<span class="tag warn" title="A3T_CONTROLLER_URL is set but the controller does not answer">controller unreachable · logical placement</span>
                {:else}<span class="tag">CE · logical placement</span>{/if}
              </p>
            </div>
            <div class="rt-actions">
              <label class="pr" title="What a trigger caller gets while a deployment here is paused. A canvas can override it.">
                While paused, callers get
                <select disabled={!isAdmin} value={current.pausedResponse}
                  onchange={(e) => setPausedResponse(current!, Number((e.target as HTMLSelectElement).value) as 503 | 404)}>
                  <option value={503}>503 Unavailable</option>
                  <option value={404}>404 Not found</option>
                </select>
              </label>
              {#if isAdmin}
                {#if current.state !== 'active'}
                  <button class="btn sm runtime" disabled={!!busy} onclick={() => runtimeAct(current!, 'resume')}>Resume runtime</button>
                {/if}
                {#if current.state === 'active'}
                  <button class="btn sm" disabled={!!busy} onclick={() => runtimeAct(current!, 'hibernate')}>Hibernate</button>
                {/if}
                {#if current.state !== 'paused'}
                  <button class="btn sm outline-danger" disabled={!!busy} onclick={() => runtimeAct(current!, 'pause')}>Pause runtime</button>
                {/if}
              {:else}
                <span class="muted small">Runtime state is changed by an admin.</span>
              {/if}
            </div>
          </header>
          {@render table(current.deployments)}
        {:else if selected === '__unplaced'}
          <header class="d-head"><div><h2>Unplaced</h2><p class="muted">Deployments whose runtime was removed after they were placed. Restart re-places them from their canvas; undeploy removes them.</p></div></header>
          {@render table(unplaced)}
        {/if}
      </main>
    </div>
  {/if}
</div>

{#snippet table(rows: Row[])}
  {#if rows.length === 0}
    <div class="empty">
      <p>Nothing is deployed on this runtime.</p>
      <p class="muted small">Deploy from a canvas, or ask in AI Chat: “deploy my flow to {selected}”.</p>
    </div>
  {:else}
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
                <button class="link mono" onclick={() => openProbe(row)}>{d.target}</button>
              </div>
              <div class="muted small">
                {d.canvas ? `canvas ${d.canvas}` : 'default canvas'} · {d.mode} · ns {d.namespace}
              </div>
            </td>
            <td>
              <span class="chip {sv.cls}" title={sv.hint}>{sv.label}</span>
              {#if d.state === 'paused'}
                <div class="muted small" title={d.pausedBy}>{d.pauseReason ? `“${d.pauseReason}” · ` : ''}{ago(d.pausedAt)} · callers get {row.callersGetWhenPaused}</div>
              {/if}
            </td>
            <td class="num">{row.stats.runsLastHour}</td>
            <td class="num">{row.stats.inFlight || '—'}</td>
            <td class="num" title={row.stats.lastError ?? ''}>
              {#if row.stats.failed}<span class="alert">{Math.round(row.stats.errorRate * 100)}%</span>{:else}—{/if}
              {#if row.stats.refused}<div class="muted small">{row.stats.refused} refused</div>{/if}
            </td>
            <td class="num">{ms(row.stats.p50Ms)} / {ms(row.stats.p95Ms)}</td>
            <td>{ago(row.stats.lastRunAt)}</td>
            <td class="acts">
              {#if d.state === 'paused'}
                <button class="btn sm runtime" disabled={!!busy} onclick={() => act(row, 'resume')}>Resume</button>
              {:else}
                <button class="btn sm" disabled={!!busy} onclick={() => { pausing = row; pauseReason = ''; }}>Pause</button>
              {/if}
              <button class="btn sm ghost" disabled={!!busy || d.state === 'paused'} title={d.state === 'paused' ? 'Resume before restarting' : 'Re-place from the current canvas'} onclick={() => act(row, 'restart')}>Restart</button>
              <button class="btn sm ghost" onclick={() => openProbe(row)}>Probe</button>
              {#if d.canvas}<button class="btn sm ghost" onclick={() => editCanvas(d)}>Edit canvas</button>{/if}
              <button class="btn sm ghost text-danger" disabled={!!busy} onclick={() => act(row, 'undeploy')}>Undeploy</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{/snippet}

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

<!-- Probe drawer -->
<SlidePanel open={!!probing} onclose={() => (probing = null)}>
  {#if probing}
    {@const d = probing.deployment}
    <div class="probe">
      <h2><span class="kind {d.kind}">{d.kind}</span> <span class="mono">{d.target}</span></h2>
      {#if probeLoading}
        <p class="muted">Probing…</p>
      {:else if probing.error}
        <p class="err">{probing.error}</p>
      {:else}
        {@const sv = SERVING[probing.serving] ?? SERVING.serving}
        <p><span class="chip {sv.cls}">{sv.label}</span> on <b>{d.runtime}</b> · {d.mode} · ns {d.namespace} · deployed {ago(d.deployedAt)}</p>

        <h4>Health</h4>
        <ul class="checks">
          {#each probing.checks as c}
            <li class:bad={!c.ok}><span class="dot {c.ok ? 'ok' : 'bad'}"></span>{c.check}{#if c.target}<span class="mono muted tgt-ref">{c.target}</span>{/if}{#if c.detail && !c.ok} — {c.detail}{/if}</li>
          {/each}
        </ul>

        <h4>Activity <span class="muted small">(last {probing.stats.runs} runs)</span></h4>
        <div class="kpis">
          <div><b>{probing.stats.runsLastHour}</b><span>runs / h</span></div>
          <div><b>{probing.stats.inFlight}</b><span>in flight</span></div>
          <div><b>{Math.round(probing.stats.errorRate * 100)}%</b><span>errors</span></div>
          <div><b>{ms(probing.stats.p50Ms)}</b><span>p50</span></div>
          <div><b>{ms(probing.stats.p95Ms)}</b><span>p95</span></div>
          <div><b>{probing.stats.refused}</b><span>refused</span></div>
        </div>

        <h4>Triggers</h4>
        {#if probing.triggers.length === 0}
          <p class="muted small">No triggers feed this deployment.</p>
        {:else}
          <ul class="trig">
            {#each probing.triggers as t}
              <li>
                <span class="chip {t.enabled ? 'ok' : 'idle'}">{t.config?.type}</span>
                <span class="mono">{t.config?.path ?? t.config?.expression ?? t.config?.source ?? ''}</span>
                <span class="muted">{t.name}</span>
              </li>
            {/each}
          </ul>
        {/if}

        <h4>Recent runs</h4>
        {#if probing.recentRuns.length === 0}
          <p class="muted small">No runs yet.</p>
        {:else}
          <table class="runs">
            <tbody>
              {#each probing.recentRuns as r}
                {@const refused = (r.error ?? '').startsWith('refused: ')}
                <tr>
                  <td><span class="dot {refused ? 'idle' : r.status === 'completed' ? 'ok' : r.status === 'failed' ? 'bad' : 'run'}"></span>{refused ? 'refused' : r.status}</td>
                  <td class="muted">{ago(r.startedAt)}</td>
                  <td class="num">{ms(r.durationMs)}</td>
                  <td class="small" class:err={!refused} class:muted={refused}>{refused ? r.error.slice(9) : (r.error ?? '')}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <button class="btn sm ghost" onclick={() => navigateTo('executions', 'Executions')}>All executions →</button>
        {/if}
      {/if}
    </div>
  {/if}
</SlidePanel>

<style>
  .rt-page { display: flex; flex-direction: column; height: 100%; overflow: auto; background: var(--bg-page); }
  .pad { padding: 24px; }
  .muted { color: var(--text-4); }
  .small { font-size: 12px; }
  .err { color: var(--err); }
  .alert { color: var(--err); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .tag { margin-left: 8px; font-size: 11px; padding: 1px 7px; border-radius: 999px; background: var(--bg-subtle); color: var(--text-3); border: 1px solid var(--border); }

  /* Host strip */
  .host { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 28px; margin: 0 24px 12px; padding: 12px 16px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); }
  .host-id { display: flex; align-items: baseline; gap: 8px; }
  .host .k { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); }
  .shared { font-size: 11px; color: var(--text-4); font-style: italic; }
  .meter { display: flex; align-items: center; gap: 8px; min-width: 220px; }
  .bar { width: 90px; height: 6px; background: var(--bg-subtle); border-radius: 3px; overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--runtime, #0f766e); transition: width .4s; }
  .bar i.hot { background: var(--err); }
  .meter .v { font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .meter em { color: var(--text-4); font-style: normal; }
  .totals { display: flex; gap: 16px; margin-left: auto; font-size: 12.5px; color: var(--text-3); }
  .totals b { color: var(--text-1); font-variant-numeric: tabular-nums; }

  .toast { margin: 0 24px 12px; padding: 8px 12px; border-radius: var(--r-sm); font-size: 13px; }
  .toast.ok { background: var(--ok-bg, #e7f6ec); color: var(--ok); }
  .toast.err { background: var(--err-bg); color: var(--err); }

  /* Layout */
  .split { display: grid; grid-template-columns: 260px 1fr; gap: 16px; padding: 0 24px 24px; min-height: 0; }
  @media (max-width: 860px) { .split { grid-template-columns: 1fr; } }
  .rts { display: flex; flex-direction: column; gap: 8px; }
  .rt { text-align: left; padding: 10px 12px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); cursor: pointer; display: flex; flex-direction: column; gap: 6px; }
  .rt:hover { border-color: var(--border-strong); }
  .rt.active { border-color: var(--runtime, #0f766e); box-shadow: inset 3px 0 0 var(--runtime, #0f766e); }
  .rt-top { display: flex; align-items: center; gap: 8px; }
  .rt-name { font-weight: 600; color: var(--text-1); font-size: 13.5px; flex: 1; }
  .rt-sub { font-size: 12px; color: var(--text-3); }
  .labels { display: flex; flex-wrap: wrap; gap: 4px; }
  .lbl, .ns { font-family: var(--font-mono); font-size: 10.5px; padding: 1px 6px; border-radius: 4px; background: var(--bg-subtle); color: var(--text-3); }
  .ns { background: var(--runtime-bg, #e6f4f1); color: var(--runtime, #0f766e); }

  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex: none; margin-right: 6px; background: var(--text-5); }
  .dot.ok { background: var(--ok); }
  .dot.bad { background: var(--err); }
  .dot.warn { background: var(--warn); }
  .dot.run { background: var(--signal, #2563eb); }
  .dot.idle { background: var(--text-5, #bbb); }
  .chip { font-size: 11px; padding: 1px 8px; border-radius: 999px; border: 1px solid currentColor; white-space: nowrap; }
  .chip.ok { color: var(--ok); }
  .chip.bad { color: var(--err); }
  .chip.warn { color: var(--warn); }
  .chip.idle { color: var(--text-4); }

  .detail { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 16px; min-width: 0; overflow-x: auto; }
  .d-head { display: flex; justify-content: space-between; gap: 16px; flex-wrap: wrap; margin-bottom: 12px; }
  .d-head h2 { font-family: var(--font-serif); font-size: 20px; font-weight: 500; margin: 0 0 4px; display: flex; align-items: center; gap: 8px; }
  .d-head p { margin: 0; font-size: 13px; }
  .rt-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .pr { font-size: 12px; color: var(--text-3); display: flex; align-items: center; gap: 6px; }
  .pr select { font-size: 12px; padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); }
  .btn.outline-danger { color: var(--err); border-color: var(--err); background: var(--bg-surface); }
  .btn.outline-danger:hover { background: var(--err-bg); }
  .btn.text-danger { color: var(--err); background: transparent; border-color: transparent; box-shadow: none; }
  .btn.text-danger:hover { background: var(--err-bg); }
  .tag.warn { color: var(--warn); border-color: var(--warn); }
  .tgt-ref { margin-left: 6px; }
  .btn.runtime { background: var(--runtime, #0f766e); color: white; border-color: var(--runtime, #0f766e); }

  table.deps { width: 100%; border-collapse: collapse; font-size: 13px; }
  .deps th { text-align: left; font-size: 11px; font-weight: 500; text-transform: uppercase; letter-spacing: .05em; color: var(--text-4); padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .deps td { padding: 10px 8px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: top; }
  .deps tr.dim td:not(.acts) { opacity: .75; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .tgt { display: flex; align-items: center; gap: 6px; }
  .kind { font-size: 10px; text-transform: uppercase; letter-spacing: .05em; padding: 1px 6px; border-radius: 4px; background: var(--spec-bg); color: var(--spec); }
  .kind.agent { background: var(--ephemeral-bg, #fff4e5); color: var(--ephemeral, #b45309); }
  .link { background: none; border: 0; padding: 0; color: var(--text-1); cursor: pointer; text-decoration: underline dotted; }
  .acts { white-space: nowrap; text-align: right; }
  .acts .btn { margin-left: 2px; }

  .empty { padding: 32px; text-align: center; color: var(--text-3); }

  /* Modal */
  .modal-bg { position: fixed; inset: 0; background: rgba(0,0,0,.25); z-index: 50; }
  .modal { position: fixed; z-index: 51; top: 20vh; left: 50%; transform: translateX(-50%); width: min(480px, calc(100vw - 32px)); background: var(--bg-surface); border-radius: var(--r); box-shadow: var(--shadow-lg); padding: 20px; }
  .modal h3 { margin: 0 0 8px; font-size: 16px; }
  .modal p { font-size: 13px; color: var(--text-2); margin: 0 0 10px; }
  .modal .callers { padding: 8px 10px; background: var(--bg-subtle); border-radius: var(--r-sm); }
  .modal label { display: flex; flex-direction: column; gap: 4px; font-size: 12.5px; color: var(--text-2); }
  .modal input { padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 13px; }
  .modal-acts { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }

  /* Probe */
  .probe { padding: 20px; display: flex; flex-direction: column; gap: 6px; }
  .probe h2 { font-size: 17px; margin: 0 0 4px; display: flex; align-items: center; gap: 8px; }
  .probe h4 { margin: 14px 0 4px; font-size: 12px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-4); font-weight: 500; }
  .probe p { margin: 0; font-size: 13px; }
  .checks, .trig { list-style: none; padding: 0; margin: 0; font-size: 13px; display: flex; flex-direction: column; gap: 4px; }
  .checks li.bad { color: var(--err); }
  .trig li { display: flex; gap: 8px; align-items: center; }
  .kpis { display: grid; grid-template-columns: repeat(6, 1fr); gap: 8px; }
  .kpis div { background: var(--bg-subtle); border-radius: var(--r-sm); padding: 8px; display: flex; flex-direction: column; align-items: center; }
  .kpis b { font-size: 15px; font-variant-numeric: tabular-nums; }
  .kpis span { font-size: 10.5px; color: var(--text-4); }
  table.runs { width: 100%; font-size: 12.5px; border-collapse: collapse; }
  .runs td { padding: 4px 6px; border-bottom: 1px solid var(--divider, var(--border)); }
</style>
