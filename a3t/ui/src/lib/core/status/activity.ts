// What the console is waiting for, for the status bar: every request counts
// while it is in flight (so the bar stays until the last one is back, not the
// first), and whether the server can be reached at all.
import { writable } from 'svelte/store';
import { messages } from './messages';

export type Kind = 'loading' | 'saving' | 'deleting' | 'running';
export type Connection = 'connecting' | 'online' | 'offline';

export interface Activity {
  /** The busiest thing in flight (saving outranks running outranks loading), or null. */
  kind: Kind | null;
  /** When the console became busy (the bar waits a moment, so quick reads do not flash). */
  since: number;
  connection: Connection;
  /** Just finished: "Saved", "Deleted", "Connected" — shown briefly. */
  done: { word: string; at: number } | null;
}

const RANK: Record<Kind, number> = { loading: 1, running: 2, saving: 3, deleting: 3 };
const spans = new Map<number, Kind>();
let next = 1;
const state = writable<Activity>({ kind: null, since: 0, connection: 'connecting', done: null });
export const activity = { subscribe: state.subscribe };

function top(): Kind | null {
  let k: Kind | null = null;
  for (const v of spans.values()) if (!k || RANK[v] > RANK[k]) k = v;
  return k;
}

export function begin(kind: Kind): number {
  const id = next++;
  const was = spans.size;
  spans.set(id, kind);
  state.update((s) => ({ ...s, kind: top(), since: was ? s.since : Date.now() }));
  return id;
}

/// `ok`: it worked — a finished save says "Saved" once nothing else is saving.
export function end(id: number, ok = true) {
  const kind = spans.get(id);
  if (!kind) return;
  spans.delete(id);
  const k = top();
  state.update((s) => {
    const stillWriting = k === 'saving' || k === 'deleting';
    const done = ok && !stillWriting && (kind === 'saving' || kind === 'deleting') ? { word: kind === 'deleting' ? 'Deleted' : 'Saved', at: Date.now() } : s.done;
    return { ...s, kind: k, done };
  });
}

/// The request layer says whether the server answered.
export function reached(ok: boolean, detail = '') {
  state.update((s) => {
    if (ok && s.connection === 'online') return s;
    if (ok) return { ...s, connection: 'online', done: s.connection === 'offline' ? { word: 'Connected', at: Date.now() } : s.done };
    if (s.connection !== 'offline') messages.error('Network error', { key: 'offline', detail: detail || 'The console could not reach the A3T API. It retries with the next request.' });
    return { ...s, connection: 'offline' };
  });
}

// ── What a request is, from its method, path and (for MCP calls) tool and action ──
const READS = /^(list|get|read|search|query|describe|status|versions|plan|show|fetch|find|count|stats|check|me|whoami|history|logs|preview|validate)/i;
const DELETES = /^(delete|remove|purge)/i;
const RUNS = /^(run|execute|deploy|start|stop|cancel|trigger|terminate|hibernate|resume|invoke|chat|send)/i;

export function actionKind(action: unknown): Kind {
  if (typeof action !== 'string' || !action) return 'running';
  if (READS.test(action)) return 'loading';
  if (DELETES.test(action)) return 'deleting';
  if (RUNS.test(action)) return 'running';
  return 'saving';
}

export function requestKind(path: string, init: RequestInit = {}): Kind {
  const method = (init.method ?? 'GET').toUpperCase();
  if (path === '/message' && typeof init.body === 'string') {
    try {
      const b = JSON.parse(init.body);
      const tool: string = b?.params?.name ?? '';
      const args = b?.params?.arguments ?? {};
      if (tool === 'execute_flow') return actionKind(args.input?.input?.action ?? args.input?.action);
      if (tool === 'get_execution_status' || READS.test(tool)) return 'loading';
      if (DELETES.test(tool)) return 'deleting';
      if (RUNS.test(tool)) return 'running';
      return 'saving';
    } catch { return 'loading'; }
  }
  if (method === 'GET' || method === 'HEAD') return 'loading';
  if (method === 'DELETE') return 'deleting';
  if (/\/auth\//.test(path) || /(search|query|sql|plan|preview|validate)/i.test(path)) return 'loading';
  if (/(deploy|run|execute|terminate|hibernate|cancel|webhook|trigger)/i.test(path)) return 'running';
  return 'saving';
}
