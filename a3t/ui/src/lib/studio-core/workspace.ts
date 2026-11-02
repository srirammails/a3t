/// The open documents of one Studio: which are open, which is active, edits
/// not yet saved, and the per-document state views keep (a run dialog…).
///
/// The Workspace mutates a state object it is given. A Svelte host passes a
/// `$state` object, so views update as it changes; other hosts (the VS Code
/// extension) pass a plain object and `subscribe`.
import { address, sameRef, type Item, type ItemRef } from './items';
import { kindById, kindFor, type ItemKind } from './kinds';
import type { StudioHost } from './host';

export type Doc = {
  /// The item's address: the tab id.
  address: string;
  ref: ItemRef;
  /// Id of the kind that handles it.
  kind: string;
  type?: string;
  data?: any;
  /// Text documents (flows): the text being edited and the text last loaded or saved.
  text?: string;
  saved?: string;
  loaded: boolean;
  isNew?: boolean;
  /// Why loading failed, if it did.
  error?: string;
  /// View state for this document (not kept across reloads).
  ui: Record<string, any>;
};

export type WorkspaceState = {
  docs: Doc[];
  active: string | null;
  /// Items opened lately, newest first (the Studio home lists them).
  recent: Item[];
};
export const emptyWorkspace = (): WorkspaceState => ({ docs: [], active: null, recent: [] });

const RECENT = 12;

type Stored = { active: string | null; docs: { ref: ItemRef; type?: string; data?: any }[]; recent?: Item[] };

export class Workspace {
  private listeners = new Set<() => void>();
  private created = 0;

  constructor(readonly app: string, readonly state: WorkspaceState, readonly host: StudioHost) {
    this.restore();
  }

  get active(): Doc | null {
    return this.find(this.state.active);
  }

  find(addr: string | null): Doc | null {
    return addr ? this.state.docs.find((d) => d.address === addr) ?? null : null;
  }

  kindOf(doc: Doc): ItemKind {
    return kindById(doc.kind);
  }

