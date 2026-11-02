/// The Explorer's data: what the projects in scope hold — Agent templates,
/// Agents, Resources (Workflows first, then a folder per resource category),
/// Triggers, Canvases, Runtimes. Loading never throws; what could not be loaded
/// is listed, so an empty tree is never a silent failure.
import { catMeta } from '$lib/core/resources/categories';
import * as flows from './adapters/flows';
import * as registry from './adapters/registry';
import { address, type Item } from './items';
import { categoryOf, TYPES_BY_CATEGORY } from './resourceTypes';

export type TreeNode = {
  key: string;
  label: string;
  count?: number;
  children?: TreeNode[];
  item?: Item;
  /// What "+" in this folder creates: a kind, and for resources a type.
  create?: { kind: string; type?: string };
  /// Open until the user folds it (folder state is remembered after that).
  open?: boolean;
};

export type Inventory = { tree: TreeNode[]; errors: string[] };

const nsOfSpec = (y: string) => (y ?? '').match(/^\s+namespace:\s*['"]?([\w.-]+)/m)?.[1] ?? '';

/// Resource folders in this order (any other category after them).
const RESOURCE_ORDER = ['database', 'llm', 'mcp', 'credentials', 'functions', 'scripts', 'variables'];
/// The kind that creates a resource of each category.
const CREATE_KIND: Record<string, string> = { database: 'database', llm: 'llm', mcp: 'mcp' };

export async function loadInventory(projects: string[]): Promise<Inventory> {
  const errors: string[] = [];
  const take = <T>(r: { ok: true; data: T } | { ok: false; error: string }, fallback: T): T => {
    if (r.ok) return r.data;
    errors.push(r.error);
    return fallback;
  };

  const [triggers, templates, agents] = await Promise.all([registry.listTriggers(), registry.listTemplates(), registry.listAgents()])
    .then(([t, p, a]) => [take(t, []), take(p, []), take(a, [])]);
  const perProject = await Promise.all(projects.map(async (ns) => {
    const [f, r] = await Promise.all([flows.list(ns), registry.listResources(ns)]);
    return { ns, flows: take(f, []), resources: take(r, []) };
  }));

  const tree = perProject.map(({ ns, flows, resources }): TreeNode => {
    // One row per item: the registry can list a name twice.
    const folder = (key: string, label: string, items: Item[], extra: Partial<TreeNode> = {}): TreeNode => {
      const unique = new Map<string, Item>();
      for (const it of items) {
        it.name = String(it.name ?? it.id ?? '?');
        if (!unique.has(address(it))) unique.set(address(it), it);
      }
      const list = [...unique.values()].sort((a, b) => a.name.localeCompare(b.name));
      return { key: `${ns}/${key}`, label, count: list.length, ...extra, children: list.map((it) => ({ key: address(it), label: it.name, item: it })) };
    };
    const byCat = new Map<string, Item[]>(RESOURCE_ORDER.map((c) => [c, []]));
    const canvases: Item[] = [];
    const runtimes: Item[] = [];
    for (const r of resources) {
      const type = r.type ?? r.type_ref ?? '';
      if (type === 'project') continue;
      if (type === 'canvas') { canvases.push({ project: ns, kind: 'canvas', name: r.name, type, data: r }); continue; }
      // Runtime pools are resources too, shown in their own folder.
      if (type === 'runtime') { runtimes.push({ project: ns, kind: 'resource', name: r.name, type, data: r }); continue; }
      const cat = categoryOf(type);
      (byCat.get(cat) ?? byCat.set(cat, []).get(cat)!).push({ project: ns, kind: 'resource', name: r.name, type, data: r });
    }
    const resourceFolders = [...byCat.entries()].map(([cat, items]) => {
      const type = TYPES_BY_CATEGORY[cat]?.[0]?.type;
      return folder(`resources/${cat}`, catMeta(cat).label, items, type ? { create: { kind: CREATE_KIND[cat] ?? 'resource', type } } : {});
    });
    const workflows = folder('resources/workflows', 'Workflows', flows.map((f: any) => ({ project: ns, kind: 'flow', name: f.name, data: f })),
      { create: { kind: 'flow' }, open: true });
    return {
      key: ns, label: ns,
      children: [
        folder('templates', 'Agent templates', templates.filter((t: any) => nsOfSpec(t.spec_yaml) === ns)
          .map((t: any) => ({ project: ns, kind: 'template', name: t.name, id: t.id, data: t }))),
        folder('agents', 'Agents', agents.filter((a: any) => (a.namespace ?? 'a3t') === ns)
          .map((a: any) => ({ project: ns, kind: 'agent', name: a.name ?? a.id, id: a.id, data: a })), { create: { kind: 'agent' } }),
        {
          key: `${ns}/resources`, label: 'Resources', open: true,
          count: workflows.count! + resourceFolders.reduce((n, f) => n + (f.count ?? 0), 0),
          children: [workflows, ...resourceFolders],
        },
        folder('triggers', 'Triggers', triggers.filter((t: any) => (t.namespace ?? 'default') === ns)
          .map((t: any) => ({ project: ns, kind: 'trigger', name: t.name ?? t.id, id: t.id, data: t })), { create: { kind: 'trigger' } }),
        folder('canvases', 'Canvases', canvases, { create: { kind: 'canvas' } }),
        folder('runtimes', 'Runtimes', runtimes, { create: { kind: 'runtime', type: 'runtime' } }),
      ],
    };
  });
  return { tree, errors };
}

/// Names already taken by items of a sort in a project (for new names).
export function namesIn(inv: Inventory, project: string, kind: Item['kind']): Set<string> {
  const out = new Set<string>();
  const walk = (n: TreeNode) => {
    if (n.item?.project === project && n.item.kind === kind) out.add(n.item.name);
    n.children?.forEach(walk);
  };
  inv.tree.forEach(walk);
  return out;
}
