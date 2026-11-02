/// Sync with the registry, shared by the Sync button, the compare view and the
/// Versions panel. The state lives on the document (`ui`), so every place that
/// shows it agrees.
///
///   Sync      check the registry. Unchanged since the edits started → push them.
///             Changed (someone pushed) → open the split view: yours (editable)
///             beside the registry's, each with its version underneath; Merge
///             and Push become available.
///   Merge     three-way merge of the registry's changes into yours. Clean → done,
///             and Sync pushes the result. Conflicts → the split view shows the
///             merged text beside the registry's, to finish by hand (arrows take
///             the registry's side of a change; or edit).
///   Push      overwrite the registry's copy with yours, whatever it has.
///   Versions  every version the registry keeps (bottom panel); any two — or one
///             and the editor — compare in the split view.
import type { Conflict, Doc, Workspace } from '$lib/studio-core';

export type Side = { label: string; version: string; text: string };
export type Compare = {
  /// `sync`: yours vs the registry's; `merge`: the merge result vs the registry's
  /// (conflicts to finish); `history`: two versions, read-only.
  mode: 'sync' | 'merge' | 'history';
  left: Side;
  right: Side;
  conflicts?: Conflict[];
};
export type SyncMode = 'sync' | 'versions' | 'merge' | 'push';

const say = (doc: Doc, ok: boolean, text: string) => { doc.ui.saved = { ok, text }; };
const versionOf = (ws: Workspace, doc: Doc, text: string) => ws.kindOf(doc).versionOf?.(text) ?? '';

/// A version conflict is known: the registry moved on since the edits started.
export const conflicted = (doc: Doc) => !!doc.ui.draftStale;

export async function sync(doc: Doc, ws: Workspace): Promise<void> {
  if (doc.ui.saving) return;
  if (doc.isNew) return push(doc, ws, { ask: false });
  doc.ui.saving = true; doc.ui.saved = null;
  const c = await ws.remoteChanged(doc);
  doc.ui.saving = false;
  if (c.error) { say(doc, false, c.error); return; }
  if (c.changed) {
    const theirs = c.text ?? '';
    doc.ui.compare = {
      mode: 'sync',
      left: { label: doc.draft ? 'Yours (draft)' : 'Yours', version: versionOf(ws, doc, doc.text ?? ''), text: '' },
      right: { label: 'Registry (changed since your edits started)', version: versionOf(ws, doc, theirs), text: theirs },
    } satisfies Compare;
    say(doc, false, 'The registry has a newer copy — Merge it into yours, or Push yours over it.');
    return;
  }
  if (!ws.isUnpushed(doc)) { say(doc, true, 'In sync with the registry.'); return; }
  await push(doc, ws, { ask: false });
}

export async function merge(doc: Doc, ws: Workspace): Promise<void> {
  if (doc.ui.saving) return;
  doc.ui.saving = true; doc.ui.saved = null;
  const r = await ws.merge(doc);
  doc.ui.saving = false;
  if (!r.ok) { say(doc, false, r.error); return; }
  if (r.conflicts.length) {
    const theirs = doc.saved ?? '';
    doc.ui.compare = {
      mode: 'merge',
      left: { label: 'Merged — finish here', version: versionOf(ws, doc, doc.text ?? ''), text: '' },
      right: { label: 'Registry', version: versionOf(ws, doc, theirs), text: theirs },
      conflicts: r.conflicts,
    } satisfies Compare;
    say(doc, false, `${r.conflicts.length} conflict${r.conflicts.length > 1 ? 's' : ''}: your values are kept there — take the registry’s with the arrows, or edit; then Sync.`);
    return;
  }
  doc.ui.compare = null;
  say(doc, true, r.applied ? `Merged ${r.applied} change${r.applied > 1 ? 's' : ''} from the registry — Sync pushes the result.` : 'Nothing to merge — Sync pushes yours.');
}

export async function push(doc: Doc, ws: Workspace, opts: { ask?: boolean } = {}): Promise<void> {
  if (doc.ui.saving) return;
  if (opts.ask !== false && conflicted(doc) && !(await ws.host.confirm(`Push ${doc.ref.name} over the registry’s newer copy? Its changes are lost (Merge keeps them).`))) return;
  doc.ui.saving = true; doc.ui.saved = null;
  const r = await ws.push(doc);
  doc.ui.saving = false;
  if (r.ok) doc.ui.compare = null;
  const v = versionOf(ws, doc, doc.text ?? '');
  say(doc, r.ok, r.ok ? `Pushed${v ? ` ${v}` : ''} to the registry.` : r.error);
}

/// Two versions side by side (read-only); `'editor'` is the text being edited.
export async function compareVersions(doc: Doc, ws: Workspace, a: string, b: string): Promise<void> {
  const kind = ws.kindOf(doc);
  const read = async (v: string): Promise<Side | string> => {
    if (v === 'editor') return { label: 'In the editor', version: versionOf(ws, doc, doc.text ?? ''), text: doc.text ?? '' };
    const r = await kind.loadVersion?.(doc, v);
    if (!r) return `${kind.label} items have no versions`;
    return r.ok ? { label: `Registry ${v}`, version: v, text: r.data } : r.error;
  };
  const [l, r] = await Promise.all([read(a), read(b)]);
  if (typeof l === 'string' || typeof r === 'string') { say(doc, false, typeof l === 'string' ? l : (r as string)); return; }
  doc.ui.compare = { mode: 'history', left: l, right: r } satisfies Compare;
}

export const closeCompare = (doc: Doc) => { doc.ui.compare = null; };
