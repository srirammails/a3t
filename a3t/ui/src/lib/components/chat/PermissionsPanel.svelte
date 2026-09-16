<script lang="ts">
  /// What this conversation has allowed or declined, and the only place a
  /// refusal can be reopened.
  ///
  /// Lifting does NOT grant. It reopens the question, so the agent asks again
  /// next time it needs the tool — a permission is then given against a real
  /// request, with its arguments visible, rather than by a click in a list.
  import { listSessionPermissions, liftPermission, type SessionPermission } from '$lib/services/api';

  let { sessionId, rev = 0 }: { sessionId: string; rev?: number } = $props();

  let items = $state<SessionPermission[]>([]);
  let loading = $state(false);
  let busyKey = $state('');
  /// Plain `let`, not `$state`: the effect both reads and writes it, and as
  /// reactive state that self-retriggers — the shape that trips Svelte's
  /// update-depth guard.
  let loadedFor = '';

  const denied = $derived(items.filter((p) => p.verdict === 'denied'));
  const granted = $derived(items.filter((p) => p.verdict === 'granted'));

  async function load() {
    if (!sessionId) { items = []; return; }
    loading = true;
    items = await listSessionPermissions(sessionId);
    loading = false;
  }

  $effect(() => {
    const token = `${sessionId}:${rev}`;
    if (loadedFor === token) return;
    loadedFor = token;
    load();
  });

  /// Forget a recorded decision — a refusal ("Ask me again") or a grant
  /// ("Revoke"). Both re-open the question rather than flipping it.
  async function lift(p: SessionPermission) {
    busyKey = p.key;
    const ok = await liftPermission(sessionId, p.key);
    busyKey = '';
    if (ok) await load();
  }

  const when = (ms: number) => {
    try { return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); }
    catch { return ''; }
  };
</script>

<div class="pp">
  <div class="pp-head">
    <span class="pp-title">Permissions in this conversation</span>
    <button class="pp-refresh" onclick={load} disabled={loading} title="Refresh" aria-label="Refresh">
      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M14 8A6 6 0 112.6 5.5" /><path d="M2 2v4h4" />
      </svg>
    </button>
  </div>

  {#if loading && items.length === 0}
    <p class="pp-empty">Loading…</p>
  {:else if items.length === 0}
    <p class="pp-empty">
      Nothing asked yet. When the agent needs a tool it isn’t already allowed, it
      will ask here.
    </p>
  {:else}
    {#if denied.length > 0}
      <h4 class="pp-sub">Declined</h4>
      {#each denied as p}
        <div class="pp-row">
          <span class="pp-tool">
            <code>{p.resource_id}</code>
            <span class="pp-meta">{p.reason} · {when(p.decided_at_ms)}</span>
          </span>
          <button class="pp-lift" disabled={busyKey === p.key} onclick={() => lift(p)}
                  title="The agent will ask again next time it needs this">
            {busyKey === p.key ? '…' : 'Ask me again'}
          </button>
        </div>
      {/each}
    {/if}

    {#if granted.length > 0}
      <h4 class="pp-sub">Allowed</h4>
      {#each granted as p}
        <div class="pp-row">
          <span class="pp-tool">
            <code>{p.resource_id}</code>
            <span class="pp-meta">
              {p.scope === 'once' ? 'once' : 'this conversation'} · {when(p.decided_at_ms)}
            </span>
          </span>
          <!-- A grant must be as revocable as a refusal is liftable, or "for this
               conversation" can only be undone by deleting the conversation.
               Like lifting, this RE-OPENS the question rather than flipping it to
               a refusal: the agent asks again next time it needs the tool. -->
          <button class="pp-lift" disabled={busyKey === p.key} onclick={() => lift(p)}
                  title="Remove this permission — the agent will ask again next time">
            {busyKey === p.key ? '…' : 'Revoke'}
          </button>
        </div>
      {/each}
    {/if}
  {/if}

  <p class="pp-foot">
    These apply to this conversation only, and are deleted with it.
  </p>
</div>

<style>
  .pp { padding: 4px 2px; }
  .pp-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
  .pp-title { font-size: 11.5px; font-weight: 600; color: var(--text-2); }
  .pp-refresh {
    width: 22px; height: 22px; display: grid; place-items: center;
    border: 1px solid transparent; border-radius: 4px;
    background: transparent; color: var(--text-4); cursor: pointer;
  }
  .pp-refresh:hover:not(:disabled) { color: var(--text-1); border-color: var(--border); }
  .pp-refresh:disabled { opacity: 0.4; cursor: default; }

  .pp-sub {
    margin: 10px 0 5px; font-size: 9.5px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.07em; color: var(--text-4);
  }
  .pp-empty { margin: 6px 0; font-size: 11.5px; color: var(--text-4); line-height: 1.5; }

  .pp-row {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 6px 8px; margin-bottom: 4px;
    border: 1px solid var(--border); border-radius: var(--r-sm, 6px);
    background: var(--bg-surface);
  }
  .pp-tool { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .pp-tool code {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11.5px; color: var(--text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .pp-meta { font-size: 10px; color: var(--text-4); }
  .pp-lift {
    flex: 0 0 auto;
    padding: 4px 9px; font-size: 11px;
    border: 1px solid var(--border-strong); border-radius: 4px;
    background: var(--bg-page); color: var(--text-1); cursor: pointer;
  }
  .pp-lift:hover:not(:disabled) { border-color: var(--spec, #5530a8); }
  .pp-lift:disabled { opacity: 0.5; cursor: default; }

  .pp-foot { margin: 10px 0 0; font-size: 10px; color: var(--text-4); line-height: 1.4; }
</style>
