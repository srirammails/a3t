import type { Item, ItemRef, Result } from '../items';
import type { QueryResult } from '../adapters/query';
import type { ChatMessage, Fired, Tool } from '../adapters/act';
import type { Doc } from '../workspace';

export type Capability = 'edit' | 'run' | 'debug' | 'create' | 'delete' | 'rename' | 'duplicate' | 'query' | 'chat' | 'tools' | 'fire';

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
  /// Ids of studio-ui components: the center view (or, with `modes`, the views a
  /// document switches between — `doc.ui.mode`, the first by default), and the
  /// region panels.
  views: { center: string; modes?: PanelDef[]; right?: PanelDef[]; bottom?: PanelDef[] };
  /// The Console section where this kind is managed in full, if any.
  console?: { section: string; title: string };

  /// A document's view state when it opens (`doc.ui`); views may bind to these.
  initialUi?(): Record<string, unknown>;
  /// Fill an opened document from the platform (text, data).
  load?(doc: Doc): Promise<void>;
  /// A new, unsaved item in `project` (the n-th this session).
  create?(project: string, n: number): { ref: ItemRef; text: string };
  /// Persist the document; answers where it now lives.
  save?(doc: Doc): Promise<Result<ItemRef>>;
  dirty?(doc: Doc): boolean;
  /// Paths (`pathKey`) of values the platform showed masked in the loaded document.
  masked?(doc: Doc): Set<string>;
  /// A count on a panel's tab (e.g. problems), or null.
  badge?(doc: Doc, panel: string): number | null;

  /// Run a statement on the item (databases), in one of its languages.
  query?(doc: Doc, text: string, lang: string): Promise<Result<QueryResult>>;
  /// The LLM's answer to the conversation so far (LLMs).
  chat?(doc: Doc, messages: ChatMessage[]): Promise<Result<string>>;
  /// The tools an MCP server offers, and calling one (MCP servers).
  tools?(doc: Doc): Promise<Result<Tool[]>>;
  callTool?(doc: Doc, tool: string, args: unknown): Promise<Result<unknown>>;
  /// Fire the item with a payload as its type would (triggers).
  fire?(doc: Doc, payload: unknown): Promise<Result<Fired>>;

  remove?(item: Item): Promise<Result>;
  rename?(item: Item, to: string): Promise<Result>;
  duplicate?(item: Item, to: string): Promise<Result>;
  /// Why a name is not acceptable, or '' when it is.
  checkName?(name: string): string;
  /// Shown when renaming: what a rename does and does not carry over.
  renameNote?: string;
}
