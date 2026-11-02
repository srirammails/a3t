<script lang="ts">
  /// The studio's explorer: what the projects in scope hold, in folders — Flows,
  /// Resources (a folder per category), Triggers, Templates, Canvases. With
  /// several projects in view, one node per project. A filter narrows every
  /// folder; folder state is remembered. Right-click (or F2 / Delete on a row)
  /// for an item's actions — which ones, and what they do, come from its kind.
  import { onMount, untrack } from 'svelte';
  import DeleteConfirm from '$lib/components/shared/DeleteConfirm.svelte';
  import { address, can, kindFor, loadInventory, namesIn, ITEM_MIME, type Inventory, type Item, type TreeNode, type Workspace } from '$lib/studio-core';

  let { workspace, pin }: {
    workspace: Workspace;
    /// A resource category shown first in each project, open (e.g. `database`).
    pin?: string;
  } = $props();

  let inventory = $state<Inventory>({ tree: [], errors: [] });
  let loading = $state(false);
  let looked = $state<string[]>([]);
  let filter = $state('');
  const FOLDERS_KEY = 'a3t.studio.explorer';
  // Read once: the workspace (and its host) is fixed for the Explorer's life.
  let open = $state<Record<string, boolean>>(untrack(() => workspace.host.storage.get(FOLDERS_KEY) as Record<string, boolean>) ?? {});

  function toggle(key: string, def: boolean) {
    open[key] = !(open[key] ?? def);
    workspace.host.storage.set(FOLDERS_KEY, open);
  }

  async function load() {
    loading = true;
    looked = workspace.host.scope.projects();
    inventory = await loadInventory(looked);
    loading = false;
  }

  // Load now, and again whenever the host's projects in scope change.
  onMount(() => {
    void load();
    return workspace.host.scope.onChange(() => void load());
  });

  /// The tree as shown: filtered, with empty folders hidden while filtering.
  function visible(nodes: TreeNode[]): TreeNode[] {
    const q = filter.trim().toLowerCase();
    if (!q) return nodes;
    const keep = (n: TreeNode): TreeNode | null => {
      if (n.item) return n.label.toLowerCase().includes(q) ? n : null;
      const kids = (n.children ?? []).map(keep).filter(Boolean) as TreeNode[];
      return kids.length ? { ...n, children: kids } : null;
    };
    return nodes.map(keep).filter(Boolean) as TreeNode[];
  }
  /// The pinned category's folder moves out of Resources to the top of its project.
  function pinned(nodes: TreeNode[]): TreeNode[] {
    if (!pin) return nodes;
    return nodes.map((ns) => {
      const resources = ns.children?.find((c) => c.key === `${ns.key}/resources`);
      const folder = resources?.children?.find((c) => c.key === `${ns.key}/resources/${pin}`);
      if (!resources || !folder) return ns;
      const rest = { ...resources, children: resources.children!.filter((c) => c !== folder), count: (resources.count ?? 0) - (folder.count ?? 0) };
      return { ...ns, children: [folder, ...ns.children!.map((c) => (c === resources ? rest : c))] };
    });
  }
  const isPinned = (n: TreeNode) => !!pin && n.key.endsWith(`/resources/${pin}`);
  const shown = $derived(visible(pinned(inventory.tree)));
  const single = $derived(shown.length === 1);
  const activeRef = $derived(workspace.active?.ref ?? null);
  const isActive = (it: Item) => !!activeRef && address(activeRef) === address(it);

  // ── Context menu: an item's actions, or a folder's ──
  type Menu = { x: number; y: number; item?: Item; folder?: { project: string; label: string; create?: TreeNode['create'] } };
  let menu = $state<Menu | null>(null);
  /// The outcome of the last action, shown under the tools.
  let notice = $state<{ ok: boolean; text: string } | null>(null);

  function openMenu(e: MouseEvent, m: Omit<Menu, 'x' | 'y'>) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, ...m };
  }
  function act(fn: () => void) { menu = null; fn(); }
  function rowKey(e: KeyboardEvent, it: Item) {
    const kind = kindFor(it);
    if (e.key === 'F2' && kind.rename) { e.preventDefault(); askName('rename', it); }
    else if (e.key === 'Delete' && kind.remove) { e.preventDefault(); delTarget = it; }
  }
  const newIn = (project: string, c: NonNullable<TreeNode['create']>) => workspace.create(c.kind, project, c.type);
  /// Click: the item's own view. Double-click / Edit: the item as code.
  const openIt = (it: Item) => workspace.open(it, { view: 'action' });
  const editIt = (it: Item) => workspace.open(it, { view: 'code' });

  /// Items drag as their address and the item itself (a flow onto a canvas…).
  function drag(e: DragEvent, it: Item) {
    if (!e.dataTransfer) return;
    const { data: _data, ...light } = it;
    e.dataTransfer.setData(ITEM_MIME, JSON.stringify(light));
    e.dataTransfer.setData('text/plain', address(it));
    e.dataTransfer.effectAllowed = 'copyLink';
  }

  // ── Rename / Duplicate: the kind does it; the open tab follows ──
  let naming = $state<{ mode: 'rename' | 'duplicate'; item: Item; value: string; error: string; busy: boolean } | null>(null);
  function askName(mode: 'rename' | 'duplicate', item: Item) {
    naming = { mode, item, value: mode === 'rename' ? item.name : `${item.name}-copy`, error: '', busy: false };
  }
  const nameProblem = $derived.by(() => {
    if (!naming) return '';
    const v = naming.value.trim();
    if (!v) return 'A name is required.';
    const bad = kindFor(naming.item).checkName?.(v);
    if (bad) return bad;
    if (v === naming.item.name) return naming.mode === 'rename' ? 'That is its name already.' : '';
    if (namesIn(inventory, naming.item.project, naming.item.kind).has(v)) return `${naming.item.project} already has one named ${v}.`;
    return '';
  });
  async function submitName() {
    if (!naming || nameProblem || naming.busy) return;
    const n = naming, to = n.value.trim(), it = n.item, kind = kindFor(it);
    n.busy = true; n.error = '';
    const r = n.mode === 'rename' ? await kind.rename!(it, to) : await kind.duplicate!(it, to);
    if (!r.ok) {
      n.busy = false; n.error = r.error;
      return;
    }
    const next: Item = { project: it.project, kind: it.kind, name: to };
    if (n.mode === 'rename') {
      notice = { ok: true, text: `Renamed ${it.name} to ${to}.` };
      workspace.renamed(it, next);
    } else {
      notice = { ok: true, text: `Duplicated ${it.name} as ${to}.` };
      workspace.open(next);
    }
    naming = null;
    await load();
  }

  // ── Delete: typed-name confirmation, then the kind's delete ──
  let delTarget = $state<Item | null>(null);
  async function remove(it: Item) {
    delTarget = null;
    const kind = kindFor(it);
    const r = await kind.remove!(it);
    if (!r.ok) { notice = { ok: false, text: `Could not delete ${it.name}: ${r.error}` }; return; }
    notice = { ok: true, text: `Deleted ${kind.label.toLowerCase()} ${it.name}.` };
    workspace.removed(it);
    await load();
  }
