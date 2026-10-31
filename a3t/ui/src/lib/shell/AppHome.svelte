<script lang="ts">
  /// An app's home: what it is for, plus the two lists you actually navigate by —
  /// the latest few items and the ones you pinned.
  ///
  /// Shown when an app is open with nothing selected. The context bar stays hidden
  /// here on purpose; choosing something from these lists is what reveals it, with
  /// that item selected.
  import { navigateToDetail } from '$lib/stores/panels.svelte.ts';
  import { APP_CONTEXT, contextLabel, trackOpen, getPins, isPinned, setPins, contextRev_ } from '$lib/stores/apps.svelte.ts';
  import { loadAppItems, type AppItem } from './appItems';
  import { setPin } from '$lib/services/api';

  let { appType, title = '', embedded = false }:
    { appType: string; title?: string; embedded?: boolean } = $props();

  /// What the app is for, in one line. Shown above the lists so an empty app still
  /// tells you what it does.
  const BLURB: Record<string, string> = {
    chat: 'Ask about the platform, build flows and agents, and run them — in conversation.',
    'flow-editor': 'Author CNCF Serverless Workflow definitions with a graph, code and form view.',
    resources: 'Databases, LLMs, MCP servers, secrets — everything agents need, in one registry.',
    agents: 'Configured agents: their template, model and the tools they may use.',
    templates: 'Reusable agent templates — the shape an agent is built from.',
    triggers: 'Webhooks and schedules that start your flows.',
    executions: 'Every run: status, timing, input and output.',
  };

  const HOW_MANY = 5;

  let items = $state<AppItem[]>([]);
  let loading = $state(true);
  let error = $state('');
  let loadedFor = '';

  const pinnedIds = $derived(getPins(appType));

  /// Items are returned newest-first by their source where that is meaningful
  /// (sessions, executions); for the rest this is simply the first few.
  const recent = $derived(items.filter((i) => !pinnedIds.includes(i.id)).slice(0, HOW_MANY));
  const pinned = $derived(
    pinnedIds.map((id) => items.find((i) => i.id === id)).filter(Boolean).slice(0, HOW_MANY) as AppItem[],
  );
  /// A pin whose item no longer exists — worth showing rather than silently dropping.
  const missingPins = $derived(pinnedIds.filter((id) => !items.some((i) => i.id === id)).length);

  $effect(() => {
    // Reload when the project or "show platform" changes, not only the app.
    const t = `${appType}|${contextRev_()}`;
    const src = APP_CONTEXT[appType]?.source;
    if (!src) { items = []; loading = false; return; }
    if (loadedFor === t) return;
    loadedFor = t;
    loading = true;
    loadAppItems(src)
      .then((r) => { items = r; error = ''; })
      .catch((e) => { error = e?.message ?? 'Failed to load'; items = []; })
      .finally(() => { loading = false; });
  });

  function open(item: AppItem) {
    trackOpen(appType, title || contextLabel(appType));
    // Selecting is what reveals the context bar — see `contextVisible`.
    navigateToDetail(appType, title || contextLabel(appType), item.id);
  }

  async function togglePin(e: MouseEvent, item: AppItem) {
    e.stopPropagation();
    setPins(await setPin(appType, item.id, !isPinned(appType, item.id)));
  }
</script>

