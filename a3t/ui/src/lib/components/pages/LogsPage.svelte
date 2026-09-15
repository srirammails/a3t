<script lang="ts">
  // Logs — the authorization decision log (every Cedar allow/deny recorded at the
  // choke points). This is the platform's security-relevant event stream. In CE
  // (allow-all) there are no decisions to record, so it reads empty.
  import { audit, getEdition } from '$lib/services/api';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import { onMount, onDestroy } from 'svelte';

  interface Rec {
    ts_ms: number; principal_kind: string; principal_id: string; on_behalf_of?: string | null;
    action: string; resource_type: string; resource_id: string;
    decision: string; policy_ids: string[]; reason?: string | null;
  }

  let records = $state<Rec[]>([]);
  let loading = $state(true);
  let error = $state('');
  let edition = $state<'ce' | 'ee' | null>(null);
  let limit = $state(100);
  let denialsOnly = $state(false);
  let q = $state('');
  let auto = $state(false);
  let timer: any = null;

  async function load() {
    loading = true; error = '';
    const ed = await getEdition();
    edition = (ed.ok && ed.data?.edition === 'ee') ? 'ee' : 'ce';
    const res = await audit.recent(limit, denialsOnly);
    if (res.ok) {
      records = (res.data?.records ?? []) as Rec[];
    } else {
      error = res.status === 403 ? 'Viewing the decision log requires an authenticated session.' : (res.error || 'Failed to load logs');
    }
    loading = false;
  }

  let shown = $derived(
    !q.trim() ? records
    : records.filter(r => `${r.principal_id} ${r.action} ${r.resource_type} ${r.resource_id} ${r.reason ?? ''}`.toLowerCase().includes(q.toLowerCase()))
  );

  function toggleAuto() {
    auto = !auto;
    if (auto) timer = setInterval(load, 4000); else { clearInterval(timer); timer = null; }
  }

  onMount(load);
  onDestroy(() => { if (timer) clearInterval(timer); });

  function when(ms: number): string { return ms ? new Date(ms).toLocaleString() : '—'; }
</script>

<div class="logs">
  <header class="head">
    <div>
      <h1>Logs</h1>
      <p class="sub">Authorization decision log — every allow/deny recorded at the policy choke points.</p>
    </div>
    <div class="head-actions">
      <button class="btn ghost" onclick={() => navigateTo('settings', 'Settings')}>⚙ Log settings</button>
      <label class="auto"><input type="checkbox" checked={auto} onchange={toggleAuto} /> Auto-refresh</label>
      <button class="btn" onclick={load} disabled={loading}>{loading ? 'Loading…' : '↻ Refresh'}</button>
    </div>
  </header>

  <div class="filters">
    <input class="inp grow" placeholder="filter by principal / action / resource / reason" bind:value={q} />
    <label class="chk"><input type="checkbox" bind:checked={denialsOnly} onchange={load} /> Denials only</label>
    <select class="inp" bind:value={limit} onchange={load}>
      {#each [50, 100, 200, 500] as n}<option value={n}>{n}</option>{/each}
    </select>
    <span class="count">{shown.length} shown</span>
  </div>

  {#if error}
    <div class="state err">{error}</div>
  {:else if edition === 'ce'}
    <div class="state">
      <p>The decision log is an <strong>Enterprise</strong> surface.</p>
      <p class="sub">This is a Community (CE) runtime — it runs allow-all, so no authorization decisions are recorded. Connect to an EE runtime (Settings → Runtime) to see the log.</p>
    </div>
  {:else if loading && records.length === 0}
    <div class="state">Loading…</div>
  {:else if shown.length === 0}
    <div class="state">No decisions recorded yet{denialsOnly ? ' (no denials)' : ''}.</div>
  {:else}
    <div class="table-wrap">
      <table class="tbl">
        <thead>
          <tr><th>Time</th><th>Decision</th><th>Principal</th><th>Action</th><th>Resource</th><th>Reason / Policy</th></tr>
        </thead>
        <tbody>
          {#each shown as r}
            <tr class:deny={r.decision === 'deny'}>
              <td class="mono" title={when(r.ts_ms)}>{when(r.ts_ms)}</td>
              <td><span class="pill {r.decision === 'deny' ? 'err' : 'ok'}">{r.decision.toUpperCase()}</span></td>
              <td class="mono">{r.principal_kind}:{r.principal_id}{#if r.on_behalf_of}<span class="obo"> obo {r.on_behalf_of}</span>{/if}</td>
              <td class="mono">{r.action}</td>
              <td class="mono">{r.resource_type}:{r.resource_id}</td>
              <td class="reason">{r.reason ?? ''}{#if r.policy_ids?.length}<span class="pids">{r.policy_ids.join(', ')}</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .logs { display: flex; flex-direction: column; height: 100%; overflow: hidden; color: var(--text-1); }
  .head { display: flex; justify-content: space-between; align-items: flex-start; padding: 20px 24px 14px; border-bottom: 1px solid var(--border); }
  .head h1 { font-family: var(--font-serif, serif); font-size: 24px; font-weight: 500; }
  .sub { font-size: 13px; color: var(--text-3); margin-top: 4px; }
  .head-actions { display: flex; align-items: center; gap: 12px; }
  .auto { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); }
  .btn { padding: 7px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 13px; cursor: pointer; }
  .btn:hover { background: var(--bg-subtle); }
  .btn.ghost { color: var(--text-3); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .filters { display: flex; align-items: center; gap: 12px; padding: 12px 24px; border-bottom: 1px solid var(--border); }
  .inp { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 13px; }
  .inp.grow { flex: 1; }
  .chk { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); }
  .count { font-size: 12px; color: var(--text-4); }
  .state { padding: 40px; text-align: center; color: var(--text-4); }
  .state.err { color: var(--warn, #d66); }
  .table-wrap { flex: 1; overflow: auto; }
  .tbl { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .tbl th { text-align: left; padding: 9px 16px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); border-bottom: 1px solid var(--border); position: sticky; top: 0; background: var(--bg-surface); }
  .tbl td { padding: 8px 16px; border-bottom: 1px solid var(--border); vertical-align: top; }
  .tbl tr.deny { background: rgba(210,90,90,0.06); }
  .mono { font-family: var(--font-mono, monospace); font-size: 11.5px; }
  .obo { color: var(--text-4); }
  .reason { color: var(--text-2); }
  .pids { display: block; font-size: 10.5px; color: var(--text-4); font-family: var(--font-mono, monospace); margin-top: 2px; }
  .pill { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 11px; font-weight: 600; border: 1px solid var(--border); }
  .pill.ok { background: rgba(60,180,110,0.14); border-color: rgba(60,180,110,0.4); color: #2fa46b; }
  .pill.err { background: rgba(210,90,90,0.14); border-color: rgba(210,90,90,0.4); color: #d25a5a; }
</style>
