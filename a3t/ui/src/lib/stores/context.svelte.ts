// The working context: which project (namespace) the dash is showing, and whether
// the platform's own namespaces are in view. Every list asks this store what to
// load instead of carrying its own namespace list — three hard-coded copies of
// ['default', 'a3t', 'system'] meant a new namespace never appeared anywhere and
// the platform's flows sat next to the user's.
//
// The project is part of the URL (`/p/<project>/<app>/…`), so a link reproduces
// what you saw; the last choice is remembered in this browser for a bare URL.

import { mcpCall } from '$lib/services/api';
import { bumpContext } from './apps.svelte.ts';

export type Project = {
  name: string;
  /// The platform's own namespace (a3t, system), hidden unless asked for.
  platform: boolean;
  flows?: number;
  resources?: number;
  triggers?: number;
};

/// What a list holds decides whether platform namespaces belong in it.
///  - `owned`: things a project makes and runs (flows, triggers, executions,
///    canvases, agents). Platform ones only with "Show platform".
///  - `shared`: things every project uses from the platform (LLM / ADB / MCP
///    resources in `a3t`, built-in templates in `system`). Always included.
export type ListKind = 'owned' | 'shared';

/// Used until the server answers, and when it can't (an older FlowR without
/// `list_projects`): the namespaces the dash always knew about.
const FALLBACK: Project[] = [
  { name: 'default', platform: false },
  { name: 'a3t', platform: true },
  { name: 'system', platform: true },
];

/// `''` is "All projects".
export const ALL = '';
const STORE_KEY = 'a3t.context';

let projects = $state<Project[]>(FALLBACK);
let project = $state<string>(ALL);
let showPlatform = $state(false);
let loaded = false;

function remembered(): { project?: string; showPlatform?: boolean } {
  try { return JSON.parse(localStorage.getItem(STORE_KEY) ?? '{}') ?? {}; } catch { return {}; }
}

function remember() {
  try { localStorage.setItem(STORE_KEY, JSON.stringify({ project, showPlatform })); } catch { /* private window */ }
}

// ── URL prefix ──

/// Split `/p/<project>/rest` into the project and the rest of the path.
export function splitContextPath(path: string): { project: string | null; rest: string } {
  const m = path.match(/^\/p\/([^/]+)(\/.*)?$/);
  if (!m) return { project: null, rest: path };
  return { project: decodeURIComponent(m[1]), rest: m[2] || '/' };
}

/// The path prefix for the current context; empty for "All projects".
export function contextPrefix(): string {
  return project ? `/p/${encodeURIComponent(project)}` : '';
}

/// Rewrite the address bar for a context change without adding a history entry.
function syncUrl() {
  if (typeof window === 'undefined') return;
  const { rest } = splitContextPath(window.location.pathname);
  const url = `${contextPrefix()}${rest === '/' && project ? '' : rest}` || '/';
  if (url !== window.location.pathname) window.history.replaceState(window.history.state, '', url + window.location.search);
}

// ── Reads ──

export function getProjects(): Project[] { return projects; }
export function getProject(): string { return project; }
export function getShowPlatform(): boolean { return showPlatform; }
export function isPlatform(ns: string | undefined): boolean {
  return !!ns && projects.some((p) => p.name === ns && p.platform);
}

/// The namespaces a list of this kind loads, in the order to show them: the
/// selected project (or every user project), then platform ones where they belong.
export function namespacesFor(kind: ListKind): string[] {
  const user = projects.filter((p) => !p.platform).map((p) => p.name);
  const platform = projects.filter((p) => p.platform).map((p) => p.name);
  const base = project ? [project] : user;
  return kind === 'shared' || showPlatform ? [...base, ...platform.filter((n) => !base.includes(n))] : base;
}

/// Whether a row from a list that isn't loaded per namespace is in view. Rows
/// without a namespace (conversations, fixed categories) always are.
export function inContext(ns: string | undefined, kind: ListKind = 'owned'): boolean {
  if (!ns) return true;
  return namespacesFor(kind).includes(ns) || (!project && !projects.some((p) => p.name === ns));
}

/// A canvas that deploys a flow is the project's; an unbound one is a preset,
/// shared like a template.
export function canvasKind(r: { config?: { flowRef?: string } }): ListKind {
  return r.config?.flowRef ? 'owned' : 'shared';
}

/// A stable key for "what the lists are showing", for pages that cache a load.
export function contextKey(): string {
  return `${project}|${showPlatform}`;
}

// ── Writes ──

export function setProject(name: string) {
  if (name === project) return;
  project = name;
  remember();
  syncUrl();
  bumpContext();
}

export function setShowPlatform(on: boolean) {
  if (on === showPlatform) return;
  showPlatform = on;
  remember();
  bumpContext();
}

/// Read the context from the URL (or what this browser remembers), then load the
/// project list. Called once by the layout before the panels read the URL.
export function initContext() {
  if (typeof window === 'undefined') return;
  const fromUrl = splitContextPath(window.location.pathname).project;
  const saved = remembered();
  project = fromUrl ?? saved.project ?? ALL;
  showPlatform = !!saved.showPlatform;
  if (fromUrl == null && project) syncUrl();
  void loadProjects();
}

/// Track a back/forward to a different project's URL.
export function contextFromUrl() {
  if (typeof window === 'undefined') return;
  const p = splitContextPath(window.location.pathname).project ?? ALL;
  if (p !== project) { project = p; remember(); bumpContext(); }
}

export async function loadProjects(force = false) {
  if (loaded && !force) return;
  loaded = true;
  const res = await mcpCall('list_projects', {});
  const list: Project[] | undefined = res.ok ? (res.data?.projects ?? undefined) : undefined;
  if (!list?.length) return; // keep the fallback
  projects = [...list].sort((a, b) => Number(a.platform) - Number(b.platform) || a.name.localeCompare(b.name));
  // A remembered project that no longer exists would show an empty dash.
  if (project && !projects.some((p) => p.name === project)) setProject(ALL);
  bumpContext();
}
