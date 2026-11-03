/// Query history and saved queries, kept per user through the host's storage.
/// Saved queries use the DB Console's key, so queries saved there carry over.
import type { StudioHost } from './host';

/// `resource` is `project/name`; `type` lets the entry reopen the right kind.
export type HistoryEntry = { resource: string; type?: string; query: string; lang: string; ts: number; ok: boolean; ms: number };
export type SavedQuery = { name: string; query: string; resource: string; type?: string; lang?: string };

const HISTORY = 'a3t.studio.db.history';
const SAVED = 'db-console-saved';
const KEEP = 100;

export function history(host: StudioHost): HistoryEntry[] {
  const v = host.storage.get(HISTORY);
  return Array.isArray(v) ? v : [];
}

export function remember(host: StudioHost, e: HistoryEntry): HistoryEntry[] {
  const next = [e, ...history(host)].slice(0, KEEP);
  host.storage.set(HISTORY, next);
  return next;
}

export function saved(host: StudioHost): SavedQuery[] {
  const v = host.storage.get(SAVED);
  return Array.isArray(v) ? v : [];
}

export function saveQuery(host: StudioHost, q: SavedQuery): SavedQuery[] {
  const next = [q, ...saved(host).filter((s) => !(s.name === q.name && s.resource === q.resource))];
  host.storage.set(SAVED, next);
  return next;
}

export function forget(host: StudioHost, q: SavedQuery): SavedQuery[] {
  const next = saved(host).filter((s) => !(s.name === q.name && s.resource === q.resource && s.query === q.query));
  host.storage.set(SAVED, next);
  return next;
}
