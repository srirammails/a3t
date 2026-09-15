<script lang="ts">
  // Executions — the run history persisted in the storage backend. Lists recent
  // workflow instances (list_executions), with a detail drawer per run
  // (get_execution_status: input / output / error / timing).
  import { executions } from '$lib/services/api';
  import { onMount, onDestroy } from 'svelte';

  interface Exec {
    instance_id: string; namespace: string; name: string; version?: string;
    status: string; started_at?: number | string; completed_at?: number | string;
    duration_us?: number; current_step?: string; input?: any; output?: any; error?: string | null;
  }

  let rows = $state<Exec[]>([]);
  let loading = $state(true);
  let error = $state('');
  let limit = $state(50);
  let fNamespace = $state('');
  let fName = $state('');
  let fStatus = $state('all');
  let auto = $state(false);
  let selected = $state<Exec | null>(null);
  let detail = $state<Exec | null>(null);
  let detailLoading = $state(false);
  let timer: any = null;

  async function load() {
    loading = true; error = '';
    const res = await executions.list({
      limit,
      namespace: fNamespace.trim() || undefined,
      name: fName.trim() || undefined,
    });
    if (res.ok) {
      const data = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
      rows = data as Exec[];
    } else {
      error = res.error || 'Failed to load executions';
    }
    loading = false;
  }

  let shown = $derived(fStatus === 'all' ? rows : rows.filter(r => r.status?.toLowerCase() === fStatus));

  async function openDetail(r: Exec) {
    selected = r; detail = r; detailLoading = true;
    const res = await executions.get(r.instance_id);
    if (res.ok && res.data) {
      detail = { ...r, ...(typeof res.data === 'object' ? res.data : {}) };
    }
    detailLoading = false;
  }

  function closeDetail() { selected = null; detail = null; }

  function toggleAuto() {
    auto = !auto;
    if (auto) { timer = setInterval(load, 4000); } else { clearInterval(timer); timer = null; }
  }

  onMount(load);
  onDestroy(() => { if (timer) clearInterval(timer); });

  // ── formatting ──
  function ms(v: number | string | undefined): number | null {
    if (v == null) return null;
    if (typeof v === 'number') return v < 1e12 ? v * 1000 : v; // secs → ms heuristic
    const t = Date.parse(v); return isNaN(t) ? null : t;
  }
  function ago(v: number | string | undefined): string {
    const t = ms(v); if (t == null) return '—';
    const d = Date.now() - t;
    if (d < 0) return 'just now';
    const s = Math.floor(d / 1000);
    if (s < 60) return `${s}s ago`;
    const m = Math.floor(s / 60); if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60); if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }
  function when(v: number | string | undefined): string {
    const t = ms(v); return t == null ? '—' : new Date(t).toLocaleString();
  }
  function dur(r: Exec): string {
    if (r.duration_us != null) {
      const msv = r.duration_us / 1000;
      if (msv < 1000) return `${Math.round(msv)}ms`;
      return `${(msv / 1000).toFixed(2)}s`;
    }
    const a = ms(r.started_at), b = ms(r.completed_at);
    if (a != null && b != null) return `${((b - a) / 1000).toFixed(2)}s`;
    return '—';
  }
  function cls(status: string): string {
    const s = (status || '').toLowerCase();
    if (s === 'completed') return 'ok';
    if (s === 'failed') return 'err';
    if (s === 'running' || s === 'queued' || s === 'pending') return 'run';
    return 'idle';
  }
  function pretty(v: any): string {
    if (v == null) return '';
    if (typeof v === 'string') { try { return JSON.stringify(JSON.parse(v), null, 2); } catch { return v; } }
    try { return JSON.stringify(v, null, 2); } catch { return String(v); }
  }

  const STATUSES = ['all', 'running', 'completed', 'failed', 'queued'];
</script>

