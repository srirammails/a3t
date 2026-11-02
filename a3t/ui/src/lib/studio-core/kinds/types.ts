import type { Item, ItemRef, Result } from '../items';
import type { QueryResult } from '../adapters/query';
import type { ChatMessage, Fired, Tool } from '../adapters/act';
import type { Doc } from '../workspace';

export type Capability = 'edit' | 'run' | 'debug' | 'create' | 'delete' | 'rename' | 'duplicate' | 'query' | 'chat' | 'tools' | 'fire';

/// A panel a kind shows in a studio region. `id` is the region's tab (kept as
/// the active tab across kinds, e.g. every kind's "form"); `component` names the
/// studio-ui panel that renders it (default: `id`).
export type PanelDef = {
  id: string; title: string; component?: string;
  /// Shown only while the kind's `badge` for it is above zero (e.g. Problems).
  onlyWithBadge?: boolean;
};

/// Everything the Studio knows about one sort of item. Adding a sort of item
/// (or a resource type with its own behaviour) means adding a kind; nothing
/// else branches on kinds.
export interface ItemKind {
  id: string;
  label: string;
  icon: string;
  match(item: Item): boolean;
  capabilities: Capability[];
  /// Where it shows: `center` names the studio-ui view for acting on it (run,
  /// query, chat, fire, design…); `code` the view for editing it as code (Edit,
  /// double-click; `doc.ui.view = 'code'`); `right` holds its Form and AI;
  /// `bottom` what acting produces (runs, results…); `left` tools beside the
  /// Explorer (the flow's step palette).
  views: { center: string; code?: string; left?: PanelDef[]; right?: PanelDef[]; bottom?: PanelDef[] };
  /// The Console section where this kind is managed in full, if any.
  console?: { section: string; title: string };

  /// A document's view state when it opens (`doc.ui`); views may bind to these.
  initialUi?(): Record<string, unknown>;
  /// Fill an opened document from the platform (text, data).
  load?(doc: Doc): Promise<void>;
  /// A new, unsaved item in `project` (the n-th this session); `type` picks the
  /// resource type for kinds that cover several.
  create?(project: string, n: number, type?: string): { ref: ItemRef; text: string; type?: string };
  /// Asked before pushing: a question to confirm with the user (e.g. "this
  /// template is used by 3 agents"), or null to push without asking.
  beforePush?(doc: Doc): Promise<string | null>;
  /// Push the document to the registry; answers where it now lives. (Saving
  /// is the Workspace's: a local draft. Deploying to a runtime is the view's.)
  push?(doc: Doc): Promise<Result<ItemRef>>;
  /// It differs from what the registry has (not pushed).
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
  /// The item's name is a field of its text (flows' `document.name`, a
  /// trigger's `name`), not typed separately when it is new.
  nameInText?: boolean;
  /// Why a name is not acceptable, or '' when it is.
  checkName?(name: string): string;
  /// Shown when renaming: what a rename does and does not carry over.
  renameNote?: string;
}
