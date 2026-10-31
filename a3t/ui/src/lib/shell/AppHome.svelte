<script lang="ts">
  /// An app's home: what it is for, plus the sections you navigate by, as
  /// foldable groups of cards: Pinned (open only when something is pinned), then
  /// Latest, and — for pages that don't render their own — My and Shared.
  ///
  /// Shown when an app is open with nothing selected. The context bar stays hidden
  /// here on purpose; choosing something from these lists is what reveals it, with
  /// that item selected.
  import { navigateToDetail } from '$lib/stores/panels.svelte.ts';
  import { APP_CONTEXT, contextLabel, trackOpen, getPins, isPinned, setPins, contextRev_ } from '$lib/stores/apps.svelte.ts';
  import { loadAppItems, type AppItem } from './appItems';
  import { setPin } from '$lib/services/api';
  import { isPlatform } from '$lib/stores/context.svelte.ts';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';

  type Section = 'my' | 'shared';
  let { appType, title = '', embedded = false, sections = [] as Section[], myLabel = 'My items', sharedLabel = 'Shared' }:
    { appType: string; title?: string; embedded?: boolean;
      /// Extra sections after Pinned and Latest, for pages without their own.
      sections?: Section[]; myLabel?: string; sharedLabel?: string } = $props();

  /// Card colour per app, matching the accent the app's own cards use.
  const COLOR: Record<string, string> = {
    agents: '#1a7158', templates: '#5530a8', canvas: '#b45309', triggers: '#0f766e',
    resources: '#3a5a85', 'flow-editor': '#5530a8', projects: '#5530a8', chat: '#3a5a85', executions: '#555',
  };
  const color = $derived(COLOR[appType] ?? '#555');
  const initials = (label: string) => label.replace(/[^a-z0-9]+/gi, ' ').trim().split(' ')
    .map((w) => w[0]).join('').slice(0, 2).toUpperCase() || '?';

  /// What the app is for, in one line. Shown above the lists so an empty app still
  /// tells you what it does.
  const BLURB: Record<string, string> = {
    chat: 'Ask about the platform, build flows and agents, and run them — in conversation.',
    'flow-editor': 'Author CNCF Serverless Workflow definitions with a graph, code and form view.',
    resources: 'Databases, LLMs, MCP servers, secrets — everything agents need, in one registry.',
    agents: 'Configured agents: their template, model and the tools they may use.',
    projects: 'Namespaces you work in, who is in them, and the rules they run under.',
    templates: 'Reusable agent templates — the shape an agent is built from.',
    triggers: 'Webhooks and schedules that start your flows.',
    executions: 'Every run: status, timing, input and output.',
  };

  const HOW_MANY = 6;

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
  /// My: the projects' own items (and items with no project); Shared: the platform's.
  const mine = $derived(items.filter((i) => !isPlatform(i.namespace)));
  const shared = $derived(items.filter((i) => isPlatform(i.namespace)));
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

  async function togglePin(item: AppItem) {
    setPins(await setPin(appType, item.id, !isPinned(appType, item.id)));
  }
</script>

<div class="home" class:embedded>
  <div class="home-body">
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
    {#snippet cards(list: AppItem[])}
      <div class="home-grid">
        {#each list as item (item.id)}
          <EntityCard
            icon={item.badge ?? initials(item.label)}
            {color}
            iconStyle={item.badge ? item.badgeStyle ?? '' : ''}
            name={item.label}
            description={item.sub ?? ''}
            tags={item.status ? [item.status] : []}
            meta=""
            actionLabel=""
            project={item.namespace}
            pinned={pinnedIds.includes(item.id)}
            onpin={() => togglePin(item)}
            onclick={() => open(item)}
          />
        {/each}
      </div>
    {/snippet}

    <FoldSection title="Pinned" count={pinned.length}
                 hint={missingPins > 0 ? `${missingPins} pinned item${missingPins === 1 ? '' : 's'} no longer exist` : (pinned.length === 0 ? 'Pin anything with ☆' : '')}>
      {@render cards(pinned)}
    </FoldSection>
    <FoldSection title="Latest" count={recent.length}>
      {@render cards(recent)}
    </FoldSection>
    {#if sections.includes('my')}
      <FoldSection title={myLabel} count={mine.length}>
        {@render cards(mine)}
      </FoldSection>
    {/if}
    {#if sections.includes('shared')}
      <FoldSection title={sharedLabel} count={shared.length} hint="From the platform, usable in every project">
        {@render cards(shared)}
      </FoldSection>
    {/if}
  {/if}
  </div>
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

  .home-body { max-width: 1100px; margin: 0 auto; }
  .home.embedded .home-body { max-width: none; }
  .home-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px; }
</style>
