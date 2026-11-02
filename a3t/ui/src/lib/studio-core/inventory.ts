/// The Explorer's data: what the projects in scope hold, in folders — Flows,
/// Resources (a folder per category), Triggers, Templates, Canvases. Loading
/// never throws; what could not be loaded is listed, so an empty tree is never
/// a silent failure.
import { categorize, catMeta } from '$lib/core/resources/categories';
import * as flows from './adapters/flows';
import * as registry from './adapters/registry';
import { address, type Item } from './items';

export type TreeNode = {
  key: string;
  label: string;
  count?: number;
  children?: TreeNode[];
  item?: Item;
  /// Kind id a "new" action in this folder creates.
  create?: string;
};

export type Inventory = { tree: TreeNode[]; errors: string[] };

const nsOfSpec = (y: string) => (y ?? '').match(/^\s+namespace:\s*['"]?([\w.-]+)/m)?.[1] ?? '';

export async function loadInventory(projects: string[]): Promise<Inventory> {
  const errors: string[] = [];
  const take = <T>(r: { ok: true; data: T } | { ok: false; error: string }, fallback: T): T => {
    if (r.ok) return r.data;
    errors.push(r.error);
    return fallback;
  };

  const [triggers, templates] = await Promise.all([registry.listTriggers(), registry.listTemplates()]).then(([t, p]) => [take(t, []), take(p, [])]);
  const perProject = await Promise.all(projects.map(async (ns) => {
    const [f, r] = await Promise.all([flows.list(ns), registry.listResources(ns)]);
    return { ns, flows: take(f, []), resources: take(r, []) };
  }));

  const tree = perProject.map(({ ns, flows, resources }): TreeNode => {
    // One row per item: the registry can list a name twice.
    const folder = (key: string, label: string, items: Item[], create?: string): TreeNode => {
      const unique = new Map<string, Item>();
      for (const it of items) {
        it.name = String(it.name ?? it.id ?? '?');
        if (!unique.has(address(it))) unique.set(address(it), it);
      }
      const list = [...unique.values()].sort((a, b) => a.name.localeCompare(b.name));
      return { key: `${ns}/${key}`, label, count: list.length, create, children: list.map((it) => ({ key: address(it), label: it.name, item: it })) };
    };
    const byCat = new Map<string, Item[]>();
    const canvases: Item[] = [];
    for (const r of resources) {
      const type = r.type ?? r.type_ref ?? '';
      if (type === 'project') continue;
      if (type === 'canvas') { canvases.push({ project: ns, kind: 'canvas', name: r.name, type, data: r }); continue; }
      const cat = categorize(type);
      (byCat.get(cat) ?? byCat.set(cat, []).get(cat)!).push({ project: ns, kind: 'resource', name: r.name, type, data: r });
    }
    const resourceFolders = [...byCat.entries()]
      .sort((a, b) => catMeta(a[0]).label.localeCompare(catMeta(b[0]).label))
      .map(([cat, items]) => folder(`resources/${cat}`, catMeta(cat).label, items));
    return {
      key: ns, label: ns,
      children: [
        folder('flows', 'Flows', flows.map((f: any) => ({ project: ns, kind: 'flow', name: f.name, data: f })), 'flow'),
        { key: `${ns}/resources`, label: 'Resources', count: resourceFolders.reduce((n, f) => n + (f.count ?? 0), 0), children: resourceFolders },
        folder('triggers', 'Triggers', triggers.filter((t: any) => (t.namespace ?? 'default') === ns)
          .map((t: any) => ({ project: ns, kind: 'trigger', name: t.name ?? t.id, id: t.id, data: t }))),
        folder('templates', 'Templates', templates.filter((t: any) => nsOfSpec(t.spec_yaml) === ns)
          .map((t: any) => ({ project: ns, kind: 'template', name: t.name, id: t.id, data: t }))),
        folder('canvases', 'Canvases', canvases),
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