  isDirty(doc: Doc | null): boolean {
    return !!doc && !!this.kindOf(doc).dirty?.(doc);
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// Open an item (or bring its tab forward). `view: 'code'` opens it for
  /// editing as code (Edit, double-click); `'action'` in its own view.
  open(item: Item, opts: { view?: 'action' | 'code' } = {}): Doc {
    const addr = address(item);
    const { data: _data, ...light } = item;
    this.state.recent = [light, ...this.state.recent.filter((r) => address(r) !== addr)].slice(0, RECENT);
    if (!this.find(addr)) {
      const kind = kindFor(item);
      const { type, data, ...ref } = item;
      this.state.docs.push({ address: addr, ref, kind: kind.id, type, data, loaded: !kind.load, ui: kind.initialUi?.() ?? {} });
    }
    const doc = this.select(addr)!;
    if (opts.view) this.view(doc, opts.view);
    return doc;
  }

  /// Show a document in its own view or as code. A code request carries a new
  /// stamp each time, so views that keep their own mode (a flow's Graph | Code)
  /// switch even when it was asked for before.
  view(doc: Doc, view: 'action' | 'code') {
    doc.ui.view = view === 'code' ? 'code' : null;
    if (view === 'code') doc.ui.codeAsked = Date.now();
    this.changed();
  }

  select(addr: string): Doc | null {
    this.state.active = addr;
    this.changed();
    return this.active;
  }

  /// Close a tab; asks first when it has unsaved edits.
  async close(addr: string): Promise<boolean> {
    const doc = this.find(addr);
    if (!doc) return true;
    if (this.isDirty(doc) && !(await this.host.confirm(`${doc.ref.name} has changes that are not saved. Close it anyway?`))) return false;
    this.drop(addr);
    return true;
  }

  /// A new, unsaved item of `kindId` (and resource `type`) in `project`. It opens
  /// as code where its kind has a code view, so it can be named and filled in.
  create(kindId: string, project: string, type?: string): Doc | null {
    const kind = kindById(kindId);
    if (!kind.create) return null;
    this.created += 1;
    const made = kind.create(project, this.created, type);
    const addr = `${address(made.ref)}#new-${this.created}`;
    const ui = { ...(kind.initialUi?.() ?? {}), view: kind.views.code ? 'code' : null };
    this.state.docs.push({ address: addr, ref: made.ref, kind: kind.id, type: made.type, text: made.text, saved: '', loaded: true, isNew: true, ui });
    return this.select(addr);
  }

  /// Fill a document from the platform, once.
  async load(doc: Doc): Promise<void> {
    if (doc.loaded || doc.isNew) return;
    await this.kindOf(doc).load?.(doc);
    doc.loaded = true;
    this.changed();
  }

  /// Save through the document's kind; the tab follows the item to where it now lives.
  async save(doc: Doc) {
    const kind = this.kindOf(doc);
    if (!kind.save) return { ok: false as const, error: `${kind.label} cannot be saved here` };
    const r = await kind.save(doc);
    if (r.ok) this.retarget(doc, r.data);
    return r;
  }

  /// The item now lives at `ref` (saved under its own name, or renamed).
  retarget(doc: Doc, ref: ItemRef) {
    const addr = address(ref);
    const other = this.state.docs.findIndex((d) => d.address === addr && d !== doc);
    if (other >= 0) this.state.docs.splice(other, 1);
    const wasActive = this.state.active === doc.address;
    doc.address = addr;
    doc.ref = ref;
    doc.isNew = false;
    doc.loaded = true;
    if (wasActive) this.state.active = addr;
    this.changed();
  }

  /// Deleted elsewhere (the Explorer): its tab goes, edits and all.
  removed(ref: ItemRef) {
    this.state.recent = this.state.recent.filter((r) => !sameRef(r, ref));
    const doc = this.state.docs.find((d) => sameRef(d.ref, ref));
    if (doc) this.drop(doc.address);
  }

  /// Renamed elsewhere: the tab stays where it was and reloads under the new name.
  renamed(from: ItemRef, to: ItemRef) {
    this.state.recent = this.state.recent.map((r) => (sameRef(r, from) ? { ...r, ...to } : r));
    const doc = this.state.docs.find((d) => sameRef(d.ref, from));
    if (!doc) return;
    this.retarget(doc, to);
    Object.assign(doc, { text: undefined, saved: undefined, loaded: !this.kindOf(doc).load });
    this.changed();
  }

  private drop(addr: string) {
    const i = this.state.docs.findIndex((d) => d.address === addr);
    if (i < 0) return;
    this.state.docs.splice(i, 1);
    if (this.state.active === addr) this.state.active = this.state.docs[Math.min(i, this.state.docs.length - 1)]?.address ?? null;
    this.changed();
  }

  private get key() { return `a3t.studio.${this.app}.tabs`; }

  private changed() {
    this.persist();
    for (const fn of this.listeners) fn();
  }

  /// Open tabs are kept as addresses; their content reloads from the platform.
  private persist() {
    const docs = this.state.docs.filter((d) => !d.isNew)
      .map((d) => ({ ref: d.ref, type: d.type }));
    const active = this.active?.isNew ? null : this.state.active;
    this.host.storage.set(this.key, { active, docs, recent: this.state.recent } satisfies Stored);
  }

  private restore() {
    const v = this.host.storage.get(this.key) as any;
    if (!v || !Array.isArray(v.docs)) return;
    // Tabs kept before items had addresses: { id: 'flow:ns/name', kind, ns, name, data }.
    const legacy: Record<string, string> = {};
    for (const d of v.docs) {
      const ref: ItemRef | null = d.ref ?? (d.ns && d.kind && d.name ? { project: d.ns, kind: d.kind, name: d.name, ...(d.data?.id ? { id: d.data.id } : {}) } : null);
      if (!ref) continue;
      const doc = this.open({ ...ref, type: d.type, data: d.data });
      if (d.id) legacy[d.id] = doc.address;
    }
    const active = legacy[v.active] ?? v.active;
    this.state.active = this.find(active) ? active : this.state.docs[0]?.address ?? null;
    if (Array.isArray(v.recent)) this.state.recent = v.recent;
  }
}
