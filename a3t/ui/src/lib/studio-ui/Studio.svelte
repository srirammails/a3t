<script lang="ts">
  /// The Studio: what a host shows (through `index.ts`). It owns its layout and
  /// workspace for `config.app`; the host supplies where it runs (`host`), what
  /// to open first, and optionally controls at the start of the tab bar (`lead`).
  import { untrack, type Snippet } from 'svelte';
  import Workbench from './Workbench.svelte';
  import HomeView from './views/HomeView.svelte';
  import { Studio as Layout } from './layout.svelte.ts';
  import { useWorkspace } from './workspace.svelte.ts';
  import { resolveItem, type Item, type StudioHost } from '$lib/studio-core';
  import type { StudioConfig } from './studios';

  let { host, config, initial = [], request = null, lead }: {
    host: StudioHost;
    config: StudioConfig;
    /// Items to open on arrival (e.g. named in the address).
    initial?: Item[];
    /// An item to open now, from the host's navigation ("Open in Studio"). A new
    /// `at` opens it again (it is shown even when its tab is already open).
    request?: { item: Item; at: string | number } | null;
    lead?: Snippet;
  } = $props();

  // A Studio is created for one config and host; a different one is a new Studio.
  const { studio, workspace } = untrack(() => {
    const ws = useWorkspace(config.app, host);
    for (const it of initial) ws.open(it);
    return { studio: new Layout(config.app, config.layout), workspace: ws };
  });

  /// Open an item (hosts call this, e.g. from their own navigation). Templates
  /// and agents named without their id are looked up first.
  export async function open(item: Item) {
    workspace.open(await resolveItem(item));
  }

  let lastAt: string | number | null = null;
  $effect(() => {
    const r = request;
    if (!r || r.at === lastAt) return;
    lastAt = r.at;
    untrack(() => void open(r.item));
  });
</script>

<Workbench {studio} {workspace} homeKind={config.homeKind} pin={config.pin} {lead}>
  {#snippet home()}
    <HomeView {config} {workspace} />
  {/snippet}
</Workbench>
