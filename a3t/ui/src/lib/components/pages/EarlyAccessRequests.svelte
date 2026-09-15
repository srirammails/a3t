<script lang="ts">
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import { execFlow } from '$lib/services/api';
  import { onMount } from 'svelte';

  interface EARequest {
    id: string;
    name: string;
    email: string;
    company: string;
    use_case: string;
    status: string;
    created_at: string;
  }

  let loading = $state(true);
  let error = $state('');
  let requests = $state<EARequest[]>([]);

  async function load() {
    loading = true;
    error = '';
    // a3t-early-access reads (.input // {}).action — pass the wrapped list action.
    const res = await execFlow('a3t-early-access', 'a3t', { input: { action: 'list' } }, { silent: true });
    if (res.ok) {
      const rows = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
      requests = (rows as EARequest[])
        .slice()
        .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''));
    } else {
      error = res.error ?? 'Failed to load requests';
    }
    loading = false;
  }

  onMount(load);

  function fmtDate(iso: string): string {
    if (!iso) return '—';
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    return d.toLocaleString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }

</script>

<div class="ea-page">
  <PageBanner headline="Who wants in." highlightWord="wants"
    subtitle="Early-access requests submitted from the sign-in page."
    description="Every submission is persisted to the early_access_requests table. Reach out from here to onboard the right people."
    accent="spec" />

  <div class="ea-toolbar">
    <h3 class="ea-title">Early Access Requests</h3>
    <span class="ea-count">{requests.length}</span>
    <span style="flex:1"></span>
    <button class="ea-refresh" onclick={load} disabled={loading}>
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2v3h-3"/></svg>
      Refresh
    </button>
  </div>

  {#if loading}
    <div class="ea-empty">Loading…</div>
  {:else if error}
    <div class="ea-empty ea-err">{error}</div>
  {:else if requests.length === 0}
    <div class="ea-empty"><p>No requests yet.</p></div>
  {:else}
    <div class="ea-table">
      <div class="ea-head">
        <span class="c-name">Name</span>
        <span class="c-email">Email</span>
        <span class="c-company">Company</span>
        <span class="c-use">What they want to build</span>
        <span class="c-status">Status</span>
        <span class="c-date">Requested</span>
      </div>
      {#each requests as r}
        <div class="ea-row">
          <span class="c-name">{r.name || '—'}</span>
          <span class="c-email"><a href={`mailto:${r.email}`}>{r.email || '—'}</a></span>
          <span class="c-company">{r.company || '—'}</span>
          <span class="c-use" title={r.use_case}>{r.use_case || '—'}</span>
          <span class="c-status"><span class="ea-badge" class:new={r.status === 'new'}>{r.status || 'new'}</span></span>
          <span class="c-date">{fmtDate(r.created_at)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ea-page { height: 100%; overflow-y: auto; padding: 20px 24px; }
  .ea-toolbar { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .ea-title { font-family: var(--font-serif); font-size: 18px; font-weight: 500; color: var(--text-1); }
  .ea-count { font-family: var(--font-mono); font-size: 11px; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .ea-refresh { display: flex; align-items: center; gap: 5px; padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 12px; font-weight: 550; cursor: pointer; }
  .ea-refresh:hover:not(:disabled) { border-color: var(--spec); color: var(--spec); }
  .ea-refresh:disabled { opacity: 0.6; cursor: default; }

  .ea-empty { padding: 60px 20px; text-align: center; color: var(--text-4); font-size: 14px; }
  .ea-err { color: var(--err, #dc2626); }

  .ea-table { border: 1px solid var(--border); border-radius: var(--r-sm); overflow: hidden; }
  .ea-head, .ea-row {
    display: grid;
    grid-template-columns: 1.1fr 1.4fr 1fr 2fr 0.7fr 1.1fr;
    gap: 12px;
    padding: 9px 14px;
    align-items: center;
  }
  .ea-head {
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4);
  }
  .ea-row { border-bottom: 1px solid var(--border); font-size: 13px; color: var(--text-1); }
  .ea-row:last-child { border-bottom: none; }
  .ea-row:hover { background: var(--bg-subtle); }
  .c-email a { color: var(--spec); text-decoration: none; font-family: var(--font-mono); font-size: 12px; }
  .c-email a:hover { text-decoration: underline; }
  .c-use { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .c-date { font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .ea-badge { font-family: var(--font-mono); font-size: 10px; text-transform: uppercase; color: var(--text-4); background: var(--bg-subtle); padding: 1px 6px; border-radius: 3px; }
  .ea-badge.new { color: var(--spec); background: var(--spec-bg); }
</style>
