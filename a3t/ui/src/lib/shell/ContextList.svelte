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
  import { loadAppItems, type AppItem } from './appItems';

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

  const shown = $derived(
    filter.trim()
      ? listed.filter((i) => i.label.toLowerCase().includes(filter.trim().toLowerCase()))
      : listed,
  );

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

<aside class="ctx" class:floating={!isContextPinned() && isContextOpen()}>
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

  {#if items.length > 8}
    <div class="ctx-filter">
      <input type="text" placeholder="Filter…" bind:value={filter} />
    </div>
  {/if}

  <div class="ctx-body">
    {#if loading}
      <div class="ctx-note">Loading…</div>
    {:else if error}
      <div class="ctx-note err">{error}</div>
    {:else if shown.length === 0}
      <div class="ctx-note">{filter ? 'Nothing matches.' : 'Nothing here yet.'}</div>
    {:else}
      {#each shown as item}
        <div class="ctx-row" class:active={item.id === activeId} class:pending={item.pending}>
          <button class="ctx-pick" onclick={() => select(item)} title={item.label}>
            {#if item.badge}
              <span class="ctx-badge" style={item.badgeStyle}>{item.badge}</span>
            {/if}
            <span class="ctx-text">
              <span class="ctx-label">{item.label}</span>
              {#if item.sub}<span class="ctx-sub">{item.sub}</span>{/if}
            </span>
          </button>
          {#if spec?.source === 'sessions' && !item.pending}
            <button class="ctx-del" onclick={(e) => removeItem(e, item)} title="Delete — this cannot be undone" aria-label="Delete">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M12 4L4 12M4 4l8 8" /></svg>
            </button>
          {/if}
        </div>
      {/each}
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
    padding: 5px 8px; font-size: 11.5px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-page); color: var(--text-1);
  }

  .ctx-body {
    flex: 1;
    min-height: 0;      /* required for overflow scrolling inside a flex column */
    overflow-y: auto;
    padding: 6px;
  }
  .ctx-note { padding: 12px 8px; font-size: 11.5px; color: var(--text-4); }
  .ctx-note.err { color: #dc2626; }

  .ctx-row { display: flex; align-items: center; border-radius: var(--r-sm); }
  .ctx-row:hover { background: var(--bg-subtle); }
  .ctx-row.active { background: var(--bg-subtle); }
  .ctx-row.active .ctx-label { color: var(--text-1); font-weight: 500; }
  .ctx-pick {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 8px;
    padding: 7px 8px;
    border: none; background: transparent; cursor: pointer; text-align: left;
  }
  .ctx-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  /* Matches the category chips on the Resources page, so the same category reads
     the same in the pane and in the app. */
  .ctx-badge {
    flex: 0 0 auto;
    width: 20px; height: 20px; border-radius: 5px;
    display: grid; place-items: center;
    font-size: 10px; font-weight: 600;
  }
  .ctx-label {
    font-size: 12px; color: var(--text-2);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ctx-sub { font-size: 10px; color: var(--text-4); }
  .ctx-row.pending .ctx-label { font-style: italic; }
  .ctx-del {
    width: 22px; height: 22px; margin-right: 4px;
    display: grid; place-items: center;
    border: none; border-radius: 4px;
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
