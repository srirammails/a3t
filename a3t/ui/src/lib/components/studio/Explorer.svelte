<script lang="ts">
  /// The studio's explorer: what the selected project holds, in folders —
  /// Flows, Resources (a folder per kind: LLM, Databases, MCP Servers, Secrets…),
  /// Triggers, Templates, Canvases. With several projects in view, one node per
  /// project. A filter narrows every folder; folder state is remembered.
  /// Right-click (or F2 / Delete on a row) for an item's actions.
  import { untrack } from 'svelte';
  import { flows as flowsMcp, resources as resourcesApi, mcpCall, execFlow } from '$lib/services/api';
  import { navigateTo } from '$lib/stores/panels.svelte.ts';
  import { namespacesFor } from '$lib/stores/context.svelte.ts';
  import { contextRev_ } from '$lib/stores/apps.svelte.ts';
  import { categorize, catMeta } from '$lib/core/resources/categories';

  import DeleteConfirm from '$lib/components/shared/DeleteConfirm.svelte';
  import { loadFlowYaml, withFlowName, FLOW_NAME, type OpenItem } from './flow';

  let { onopen, onnewflow, onremoved, onrenamed, activeId = '', dirtyIds = [] }: {
    onopen: (it: OpenItem) => void;
    onnewflow: (ns: string) => void;
    /// An item was deleted (its tab, if open, goes with it).
    onremoved?: (key: string) => void;
    /// A flow now lives under another name (its tab, if open, follows it).
    onrenamed?: (oldKey: string, it: OpenItem) => void;
    activeId?: string;
    /// Open tabs with edits not yet deployed, so rename and delete can warn.
    dirtyIds?: string[];
  } = $props();

  type Node = { key: string; label: string; count?: number; children?: Node[]; item?: OpenItem; icon?: string };

  let tree = $state<Node[]>([]);
  let loading = $state(false);
  /// What could not be loaded, so an empty tree is never a silent failure.
  let errors = $state<string[]>([]);
  let looked = $state<string[]>([]);
  let filter = $state('');
  let open = $state<Record<string, boolean>>(read());

  function read(): Record<string, boolean> {
    try { return JSON.parse(localStorage.getItem('a3t.studio.explorer') ?? '{}'); } catch { return {}; }
  }
  function toggle(key: string, def: boolean) {
    open[key] = !(open[key] ?? def);
    try { localStorage.setItem('a3t.studio.explorer', JSON.stringify(open)); } catch { /* private window */ }
  }
  const rows = (d: any, ...keys: string[]) => (Array.isArray(d) ? d : keys.map((k) => d?.[k]).find(Array.isArray) ?? []);
  const nsOfSpec = (y: string) => (y ?? '').match(/^\s+namespace:\s*['"]?([\w.-]+)/m)?.[1] ?? '';
  const keyOf = (it: OpenItem) => `${it.kind}:${it.ns}/${it.id ?? it.name}`;

  /// A call that never throws: failures are collected, not lost.
  async function safe(what: string, call: () => Promise<any>): Promise<any> {
    try {
      const r = await call();
      if (r && r.ok === false) errors = [...errors, `${what}: ${r.error ?? 'failed'}`];
      return r;
    } catch (e) {
      errors = [...errors, `${what}: ${(e as Error).message}`];
      return { ok: false };
    }
  }

  async function load() {
    loading = true;
    errors = [];
    const namespaces = namespacesFor('owned');
    looked = namespaces;
    const [triggersRes, templatesRes] = await Promise.all([
      safe('triggers', () => mcpCall('list_triggers', {})),
      safe('templates', () => execFlow('a3t-templates-list', 'a3t', { org_id: 'org-default' }, { silent: true })),
    ]);
    const triggers = triggersRes.ok ? rows(triggersRes.data, 'triggers') : [];
    const templates = templatesRes.ok ? rows(templatesRes.data, 'rows') : [];
    const perNs = await Promise.all(namespaces.map(async (ns) => {
      const [f, r] = await Promise.all([
        safe(`flows in ${ns}`, () => flowsMcp.list(ns)),
        safe(`resources in ${ns}`, () => mcpCall('list_resources', { namespace: ns })),
      ]);
      return { ns, flows: f.ok ? rows(f.data, 'workflows', 'flows') : [], resources: r.ok ? rows(r.data, 'resources') : [] };
    }));
    tree = perNs.map(({ ns, flows, resources }) => {
      // One row per item: the registry can list a name twice (e.g. a resource
      // kept in two places), and rows are keyed by kind / project / name.
      const folder = (key: string, label: string, items: OpenItem[], icon = ''): Node => {
        const unique = new Map<string, OpenItem>();
        for (const it of items) {
          it.name = String(it.name ?? it.id ?? '?');
          if (!unique.has(keyOf(it))) unique.set(keyOf(it), it);
        }
        const list = [...unique.values()].sort((a, b) => a.name.localeCompare(b.name));
        return { key: `${ns}/${key}`, label, icon, count: list.length, children: list.map((it) => ({ key: keyOf(it), label: it.name, item: it })) };
      };
      const byCat = new Map<string, OpenItem[]>();
      const canvases: OpenItem[] = [];
      for (const r of resources) {
        const type = r.type ?? r.type_ref ?? '';
        if (type === 'project') continue;
        const it: OpenItem = { kind: type === 'canvas' ? 'canvas' : 'resource', ns, name: r.name, data: r };
        if (type === 'canvas') { canvases.push(it); continue; }
        const cat = categorize(type);
        (byCat.get(cat) ?? byCat.set(cat, []).get(cat)!).push(it);
      }
      const resourceFolders: Node[] = [...byCat.entries()]
        .sort((a, b) => catMeta(a[0]).label.localeCompare(catMeta(b[0]).label))
        .map(([cat, items]) => folder(`resources/${cat}`, catMeta(cat).label, items));
      return {
        key: ns, label: ns,
        children: [
          folder('flows', 'Flows', flows.map((f: any) => ({ kind: 'flow' as const, ns, name: f.name, data: f }))),
          { key: `${ns}/resources`, label: 'Resources', count: resourceFolders.reduce((n, f) => n + (f.count ?? 0), 0), children: resourceFolders },
          folder('triggers', 'Triggers', triggers.filter((t: any) => (t.namespace ?? 'default') === ns).map((t: any) => ({ kind: 'trigger' as const, ns, name: t.name ?? t.id, id: t.id, data: t }))),
          folder('templates', 'Templates', templates.filter((t: any) => nsOfSpec(t.spec_yaml) === ns).map((t: any) => ({ kind: 'template' as const, ns, name: t.name, id: t.id, data: t }))),
          folder('canvases', 'Canvases', canvases),
        ],
      };
    });
    loading = false;
  }

  let loadedRev = -1;
  $effect(() => {
    const rev = contextRev_();
    if (rev === loadedRev) return;
    loadedRev = rev;
    untrack(() => void load());
  });

  /// The tree as shown: filtered, with empty folders hidden while filtering.
  function visible(nodes: Node[]): Node[] {
    const q = filter.trim().toLowerCase();
    if (!q) return nodes;
    const keep = (n: Node): Node | null => {
      if (n.item) return n.label.toLowerCase().includes(q) ? n : null;
      const kids = (n.children ?? []).map(keep).filter(Boolean) as Node[];
      return kids.length ? { ...n, children: kids } : null;
    };
    return nodes.map(keep).filter(Boolean) as Node[];
  }
  const shown = $derived(visible(tree));
  const single = $derived(shown.length === 1);
  const icons: Record<string, string> = { flow: 'ƒ', resource: '◆', trigger: '⚡', template: '▤', canvas: '▦' };

  // ── Context menu: an item's actions, or a folder's ──
  /// Where each kind is edited in full (flows are edited here).
  const APPS: Record<string, [string, string]> = {
    resource: ['resources', 'Resources'], trigger: ['triggers', 'Triggers'],
    template: ['templates', 'Templates'], canvas: ['canvas', 'Canvas'],
  };
  type Menu = { x: number; y: number; item?: OpenItem; folder?: { ns: string; label: string } };
  let menu = $state<Menu | null>(null);
  /// The outcome of the last action, shown under the tools.
  let notice = $state<{ ok: boolean; text: string } | null>(null);

  function openMenu(e: MouseEvent, m: Omit<Menu, 'x' | 'y'>) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, ...m };
  }
  function act(fn: () => void) { menu = null; fn(); }
  function rowKey(e: KeyboardEvent, it: OpenItem) {
    if (e.key === 'F2' && it.kind === 'flow') { e.preventDefault(); askName('rename', it); }
    else if (e.key === 'Delete') { e.preventDefault(); delTarget = it; }
  }

  // ── Rename / Duplicate (flows): a copy under the new name, then the old one removed ──
  let naming = $state<{ mode: 'rename' | 'duplicate'; item: OpenItem; value: string; error: string; busy: boolean } | null>(null);
  function askName(mode: 'rename' | 'duplicate', item: OpenItem) {
    naming = { mode, item, value: mode === 'rename' ? item.name : `${item.name}-copy`, error: '', busy: false };
  }
  const flowNames = (ns: string) => new Set(tree.find((n) => n.key === ns)?.children?.find((c) => c.label === 'Flows')?.children?.map((c) => c.label) ?? []);
  const nameProblem = $derived.by(() => {
    if (!naming) return '';
    const v = naming.value.trim();
    if (!v) return 'A name is required.';
    if (!FLOW_NAME.test(v)) return 'Lowercase letters, digits and dashes (not at either end).';
    if (v === naming.item.name) return naming.mode === 'rename' ? 'That is its name already.' : '';
    if (flowNames(naming.item.ns).has(v)) return `${naming.item.ns} already has a flow named ${v}.`;
    return '';
  });
  async function submitName() {
    if (!naming || nameProblem || naming.busy) return;
    const n = naming, name = n.value.trim(), it = n.item;
    n.busy = true; n.error = '';
    const { yaml, error } = await loadFlowYaml(it.ns, it.name);
    if (error) { n.busy = false; n.error = error; return; }
    const made = await mcpCall('create_flow', { yaml: withFlowName(yaml, name) });
    if (!made.ok) { n.busy = false; n.error = made.error ?? 'Could not create the flow'; return; }
    const next: OpenItem = { kind: 'flow', ns: it.ns, name };
    if (n.mode === 'rename') {
      const gone = await resourcesApi.remove(it.ns, it.name);
      if (!gone.ok) {
        // The copy exists; say so rather than leave two flows unexplained.
        notice = { ok: false, text: `Copied to ${name}, but ${it.name} could not be removed: ${gone.error ?? gone.status}` };
      } else {
        notice = { ok: true, text: `Renamed ${it.name} to ${name}.` };
        onrenamed?.(keyOf(it), next);
      }
    } else {
      notice = { ok: true, text: `Duplicated ${it.name} as ${name}.` };
      onopen(next);
    }
    naming = null;
    await load();
  }

  // ── Delete: typed-name confirmation, then the kind's own delete ──
  let delTarget = $state<OpenItem | null>(null);
  async function remove(it: OpenItem) {
    delTarget = null;
    const res = it.kind === 'trigger' ? await mcpCall('delete_trigger', { id: it.id })
      : it.kind === 'template' ? await execFlow('a3t-templates-delete', 'a3t', { id: it.id, org_id: 'org-default' })
      // Flows, canvases and resources share the registry's delete (it refuses a deployed flow).
      : await resourcesApi.remove(it.ns, it.name);
    if (!res.ok) { notice = { ok: false, text: `Could not delete ${it.name}: ${res.error ?? res.status}` }; return; }
    notice = { ok: true, text: `Deleted ${it.kind} ${it.name}.` };
    onremoved?.(keyOf(it));
    await load();
  }
