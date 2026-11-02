/// Three-way merge of a document's YAML (or JSON) text: the registry copy the
/// draft started from (`base`), the draft (`mine`) and the registry copy now
/// (`theirs`). The registry's changes are applied to the draft wherever the
/// draft left that place alone; where both changed the same place differently,
/// the draft's value is kept and the place is reported as a conflict. The
/// draft's own layout and comments are kept. Plain TypeScript (studio-core).
///
/// Maps merge key by key. Lists of one-key maps with distinct keys (a flow's
/// `do`, a switch's cases, `use.extensions`) merge by name, like maps; other
/// lists are one value.
import * as YAML from 'yaml';

export type Conflict = { path: (string | number)[]; mine: unknown; theirs: unknown };
export type Merged = { text: string; conflicts: Conflict[]; applied: number };

type Path = (string | number)[];
const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
/// A list whose items are one-key maps with distinct keys: named items.
function named(v: unknown): v is Record<string, unknown>[] {
  if (!Array.isArray(v) || !v.length) return false;
  const keys = v.map((x) => (isObj(x) && Object.keys(x).length === 1 ? Object.keys(x)[0] : null));
  return keys.every((k) => k !== null) && new Set(keys).size === keys.length;
}
const nameOf = (x: Record<string, unknown>) => Object.keys(x)[0];
const asMap = (l: Record<string, unknown>[]) => new Map(l.map((x) => [nameOf(x), x[nameOf(x)]] as const));

export function merge3(baseText: string, mineText: string, theirsText: string): Merged {
  const base = YAML.parse(baseText || '{}') ?? {};
  const theirs = YAML.parse(theirsText || '{}') ?? {};
  const doc = YAML.parseDocument(mineText || '{}');
  const mine = doc.toJS() ?? {};
  const conflicts: Conflict[] = [];
  let applied = 0;

  /// The draft's path to a named item (`[…, 'do', 'ask']` → `[…, 'do', 2, 'ask']`).
  function real(path: Path): Path | null {
    const out: Path = [];
    let at: unknown = mine;
    for (const seg of path) {
      if (named(at) && typeof seg === 'string') {
        const i = (at as Record<string, unknown>[]).findIndex((x) => nameOf(x) === seg);
        if (i < 0) return null;
        out.push(i, seg);
        at = (at as Record<string, unknown>[])[i][seg];
      } else {
        out.push(seg);
        at = isObj(at) || Array.isArray(at) ? (at as any)[seg] : undefined;
      }
    }
    return out;
  }
  function getMine(path: Path): unknown {
    const p = real(path);
    return p ? doc.getIn(p) && YAML.isNode(doc.getIn(p, true)) ? (doc.getIn(p, true) as YAML.Node).toJSON() : doc.getIn(p) : undefined;
  }
  function apply(path: Path, value: unknown) {
    applied++;
    const parentPath = path.slice(0, -1);
    const last = path[path.length - 1];
    const parentMine = parentPath.length ? getMine(parentPath) : mine;
    // A named item: added, replaced or removed in the draft's list.
    if (named(parentMine) || (Array.isArray(parentMine) && typeof last === 'string')) {
      const lp = real(parentPath)!;
      const seq = doc.getIn(lp, true) as YAML.YAMLSeq;
      const i = (parentMine as Record<string, unknown>[]).findIndex((x) => nameOf(x) === last);
      if (value === undefined) { if (i >= 0) seq.items.splice(i, 1); return; }
      const node = doc.createNode({ [last as string]: value });
      if (i >= 0) seq.items[i] = node; else seq.items.push(node);
      return;
    }
    const p = real(path) ?? path;
    if (value === undefined) doc.deleteIn(p);
    else doc.setIn(p, doc.createNode(value));
  }

  function walk(path: Path, b: unknown, m: unknown, t: unknown) {
    if (eq(b, t) || eq(m, t)) return;              // they did not change it, or both made the same change
    if (eq(b, m)) { apply(path, t); return; }       // only they changed it
    // Both changed it: go inside when all three are maps or named lists.
    const maps = [b, m, t].every((x) => x === undefined || isObj(x)) && isObj(m) && isObj(t);
    const lists = [b, m, t].every((x) => x === undefined || named(x)) && named(m) && named(t);
    if (maps || lists) {
      const bm = lists ? (b ? asMap(b as any) : new Map()) : new Map(Object.entries((b as object) ?? {}));
      const mm = lists ? asMap(m as any) : new Map(Object.entries(m as object));
      const tm = lists ? asMap(t as any) : new Map(Object.entries(t as object));
      for (const k of new Set([...bm.keys(), ...mm.keys(), ...tm.keys()])) walk([...path, k], bm.get(k), mm.get(k), tm.get(k));
      return;
    }
    conflicts.push({ path, mine: m, theirs: t });
  }
  walk([], base, mine, theirs);
  return { text: doc.toString(), conflicts, applied };
}
