<script lang="ts">
  /// The Studio's frame: the shell's regions with the Explorer on the left, open
  /// documents in tabs in the middle, and the right and bottom panels of the
  /// active document's kind.
  import type { Snippet } from 'svelte';
  import StudioShell from './StudioShell.svelte';
  import Explorer from './Explorer.svelte';
  import DocTabs from './DocTabs.svelte';
  import { VIEWS, PANELS } from './registry';
  import type { Studio } from './layout.svelte.ts';
  import { kindById, type Workspace } from '$lib/studio-core';

  let { studio, workspace, homeKind, create = [], home, lead, pin }: {
    studio: Studio;
    workspace: Workspace;
    /// The kind whose panels show while no document (or one without panels) is active.
    homeKind: string;
    /// Kinds the tab bar offers "+ New …" for.
    create?: string[];
    /// Shown in the editor area with nothing open.
    home?: Snippet;
    /// The host's controls at the start of the tab bar.
    lead?: Snippet;
    /// A resource category the Explorer shows first, open (e.g. `database`).
    pin?: string;
  } = $props();

  const active = $derived(workspace.active);
  const kind = $derived(active ? workspace.kindOf(active) : null);
  const fallback = $derived(kindById(homeKind));
  const right = $derived(kind?.views.right ?? fallback.views.right ?? []);
  const bottom = $derived(kind?.views.bottom ?? fallback.views.bottom ?? []);

  const tabs = $derived(workspace.state.docs.map((d) => {
    const k = workspace.kindOf(d);
    return { id: d.address, title: k.title?.(d) ?? d.ref.name, icon: k.icon, hint: `${k.label} ${d.ref.project}/${d.ref.name}`, dirty: workspace.isDirty(d) };
  }));

  const newDoc = (kindId: string) => workspace.create(kindId, workspace.host.scope.current() ?? workspace.host.scope.projects()[0] ?? 'a3t');
</script>

<div class="wb">
  <StudioShell
    {studio}
    left={{ panels: [{ id: 'explorer', title: 'Explorer' }], panel: leftPanel }}
    right={right.length ? { panels: right, panel: sidePanel } : undefined}
    bottom={bottom.length ? { panels: bottom, panel: sidePanel, badge: (id) => (active ? kind?.badge?.(active, id) ?? null : null) } : undefined}
    center={centerArea}
  />
</div>

{#snippet leftPanel(_id: string)}
  <Explorer {workspace} {pin} />
{/snippet}

{#snippet sidePanel(id: string)}
  {@const def = [...right, ...bottom].find((p) => p.id === id)}
  {@const P = PANELS[def?.component ?? id]}
  {#if P}<P {id} doc={active} {workspace} {studio} />{/if}
{/snippet}

{#snippet centerArea()}
  <div class="center-head">
    {@render lead?.()}
    <DocTabs {studio} {tabs} active={workspace.state.active} onselect={(id) => workspace.select(id)} onclose={(id) => void workspace.close(id)}>
      {#snippet actions()}
        {#each create as c (c)}
          <button class="new" onclick={() => newDoc(c)} title="New {kindById(c).label.toLowerCase()}">+ New {kindById(c).label.toLowerCase()}</button>
        {/each}
      {/snippet}
    </DocTabs>
  </div>
  <div class="center-body">
    {#if !active || !kind}
      {@render home?.()}
    {:else}
      {#key active.address}
        {@const V = VIEWS[kind.views.center]}
        {#if V}<V doc={active} {workspace} {studio} />{/if}
      {/key}
    {/if}
  </div>
{/snippet}

<style>
  .wb { height: 100%; min-height: 0; }
  .center-head { display: flex; align-items: center; gap: 4px; padding-left: 6px; background: var(--bg-page); border-bottom: 1px solid var(--border); }
  .center-head :global(.tabs) { flex: 1; border-bottom: 0; }
  .center-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .center-body > :global(*) { flex: 1; min-height: 0; }
  .new { border: 0; background: transparent; color: var(--spec); font-size: 12px; padding: 0 10px; cursor: pointer; white-space: nowrap; }
  .new:hover { background: var(--bg-subtle); }
</style>
