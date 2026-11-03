<script lang="ts">
  /// The warnings and errors the console has kept, in the same pop-in as Profile:
  /// the list on the left (newest first, unread in bold), the selected one in full
  /// on the right. Opened from the bell in the header, or "All notifications" in
  /// the status bar.
  import { notifications, notificationsPanel, type Message } from '$lib/core/status/messages';

  let list = $state<Message[]>([]);
  let panel = $state<{ open: boolean; id: number | null }>({ open: false, id: null });
  $effect(() => notifications.subscribe((l) => (list = l)));
  $effect(() => notificationsPanel.subscribe((p) => (panel = p)));

  type Filter = 'all' | 'error' | 'warning';
  let filter = $state<Filter>('all');
  let closing = $state(false);
  let copied = $state(false);

  const shown = $derived(filter === 'all' ? list : list.filter((m) => m.type === filter));
  const errors = $derived(list.filter((m) => m.type === 'error').length);
  const warnings = $derived(list.filter((m) => m.type === 'warning').length);
  const selected = $derived(list.find((m) => m.id === panel.id) ?? shown[0] ?? null);

  // What is shown is read.
  $effect(() => { if (panel.open && selected && !selected.read) notifications.markRead(selected.id); });

  function pick(m: Message) { notificationsPanel.set({ open: true, id: m.id }); }
  function close() {
    closing = true;
    setTimeout(() => { notificationsPanel.set({ open: false, id: null }); closing = false; }, 200);
  }
  function remove(m: Message) {
    const i = shown.findIndex((x) => x.id === m.id);
    const next = shown[i + 1] ?? shown[i - 1] ?? null;
    notifications.remove(m.id);
    notificationsPanel.set({ open: true, id: next?.id ?? null });
  }
  async function copy(m: Message) {
    const text = [m.title, m.detail, m.source ? `source: ${m.source}` : '', `at: ${new Date(m.at).toISOString()}`].filter(Boolean).join('\n\n');
    try { await navigator.clipboard?.writeText(text); copied = true; setTimeout(() => (copied = false), 1200); } catch { /* no clipboard */ }
  }
  function ago(t: number) {
    const s = Math.round((Date.now() - t) / 1000);
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    return new Date(t).toLocaleDateString();
  }
  const when = (t: number) => new Date(t).toLocaleString();
  function onkey(e: KeyboardEvent) { if (panel.open && e.key === 'Escape') close(); }
</script>

<svelte:window onkeydown={onkey} />

