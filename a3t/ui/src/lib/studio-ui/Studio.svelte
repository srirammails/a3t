<script lang="ts">
  /// The Studio: what a host shows (through `index.ts`). It owns its layout and
  /// workspace for `config.app`; the host supplies where it runs (`host`), what
  /// to open first, and optionally controls at the start of the tab bar (`lead`).
  import { untrack, type Snippet } from 'svelte';
  import Workbench from './Workbench.svelte';
  import HomeView from './views/HomeView.svelte';
  import { Studio as Layout } from './layout.svelte.ts';
  import { useWorkspace } from './workspace.svelte.ts';
  import type { Item, StudioHost } from '$lib/studio-core';
  import type { StudioConfig } from './studios';

  let { host, config, initial = [], lead }: {
    host: StudioHost;
    config: StudioConfig;
    /// Items to open on arrival (e.g. named in the address).
    initial?: Item[];
    lead?: Snippet;
  } = $props();

  // A Studio is created for one config and host; a different one is a new Studio.
  const { studio, workspace } = untrack(() => {
    const ws = useWorkspace(config.app, host);
    for (const it of initial) ws.open(it);
    return { studio: new Layout(config.app, config.layout), workspace: ws };
  });

  /// Open an item (hosts call this, e.g. from their own navigation).
  export function open(item: Item) {
    workspace.open(item);
  }
</script>

<Workbench {studio} {workspace} homeKind={config.homeKind} create={config.create} pin={config.pin} {lead}>
  {#snippet home()}
    <HomeView {config} {workspace} />
  {/snippet}
</Workbench>
