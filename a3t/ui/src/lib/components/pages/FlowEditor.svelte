<script lang="ts">
  /// The Console's Studio page (menu: Studio; /studio, and /flow-editor for old
  /// links), hosted by the web dash. The Studio is used only through its entry point.
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import { Studio, STUDIOS } from '$lib/studio-ui';
  import { webHost } from '$lib/host/web.svelte.ts';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { parseStudioItem } from '$lib/stores/panels.svelte.ts';

  // An item named in the address (/studio/<kind>:<project>~<name>, or the older
  // /studio/<project>~<name> for a flow) opens on arrival, and again whenever an
  // "Open in Studio" elsewhere names one (each navigation is a new panel id).
  const request = $derived.by(() => {
    const p = getActivePanel();
    if (p?.type !== 'flow-editor') return null;
    const item = parseStudioItem(p.props?.selectedId);
    return item ? { item, at: p.id } : null;
  });
</script>

<div class="page">
  <Studio host={webHost} config={STUDIOS.flows} {request}>
    {#snippet lead()}<SubmenuToggle appType="flow-editor" />{/snippet}
  </Studio>
</div>

<style>
  .page { height: 100%; min-height: 0; }
</style>
