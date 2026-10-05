<script lang="ts">
  // Security — Identity and Policies. The section is chosen from the shell
  // submenu (≡ / pin), like every other section, rather than an in-page rail.
  // Both are EE/admin features; each sub-page handles its own gating.
  import PoliciesPage from './PoliciesPage.svelte';
  import IdentityPage from './IdentityPage.svelte';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';

  // The page remounts when the selection changes (PanelStack keys on it), so
  // reading it once is enough.
  const active: 'identity' | 'policies' =
    getActivePanel()?.props?.selectedId === 'policies' ? 'policies' : 'identity';
</script>

<div class="security">
  {#if active === 'policies'}
    <PoliciesPage />
  {:else}
    <IdentityPage />
  {/if}
</div>

<style>
  .security { height: 100%; overflow: hidden; display: flex; flex-direction: column; }
  .security > :global(*) { flex: 1; min-height: 0; }
</style>
