<script lang="ts">
  /// The studio's explorer: what the selected project holds, in folders —
  /// Flows, Resources (a folder per kind: LLM, Databases, MCP Servers, Secrets…),
  /// Triggers, Templates, Canvases. With several projects in view, one node per
  /// project. A filter narrows every folder; folder state is remembered.
  import { untrack } from 'svelte';
  import { flows as flowsMcp, mcpCall, execFlow } from '$lib/services/api';
  import { namespacesFor } from '$lib/stores/context.svelte.ts';
  import { contextRev_ } from '$lib/stores/apps.svelte.ts';
  import { categorize, catMeta } from '$lib/core/resources/categories';

  import type { OpenItem } from './flow';

  let { onopen, onnewflow, activeId = '' }: { onopen: (it: OpenItem) => void; onnewflow: (ns: string) => void; activeId?: string } = $props();

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
</script>

<div class="ex">
  <div class="ex-tools">
    <input placeholder="Filter" bind:value={filter} />
    <button title="Refresh" onclick={() => load()} disabled={loading}>↻</button>
  </div>
  {#if loading && !tree.length}<div class="ex-empty">Loading…</div>{/if}
  {#if !loading && !shown.length}<div class="ex-empty">{filter ? 'Nothing matches.' : 'No project in view — pick one in the project switcher (or turn on Show platform for a3t / system).'}</div>{/if}
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
        <button class="row leaf" class:on={activeId === n.key} style="padding-left: {10 + depth * 14}px" onclick={() => onopen(n.item!)} title="{n.item.kind} {n.item.ns}/{n.label}">
          <span class="ic {n.item.kind}">{icons[n.item.kind]}</span><span class="nm">{n.label}</span>
        </button>
      {:else}
        {@const def = n.label === 'Flows'}
        {@const isOpen = (open[n.key] ?? def) || !!filter}
        <div class="folder-row" style="padding-left: {4 + depth * 14}px">
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
</style>
