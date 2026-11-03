<script lang="ts">
  /// The Console's Query Console page: the Studio with the database
  /// configuration (Explorer pinned to Databases), hosted by the web dash. The
  /// previous console stays one release behind "Classic console".
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import DbConsoleClassic from './DbConsoleClassic.svelte';
  import { Studio, STUDIOS } from '$lib/studio-ui';
  import { webHost } from '$lib/host/web.svelte.ts';

  let classic = $state(webHost.storage.get('a3t.db.classic') === true);
  function use(c: boolean) {
    classic = c;
    webHost.storage.set('a3t.db.classic', c);
  }
</script>

<div class="page">
  {#if classic}
    <div class="back"><button class="lnk" onclick={() => use(false)}>← Back to the Studio console</button></div>
    <div class="classic"><DbConsoleClassic /></div>
  {:else}
    <Studio host={webHost} config={STUDIOS.db}>
      {#snippet lead()}
        <SubmenuToggle appType="db-console" />
        <button class="lnk" title="The previous Query Console, kept for one release" onclick={() => use(true)}>Classic console</button>
      {/snippet}
    </Studio>
  {/if}
</div>

<style>
  .page { height: 100%; min-height: 0; display: flex; flex-direction: column; }
  .page > :global(*) { flex: 1; min-height: 0; }
  .back { flex: 0 0 auto; padding: 4px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .classic { display: flex; flex-direction: column; }
  .classic > :global(*) { flex: 1; min-height: 0; }
  .lnk { border: 0; background: transparent; color: var(--text-3); font-size: 11.5px; cursor: pointer; white-space: nowrap; padding: 0 8px; }
  .lnk:hover { color: var(--spec); }
</style>
