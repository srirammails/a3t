import type { Item, ItemRef, Result } from '../items';
import type { Doc } from '../workspace';

export type Capability = 'edit' | 'run' | 'debug' | 'create' | 'delete' | 'rename' | 'duplicate';

/// A panel a kind shows in a studio region. `id` names a panel component in
/// studio-ui's registry.
export type PanelDef = { id: string; title: string };

/// Everything the Studio knows about one sort of item. Adding a sort of item
/// (or a resource type with its own behaviour) means adding a kind; nothing
/// else branches on kinds.
export interface ItemKind {
  id: string;
  label: string;
  icon: string;
  match(item: Item): boolean;
  capabilities: Capability[];
  /// Ids of studio-ui components: the center view, and the region panels.
  views: { center: string; right?: PanelDef[]; bottom?: PanelDef[] };
  /// The Console section where this kind is managed in full, if any.
  console?: { section: string; title: string };

  /// Fill an opened document from the platform (text, data).
  load?(doc: Doc): Promise<void>;
  /// A new, unsaved item in `project` (the n-th this session).
  create?(project: string, n: number): { ref: ItemRef; text: string };
  /// Persist the document; answers where it now lives.
  save?(doc: Doc): Promise<Result<ItemRef>>;
  dirty?(doc: Doc): boolean;
  /// A count on a panel's tab (e.g. problems), or null.
  badge?(doc: Doc, panel: string): number | null;

  remove?(item: Item): Promise<Result>;
  rename?(item: Item, to: string): Promise<Result>;
  duplicate?(item: Item, to: string): Promise<Result>;
  /// Why a name is not acceptable, or '' when it is.
  checkName?(name: string): string;
  /// Shown when renaming: what a rename does and does not carry over.
  renameNote?: string;
}
