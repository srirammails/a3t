// The working context: which project (namespace) the dash is showing, and whether
// the platform's own namespaces are in view. Every list asks this store what to
// load instead of carrying its own namespace list — three hard-coded copies of
// ['default', 'a3t', 'system'] meant a new namespace never appeared anywhere and
// the platform's flows sat next to the user's.
//
// The project is part of the URL (`/p/<project>/<app>/…`), so a link reproduces
// what you saw; the last choice is remembered in this browser for a bare URL.

import { mcpCall, projectsApi, type ProjectInfo, type ProfileInfo, type OrgRole } from '$lib/services/api';
import { bumpContext } from './apps.svelte.ts';

export type Project = {
  name: string;
  /// The platform's own namespace (a3t, system), hidden unless asked for.
  platform: boolean;
  flows?: number;
  resources?: number;
  triggers?: number;
  /// The project as the a3t-projects flow shows it to you (owner, members,
  /// your role, …). Absent for a namespace that predates projects, such as
  /// `default`, which stays open to everyone.
  info?: ProjectInfo;
};

/// What a list holds decides whether platform namespaces belong in it.
///  - `owned`: things a project makes and runs (flows, triggers, executions,
///    canvases). Platform ones only with "Show platform".
///  - `shared`: things every project uses from the platform (LLM / ADB / MCP
///    resources, built-in templates, presets). Shown under "All projects"; with
///    a project selected, a list shows that project's items only.
///  - `picker`: a dropdown choosing what to use (an LLM, an MCP server, a target
///    flow) — always offers the platform's, whatever is selected.
export type ListKind = 'owned' | 'shared' | 'picker';

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
let profile = $state<ProfileInfo | null>(null);
let myOrgs = $state<{ handle: string; id: string; role: OrgRole }[]>([]);

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
/// Your profile handle (the owner part of your personal projects' namespaces).
export function getProfile(): ProfileInfo | null { return profile; }
/// The orgs you belong to, with your role in each.
export function getMyOrgs() { return myOrgs; }

/// Your role in a project as the server decided it (owner, developer, …,
/// org-admin, org-member, superadmin), or null for a namespace that predates
/// projects.
export function roleIn(p: Project): string | null {
  return p.info?.role ?? null;
}

/// A project is "mine" when I own it or am in it — not when I only see it as
/// superadmin or as a plain member of its org.
export function isMine(p: Project): boolean {
  const r = roleIn(p);
  return r != null && r !== 'superadmin' && r !== 'org-member';
}

/// Every project in the list is one the server let you see; legacy namespaces
/// (no project) are open to everyone.
export function canSee(_p: Project): boolean {
  return true;
}

export function isPlatform(ns: string | undefined): boolean {
  return !!ns && projects.some((p) => p.name === ns && p.platform);
}

/// The namespaces a list of this kind loads, in the order to show them: the
/// selected project (or every user project), then platform ones where they belong.
export function namespacesFor(kind: ListKind): string[] {
  const user = projects.filter((p) => !p.platform && canSee(p)).map((p) => p.name);
  const platform = projects.filter((p) => p.platform).map((p) => p.name);
  const base = project ? [project] : user;
  const withPlatform = showPlatform || kind === 'picker' || (kind === 'shared' && !project);
  return withPlatform ? [...base, ...platform.filter((n) => !base.includes(n))] : base;
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

/// A project pill: select its project, or — when it is already the selection —
/// clear the filter back to all projects.
export function toggleProject(name: string) {
  setProject(name === project ? ALL : name);
}

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
  // Namespaces and their counts from the registry; projects (who owns them, your
  // role) from the a3t-projects flow, which leaves out the ones you can't see.
  const [res, mine] = await Promise.all([mcpCall('list_projects', {}), projectsApi.list()]);
  const list: Project[] = res.ok ? (res.data?.projects ?? []) : [];
  const infos: ProjectInfo[] = mine.ok ? (mine.data?.projects ?? []) : [];
  const hidden = new Set<string>(mine.ok ? (mine.data?.project_namespaces ?? []) : []);
  profile = mine.ok ? (mine.data?.profile ?? null) : null;
  myOrgs = mine.ok ? (mine.data?.orgs ?? []) : [];
  if (!list.length && !infos.length) return; // keep the fallback
  const byName = new Map(list.map((p) => [p.name, p]));
  // A project that holds nothing yet isn't in the registry's namespace list.
  for (const i of infos) if (!byName.has(i.namespace)) byName.set(i.namespace, { name: i.namespace, platform: false });
  projects = [...byName.values()]
    .map((p) => ({ ...p, info: infos.find((i) => i.namespace === p.name) }))
    // A project you can't see is left out entirely, not shown as open.
    .filter((p) => p.info || p.platform || !hidden.has(p.name))
    .sort((a, b) => Number(a.platform) - Number(b.platform) || a.name.localeCompare(b.name));
  // A remembered project that no longer exists would show an empty dash.
  if (project && !projects.some((p) => p.name === project)) setProject(ALL);
  bumpContext();
}
