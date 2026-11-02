/// Query history and saved queries as Svelte state, so the Query view and the
/// History / Saved panels stay in step. Storage is studio-core's (`queries.ts`).
import { forget, history, remember, saved, saveQuery, type HistoryEntry, type SavedQuery, type StudioHost, type Workspace } from '$lib/studio-core';

export const queryLists = $state<{ history: HistoryEntry[]; saved: SavedQuery[]; loaded: boolean }>({ history: [], saved: [], loaded: false });

export function loadQueryLists(host: StudioHost) {
  if (queryLists.loaded) return;
  queryLists.history = history(host);
  queryLists.saved = saved(host);
  queryLists.loaded = true;
}

export const addHistory = (host: StudioHost, e: HistoryEntry) => { queryLists.history = remember(host, e); };
export const addSaved = (host: StudioHost, q: SavedQuery) => { queryLists.saved = saveQuery(host, q); };
export const removeSaved = (host: StudioHost, q: SavedQuery) => { queryLists.saved = forget(host, q); };

/// Put a remembered query into its database's document (opening it if needed).
export function reuse(workspace: Workspace, q: { resource: string; type?: string; query: string; lang?: string }) {
  const [project, ...rest] = q.resource.split('/');
  const doc = workspace.open({ project, kind: 'resource', name: rest.join('/'), type: q.type });
  doc.ui.mode = 'query';
  doc.ui.query = q.query;
  if (q.lang) doc.ui.lang = q.lang;
}
