<script lang="ts">
  /// The per-app context list: conversations in Chat, flows in the Flow Editor,
  /// resources in Resources. It occupies the side region next to the app rail and
  /// is independently pinnable — unpinned it is revealed on demand from the Active
  /// Apps menu, which is what frees the column for a wider main view.
  import { navigateTo, navigateToDetail, getActivePanel } from '$lib/stores/panels.svelte.ts';
  import {
    APP_CONTEXT, contextLabel, isContextPinned, toggleContextPinned,
    isContextOpen, closeContextPane, trackOpen, contextRev_,
  } from '$lib/stores/apps.svelte.ts';
  import { deleteChatSession } from '$lib/services/api';
  import { loadAppItems, groupByNamespace, type AppItem } from './appItems';

  let { appType }: { appType: string } = $props();

  type Item = AppItem;

  let items = $state<Item[]>([]);
  let loading = $state(false);
  let error = $state('');
  let filter = $state('');
  /// Plain `let`, not `$state`: the effect below both reads and writes it, and as
  /// reactive state that self-retriggers — the shape that trips Svelte's
  /// update-depth guard and froze reactivity elsewhere in this app.
  let loadedFor = '';

  const spec = $derived(APP_CONTEXT[appType]);
  const activeId = $derived(getActivePanel()?.props?.selectedId ?? '');

  /// A conversation is created server-side by its FIRST TURN, so a chat you have
  /// just opened is in no list until you send something — which reads as the new
  /// session having been lost. Show the open one regardless, marked as unsent.
  const listed = $derived.by<Item[]>(() => {
    if (spec?.source !== 'sessions' || !activeId) return items;
    if (items.some((i) => i.id === activeId)) return items;
    const unsent: Item = { id: activeId, label: 'New conversation', sub: 'not sent yet', pending: true };
    return [unsent, ...items];
  });

  /// A hierarchical list (items carry `depth`) renders as a tree instead of
  /// namespace groups.
  const isTree = $derived(listed.some((i) => i.depth != null));

  /// Each tree row's ancestors, from the depth-first order.
  const ancestors = $derived.by(() => {
    const m = new Map<string, string[]>();
    const stack: Item[] = [];
    for (const it of listed) {
      const d = it.depth ?? 0;
      while (stack.length && (stack[stack.length - 1].depth ?? 0) >= d) stack.pop();
      m.set(it.id, stack.map((s) => s.id));
      stack.push(it);
    }
    return m;
  });
  const hasChildren = (i: number) => (listed[i + 1]?.depth ?? -1) > (listed[i]?.depth ?? 0);

  const shown = $derived.by<Item[]>(() => {
    const q = filter.trim().toLowerCase();
    if (isTree) {
      if (!q) return listed.filter((i) => !(ancestors.get(i.id) ?? []).some((a) => collapsed.has(a)));
      // A match keeps its ancestors, so it is still shown where it lives.
      const keep = new Set<string>();
      for (const i of listed) {
        if (i.label.toLowerCase().includes(q)) { keep.add(i.id); for (const a of ancestors.get(i.id) ?? []) keep.add(a); }
      }
      return listed.filter((i) => keep.has(i.id));
    }
    if (!q) return listed;
    // Namespace is matched too, so typing "a3t" narrows to that group.
    return listed.filter((i) =>
      i.label.toLowerCase().includes(q) || (i.namespace ?? '').toLowerCase().includes(q));
  });

  /// Rows grouped under their namespace, or null for lists that have none
  /// (conversations, resource categories) — those stay flat.
  const groups = $derived(groupByNamespace(shown));

  // ── Collapsed namespace groups, remembered per app in this browser ──
  const collapseKey = $derived(`a3t.ctx.collapsed.${appType}`);
  let collapsed = $state<Set<string>>(new Set());
  /// Plain `let` for the same reason as `loadedFor`: read and written below.
  let collapseLoadedFor = '';
  $effect(() => {
    const key = collapseKey;
    if (collapseLoadedFor === key) return;
    collapseLoadedFor = key;
    try { collapsed = new Set(JSON.parse(localStorage.getItem(key) ?? '[]')); }
    catch { collapsed = new Set(); }
  });
  function toggleGroup(ns: string) {
    const next = new Set(collapsed);
    next.has(ns) ? next.delete(ns) : next.add(ns);
    collapsed = next;
    try { localStorage.setItem(collapseKey, JSON.stringify([...next])); } catch {}
  }
  /// A filter shows every match: a collapsed group must not hide what was searched for.
  const isOpen = (ns: string) => !!filter.trim() || !collapsed.has(ns);

  /// Dot colour for a short state word. Unknown states get no dot rather than a
  /// guessed colour.
  function statusTone(s?: string): string | null {
    if (!s) return null;
    const v = s.toLowerCase();
    if (['completed', 'active', 'running', 'enabled', 'healthy', 'synced'].includes(v)) return v === 'running' ? 'run' : 'ok';
    if (['failed', 'error', 'cancelled', 'degraded'].includes(v)) return 'bad';
    if (['draft', 'disabled', 'queued', 'pending', 'hibernated'].includes(v)) return 'idle';
    // Paused is deliberate but not serving — it must stand out from "asleep".
    if (v === 'paused') return 'bad';
    return null;
  }

  /// Avatar letter: first alphanumeric of the label, so "a3t-users" reads as "A".
  const initial = (label: string) => (label.match(/[a-z0-9]/i)?.[0] ?? '·').toUpperCase();

  async function load(source: string) {
    loading = true;
    error = '';
    try {
      items = await loadAppItems(source);
    } catch (e: any) {
      error = e?.message ?? 'Failed to load';
      items = [];
    } finally {
      loading = false;
    }
  }

  // Reload when the app changes, or when the app signals it changed the data —
  // a first chat turn creates the session row, and without the revision the new
  // conversation stayed missing from this list until a manual refresh.
  let loadedRev = -1;
  $effect(() => {
    const t = appType;
    const rev = contextRev_();
    const src = APP_CONTEXT[t]?.source;
    if (!src) { items = []; loadedFor = ''; return; }
    if (loadedFor === t && loadedRev === rev) return;
    loadedFor = t;
    loadedRev = rev;
    load(src);
  });

  function select(item: Item) {
    const panel = getActivePanel();
    const title = panel?.title ?? contextLabel(appType);
    trackOpen(appType, title);
    if (spec?.focuses) navigateToDetail(appType, title, item.id);
    else navigateTo(appType, title);
    // A transient pane has done its job once something is chosen.
    if (!isContextPinned()) closeContextPane();
  }

  // An unpinned submenu is transient: a click anywhere on the page hides it.
  // Pinning is how you keep it. The ≡ toggle is excluded so it can still toggle,
  // and clicks inside the list (picking, filtering) leave it open.
  let asideEl: HTMLElement | undefined = $state();
  $effect(() => {
    if (isContextPinned() || !isContextOpen()) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (!t || asideEl?.contains(t) || t.closest('.smt')) return;
      closeContextPane();
    };
    // Next tick: the click that opened the pane must not also close it.
    const id = setTimeout(() => window.addEventListener('pointerdown', onDown, true));
    return () => { clearTimeout(id); window.removeEventListener('pointerdown', onDown, true); };
  });

  function newChat() {
    trackOpen('chat', 'AI Chat');
    // A fresh client id; the transcript row appears once the first turn completes.
    const id = `session-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    navigateToDetail('chat', 'AI Chat', id);
    if (!isContextPinned()) closeContextPane();
  }

  /// A blank editor: navigating with no selection is what FlowEditor treats as a
  /// new flow, so there is no separate "new" state to keep in sync.
  function newFlow() {
    trackOpen('flow-editor', 'Flow Editor');
    navigateTo('flow-editor', 'Flow Editor');
    if (!isContextPinned()) closeContextPane();
  }

  async function removeItem(e: MouseEvent, item: Item) {
    e.stopPropagation();
    if (spec?.source !== 'sessions' || !item.serverId) return;
    // Hard delete, and the only thing that removes a transcript (sessions have no
    // TTL) — so drop it from the list only once the server has confirmed.
    const ok = await deleteChatSession(item.serverId);
    if (!ok) return;
    items = items.filter((i) => i.id !== item.id);
    // Deleting the conversation you are READING left it on screen: the chat still
    // pointed at it, and since it was no longer in the list it even reappeared
    // here as a phantom "New conversation". Move to a genuinely new one.
    if (item.id === activeId) newChat();
  }
</script>

{#snippet row(item: Item)}
  {@const idx = listed.indexOf(item)}
  <div class="ctx-row" class:active={item.id === activeId} class:pending={item.pending}
       style={item.depth != null ? `padding-left:${item.depth * 14}px` : ''}>
    {#if isTree}
      {#if hasChildren(idx)}
        <button class="ctx-twist" onclick={() => toggleGroup(item.id)} aria-expanded={isOpen(item.id)} aria-label="Expand">
          <svg class="ctx-chev" class:open={isOpen(item.id)} width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M6 3l5 5-5 5" /></svg>
        </button>
      {:else}
        <span class="ctx-twist"></span>
      {/if}
    {/if}
    <button class="ctx-pick" onclick={() => select(item)} title={item.namespace ? `${item.namespace}/${item.label}` : item.label}>
      {#if item.badge}
        <span class="ctx-badge" style={item.badgeStyle}>{item.badge}</span>
      {:else if spec?.source !== 'sessions'}
        <span class="ctx-avatar">{initial(item.label)}</span>
      {/if}
      <span class="ctx-text">
        <span class="ctx-label">{item.label}</span>
        {#if item.sub}<span class="ctx-sub">{item.sub}</span>{/if}
      </span>
      {#if statusTone(item.status)}
        <span class="ctx-dot {statusTone(item.status)}" title={item.status}></span>
      {/if}
    </button>
    {#if spec?.source === 'sessions' && !item.pending}
      <button class="ctx-del" onclick={(e) => removeItem(e, item)} title="Delete — this cannot be undone" aria-label="Delete">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M12 4L4 12M4 4l8 8" /></svg>
      </button>
    {/if}
  </div>
{/snippet}

<aside class="ctx" bind:this={asideEl} class:floating={!isContextPinned() && isContextOpen()}>
  <div class="ctx-head">
    <span class="ctx-title">{contextLabel(appType)}</span>
    <div class="ctx-actions">
      {#if spec?.source === 'sessions'}
        <button class="ctx-icon" onclick={newChat} title="New conversation" aria-label="New conversation">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10" /></svg>
        </button>
      {:else if spec?.source === 'flows'}
        <!-- The editor's own "New flow" lived in the panel this pane replaced. -->
        <button class="ctx-icon" onclick={newFlow} title="New flow" aria-label="New flow">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10" /></svg>
        </button>
      {/if}
      <button
        class="ctx-icon"
        onclick={() => load(spec!.source)}
        title="Refresh"
        aria-label="Refresh"
        disabled={loading}
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M14 8A6 6 0 112.6 5.5" /><path d="M2 2v4h4" />
        </svg>
      </button>
      <button
        class="ctx-icon"
        class:on={isContextPinned()}
        onclick={() => toggleContextPinned()}
        title={isContextPinned() ? 'Unpin this list (widens the main view)' : 'Pin this list'}
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 10v4M5 3h6l-1 6H6L5 3z" />
        </svg>
      </button>
      {#if !isContextPinned()}
        <button class="ctx-icon" onclick={() => closeContextPane()} title="Close" aria-label="Close">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M12 4L4 12M4 4l8 8" /></svg>
        </button>
      {/if}
    </div>
  </div>

  {#if items.length > 8 || (groups?.length ?? 0) > 1}
    <div class="ctx-filter">
      <input type="text" placeholder="Filter by name or namespace…" bind:value={filter} />
    </div>
  {/if}

  <div class="ctx-body">
    {#if loading && items.length === 0}
      <div class="ctx-note">Loading…</div>
    {:else if error}
      <div class="ctx-note err">{error}</div>
    {:else if shown.length === 0}
      <div class="ctx-note">{filter ? 'Nothing matches.' : 'Nothing here yet.'}</div>
    {:else if isTree}
      {#each shown as item (item.id)}{@render row(item)}{/each}
    {:else if groups}
      {#each groups as g (g.ns)}
        <section class="ctx-group">
          <button class="ctx-ghead" onclick={() => toggleGroup(g.ns)} aria-expanded={isOpen(g.ns)}>
            <svg class="ctx-chev" class:open={isOpen(g.ns)} width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2"><path d="M6 3l5 5-5 5" /></svg>
            <span class="ctx-gname">{g.ns || 'no namespace'}</span>
            <span class="ctx-gcount">{g.items.length}</span>
          </button>
          {#if isOpen(g.ns)}
            <div class="ctx-gitems">
              {#each g.items as item}{@render row(item)}{/each}
            </div>
          {/if}
        </section>
      {/each}
    {:else}
      {#each shown as item}{@render row(item)}{/each}
    {/if}
  </div>

  {#if spec && !spec.focuses && items.length > 0}
    <div class="ctx-foot">Selecting opens {contextLabel(appType).toLowerCase()} in this app; jumping straight to an item isn’t wired up for it yet.</div>
  {/if}
</aside>

<style>
  .ctx {
    grid-area: context;
    display: flex;
    flex-direction: column;
    /* Grid/flex items default to min-height:auto, which lets a long list grow past
       the row instead of scrolling — the bug that hid older conversations. */
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid var(--border);
    background: var(--bg-surface);
  }
  /* Unpinned, the pane is revealed over the main view rather than resizing it. */
  .ctx.floating {
    position: absolute; top: 0; bottom: 0; left: 0; z-index: 60;
    width: 280px;
    box-shadow: 6px 0 24px rgb(0 0 0 / 0.12);
  }

  .ctx-head {
    flex: 0 0 auto;
    display: flex; align-items: center; justify-content: space-between;
    gap: 6px; padding: 10px 8px 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .ctx-title {
    font-family: var(--font-serif); font-size: 13px; font-weight: 500;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ctx-actions { display: flex; align-items: center; gap: 2px; }
  .ctx-icon {
    width: 24px; height: 24px;
    display: grid; place-items: center;
    border: 1px solid transparent; border-radius: 5px;
    background: transparent; color: var(--text-4); cursor: pointer;
  }
  .ctx-icon:hover:not(:disabled) { color: var(--text-1); border-color: var(--border); }
  .ctx-icon:disabled { opacity: 0.4; cursor: default; }
  .ctx-icon.on { color: var(--text-1); background: var(--bg-subtle); border-color: var(--border); }

  .ctx-filter { flex: 0 0 auto; padding: 8px 10px; border-bottom: 1px solid var(--divider, var(--border)); }
  .ctx-filter input {
    width: 100%; box-sizing: border-box;
    padding: 6px 9px; font-size: 11.5px;
    border: 1px solid var(--border); border-radius: 7px;
    background: var(--bg-page); color: var(--text-1);
    outline: none; transition: border-color 0.1s ease, box-shadow 0.1s ease;
  }
  .ctx-filter input:focus {
    border-color: var(--spec, #5530a8);
    box-shadow: 0 0 0 3px var(--spec-bg, #ede7ff);
  }

  .ctx-body {
    flex: 1;
    min-height: 0;      /* required for overflow scrolling inside a flex column */
    overflow-y: auto;
    padding: 6px 6px 10px;
  }
  .ctx-note { padding: 14px 10px; font-size: 11.5px; color: var(--text-4); }
  .ctx-note.err { color: #dc2626; }

  /* ── Namespace groups ── */
  .ctx-group + .ctx-group { margin-top: 6px; }
  .ctx-ghead {
    position: sticky; top: -6px; z-index: 1;
    width: 100%; display: flex; align-items: center; gap: 6px;
    padding: 7px 8px 5px;
    border: none; background: var(--bg-surface); cursor: pointer;
    color: var(--text-4); text-align: left;
  }
  .ctx-ghead:hover { color: var(--text-2); }
  .ctx-chev { flex: 0 0 auto; transition: transform 0.12s ease; }
  .ctx-chev.open { transform: rotate(90deg); }
  .ctx-gname {
    flex: 1; min-width: 0;
    font-size: 10px; font-weight: 600; letter-spacing: 0.07em; text-transform: uppercase;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ctx-gcount {
    flex: 0 0 auto; min-width: 18px; padding: 0 5px;
    font-size: 10px; line-height: 16px; text-align: center; font-variant-numeric: tabular-nums;
    border-radius: 8px; background: var(--bg-subtle); color: var(--text-4);
  }
  .ctx-gitems { display: flex; flex-direction: column; gap: 1px; }

  /* ── Rows ── */
  .ctx-twist {
    flex: none; width: 16px; height: 22px; display: grid; place-items: center;
    background: none; border: 0; padding: 0; color: var(--text-4); cursor: pointer;
  }
  .ctx-row {
    position: relative;
    display: flex; align-items: center;
    border-radius: 7px;
    transition: background 0.1s ease;
  }
  .ctx-row:hover { background: var(--bg-subtle); }
  .ctx-row.active { background: var(--bg-subtle); }
  /* A bar, not just a tint, so the open item is findable in a long list. */
  .ctx-row.active::before {
    content: ''; position: absolute; left: 0; top: 7px; bottom: 7px;
    width: 3px; border-radius: 0 3px 3px 0; background: var(--spec, #5530a8);
  }
  .ctx-pick {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 9px;
    padding: 6px 8px 6px 10px;
    border: none; background: transparent; cursor: pointer; text-align: left;
  }
  .ctx-avatar {
    flex: 0 0 auto;
    width: 24px; height: 24px; border-radius: 6px;
    display: grid; place-items: center;
    font-size: 11px; font-weight: 600;
    color: var(--text-3); background: var(--bg-page);
    border: 1px solid var(--border);
  }
  .ctx-row.active .ctx-avatar { color: var(--spec, #5530a8); border-color: var(--spec-bg, #ede7ff); background: var(--spec-bg, #ede7ff); }
  .ctx-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  /* Matches the category chips on the Resources page, so the same category reads
     the same in the pane and in the app. */
  .ctx-badge {
    flex: 0 0 auto;
    width: 24px; height: 24px; border-radius: 6px;
    display: grid; place-items: center;
    font-size: 10px; font-weight: 600;
  }
  .ctx-label {
    font-size: 12.5px; line-height: 1.3; color: var(--text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ctx-row.active .ctx-label { font-weight: 600; }
  .ctx-sub {
    font-size: 10.5px; line-height: 1.3; color: var(--text-4);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ctx-row.pending .ctx-label { font-style: italic; color: var(--text-3); }

  .ctx-dot { flex: 0 0 auto; width: 7px; height: 7px; border-radius: 50%; }
  .ctx-dot.ok   { background: #16a34a; }
  .ctx-dot.run  { background: #2563eb; }
  .ctx-dot.bad  { background: #dc2626; }
  .ctx-dot.idle { background: transparent; border: 1.5px solid var(--text-5, #b8b8b8); }

  .ctx-del {
    width: 22px; height: 22px; margin-right: 4px;
    display: grid; place-items: center;
    border: none; border-radius: 5px;
    background: transparent; color: var(--text-4); cursor: pointer;
    opacity: 0;
  }
  .ctx-row:hover .ctx-del { opacity: 1; }
  .ctx-del:hover { color: #dc2626; background: var(--bg-surface); }

  .ctx-foot {
    flex: 0 0 auto;
    padding: 8px 12px; border-top: 1px solid var(--border);
    font-size: 10px; line-height: 1.35; color: var(--text-4);
  }
</style>
