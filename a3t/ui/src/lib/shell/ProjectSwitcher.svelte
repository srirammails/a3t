<script lang="ts">
  /// The working context in the top bar: which project (namespace) every list
  /// shows. Platform namespaces (a3t, system) are offered only to a superadmin who
  /// turns on "Show platform"; everyone else works in their projects.
  import {
    ALL, getProjects, getProject, getShowPlatform, setProject, setShowPlatform, loadProjects, canSee,
  } from '$lib/stores/context.svelte.ts';
  import { userRole } from '$lib/stores/auth';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';

  let open = $state(false);
  let query = $state('');

  const project = $derived(getProject());
  const showPlatform = $derived(getShowPlatform());
  const canSeePlatform = $derived($userRole === 'superadmin');
  const current = $derived(getProjects().find((p) => p.name === project));

  const listed = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return getProjects()
      .filter((p) => (p.platform ? canSeePlatform && showPlatform : canSee(p)))
      .filter((p) => !q || p.name.toLowerCase().includes(q));
  });

  function toggle() {
    open = !open;
    // Pick up namespaces created since the dash loaded.
    if (open) { query = ''; void loadProjects(true); }
  }

  function pick(name: string) {
    setProject(name);
    open = false;
  }

  function counts(p: { flows?: number; resources?: number; triggers?: number }): string {
    const parts = [
      p.flows ? `${p.flows} flow${p.flows === 1 ? '' : 's'}` : '',
      p.resources ? `${p.resources} res` : '',
      p.triggers ? `${p.triggers} trg` : '',
    ].filter(Boolean);
    return parts.join(' · ') || 'empty';
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') open = false;
    if (e.key === 'Enter' && listed.length) pick(listed[0].name);
  }
</script>

<div class="ps">
  <button class="ps-btn" class:platform={current?.platform} onclick={toggle} aria-haspopup="listbox" aria-expanded={open}
          title="Project — every list shows this namespace">
    <span class="ps-icon" aria-hidden="true">▣</span>
    <span class="ps-label">{project || 'All projects'}</span>
    {#if current?.platform}<span class="ps-tag">platform</span>{/if}
    <svg class="chev" class:open width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6l4 4 4-4"/></svg>
  </button>

  {#if open}
    <div class="ps-menu" role="listbox">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="ps-search" placeholder="Find a project…" bind:value={query} onkeydown={onKey} autofocus />
      <button class="ps-item" class:active={project === ALL} role="option" aria-selected={project === ALL} onclick={() => pick(ALL)}>
        <span class="ps-name">All projects</span>
        <span class="ps-sub">{getProjects().filter((p) => !p.platform && canSee(p)).length}</span>
      </button>
      <div class="ps-sep"></div>
      {#each listed as p (p.name)}
        <button class="ps-item" class:active={project === p.name} role="option" aria-selected={project === p.name} onclick={() => pick(p.name)}>
          <span class="ps-name">{p.info?.display_name || p.name}{#if p.platform}<span class="ps-tag">platform</span>{/if}</span>
          <span class="ps-sub">{counts(p)}</span>
        </button>
      {:else}
        <div class="ps-empty">No project matches “{query}”.</div>
      {/each}
      <div class="ps-sep"></div>
      <button class="ps-item ps-new" onclick={() => { open = false; navigateTo('projects', 'Projects', { create: true }); }}>
        <span class="ps-name">+ New project</span>
      </button>
      <button class="ps-item" onclick={() => { open = false; navigateTo('projects', 'Projects'); }}>
        <span class="ps-name">Manage projects…</span>
      </button>
      {#if canSeePlatform}
        <div class="ps-sep"></div>
        <label class="ps-toggle">
          <input type="checkbox" checked={showPlatform} onchange={(e) => setShowPlatform(e.currentTarget.checked)} />
          Show platform (a3t, system)
        </label>
      {/if}
    </div>
  {/if}
</div>

{#if open}
  <button class="ps-backdrop" onclick={() => (open = false)} aria-label="Close project menu"></button>
{/if}

<style>
  .ps { position: relative; }
  .ps-btn {
    display: flex; align-items: center; gap: 7px;
    height: 32px; padding: 0 10px; border-radius: var(--r-sm);
    border: 1px solid var(--border); background: var(--bg-surface);
    font-size: 13px; font-weight: 500; color: var(--text-1); cursor: pointer;
    max-width: 260px;
  }
  .ps-btn:hover { background: var(--bg-subtle); }
  .ps-btn.platform { border-color: var(--spec); }
  .ps-icon { color: var(--spec); font-size: 12px; }
  .ps-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chev { color: var(--text-4); transition: transform 0.2s; flex: none; }
  .chev.open { transform: rotate(180deg); }
  .ps-tag {
    font-family: var(--font-mono); font-size: 9.5px; color: var(--spec);
    background: var(--spec-bg); padding: 1px 5px; border-radius: 3px; margin-left: 6px;
  }

  .ps-menu {
    position: absolute; top: 100%; left: 0; margin-top: 6px; z-index: 100;
    width: 280px; max-height: 420px; overflow-y: auto;
    background: var(--bg-surface); border: 1px solid var(--border);
    border-radius: var(--r); box-shadow: var(--shadow-lg); padding: 6px;
  }
  .ps-search {
    width: 100%; box-sizing: border-box; height: 30px; padding: 0 8px; margin-bottom: 4px;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-subtle); color: var(--text-1); font-size: 12.5px;
  }
  .ps-item {
    display: flex; align-items: baseline; justify-content: space-between; gap: 10px;
    width: 100%; padding: 7px 8px; border: none; background: none; border-radius: var(--r-sm);
    font-size: 13px; color: var(--text-2); cursor: pointer; text-align: left;
  }
  .ps-item:hover { background: var(--bg-subtle); color: var(--text-1); }
  .ps-item.active { background: var(--spec-bg); color: var(--spec); font-weight: 550; }
  .ps-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ps-sub { font-size: 11px; color: var(--text-4); white-space: nowrap; flex: none; }
  .ps-new .ps-name { color: var(--spec); font-weight: 550; }
  .ps-sep { height: 1px; background: var(--border); margin: 5px 2px; }
  .ps-empty { padding: 8px; font-size: 12px; color: var(--text-4); }
  .ps-toggle {
    display: flex; align-items: center; gap: 8px; padding: 7px 8px;
    font-size: 12.5px; color: var(--text-2); cursor: pointer;
  }
  .ps-backdrop {
    position: fixed; inset: 0; background: transparent; border: none; z-index: 50; cursor: default;
  }
</style>
