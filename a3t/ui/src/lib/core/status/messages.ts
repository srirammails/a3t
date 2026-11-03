// Messages and notifications.
//
// A message is shown once in the status bar at the top (see shell/StatusBar):
// a word in its colour — Saved, Warning, Error — with ▾ to read the rest.
// Warnings and errors are also kept as notifications: the bell in the header
// counts the unread ones, and the notifications pop-in lists them all.
//
// The same message again (a poll failing every few seconds) is not a new one:
// it is counted on the one already there, so neither the bar nor the list fill
// up with copies.
import { get, writable } from 'svelte/store';

export type MessageType = 'success' | 'error' | 'warning' | 'info';

export interface Message {
  id: number;
  type: MessageType;
  /** One short line: what happened. */
  title: string;
  /** Everything the caller said, when it is more than the title. */
  detail: string;
  /** What raised it — a flow, a page — when known. */
  source?: string;
  /** First and latest time it happened, and how often. */
  first: number;
  at: number;
  count: number;
  read: boolean;
  /** Same key → same message (counted, not repeated). */
  key: string;
}

export interface MessageOpts { source?: string; key?: string; detail?: string }

let nextId = 1;

/// A short, readable title for what a caller said; the full text stays as detail.
export function summarize(text: string): string {
  const t = text.trim();
  if (/^Network error/i.test(t) || /Failed to fetch|NetworkError|ECONNREFUSED/i.test(t)) return 'Can’t reach the server';
  if (/^Session expired/i.test(t)) return 'Your session expired — sign in again';
  let line = t.split('\n')[0].replace(/^(Error|error|ERROR):\s*/, '').replace(/^"|"$/g, '');
  // A JSON body: its message, if it has one.
  if (/^[{[]/.test(line)) {
    try { const j = JSON.parse(t); line = String(j.message ?? j.error ?? j.detail ?? 'The server sent an error'); } catch { line = 'The server sent an error'; }
  }
  const http = line.match(/^HTTP (\d{3})$/);
  if (http) line = `The server answered ${http[1]}${({ 400: ' (bad request)', 403: ' (not allowed)', 404: ' (not found)', 409: ' (conflict)', 500: ' (server error)', 502: ' (bad gateway)', 503: ' (unavailable)', 504: ' (timed out)' } as Record<string, string>)[http[1]] ?? ''}`;
  if (line.length > 90) line = line.slice(0, 88).replace(/\s+\S*$/, '') + '…';
  return line.charAt(0).toUpperCase() + line.slice(1);
}

// ── Notifications: the warnings and errors, kept ──
const STORE = 'a3t.notifications';
const KEEP = 100;
function loadKept(): Message[] {
  try {
    const v = JSON.parse(localStorage.getItem(STORE) ?? '[]');
    if (Array.isArray(v)) { for (const m of v) nextId = Math.max(nextId, (m.id ?? 0) + 1); return v; }
  } catch { /* none, or no storage */ }
  return [];
}
function createNotifications() {
  const store = writable<Message[]>(typeof window !== 'undefined' ? loadKept() : []);
  store.subscribe((list) => {
    if (typeof window === 'undefined') return;
    try { localStorage.setItem(STORE, JSON.stringify(list.slice(0, KEEP))); } catch { /* private window */ }
  });
  return {
    subscribe: store.subscribe,
    /// Newest first; the same key moves the old entry to the top and counts it.
    record(m: Message) {
      store.update((list) => {
        const old = list.find((x) => x.key === m.key);
        const rest = list.filter((x) => x.key !== m.key);
        const next = old ? { ...old, ...m, id: old.id, first: old.first, count: old.count + 1, read: false } : m;
        return [next, ...rest].slice(0, KEEP);
      });
    },
    markRead(id?: number) { store.update((l) => l.map((m) => (id === undefined || m.id === id ? { ...m, read: true } : m))); },
    remove(id: number) { store.update((l) => l.filter((m) => m.id !== id)); },
    clear() { store.set([]); },
    get: (id: number) => get(store).find((m) => m.id === id),
  };
}
export const notifications = createNotifications();

/// The notifications pop-in: open, and which one it shows.
export const notificationsPanel = writable<{ open: boolean; id: number | null }>({ open: false, id: null });
export function openNotifications(id: number | null = null) {
  notificationsPanel.set({ open: true, id });
}

// ── Messages: the latest one, for the status bar ──
function createMessages() {
  /// The one the bar shows (null: nothing to say).
  const live = writable<Message | null>(null);

  function add(type: MessageType, text: string, opts: MessageOpts = {}) {
    const full = (opts.detail ? `${text}\n\n${opts.detail}` : text).trim();
    const title = summarize(text);
    // Every "cannot reach the server" is one notification, whoever noticed it.
    const key = opts.key ?? (title === 'Can’t reach the server' ? 'offline' : `${type}:${opts.source ?? ''}:${title}`);
    const now = Date.now();
    const cur = get(live);
    const same = cur && cur.key === key;
    const m: Message = {
      id: same ? cur.id : nextId++, type, title, detail: full === title ? '' : full,
      source: opts.source, first: same ? cur.first : now, at: now, count: same ? cur.count + 1 : 1, read: false, key,
    };
    live.set(m);
    if (type === 'error' || type === 'warning') notifications.record(m);
    return m.id;
  }

  return {
    subscribe: live.subscribe,
    success: (text: string, opts?: MessageOpts) => add('success', text, opts),
    error: (text: string, opts?: MessageOpts) => add('error', text, opts),
    warning: (text: string, opts?: MessageOpts) => add('warning', text, opts),
    info: (text: string, opts?: MessageOpts) => add('info', text, opts),
    /// The bar is done with it (it stays in the notifications).
    remove(id?: number) { live.update((m) => (m && (id === undefined || m.id === id) ? null : m)); },
  };
}

export const messages = createMessages();
