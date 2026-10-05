// Shell state for the app shell's two navigational surfaces: the app rail and
// the per-app context list.
//
// Both occupy the same side region and are independently pinnable, so the shell
// has four layouts (rail+list, rail only, list only, neither). Unpinned surfaces
// are still reachable from the header — the Squares palette for apps, the Active
// Apps menu for the context list — so nothing becomes unreachable by unpinning.
//
// "Active apps" is taskbar-shaped: an app opened stays in the list until it is
// explicitly closed. It is deliberately NOT keep-alive — switching apps remounts
// the page and it restores from the server or localStorage, which is how every
// page in this console already works.

const LS_OPEN = 'a3t.shell.openApps';
const LS_RAIL = 'a3t.shell.railPinned';
// v2: the meaning changed. It used to mean "show the bar" (default on); it now
// means "keep the bar even with nothing selected" (default off), because the
// bar otherwise follows the selection. A new key so existing browsers adopt
// the new default instead of inheriting `true` and never seeing the home page.
const LS_CTX = 'a3t.shell.contextPinned.v2';
const LS_CTX_W = 'a3t.shell.contextW';

export interface OpenApp {
  type: string;
  title: string;
}

function load<T>(key: string, fallback: T): T {
  if (typeof window === 'undefined') return fallback;
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

function save(key: string, value: unknown) {
  if (typeof window === 'undefined') return;
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* private mode / blocked storage — the shell still works, it just forgets */
  }
}

// ── Which apps have a context list, and what it is called ──
//
// An app absent from this table has no list, so its context column collapses
// rather than showing an empty pane. `source` is resolved by ContextList.
/// `focuses` records whether the app's page actually acts on a selected item
/// (it reads `props.selectedId`). Where it does not, the list still navigates to
/// the app and stays useful as an overview, but it cannot yet jump to the item —
/// the pane says so rather than looking broken. Canvas is absent because there is
/// no list tool for it on the server.
export const APP_CONTEXT: Record<string, { label: string; source: string; focuses: boolean }> = {
  chat: { label: 'Conversations', source: 'sessions', focuses: true },
  agents: { label: 'Agents', source: 'agents', focuses: true },
  templates: { label: 'Templates', source: 'templates', focuses: true },
  'flow-editor': { label: 'Flows', source: 'flows', focuses: true },
  // Resources lists its FIXED subtype categories, not instances: the page is a
  // per-category browser, so the pane offers exactly the categories it filters to.
  resources: { label: 'Resource Types', source: 'resource-categories', focuses: true },
  triggers: { label: 'Triggers', source: 'triggers', focuses: false },
  executions: { label: 'Executions', source: 'executions', focuses: false },
  runtime: { label: 'Hosts & Runtimes', source: 'runtimes', focuses: true },
  canvas: { label: 'Canvases', source: 'canvases', focuses: true },
};

export function hasContext(type: string | undefined | null): boolean {
  return !!type && type in APP_CONTEXT;
}

export function contextLabel(type: string | undefined | null): string {
  return (type && APP_CONTEXT[type]?.label) || 'Context';
}

// ── State ──

let openApps = $state<OpenApp[]>(load<OpenApp[]>(LS_OPEN, []));
let railPinned = $state<boolean>(load<boolean>(LS_RAIL, true));
let contextPinned = $state<boolean>(load<boolean>(LS_CTX, false));
/// Transient reveal for an unpinned context list — the "open list pane" action
/// on the Active Apps menu. Never persisted: an overlay should not survive a
/// reload the user did not ask to keep it through.
let contextOpen = $state<boolean>(false);
let contextW = $state<number>(load<number>(LS_CTX_W, 260));

export const RAIL_W = 104;
export const CONTEXT_MIN = 180;
export const CONTEXT_MAX = 460;

// ── Active apps (taskbar) ──

export function getOpenApps(): OpenApp[] {
  return openApps;
}

