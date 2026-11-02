/// Items: everything the Studio shows, with one address used as the tab id, the
/// drag payload, the web route and (later) the VS Code URI:
///
///   a3t://<project>/<kind>/<name or id>
///
/// `kind` here is the item's sort as the platform stores it (flow, resource,
/// trigger, template, canvas, run…); which behaviour module handles it is decided by
/// the kinds registry (`kinds/`), which may look at `type` too.

export type ItemSort = 'flow' | 'resource' | 'trigger' | 'template' | 'canvas' | 'run';

export type ItemRef = {
  project: string;
  kind: ItemSort;
  name: string;
  /// Id-keyed items (triggers, templates): the platform's id.
  id?: string;
};

export type Item = ItemRef & {
  /// Resource type (postgres, llm, mcp…), when there is one.
  type?: string;
  /// What the listing returned, for views that show it as is.
  data?: any;
};

export function address(ref: ItemRef): string {
  return `a3t://${ref.project}/${ref.kind}/${encodeURIComponent(ref.id ?? ref.name)}`;
}

export const sameRef = (a: ItemRef, b: ItemRef) => address(a) === address(b);

/// The drag payload type for an item (its JSON, without listing data).
export const ITEM_MIME = 'application/x-a3t-item';

/// What every adapter returns: never throws, says what went wrong.
export type Result<T = void> = { ok: true; data: T } | { ok: false; error: string };

export const ok = <T>(data: T): Result<T> => ({ ok: true, data });
export const fail = (error: string): Result<never> => ({ ok: false, error });
