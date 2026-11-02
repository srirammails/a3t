<script lang="ts">
  /// The Studio's frame: the shell's regions with the Explorer on the left, open
  /// documents in tabs in the middle, and the right and bottom panels of the
  /// active document's kind. Any panel can be dragged to another region or into
  /// the editor area as a tab; closing it there sends it home, minimized. The
  /// Studio has its own context menu everywhere instead of the browser's.
  import { untrack, type Snippet } from 'svelte';
  import StudioShell from './StudioShell.svelte';
  import Explorer from './Explorer.svelte';
  import DocTabs from './DocTabs.svelte';
  import ContextMenu, { type MenuItem } from './parts/ContextMenu.svelte';
  import { VIEWS, PANELS } from './registry';
  import type { RegionId, Studio } from './layout.svelte.ts';
  import { kindById, type PanelDef, type Workspace } from '$lib/studio-core';

  let { studio, workspace, homeKind, home, lead, pin }: {
    studio: Studio;
    workspace: Workspace;
    /// The kind whose panels show while no document (or one without panels) is active.
    homeKind: string;
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

  // ── Panels: each has a home region; the user may have moved it ──
  type Placed = PanelDef & { home: RegionId };
  const panels = $derived<Placed[]>([
    { id: 'explorer', title: 'Explorer', home: 'left' },
    // An open item's own panels; the home kind's only while nothing is open.
    ...((kind ? kind.views.right : fallback.views.right) ?? []).map((p) => ({ ...p, home: 'right' as const })),
    ...((kind ? kind.views.bottom : fallback.views.bottom) ?? []).map((p) => ({ ...p, home: 'bottom' as const })),
  ]);
  const where = (p: Placed) => studio.state.placement[p.id] ?? p.home;
  const inRegion = (r: RegionId | 'center') => panels.filter((p) => where(p) === r);
  const left = $derived(inRegion('left'));
  const right = $derived(inRegion('right'));
  const bottom = $derived(inRegion('bottom'));
  const centered = $derived(inRegion('center'));
  /// The panel shown in the editor area instead of the document, if any.
  const centerPanel = $derived(centered.find((p) => p.id === studio.state.centerPanel) ?? null);
  const badge = (id: string) => (active ? kind?.badge?.(active, id) ?? null : null);

  const tabs = $derived([
    ...workspace.state.docs.map((d) => {
      const k = workspace.kindOf(d);
      return { id: d.address, title: k.title?.(d) ?? d.ref.name, icon: k.icon, hint: `${k.label} ${d.ref.project}/${d.ref.name}`, dirty: workspace.isDirty(d) };
    }),
    ...centered.map((p) => ({ id: `panel:${p.id}`, title: p.title, icon: '▤', hint: `${p.title} panel — close to send it back` })),
  ]);
  const activeTab = $derived(centerPanel ? `panel:${centerPanel.id}` : workspace.state.active);
  // Opening or switching documents (Explorer, Recent, a link) brings documents back to the front.
  let lastActive = untrack(() => workspace.state.active);
  $effect(() => {
    const now = workspace.state.active;
    if (now !== lastActive) { lastActive = now; studio.state.centerPanel = null; }
  });

  function selectTab(id: string) {
    if (id.startsWith('panel:')) { studio.state.centerPanel = id.slice(6); return; }
    studio.state.centerPanel = null;
    workspace.select(id);
  }
  function closeTab(id: string) {
    if (id.startsWith('panel:')) {
      const p = panels.find((x) => x.id === id.slice(6));
      if (p) studio.unplace(p.id, p.home);
      return;
    }
    void workspace.close(id);
  }


  // ── Context menu: the Studio's own, wherever the user right-clicks ──
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function oncontextmenu(e: MouseEvent) {
    // Parts with their own menu (Explorer rows, the flow graph) handle it first.
    if (e.defaultPrevented) return;
    e.preventDefault();
    const el = e.target as HTMLElement;
    const items: MenuItem[] = [];
    const field = el.closest('input, textarea, .cm-editor') as HTMLElement | null;
    if (field) items.push(...editItems(field), 'sep');
    const tab = (el.closest('[data-tab]') as HTMLElement | null)?.dataset.tab;
    const panel = (el.closest('[data-panel]') as HTMLElement | null)?.dataset.panel;
    if (tab) items.push(...tabItems(tab));
    else if (panel) items.push(...panelItems(panel));
    else if (el.closest('.center-body')) items.push(...(centerPanel ? panelItems(centerPanel.id) : docItems()));
    if (items.length) menu = { x: e.clientX, y: e.clientY, items };
  }

  function docItems(): MenuItem[] {
    const d = active;
    if (!d || !kind) return [];
    const code = d.ui.view === 'code';
    return [
      ...(kind.views.code && kind.views.code !== kind.views.center
        ? [code ? { label: 'Back to its view', icon: '←', run: () => workspace.view(d, 'action') } : { label: 'Edit as code', icon: '</>', run: () => workspace.view(d, 'code') }]
        : []),
      { label: 'Save', icon: '⤓', kbd: 'Ctrl+S', disabled: !kind.save || !workspace.isDirty(d), run: () => void workspace.save(d) },
      'sep',
      { label: 'Copy name', icon: '⎘', run: () => void workspace.host.clipboard(d.ref.name) },
      { label: 'Close', icon: '×', run: () => void workspace.close(d.address) },
    ];
  }

  function tabItems(id: string): MenuItem[] {
    if (id.startsWith('panel:')) return panelItems(id.slice(6));
    const d = workspace.find(id);
    if (!d) return [];
    const k = workspace.kindOf(d);
    const others = workspace.state.docs.filter((x) => x.address !== id).map((x) => x.address);
    return [
      { label: 'Open', icon: '▸', run: () => { studio.state.centerPanel = null; workspace.open({ ...d.ref, type: d.type }, { view: 'action' }); } },
      ...(k.views.code ? [{ label: 'Edit as code', icon: '</>', run: () => { studio.state.centerPanel = null; workspace.open({ ...d.ref, type: d.type }, { view: 'code' }); } }] : []),
      { label: 'Save', icon: '⤓', disabled: !k.save || !workspace.isDirty(d), run: () => void workspace.save(d) },
      'sep',
      { label: 'Copy name', icon: '⎘', run: () => void workspace.host.clipboard(d.ref.name) },
      'sep',
      { label: 'Close', icon: '×', run: () => void workspace.close(id) },
      { label: 'Close others', disabled: !others.length, run: () => void closeAll(others) },
      { label: 'Close all', run: () => void closeAll(workspace.state.docs.map((x) => x.address)) },
    ];
  }
  async function closeAll(ids: string[]) {
    for (const id of ids) if (!(await workspace.close(id))) break;
  }

  function panelItems(id: string): MenuItem[] {
    const p = panels.find((x) => x.id === id);
    if (!p) return [];
    const at = where(p);
    const to = (r: RegionId | 'center', label: string): MenuItem => ({ label, icon: '↦', disabled: at === r, run: () => studio.place(id, r) });
    return [
      to('left', 'Move to the left'), to('right', 'Move to the right'), to('bottom', 'Move to the bottom'), to('center', 'Open in the editor area'),
      { label: `Back to the ${p.home}`, icon: '↺', disabled: at === p.home, run: () => studio.unplace(id, p.home) },
      'sep',
      ...(at === 'center' ? [] : [
        { label: 'Minimize', icon: '⇲', run: () => studio.minimize(at as RegionId) },
        { label: studio.state.maximized === id ? 'Restore' : 'Maximize', icon: '⤢', run: () => studio.toggleMaximize(id) },
      ]),
    ];
  }

  /// Cut / copy / paste / select all for text fields and code editors.
  function editItems(field: HTMLElement): MenuItem[] {
    const input = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement ? field : null;
    const editor = field.closest('.cm-editor')?.querySelector('.cm-content') as HTMLElement | null;
    const target = input ?? editor;
    const selected = () => (input ? input.value.slice(input.selectionStart ?? 0, input.selectionEnd ?? 0) : window.getSelection()?.toString() ?? '');
    const readonly = input ? input.readOnly || input.disabled : editor?.getAttribute('contenteditable') !== 'true';
    const focus = () => target?.focus();
    return [
      { label: 'Cut', icon: '✂', disabled: readonly || !selected(), run: () => { focus(); void workspace.host.clipboard(selected()); document.execCommand('delete'); } },
      { label: 'Copy', icon: '⎘', disabled: !selected(), run: () => void workspace.host.clipboard(selected()) },
      { label: 'Paste', icon: '⎀', disabled: readonly, run: async () => { const t = await navigator.clipboard?.readText?.(); focus(); if (t) document.execCommand('insertText', false, t); } },
      { label: 'Select all', icon: '⬚', run: () => { focus(); if (input) input.select(); else document.execCommand('selectAll'); } },
    ];
  }
</script>

<div class="wb" {oncontextmenu} role="presentation">
  <StudioShell
    {studio}
    left={left.length ? { panels: left, panel: panelBody, badge } : undefined}
    right={right.length ? { panels: right, panel: panelBody, badge } : undefined}
    bottom={bottom.length ? { panels: bottom, panel: panelBody, badge } : undefined}
    center={centerArea}
  />
</div>

{#if menu}<ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

{#snippet panelBody(id: string)}
  {#if id === 'explorer'}
    <Explorer {workspace} {pin} />
  {:else}
    {@const def = panels.find((p) => p.id === id)}
    {@const P = PANELS[def?.component ?? id]}
    {#if P}<P {id} doc={active} {workspace} {studio} />{/if}
  {/if}
{/snippet}

{#snippet centerArea()}
  <div class="center-head">
    {@render lead?.()}
    <DocTabs {studio} {tabs} active={activeTab} onselect={selectTab} onclose={closeTab} />
  </div>
  <div class="center-body">
    {#if centerPanel}
      <div class="center-panel" data-panel={centerPanel.id}>{@render panelBody(centerPanel.id)}</div>
    {:else if !active || !kind}
      {@render home?.()}
    {:else}
      {#key active.address}
        {@const V = VIEWS[active.ui.view === 'code' && kind.views.code ? kind.views.code : kind.views.center]}
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
  .center-panel { display: flex; flex-direction: column; overflow: auto; }
</style>