/// Record an app as open. Idempotent, and refreshes the title so a renamed menu
/// entry does not leave a stale label in the list.
export function trackOpen(type: string, title: string) {
  if (!type) return;
  const i = openApps.findIndex((a) => a.type === type);
  if (i >= 0) {
    if (openApps[i].title !== title) {
      openApps = openApps.map((a) => (a.type === type ? { ...a, title } : a));
      save(LS_OPEN, openApps);
    }
    return;
  }
  openApps = [...openApps, { type, title }];
  save(LS_OPEN, openApps);
}

/// Close an app. Returns the app that should be shown instead when the closed one
/// was active — the caller navigates, since navigation lives in the panel store.
export function closeApp(type: string): OpenApp | null {
  const i = openApps.findIndex((a) => a.type === type);
  if (i < 0) return null;
  openApps = openApps.filter((a) => a.type !== type);
  save(LS_OPEN, openApps);
  if (openApps.length === 0) return null;
  // Prefer the neighbour to the left, matching editor-tab behaviour.
  return openApps[Math.max(0, i - 1)] ?? null;
}

export function isOpen(type: string): boolean {
  return openApps.some((a) => a.type === type);
}

// ── Pinning ──

export function isRailPinned(): boolean {
  return railPinned;
}

export function toggleRailPinned() {
  railPinned = !railPinned;
  save(LS_RAIL, railPinned);
}

export function isContextPinned(): boolean {
  return contextPinned;
}

export function toggleContextPinned() {
  contextPinned = !contextPinned;
  save(LS_CTX, contextPinned);
  // Pinning absorbs any transient reveal, so the pane does not end up drawn twice.
  if (contextPinned) contextOpen = false;
}

export function isContextOpen(): boolean {
  return contextOpen;
}

/// Reveal the context list without pinning it — the Active Apps "list pane" path.
export function openContextPane() {
  contextOpen = true;
}

export function closeContextPane() {
  contextOpen = false;
}

/// Whether the context list should be drawn for this app.
///
/// It follows the SELECTION: with nothing open the app shows its home page and
/// the bar stays out of the way; opening an item reveals it with that item
/// selected. Pinning is an override for keeping it on the home page too.
///
/// The layout derives the docked/floating split itself (it also needs the width),
/// so this is the single definition of the rule for anything else that asks.
export function contextVisible(type: string | undefined | null, hasSelection: boolean): boolean {
  return hasContext(type) && (hasSelection || contextPinned || contextOpen);
}

// ── Pins ──
//
// Server-owned, per user, so a pin follows you across devices exactly as the
// conversations themselves do. Loaded once per session and kept here so every
// surface (home page, context list) reads one copy.
let pins = $state<Record<string, string[]>>({});
let pinsLoaded = false;

export function getPins(kind: string): string[] {
  return pins[kind] ?? [];
}

export function isPinned(kind: string, id: string): boolean {
  return (pins[kind] ?? []).includes(id);
}

export function setPins(next: Record<string, string[]>) {
  pins = next ?? {};
}

export function pinsAreLoaded(): boolean {
  return pinsLoaded;
}

export function markPinsLoaded() {
  pinsLoaded = true;
}

// ── Context list freshness ──
//
// The list loads when the app changes; it has no way to know that the app just
// created something (a first chat turn writes a new session row). A revision
// counter the page bumps is cheaper and more precise than polling.
let contextRev = $state(0);

export function contextRev_(): number {
  return contextRev;
}

/// Tell the context list its data changed and it should reload.
export function bumpContext() {
  contextRev += 1;
}

// ── First message of a brand-new conversation ──
//
// A bare `/chat` shows a centred composer and creates NOTHING; the session only
// exists once you send. But sending has to navigate to `/chat/<id>`, which
// remounts the page — so the typed message is handed over here rather than lost
// in the unmount. Module state, never persisted: it is in flight for one tick.
let pendingFirstMessage: string | null = null;

export function setPendingMessage(text: string) {
  pendingFirstMessage = text;
}

/// Read once and clear — a handover, not a store.
export function takePendingMessage(): string | null {
  const t = pendingFirstMessage;
  pendingFirstMessage = null;
  return t;
}

export function getContextW(): number {
  return contextW;
}

export function setContextW(w: number) {
  contextW = Math.min(CONTEXT_MAX, Math.max(CONTEXT_MIN, Math.round(w)));
  save(LS_CTX_W, contextW);
}