<div class="execs">
  <header class="head">
    <div>
      <h1>Executions</h1>
      <p class="sub">Recent workflow runs from the storage backend. Click a run for its input, output, and errors.</p>
    </div>
    <div class="head-actions">
      <label class="auto"><input type="checkbox" checked={auto} onchange={toggleAuto} /> Auto-refresh</label>
      <button class="btn" onclick={load} disabled={loading}>{loading ? 'Loading…' : '↻ Refresh'}</button>
    </div>
  </header>

  <div class="filters">
    <input class="inp" placeholder="namespace" bind:value={fNamespace} onchange={load} />
    <input class="inp" placeholder="flow name" bind:value={fName} onchange={load} />
    <select class="inp" bind:value={fStatus}>{#each STATUSES as s}<option value={s}>{s}</option>{/each}</select>
    <select class="inp" bind:value={limit} onchange={load}>
      {#each [25, 50, 100, 200] as n}<option value={n}>{n} rows</option>{/each}
    </select>
    <span class="count">{shown.length} shown</span>
  </div>

  <div class="body" class:split={!!selected}>
    <div class="table-wrap">
      {#if error}
        <div class="state err">{error}</div>
      {:else if loading && rows.length === 0}
        <div class="state">Loading…</div>
      {:else if shown.length === 0}
        <div class="state">No executions{fStatus !== 'all' ? ` with status "${fStatus}"` : ''} yet.</div>
      {:else}
        <table class="tbl">
          <thead>
            <tr><th>Status</th><th>Flow</th><th>Started</th><th>Duration</th><th>Step</th></tr>
          </thead>
          <tbody>
            {#each shown as r}
              <tr class:active={selected?.instance_id === r.instance_id} onclick={() => openDetail(r)}>
                <td><span class="pill {cls(r.status)}">{r.status}</span></td>
                <td class="flow"><span class="fn">{r.namespace}/{r.name}</span>{#if r.version}<span class="ver">v{r.version}</span>{/if}<span class="iid">{r.instance_id}</span></td>
                <td title={when(r.started_at)}>{ago(r.started_at)}</td>
                <td>{dur(r)}</td>
                <td class="step">{r.current_step ?? '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    {#if selected && detail}
      <aside class="drawer">
        <div class="dr-head">
          <span class="pill {cls(detail.status)}">{detail.status}</span>
          <button class="x" onclick={closeDetail} aria-label="Close">✕</button>
        </div>
        <h2>{detail.namespace}/{detail.name}</h2>
        <div class="kv"><span>Instance</span><code>{detail.instance_id}</code></div>
        <div class="kv"><span>Started</span><span>{when(detail.started_at)}</span></div>
        <div class="kv"><span>Completed</span><span>{when(detail.completed_at)}</span></div>
        <div class="kv"><span>Duration</span><span>{dur(detail)}</span></div>
        {#if detail.current_step}<div class="kv"><span>Current step</span><span>{detail.current_step}</span></div>{/if}
        {#if detailLoading}<div class="state sm">Loading detail…</div>{/if}

        {#if detail.error}
          <h3 class="err-h">Error</h3>
          <pre class="code err-box">{detail.error}</pre>
        {/if}
        <h3>Input</h3>
        <pre class="code">{pretty(detail.input) || '—'}</pre>
        <h3>Output</h3>
        <pre class="code">{pretty(detail.output) || '—'}</pre>
      </aside>
    {/if}
  </div>
</div>

<style>
  .execs { display: flex; flex-direction: column; height: 100%; overflow: hidden; color: var(--text-1); }
  .head { display: flex; justify-content: space-between; align-items: flex-start; padding: 20px 24px 14px; border-bottom: 1px solid var(--border); }
  .head h1 { font-family: var(--font-serif, serif); font-size: 24px; font-weight: 500; }
  .sub { font-size: 13px; color: var(--text-3); margin-top: 4px; }
  .head-actions { display: flex; align-items: center; gap: 12px; }
  .auto { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); }
  .btn { padding: 7px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 13px; cursor: pointer; }
  .btn:hover { background: var(--bg-subtle); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .filters { display: flex; align-items: center; gap: 10px; padding: 12px 24px; border-bottom: 1px solid var(--border); }
  .inp { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 13px; }
  .count { margin-left: auto; font-size: 12px; color: var(--text-4); }
  .body { flex: 1; overflow: hidden; display: grid; grid-template-columns: 1fr; }
  .body.split { grid-template-columns: 1fr 420px; }
  .table-wrap { overflow: auto; }
  .state { padding: 40px; text-align: center; color: var(--text-4); }
  .state.err { color: var(--warn, #d66); }
  .state.sm { padding: 12px; font-size: 12px; }
  .tbl { width: 100%; border-collapse: collapse; font-size: 13px; }
  .tbl th { text-align: left; padding: 10px 16px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); border-bottom: 1px solid var(--border); position: sticky; top: 0; background: var(--bg-surface); }
  .tbl td { padding: 10px 16px; border-bottom: 1px solid var(--border); vertical-align: top; }
  .tbl tbody tr { cursor: pointer; }
  .tbl tbody tr:hover { background: var(--bg-subtle); }
  .tbl tbody tr.active { background: var(--bg-subtle); }
  .flow { display: flex; flex-direction: column; gap: 2px; }
  .fn { font-weight: 600; }
  .ver { font-size: 11px; color: var(--text-4); }
  .iid { font-size: 10.5px; color: var(--text-4); font-family: var(--font-mono, monospace); }
  .step { font-family: var(--font-mono, monospace); font-size: 12px; color: var(--text-3); }
  .pill { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 11px; font-weight: 600; border: 1px solid var(--border); }
  .pill.ok { background: rgba(60,180,110,0.14); border-color: rgba(60,180,110,0.4); color: #2fa46b; }
  .pill.err { background: rgba(210,90,90,0.14); border-color: rgba(210,90,90,0.4); color: #d25a5a; }
  .pill.run { background: rgba(90,140,230,0.14); border-color: rgba(90,140,230,0.4); color: #5a8ce6; }
  .pill.idle { color: var(--text-3); }
  .drawer { border-left: 1px solid var(--border); overflow-y: auto; padding: 16px 18px; display: flex; flex-direction: column; gap: 8px; background: var(--bg-surface); }
  .dr-head { display: flex; justify-content: space-between; align-items: center; }
  .x { background: none; border: none; color: var(--text-3); cursor: pointer; font-size: 14px; }
  .drawer h2 { font-size: 15px; font-weight: 600; word-break: break-all; }
  .drawer h3 { font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); margin-top: 10px; }
  .drawer h3.err-h { color: var(--warn, #d66); }
  .kv { display: flex; justify-content: space-between; gap: 10px; font-size: 12px; color: var(--text-3); }
  .kv code { font-family: var(--font-mono, monospace); font-size: 11px; color: var(--text-2); word-break: break-all; }
  .code { padding: 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, #12141c); color: var(--text-1); font-family: var(--font-mono, monospace); font-size: 11.5px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; max-height: 260px; overflow: auto; }
  .code.err-box { border-color: rgba(210,90,90,0.4); color: #e08; }
</style>
