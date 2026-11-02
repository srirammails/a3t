<script lang="ts">
  /// The Console's Flow Editor page: the Studio with the flows configuration,
  /// hosted by the web dash. The Studio is used only through its entry point.
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import { Studio, STUDIOS } from '$lib/studio-ui';
  import { webHost } from '$lib/host/web.svelte.ts';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import type { Item } from '$lib/studio-core';

  // A flow named in the address (/flow-editor/<ns>~<name>) opens on arrival.
  const sel = getActivePanel()?.props?.selectedId as string | undefined;
  const initial: Item[] = sel?.includes('~') ? [{ project: sel.split('~')[0], kind: 'flow', name: sel.split('~')[1] }] : [];
</script>

<div class="page">
  <Studio host={webHost} config={STUDIOS.flows} {initial}>
    {#snippet lead()}<SubmenuToggle appType="flow-editor" />{/snippet}
  </Studio>
</div>

<style>
  .page { height: 100%; min-height: 0; }
</style>
