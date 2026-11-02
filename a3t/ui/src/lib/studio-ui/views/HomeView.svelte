<script lang="ts">
  /// The Studio with nothing open: what it is for (the banner, like the Console's
  /// section pages), "+ New …", and the items opened lately.
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import { kindFor, type Workspace } from '$lib/studio-core';
  import type { StudioConfig } from '../studios';

  let { config, workspace }: { config: StudioConfig; workspace: Workspace } = $props();

  const recent = $derived(workspace.state.recent);
  const project = () => workspace.host.scope.current() ?? workspace.host.scope.projects()[0] ?? 'a3t';
</script>

<div class="home">
  <PageBanner headline={config.home.headline} highlightWord={config.home.highlight} subtitle={config.home.subtitle}
    description={config.home.description} accent={config.home.accent} docsPath={config.home.docsPath} />

  {#if config.create.length}
    <div class="actions">
      {#each config.create as c, i (`${c.kind}/${c.type ?? ''}`)}
        <button class="btn {i === 0 ? 'spec' : 'ghost'} sm" onclick={() => workspace.create(c.kind, project(), c.type)}>+ {c.label}</button>
      {/each}
    </div>
  {/if}

  <section>
    <h3>Recent</h3>
    {#if recent.length}
      <ul class="recent">
        {#each recent as it (`${it.project}/${it.kind}/${it.id ?? it.name}`)}
          {@const k = kindFor(it)}
          <li>
            <button onclick={() => workspace.open(it)} title="{k.label} {it.project}/{it.name}">
              <span class="ic">{k.icon}</span><span class="nm">{it.name}</span><span class="pj">{it.project}</span><span class="kd">{k.label}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="none">Nothing opened yet — pick something in the Explorer.</p>
    {/if}
  </section>
</div>

<style>
  .home { overflow-y: auto; padding: 20px 32px 40px; }
  .actions { margin: -8px 0 20px; display: flex; gap: 8px; flex-wrap: wrap; }
  h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; color: var(--text-2); margin: 0 0 8px; }
  .recent { list-style: none; margin: 0; padding: 0; max-width: 720px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); }
  .recent li + li { border-top: 1px solid var(--border); }
  .recent button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 12px; border: 0; background: transparent; cursor: pointer; text-align: left; font-size: 13px; color: var(--text-1); }
  .recent button:hover { background: var(--bg-subtle); }
  .ic { width: 16px; text-align: center; color: var(--spec); }
  .nm { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pj { font-family: var(--font-mono); font-size: 11.5px; color: var(--text-3); }
  .kd { font-size: 11px; color: var(--text-4); width: 70px; text-align: right; }
  .none { color: var(--text-4); font-size: 13px; }
</style>