{#if panel.open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="slide-overlay" class:closing onclick={close}></div>
  <div class="slide-panel" class:closing role="dialog" aria-label="Notifications">
    <div class="sp-layout">
      <!-- LEFT: the list -->
      <div class="sp-tree-panel">
        <div class="sp-tree-head">
          <div class="sp-tree-icon">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 11V7a4 4 0 018 0v4l1.5 1.5h-11zM6.5 14a1.5 1.5 0 003 0" /></svg>
          </div>
          <div class="sp-tree-info">
            <div class="sp-tree-name">Notifications</div>
            <div class="sp-tree-meta">{errors} error{errors === 1 ? '' : 's'} · {warnings} warning{warnings === 1 ? '' : 's'}</div>
          </div>
        </div>
        <div class="filters">
          {#each [['all', 'All'], ['error', 'Errors'], ['warning', 'Warnings']] as [f, label] (f)}
            <button class:active={filter === f} onclick={() => (filter = f as Filter)}>{label}</button>
          {/each}
        </div>
        <nav class="sp-tree">
          {#each shown as m (m.id)}
            <button class="nt" class:active={selected?.id === m.id} class:unread={!m.read} onclick={() => pick(m)}>
              <span class="mark {m.type}" aria-label={m.type}></span>
              <span class="nt-body">
                <span class="nt-title">{m.title}</span>
                <span class="nt-meta">{ago(m.at)}{#if m.source} · {m.source}{/if}{#if m.count > 1} · ×{m.count}{/if}</span>
              </span>
            </button>
          {:else}
            <div class="empty-l">{filter === 'all' ? 'Nothing yet.' : `No ${filter}s.`}</div>
          {/each}
        </nav>
        {#if list.length}
          <div class="tree-foot">
            <button onclick={() => notifications.markRead()}>Mark all read</button>
            <button onclick={() => { notifications.clear(); notificationsPanel.set({ open: true, id: null }); }}>Clear all</button>
          </div>
        {/if}
      </div>

      <!-- RIGHT: the selected one -->
      <div class="sp-main">
        <div class="sp-close-bar">
          {#if selected}
            <div class="sp-btn-wrap">
              <button class="sp-close" onclick={() => copy(selected)} aria-label="Copy">
                {#if copied}
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M3.5 8.5l3 3 6-7" /></svg>
                {:else}
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="5" y="5" width="9" height="9" rx="1.5" /><path d="M11 5V3a1 1 0 00-1-1H3a1 1 0 00-1 1v7a1 1 0 001 1h2" /></svg>
                {/if}
              </button>
              <span class="sp-tooltip">{copied ? 'Copied' : 'Copy'}</span>
            </div>
            <div class="sp-btn-wrap">
              <button class="sp-close" onclick={() => remove(selected)} aria-label="Delete">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4" /></svg>
              </button>
              <span class="sp-tooltip">Delete</span>
            </div>
          {/if}
          <div class="sp-btn-wrap">
            <button class="sp-close" onclick={close} aria-label="Close">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8" /></svg>
            </button>
            <span class="sp-tooltip">Close</span>
          </div>
        </div>
        <div class="sp-content">
          {#if selected}
            <div class="sp-form">
              <span class="badge {selected.type}">{selected.type === 'error' ? 'Error' : 'Warning'}</span>
              <h3>{selected.title}</h3>
              <dl class="facts">
                {#if selected.source}<dt>From</dt><dd class="mono">{selected.source}</dd>{/if}
                <dt>When</dt><dd>{when(selected.at)}</dd>
                {#if selected.count > 1}<dt>Times</dt><dd>{selected.count}, first {when(selected.first)}</dd>{/if}
              </dl>
              {#if selected.detail}
                <div class="lbl">Message</div>
                <pre class="detail">{selected.detail}</pre>
              {/if}
            </div>
          {:else}
            <div class="empty">
              <svg width="28" height="28" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M4 11V7a4 4 0 018 0v4l1.5 1.5h-11zM6.5 14a1.5 1.5 0 003 0" /></svg>
              <p>No warnings or errors. They are kept here when they happen.</p>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* The pop-in: the same slide panel as Profile (shell/ProfileModal). */
  .slide-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.12); z-index: 100; animation: overlayIn 0.2s ease-out; }
  .slide-overlay.closing { animation: overlayOut 0.2s ease-in forwards; }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes overlayOut { from { opacity: 1; } to { opacity: 0; } }
  .slide-panel {
    position: fixed; top: 42px; left: 110px; right: 6px; bottom: 6px;
    background: var(--bg-page); border: 1px solid var(--border); border-radius: 14px; z-index: 101; overflow: hidden;
    box-shadow: 0 8px 40px rgba(0,0,0,0.14), 0 2px 10px rgba(0,0,0,0.06), -4px 0 20px rgba(0,0,0,0.04);
    animation: panelIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .slide-panel.closing { animation: panelOut 0.2s ease-in forwards; }
  @keyframes panelIn { from { transform: translateX(100%); } to { transform: translateX(0); } }
  @keyframes panelOut { from { transform: translateX(0); } to { transform: translateX(100%); } }

  .sp-layout { display: grid; grid-template-columns: 300px 1fr; height: 100%; }
  .sp-tree-panel { display: flex; flex-direction: column; background: var(--bg-surface); border-right: 1px solid var(--border); min-height: 0; }
  .sp-tree-head { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--border); }
  .sp-tree-icon { width: 28px; height: 28px; border-radius: 6px; display: grid; place-items: center; color: white; flex-shrink: 0; background: linear-gradient(135deg, var(--warn), var(--err)); }
  .sp-tree-info { min-width: 0; }
  .sp-tree-name { font-family: var(--font-serif); font-size: 13px; font-weight: 500; }
  .sp-tree-meta { font-size: 10px; color: var(--text-4); margin-top: 1px; }

  .filters { display: flex; gap: 2px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .filters button { border: 0; background: none; padding: 4px 9px; border-radius: 5px; font-size: 11.5px; color: var(--text-3); cursor: pointer; }
  .filters button:hover { background: var(--bg-subtle); color: var(--text-1); }
  .filters button.active { background: var(--bg-subtle); color: var(--text-1); font-weight: 550; }

  .sp-tree { display: flex; flex-direction: column; padding: 4px 0; flex: 1; overflow-y: auto; min-height: 0; }
  .nt { display: flex; align-items: flex-start; gap: 9px; padding: 8px 14px; border: 0; background: none; cursor: pointer; text-align: left; color: var(--text-3); }
  .nt:hover { background: var(--bg-subtle); color: var(--text-1); }
  .nt.active { background: var(--bg-subtle); color: var(--text-1); box-shadow: inset 3px 0 0 var(--spec, #c47028); }
  .nt.unread .nt-title { font-weight: 600; color: var(--text-1); }
  .mark { width: 8px; height: 8px; border-radius: 50%; margin-top: 4px; flex-shrink: 0; }
  .mark.error { background: var(--err); }
  .mark.warning { background: var(--warn); }
  .nt-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .nt-title { font-size: 12px; line-height: 1.35; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
  .nt-meta { font-size: 10.5px; color: var(--text-4); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .empty-l { padding: 16px 14px; font-size: 12px; color: var(--text-4); }
  .tree-foot { display: flex; justify-content: space-between; padding: 8px 14px; border-top: 1px solid var(--border); }
  .tree-foot button { border: 0; background: none; font-size: 11.5px; color: var(--spec); cursor: pointer; padding: 0; }
  .tree-foot button:hover { text-decoration: underline; }

  .sp-main { overflow: hidden; position: relative; padding-top: 44px; }
  .sp-close-bar { position: absolute; top: 0; left: 0; right: 0; height: 44px; display: flex; align-items: center; justify-content: flex-end; gap: 6px; padding: 0 12px; z-index: 10; background: var(--bg-page); border-bottom: 1px solid var(--border); }
  .sp-btn-wrap { position: relative; }
  .sp-tooltip { position: absolute; bottom: -30px; left: 50%; transform: translateX(-50%); padding: 4px 10px; background: var(--text-1); color: var(--bg-page); font-size: 11px; font-weight: 500; white-space: nowrap; border-radius: 5px; pointer-events: none; opacity: 0; transition: opacity 0.15s 0.3s; z-index: 20; }
  .sp-tooltip::before { content: ''; position: absolute; bottom: 100%; left: 50%; transform: translateX(-50%); border: 4px solid transparent; border-bottom-color: var(--text-1); }
  .sp-btn-wrap:hover .sp-tooltip { opacity: 1; }
  .sp-close { width: 32px; height: 32px; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-surface); color: var(--text-3); cursor: pointer; display: grid; place-items: center; transition: all 0.12s; }
  .sp-close:hover { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }

  .sp-content { height: 100%; overflow-y: auto; }
  .sp-form { padding: 24px 32px; max-width: 820px; }
  .sp-form h3 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; margin: 10px 0 14px; line-height: 1.35; overflow-wrap: anywhere; }
  .badge { display: inline-block; font-size: 10.5px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; padding: 2px 8px; border-radius: 10px; }
  .badge.error { background: var(--err-bg); color: var(--err); }
  .badge.warning { background: var(--warn-bg); color: var(--warn); }
  .facts { display: grid; grid-template-columns: 70px 1fr; gap: 6px 12px; margin: 0 0 18px; font-size: 12.5px; }
  .facts dt { color: var(--text-4); }
  .facts dd { margin: 0; color: var(--text-2); }
  .mono { font-family: var(--font-mono); font-size: 12px; }
  .lbl { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-4); margin-bottom: 6px; }
  .detail { margin: 0; padding: 12px 14px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm, 6px);
            font: 12px/1.55 var(--font-mono); color: var(--text-1); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--text-4); font-size: 12.5px; }
</style>