</script>

<div class="ex">
  <div class="ex-tools">
    <input placeholder="Filter" bind:value={filter} />
    <button title="Refresh" onclick={() => load()} disabled={loading}>↻</button>
  </div>
  {#if loading && !inventory.tree.length}<div class="ex-empty">Loading…</div>{/if}
  {#if !loading && !shown.length}<div class="ex-empty">{filter ? 'Nothing matches.' : 'No project in view — pick one in the project switcher (or turn on Show platform for a3t / system).'}</div>{/if}
  {#if notice}
    <div class="ex-note" class:bad={!notice.ok}><span>{notice.text}</span><button title="Dismiss" onclick={() => (notice = null)}>×</button></div>
  {/if}
  {#if inventory.errors.length}<div class="ex-err" title={inventory.errors.join('\n')}>Could not load: {inventory.errors.join('; ')}</div>{/if}
  <div class="ex-scope" title="Projects listed here">{looked.length ? looked.join(', ') : '—'}</div>
  <ul class="tree">
    {#each shown as ns (ns.key)}
      {#if single}
        {@render children(ns.children ?? [], 0, ns.key)}
      {:else}
        <li>
          <button class="row folder" onclick={() => toggle(ns.key, true)}>
            <span class="tw">{(open[ns.key] ?? true) || filter ? '▾' : '▸'}</span><span class="nm proj">{ns.label}</span>
          </button>
          {#if (open[ns.key] ?? true) || filter}<ul>{@render children(ns.children ?? [], 1, ns.key)}</ul>{/if}
        </li>
      {/if}
    {/each}
  </ul>
</div>

{#snippet children(nodes: TreeNode[], depth: number, project: string)}
  {#each nodes as n (n.key)}
    <li>
      {#if n.item}
        {@const it = n.item}
        {@const kind = kindFor(it)}
        <button class="row leaf" class:on={isActive(it)} style="padding-left: {10 + depth * 14}px" onclick={() => openIt(it)} ondblclick={() => editIt(it)} title="{kind.label} {it.project}/{n.label} — double-click to edit"
          draggable="true" ondragstart={(e) => drag(e, it)}
          oncontextmenu={(e) => openMenu(e, { item: it })} onkeydown={(e) => rowKey(e, it)}>
          <span class="ic {it.kind}">{kind.icon}</span><span class="nm">{n.label}</span>
        </button>
      {:else}
        {@const def = !!n.open || isPinned(n)}
        {@const isOpen = (open[n.key] ?? def) || !!filter}
        <div class="folder-row" style="padding-left: {4 + depth * 14}px" role="presentation" oncontextmenu={(e) => openMenu(e, { folder: { project, label: n.label, create: n.create } })}>
          <button class="row folder" onclick={() => toggle(n.key, def)}>
            <span class="tw">{isOpen ? '▾' : '▸'}</span><span class="nm">{n.label}</span><span class="ct">{n.count ?? ''}</span>
          </button>
          {#if n.create}{@const c = n.create}<button class="add" title="New in {n.label}" onclick={() => newIn(project, c)}>+</button>{/if}
        </div>
        {#if isOpen}
          <ul>
            {#if !(n.children ?? []).length}<li class="none" style="padding-left: {26 + depth * 14}px">empty</li>{/if}
            {@render children(n.children ?? [], depth + 1, project)}
          </ul>
        {/if}
      {/if}
    </li>
  {/each}
{/snippet}

{#if menu}
  {@const it = menu.item}
  <div class="cm-overlay" role="presentation" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
  <div class="cm" role="menu" style="left: {Math.min(menu.x, window.innerWidth - 240)}px; top: {Math.min(menu.y, window.innerHeight - 260)}px">
    {#if it}
      {@const kind = kindFor(it)}
      <div class="cm-head"><span>{it.name}</span><code>{kind.label.toLowerCase()} · {it.project}</code></div>
      <button role="menuitem" onclick={() => act(() => openIt(it))}><span class="cm-ic">▸</span>Open</button>
      <button role="menuitem" onclick={() => act(() => editIt(it))}><span class="cm-ic">✎</span>{can(kind, 'edit') ? 'Edit' : 'View as code'}</button>
      {#if kind.console}
        {@const c = kind.console}
        <button role="menuitem" onclick={() => act(() => workspace.host.openConsole(c.section, c.title))}><span class="cm-ic">↗</span>Edit in {c.title}</button>
      {/if}
      {#if kind.rename}
        <button role="menuitem" onclick={() => act(() => askName('rename', it))}><span class="cm-ic">A</span>Rename…<kbd>F2</kbd></button>
      {/if}
      {#if kind.duplicate}
        <button role="menuitem" onclick={() => act(() => askName('duplicate', it))}><span class="cm-ic">⧉</span>Duplicate…</button>
      {/if}
      <button role="menuitem" onclick={() => act(() => void workspace.host.clipboard(it.name))}><span class="cm-ic">⎘</span>Copy name</button>
      {#if kind.remove}
        <hr />
        <button role="menuitem" class="danger" onclick={() => act(() => (delTarget = it))}><span class="cm-ic">🗑</span>Delete…<kbd>Del</kbd></button>
      {/if}
    {:else if menu.folder}
      {@const f = menu.folder}
      {#if f.create}
        {@const c = f.create}
        <button role="menuitem" onclick={() => act(() => newIn(f.project, c))}><span class="cm-ic">+</span>New in {f.label}</button>
      {/if}
      <button role="menuitem" onclick={() => act(() => load())}><span class="cm-ic">↻</span>Refresh</button>
    {/if}
  </div>
{/if}

{#if naming}
  {@const n = naming}
  {@const kind = kindFor(n.item)}
  <div class="nd-overlay" role="presentation" onclick={() => !n.busy && (naming = null)}></div>
  <form class="nd" onsubmit={(e) => { e.preventDefault(); void submitName(); }}>
    <h3>{n.mode === 'rename' ? 'Rename' : 'Duplicate'} <span class="mono">{n.item.project}/{n.item.name}</span></h3>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="mono" bind:value={n.value} autofocus spellcheck="false" onkeydown={(e) => e.key === 'Escape' && !n.busy && (naming = null)} />
    {#if nameProblem && n.value !== n.item.name}<div class="nd-err">{nameProblem}</div>{/if}
    {#if n.error}<div class="nd-err">{n.error}</div>{/if}
    {#if n.mode === 'rename'}
      {#if kind.renameNote}<p class="nd-note">{kind.renameNote}</p>{/if}
      {#if workspace.isDirty(workspace.find(address(n.item)))}<p class="nd-note warn">Its open tab has changes that are not saved; they will be discarded.</p>{/if}
    {/if}
    <div class="nd-actions">
      <button type="button" class="btn ghost sm" disabled={n.busy} onclick={() => (naming = null)}>Cancel</button>
      <button type="submit" class="btn spec sm" disabled={!!nameProblem || n.busy}>{n.busy ? 'Saving…' : n.mode === 'rename' ? 'Rename' : 'Duplicate'}</button>
    </div>
  </form>
{/if}

<DeleteConfirm open={!!delTarget} entityType={delTarget ? kindFor(delTarget).label.toLowerCase() : 'item'} entityName={delTarget?.name ?? ''}
  onconfirm={() => delTarget && remove(delTarget)} oncancel={() => (delTarget = null)} />

<style>
  .ex { display: flex; flex-direction: column; height: 100%; font-size: 12.5px; }
  .ex-tools { display: flex; gap: 4px; padding: 6px; border-bottom: 1px solid var(--border); }
  .ex-tools input { flex: 1; min-width: 0; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 12px; }
  .ex-tools button { border: 0; background: transparent; cursor: pointer; color: var(--text-3); width: 26px; }
  .ex-empty { padding: 12px; color: var(--text-4); }
  .ex-err { margin: 6px; padding: 6px 8px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 11.5px; }
  .ex-scope { padding: 4px 10px; font-size: 10.5px; color: var(--text-4); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tree, .tree ul { list-style: none; margin: 0; padding: 0; }
  .tree { overflow-y: auto; flex: 1; padding: 4px 0; }
  .row { display: flex; align-items: center; gap: 6px; width: 100%; border: 0; background: transparent; cursor: pointer; text-align: left; padding: 3px 8px; color: var(--text-2); font-size: 12.5px; }
  .row:hover { background: var(--bg-subtle); }
  .row.on { background: var(--spec-bg); color: var(--text-1); }
  .folder-row { display: flex; align-items: center; }
  .folder-row:hover .add { visibility: visible; }
  .folder { font-weight: 500; color: var(--text-1); }
  .proj { font-family: var(--font-mono); font-size: 12px; }
  .tw { width: 10px; color: var(--text-4); font-size: 10px; }
  .nm { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ct { font-size: 10.5px; color: var(--text-4); }
  .add { visibility: hidden; border: 0; background: transparent; cursor: pointer; color: var(--spec); width: 22px; font-size: 14px; }
  .ic { width: 14px; text-align: center; font-size: 11px; color: var(--text-3); }
  .ic.flow { color: var(--spec); } .ic.trigger { color: var(--warn, #b45309); } .ic.resource { color: var(--control-ink, #3a5a85); }
  .none { color: var(--text-4); font-size: 11.5px; padding: 2px 0; }
  .ex-note { display: flex; align-items: flex-start; gap: 6px; margin: 6px; padding: 6px 8px; border-radius: var(--r-sm); background: var(--runtime-bg, #e3f4ef); color: var(--runtime, #1a7158); font-size: 11.5px; }
  .ex-note.bad { background: var(--err-bg, #fde8e8); color: var(--err, #b42318); }
  .ex-note span { flex: 1; }
  .ex-note button { border: 0; background: transparent; color: inherit; cursor: pointer; padding: 0 2px; }
  /* Same menu as the graph's (FlowEditorPane). */
  .cm-overlay { position: fixed; inset: 0; z-index: 90; }
  .cm { position: fixed; z-index: 91; min-width: 210px; max-width: 280px; display: flex; flex-direction: column; padding: 4px;
        background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); font-size: 12.5px; }
  .cm-head { display: flex; flex-direction: column; gap: 1px; padding: 5px 8px 6px; border-bottom: 1px solid var(--border); margin-bottom: 3px; }
  .cm-head span { font-weight: 600; color: var(--text-1); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cm-head code { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-4); }
  .cm button { display: flex; align-items: center; gap: 8px; border: 0; background: transparent; padding: 5px 8px; border-radius: 4px;
               cursor: pointer; text-align: left; color: var(--text-1); font-size: 12.5px; }
  .cm button:hover { background: var(--bg-subtle); }
  .cm button.danger { color: var(--err, #b42318); }
  .cm kbd { margin-left: auto; font-family: var(--font-mono); font-size: 10.5px; color: var(--text-4); }
  .cm-ic { width: 16px; text-align: center; color: var(--text-3); font-size: 11px; }
  .cm hr { border: 0; border-top: 1px solid var(--border); margin: 3px 0; }
  .nd-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.3); z-index: 200; }
  .nd { position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); z-index: 201; width: 420px; max-width: 90vw;
        display: flex; flex-direction: column; gap: 10px; padding: 20px 24px; background: var(--bg-surface);
        border: 1px solid var(--border); border-radius: 12px; box-shadow: var(--shadow-lg); }
  .nd h3 { font-family: var(--font-serif); font-size: 16px; font-weight: 500; }
  .nd h3 .mono, .nd input.mono { font-family: var(--font-mono); }
  .nd h3 .mono { font-size: 13px; color: var(--text-3); }
  .nd input { padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 13px; }
  .nd input:focus { outline: none; border-color: var(--spec); }
  .nd-err { color: var(--err, #b42318); font-size: 12px; }
  .nd-note { color: var(--text-3); font-size: 12px; line-height: 1.5; }
  .nd-note.warn { color: var(--warn, #b45309); }
  .nd-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