</script>

<div class="ex">
  <div class="ex-tools">
    <input placeholder="Filter" bind:value={filter} />
    <button title="Refresh" onclick={() => load()} disabled={loading}>↻</button>
  </div>
  {#if loading && !tree.length}<div class="ex-empty">Loading…</div>{/if}
  {#if !loading && !shown.length}<div class="ex-empty">{filter ? 'Nothing matches.' : 'No project in view — pick one in the project switcher (or turn on Show platform for a3t / system).'}</div>{/if}
  {#if notice}
    <div class="ex-note" class:bad={!notice.ok}><span>{notice.text}</span><button title="Dismiss" onclick={() => (notice = null)}>×</button></div>
  {/if}
  {#if errors.length}<div class="ex-err" title={errors.join('\n')}>Could not load: {errors.join('; ')}</div>{/if}
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

{#snippet children(nodes: Node[], depth: number, ns: string)}
  {#each nodes as n (n.key)}
    <li>
      {#if n.item}
        <button class="row leaf" class:on={activeId === n.key} style="padding-left: {10 + depth * 14}px" onclick={() => onopen(n.item!)} title="{n.item.kind} {n.item.ns}/{n.label}"
          oncontextmenu={(e) => openMenu(e, { item: n.item })} onkeydown={(e) => rowKey(e, n.item!)}>
          <span class="ic {n.item.kind}">{icons[n.item.kind]}</span><span class="nm">{n.label}</span>
        </button>
      {:else}
        {@const def = n.label === 'Flows'}
        {@const isOpen = (open[n.key] ?? def) || !!filter}
        <div class="folder-row" style="padding-left: {4 + depth * 14}px" role="presentation" oncontextmenu={(e) => openMenu(e, { folder: { ns, label: n.label } })}>
          <button class="row folder" onclick={() => toggle(n.key, def)}>
            <span class="tw">{isOpen ? '▾' : '▸'}</span><span class="nm">{n.label}</span><span class="ct">{n.count ?? ''}</span>
          </button>
          {#if n.label === 'Flows'}<button class="add" title="New flow" onclick={() => onnewflow(ns)}>+</button>{/if}
        </div>
        {#if isOpen}
          <ul>
            {#if !(n.children ?? []).length}<li class="none" style="padding-left: {26 + depth * 14}px">empty</li>{/if}
            {@render children(n.children ?? [], depth + 1, ns)}
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
      <div class="cm-head"><span>{it.name}</span><code>{it.kind} · {it.ns}</code></div>
      <button role="menuitem" onclick={() => act(() => onopen(it))}><span class="cm-ic">✎</span>{it.kind === 'flow' ? 'Edit' : 'Open'}</button>
      {#if APPS[it.kind]}
        <button role="menuitem" onclick={() => act(() => navigateTo(APPS[it.kind][0], APPS[it.kind][1]))}><span class="cm-ic">↗</span>Edit in {APPS[it.kind][1]}</button>
      {/if}
      {#if it.kind === 'flow'}
        <button role="menuitem" onclick={() => act(() => askName('rename', it))}><span class="cm-ic">A</span>Rename…<kbd>F2</kbd></button>
        <button role="menuitem" onclick={() => act(() => askName('duplicate', it))}><span class="cm-ic">⧉</span>Duplicate…</button>
      {/if}
      <button role="menuitem" onclick={() => act(() => navigator.clipboard?.writeText(it.name))}><span class="cm-ic">⎘</span>Copy name</button>
      <hr />
      <button role="menuitem" class="danger" onclick={() => act(() => (delTarget = it))}><span class="cm-ic">🗑</span>Delete…<kbd>Del</kbd></button>
    {:else if menu.folder}
      {@const f = menu.folder}
      {#if f.label === 'Flows'}
        <button role="menuitem" onclick={() => act(() => onnewflow(f.ns))}><span class="cm-ic">+</span>New flow</button>
      {/if}
      <button role="menuitem" onclick={() => act(() => load())}><span class="cm-ic">↻</span>Refresh</button>
    {/if}
  </div>
{/if}

{#if naming}
  {@const n = naming}
  <div class="nd-overlay" role="presentation" onclick={() => !n.busy && (naming = null)}></div>
  <form class="nd" onsubmit={(e) => { e.preventDefault(); void submitName(); }}>
    <h3>{n.mode === 'rename' ? 'Rename' : 'Duplicate'} <span class="mono">{n.item.ns}/{n.item.name}</span></h3>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="mono" bind:value={n.value} autofocus spellcheck="false" onkeydown={(e) => e.key === 'Escape' && !n.busy && (naming = null)} />
    {#if nameProblem && n.value !== n.item.name}<div class="nd-err">{nameProblem}</div>{/if}
    {#if n.error}<div class="nd-err">{n.error}</div>{/if}
    {#if n.mode === 'rename'}
      <p class="nd-note">Saves the flow under the new name, then removes the old one. Past runs stay under {n.item.name}; triggers and canvases that name it need rewiring. A deployed flow cannot be renamed.</p>
      {#if dirtyIds.includes(keyOf(n.item))}<p class="nd-note warn">Its open tab has changes that are not deployed; they will be discarded.</p>{/if}
    {/if}
    <div class="nd-actions">
      <button type="button" class="btn ghost sm" disabled={n.busy} onclick={() => (naming = null)}>Cancel</button>
      <button type="submit" class="btn spec sm" disabled={!!nameProblem || n.busy}>{n.busy ? 'Saving…' : n.mode === 'rename' ? 'Rename' : 'Duplicate'}</button>
    </div>
  </form>
{/if}

<DeleteConfirm open={!!delTarget} entityType={delTarget?.kind ?? 'item'} entityName={delTarget?.name ?? ''}
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
