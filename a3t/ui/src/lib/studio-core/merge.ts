/// Three-way merge of a document's text: the registry copy the draft started
/// from (`base`), the draft (`mine`) and the registry copy now (`theirs`). The
/// registry's changes go into the draft wherever the draft left that place
/// alone; where both changed the same place differently, the draft's version is
/// kept and the place is reported as a conflict. Plain TypeScript (studio-core).
///
/// `merge3` merges by lines first, as git does, so every line nobody changed
/// stays exactly as it was (indentation, quotes, comments, folded text). Only
/// when lines clash does it try the structure (`mergeStructural`): maps key by
/// key, lists of named items (a flow's `do`, cases, extensions) by name — two
/// sides adding different tasks at the end clash as lines but not as structure.
import * as YAML from 'yaml';

export type Conflict = { path: (string | number)[]; mine: unknown; theirs: unknown };
export type Merged = { text: string; conflicts: Conflict[]; applied: number };

// ── By lines ──────────────────────────────────────────────────────────
type Hunk = { a: number; b: number; lines: string[] };   // replace base[a, b) by lines

/// The changes from `x` to `y` as hunks over `x`'s lines (longest common subsequence).
function hunks(x: string[], y: string[]): Hunk[] {
  const n = x.length, m = y.length;
  const L: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) L[i][j] = x[i] === y[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
  const out: Hunk[] = [];
  let i = 0, j = 0, cur: Hunk | null = null;
  const flush = () => { if (cur) { out.push(cur); cur = null; } };
  while (i < n || j < m) {
    if (i < n && j < m && x[i] === y[j]) { flush(); i++; j++; }
    else if (j < m && (i === n || L[i][j + 1] >= L[i + 1][j])) { cur ??= { a: i, b: i, lines: [] }; cur.lines.push(y[j++]); }
    else { cur ??= { a: i, b: i, lines: [] }; cur.b = ++i; }
  }
  flush();
  return out;
}

/// Line three-way merge; conflicting regions keep the draft's lines.
export function mergeLines(baseText: string, mineText: string, theirsText: string): Merged & { regions: { from: number; to: number }[] } {
  const base = baseText.split('\n'), mine = mineText.split('\n'), theirs = theirsText.split('\n');
  const hm = hunks(base, mine).map((h) => ({ ...h, side: 'm' as const }));
  const ht = hunks(base, theirs).map((h) => ({ ...h, side: 't' as const }));
  const all = [...hm, ...ht].sort((p, q) => p.a - q.a || p.b - q.b);
  const out: string[] = [];
  const regions: { from: number; to: number }[] = [];
  let pos = 0, applied = 0, k = 0;
  while (k < all.length) {
    // A group: hunks that overlap, or insertions at the same place. Changes to
    // neighbouring lines (two keys side by side) do not clash.
    const a = all[k].a;
    let b = all[k].b;
    const group = [all[k++]];
    const insertsAt = (p: number) => group.some((h) => h.a === p && h.b === p);
    while (k < all.length && (all[k].a < b || (all[k].a === b && all[k].a === all[k].b && insertsAt(b)))) { b = Math.max(b, all[k].b); group.push(all[k++]); }
    out.push(...base.slice(pos, a));
    const ms = group.filter((h) => h.side === 'm'), ts = group.filter((h) => h.side === 't');
    // One side's text over [a, b): base outside its hunks, its lines inside.
    const sideText = (hs: Hunk[]) => { const r: string[] = []; let p = a; for (const h of hs) { r.push(...base.slice(p, h.a), ...h.lines); p = h.b; } r.push(...base.slice(p, b)); return r; };
    if (!ts.length) out.push(...sideText(ms));
    else if (!ms.length) { out.push(...sideText(ts)); applied++; }
    else {
      const mt = sideText(ms), tt = sideText(ts);
      if (mt.join('\n') !== tt.join('\n')) regions.push({ from: out.length + 1, to: out.length + Math.max(1, mt.length) });
      out.push(...mt);
    }
    pos = b;
  }
  out.push(...base.slice(pos));
  return { text: out.join('\n'), conflicts: regions.map((r) => ({ path: [r.from === r.to ? `line ${r.from}` : `lines ${r.from}–${r.to}`], mine: null, theirs: null })), applied, regions };
}

/// Lines first; the structure when lines clash and the structure does not.
export function merge3(baseText: string, mineText: string, theirsText: string): Merged {
  const byLines = mergeLines(baseText, mineText, theirsText);
  if (!byLines.conflicts.length) return byLines;
  try {
    const byKeys = mergeStructural(baseText, mineText, theirsText);
    if (byKeys.conflicts.length < byLines.conflicts.length) return byKeys;
    // The same clashes either way: name each by its line and its key path.
    if (byKeys.conflicts.length === byLines.conflicts.length) {
      return { ...byLines, conflicts: byLines.conflicts.map((c, i) => ({ ...byKeys.conflicts[i], path: [...byKeys.conflicts[i].path, `(${c.path[0]})`] })) };
    }
  } catch { /* not YAML: the lines stand */ }
  return byLines;
}

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

export function mergeStructural(baseText: string, mineText: string, theirsText: string): Merged {
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
  // Written in the draft's style: lists indented under their key or not, and
  // long text not re-wrapped.
  const indentSeq = !/^([ \t]*)[^\s#-][^\n]*:[ \t]*\n\1- /m.test(mineText);
  return { text: applied ? doc.toString({ indentSeq, lineWidth: 0 }) : mineText, conflicts, applied };
}