<div class="home" class:embedded>
  {#if !embedded}
    <div class="home-head">
      <h1 class="home-title">{title || contextLabel(appType)}</h1>
      <p class="home-blurb">{BLURB[appType] ?? ''}</p>
    </div>
  {/if}

  {#if error}
    <div class="home-note err">{error}</div>
  {:else if loading}
    <div class="home-note">Loading…</div>
  {:else}
    <div class="home-lists">
      <section class="home-col">
        <h2 class="home-sub">Pinned</h2>
        {#if pinned.length === 0}
          <p class="home-empty">Nothing pinned yet — use the ★ on any item.</p>
        {:else}
          {#each pinned as item}
            <!-- A row is a div, not a button: the pin is itself a button and nesting
                 them makes the browser rewrite the markup out from under Svelte. -->
            <div class="home-row">
              <button class="home-open" onclick={() => open(item)} title={item.label}>
                {#if item.badge}<span class="home-badge" style={item.badgeStyle}>{item.badge}</span>{/if}
                <span class="home-text">
                  <span class="home-label">{item.label}</span>
                  {#if item.namespace || item.sub}<span class="home-meta">{[item.namespace, item.sub].filter(Boolean).join(' · ')}</span>{/if}
                </span>
              </button>
              <button class="home-pin on" onclick={(e) => togglePin(e, item)} title="Unpin" aria-label="Unpin">★</button>
            </div>
          {/each}
        {/if}
        {#if missingPins > 0}
          <p class="home-empty">{missingPins} pinned item{missingPins === 1 ? '' : 's'} no longer exist.</p>
        {/if}
      </section>

      <section class="home-col">
        <h2 class="home-sub">Latest</h2>
        {#if recent.length === 0}
          <p class="home-empty">Nothing here yet.</p>
        {:else}
          {#each recent as item}
            <!-- A row is a div, not a button: the pin is itself a button and nesting
                 them makes the browser rewrite the markup out from under Svelte. -->
            <div class="home-row">
              <button class="home-open" onclick={() => open(item)} title={item.label}>
                {#if item.badge}<span class="home-badge" style={item.badgeStyle}>{item.badge}</span>{/if}
                <span class="home-text">
                  <span class="home-label">{item.label}</span>
                  {#if item.namespace || item.sub}<span class="home-meta">{[item.namespace, item.sub].filter(Boolean).join(' · ')}</span>{/if}
                </span>
              </button>
              <button class="home-pin" onclick={(e) => togglePin(e, item)} title="Pin" aria-label="Pin">☆</button>
            </div>
          {/each}
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .home {
    height: 100%; overflow-y: auto;
    padding: 40px 32px;
    box-sizing: border-box;
  }
  /* Inside the chat landing the composer already provides the heading. */
  .home.embedded { height: auto; padding: 8px 0 0; overflow: visible; }

  .home-head { max-width: 900px; margin: 0 auto 28px; text-align: center; }
  .home-title {
    font-family: var(--font-serif); font-size: 28px; font-weight: 500;
    margin: 0 0 6px; color: var(--text-1);
  }
  .home-blurb { margin: 0; font-size: 13px; color: var(--text-4); }

  .home-note { text-align: center; font-size: 12px; color: var(--text-4); padding: 24px; }
  .home-note.err { color: #dc2626; }

  .home-lists {
    max-width: 900px; margin: 0 auto;
    display: grid; grid-template-columns: 1fr 1fr; gap: 28px;
  }
  /* One column when the pane is narrow — two would truncate every label. */
  @media (max-width: 820px) {
    .home-lists { grid-template-columns: 1fr; gap: 20px; }
  }

  .home-sub {
    font-size: 10px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.07em; color: var(--text-4);
    margin: 0 0 8px; padding-left: 2px;
  }
  .home-empty { margin: 0; font-size: 11.5px; color: var(--text-5); padding: 6px 2px; }

  .home-row {
    width: 100%; box-sizing: border-box;
    display: flex; align-items: center; gap: 4px;
    padding: 0 8px 0 0; margin-bottom: 4px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface);
  }
  .home-row:hover { border-color: var(--border-strong); }
  .home-open {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 9px;
    padding: 9px 10px;
    border: none; background: transparent; cursor: pointer; text-align: left;
  }
  .home-badge {
    flex: 0 0 auto; width: 20px; height: 20px; border-radius: 5px;
    display: grid; place-items: center; font-size: 10px; font-weight: 600;
  }
  .home-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .home-label {
    font-size: 12.5px; color: var(--text-1);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .home-meta { font-size: 10px; color: var(--text-4); }
  .home-pin {
    flex: 0 0 auto; width: 22px; height: 22px;
    border: none; border-radius: 4px; background: transparent;
    color: var(--text-5); cursor: pointer; font-size: 13px; line-height: 1;
  }
  .home-pin:hover { color: var(--text-1); background: var(--bg-subtle); }
  .home-pin.on { color: var(--spec-ink, var(--text-1)); }
</style>
