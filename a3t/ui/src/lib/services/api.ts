// A3T API Client — handles JWT auth, token refresh, and all backend calls
//
// Base URL is configured via environment or defaults to same origin.
// All requests include Authorization: Bearer <token> when authenticated.

import { API_BASE } from '$lib/config';

interface ApiResponse<T = any> {
  ok: boolean;
  data?: T;
  error?: string;
  status: number;
  /// The error response's body. A refusal can carry more than a message — a
  /// runtime delete answers 409 with the reschedule plan to show first.
  detail?: any;
}

// ── Token management ──

let accessToken: string | null = null;
let refreshToken: string | null = null;
let onAuthExpired: (() => void) | null = null;

export function setTokens(access: string, refresh: string) {
  accessToken = access;
  refreshToken = refresh;
}

export function clearTokens() {
  accessToken = null;
  refreshToken = null;
}

export function getAccessToken(): string | null {
  return accessToken;
}

export function onTokenExpired(callback: () => void) {
  onAuthExpired = callback;
}

// ── Core fetch wrapper ──

async function request<T = any>(
  path: string,
  options: RequestInit = {},
  requireAuth = true,
): Promise<ApiResponse<T>> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> || {}),
  };

  if (requireAuth && accessToken) {
    headers['Authorization'] = `Bearer ${accessToken}`;
  }

  try {
    const res = await fetch(`${API_BASE}${path}`, {
      ...options,
      headers,
    });

    // Token expired — try refresh
    if (res.status === 401 && requireAuth && refreshToken) {
      const refreshed = await tryRefresh();
      if (refreshed) {
        // Retry with new token
        headers['Authorization'] = `Bearer ${accessToken}`;
        const retry = await fetch(`${API_BASE}${path}`, { ...options, headers });
        const data = await retry.json().catch(() => null);
        return { ok: retry.ok, data, status: retry.status, error: retry.ok ? undefined : data?.error };
      }
      // Refresh failed — session expired
      if (onAuthExpired) onAuthExpired();
      return { ok: false, error: 'Session expired', status: 401 };
    }

    const data = await res.json().catch(() => null);
    return {
      ok: res.ok,
      data: res.ok ? data : undefined,
      // Prefer the human sentence over the code ("confirm required").
      error: res.ok ? undefined : (data?.message || data?.error || `HTTP ${res.status}`),
      status: res.status,
      detail: res.ok ? undefined : data,
    };
  } catch (e) {
    return { ok: false, error: `Network error: ${e}`, status: 0 };
  }
}

async function tryRefresh(): Promise<boolean> {
  try {
    const res = await fetch(`${API_BASE}/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (res.ok) {
      const data = await res.json();
      accessToken = data.access_token;
      refreshToken = data.refresh_token;
      return true;
    }
  } catch {}
  return false;
}

// ── Auth endpoints (public — no JWT needed) ──

export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
  user: {
    id: string;
    org_id: string;
    email: string;
    name: string;
    role: string;
  };
}

export async function login(email: string, password: string): Promise<ApiResponse<LoginResponse>> {
  return request<LoginResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  }, false);
}

export async function signup(name: string, email: string, password: string): Promise<ApiResponse<LoginResponse>> {
  return request<LoginResponse>('/auth/signup', {
    method: 'POST',
    body: JSON.stringify({ name, email, password }),
  }, false);
}

export async function getMe(): Promise<ApiResponse<LoginResponse['user']>> {
  return request('/auth/me');
}

// ── MCP tool calls (authenticated) ──

// ── Platform database (FlowR EE: embedded LanceDB or ADB platform tables) ──

export interface PlatformDbStatus { backend: 'lancedb' | 'adb'; describe: string; adb_supported: boolean; config: Record<string, any> | null }

export async function getPlatformDb(): Promise<ApiResponse<PlatformDbStatus>> {
  return request<PlatformDbStatus>('/registry/platform-db');
}

// ── ADB embedding settings (ADB W9.10), through FlowR ──

export interface AdbEmbeddingSettings { provider: 'local' | 'openai'; model: string; dimension: number; url?: string; api_key_env?: string }
export interface AdbEmbeddingStatus { source: 'settings' | 'default'; settings: AdbEmbeddingSettings; active: { model: string; dimension: number } | null; pending: number }

/** The ADB at `adbUrl`: its embedding settings and how many records await a vector. */
export async function getAdbEmbedding(adbUrl: string): Promise<ApiResponse<{ success: boolean; data?: AdbEmbeddingStatus; error?: string }>> {
  return request(`/registry/adb-embedding?url=${encodeURIComponent(adbUrl)}`);
}

/** Save new embedding settings on the ADB at `adbUrl`; ADB checks them, then restarts to apply. */
export async function setAdbEmbedding(adbUrl: string, settings: AdbEmbeddingSettings): Promise<ApiResponse<{ success: boolean; data?: { restart: boolean; reembed: boolean }; error?: string }>> {
  return request('/registry/adb-embedding', { method: 'PUT', body: JSON.stringify({ url: adbUrl, settings }) });
}

// ── ADB SQL (superadmin debug view over everything in ADB's Lance), through FlowR ──

export interface AdbSqlResult { columns: string[]; rows: Record<string, string | null>[] }

/** Read-only SQL on the ADB resource `namespace/name` (memory, memory_current, memory_raw, platform tables). Superadmin only. */
export async function adbSql(resource: string, sql: string, limit?: number): Promise<ApiResponse<{ success: boolean; data?: AdbSqlResult; error?: string }>> {
  return request('/registry/adb-sql', { method: 'POST', body: JSON.stringify({ resource, sql, limit }) });
}

// ── ADB hot cache rules (ADB W10), through FlowR ──

export type AdbCacheKeep = 'all' | 'none' | { last: number } | { bytes: number | string } | { within: string };
export interface AdbCacheRule { memory: 'working' | 'tools' | 'procedural' | 'semantic' | 'episodic'; scope?: string; keep: AdbCacheKeep }
export interface AdbCacheSettings { max_bytes: number | string; rules: AdbCacheRule[]; vectors?: boolean }
export interface AdbCacheBucket { memory: string; scope: string; rows: number; bytes: number; complete: boolean }
export interface AdbCacheStats { rows: number; bytes: number; max_bytes: number; hits: number; misses: number; mismatches: number; buckets: AdbCacheBucket[] }
export interface AdbCacheStatus { mode: 'on' | 'off' | 'shadow'; source: 'settings' | 'environment' | 'default'; settings: AdbCacheSettings; stats: AdbCacheStats | null }

/** The ADB at `adbUrl`: its hot cache rules and figures. */
export async function getAdbCache(adbUrl: string): Promise<ApiResponse<{ success: boolean; data?: AdbCacheStatus; error?: string }>> {
  return request(`/registry/adb-cache?url=${encodeURIComponent(adbUrl)}`);
}

/** Save hot cache rules on the ADB at `adbUrl`; it rebuilds its cache with them at once. */
export async function setAdbCache(adbUrl: string, settings: AdbCacheSettings): Promise<ApiResponse<{ success: boolean; data?: { stats: AdbCacheStats | null; took_ms: number }; error?: string }>> {
  return request('/registry/adb-cache', { method: 'PUT', body: JSON.stringify({ url: adbUrl, settings }) });
}

/** Copy every platform table to `target`, then switch A3T to it. */
export async function switchPlatformDb(target: { backend: 'adb' | 'lancedb'; url?: string; flight_url?: string }): Promise<ApiResponse<{ from: string; to: string; tables: number; rows: number }>> {
  return request('/registry/platform-db/switch', { method: 'POST', body: JSON.stringify(target) });
}

export async function mcpCall(toolName: string, args: Record<string, any> = {}): Promise<ApiResponse> {
  const res = await request('/message', {
    method: 'POST',
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'tools/call',
      id: Date.now(),
      params: { name: toolName, arguments: args },
    }),
  });

  if (res.ok && res.data?.result) {
    const content = res.data.result.content?.[0];
    // Check isError FIRST. A failed tool call still carries its message as text,
    // so extracting text before this returned {ok:true, data:"id is required"}
    // and callers rendered the error as if it were the artifact's body.
    if (res.data.result.isError) {
      return { ok: false, error: content?.text || 'MCP error', status: 400 };
    }
    if (content?.type === 'text') {
      try {
        return { ok: true, data: JSON.parse(content.text), status: 200 };
      } catch {
        return { ok: true, data: content.text, status: 200 };
      }
    }
    return { ok: true, data: res.data.result, status: 200 };
  }

  return res;
}

// ── Convenience wrappers ──

export const flows = {
  list: (namespace?: string) => mcpCall('list_flows', namespace ? { namespace } : {}),
  get: (name: string, namespace = 'default') => mcpCall('get_flow', { name, namespace }),
  execute: (name: string, namespace = 'default', input?: any) => mcpCall('execute_flow', { name, namespace, ...(input ? { input } : {}) }),
  status: (instanceId: string) => mcpCall('get_execution_status', { instance_id: instanceId }),
  getExecStatus: (instanceId: string) => mcpCall('get_execution_status', { instance_id: instanceId }),
};

export const resources = {
  list: (namespace?: string) => mcpCall('list_resources', namespace ? { namespace } : {}),
  get: (name: string, namespace = 'default') => mcpCall('get_resource', { name, namespace }),
  create: (name: string, typeRef: string, config: any, namespace = 'default') =>
    mcpCall('create_resource', { name, namespace, type_ref: typeRef, config }),
  /** Update a resource's config (registry PUT — body is the new config object). */
  update: (namespace: string, name: string, config: any) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, {
      method: 'PUT', body: JSON.stringify(config),
    }),
  /** Delete a resource (registry REST DELETE — the MCP delete_resource tool is Cedar-gated in EE).
   *  A runtime with deployments answers 409 + its reschedule plan; repeat with `confirm` = its name. */
  remove: (namespace: string, name: string, confirm?: string) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}${confirm ? `?confirm=${encodeURIComponent(confirm)}` : ''}`, {
      method: 'DELETE',
    }),
};

// ── Projects, orgs and profiles ──
// All three are A3T flows (a3t/backend/flows/a3t): a3t-projects, a3t-orgs
// and a3t-profile, over platform-DB tables. The flows decide who may see and
// change what, from the caller's principal — the UI only asks.
//
// A personal project's namespace is <profile handle>-<name>; an org's is
// <org handle>-<name>. Moving a personal project to an org renames it.
export type ProjectRole = 'owner' | 'developer' | 'operator' | 'viewer';
export type ProjectMember = { kind: 'user' | 'group'; id: string; email?: string; name?: string; role: ProjectRole };
export type ProjectInfo = {
  namespace: string;
  owner_kind: 'user' | 'org';
  owner: string;
  name: string;
  display_name?: string;
  description?: string;
  private: boolean;
  members: ProjectMember[];
  /// Project rules on top of the org baseline (Projects → Policy): preset keys,
  /// and `runtime` (isolated | shared; unset = the org's setting).
  policy: Record<string, any>;
  environments: string[];
  created_by?: string;
  created_at?: string;
  /// The caller's role: owner/developer/operator/viewer (member), org-admin,
  /// org-securityadmin, org-member, or superadmin.
  role: string;
  can_admin: boolean;
};
export type OrgRole = 'member' | 'admin' | 'securityadmin';
export type OrgInfo = {
  id: string; handle: string; display_name: string; created_at?: string;
  my_role: OrgRole | null; can_admin: boolean;
  /// Where the org's flows run: its own runtimes only, or the shared ones.
  runtime: 'isolated' | 'shared';
  members: { user_id: string; email?: string; name?: string; role: OrgRole }[];
};
export type ProfileInfo = { handle: string; user_id: string; email?: string; name?: string };

/// Run one of the project flows; a flow's own refusal ({ ok: false, error })
/// comes back as an error.
async function projectFlow(name: string, input: Record<string, any>): Promise<{ ok: boolean; data?: any; error?: string }> {
  const res = await execFlow(name, 'a3t', input, { silent: true });
  if (!res.ok) return res;
  if (res.data?.ok === false) return { ok: false, error: res.data.error ?? 'Refused' };
  return { ok: true, data: res.data };
}

export const projectsApi = {
  /** Projects you can see (with your role), your profile, your orgs, and every
   *  namespace that is a project (to hide the ones you can't see). */
  list: () => projectFlow('a3t-projects', { action: 'list' }),
  create: (p: { owner_kind: 'user' | 'org'; owner: string; name: string; display_name?: string; description?: string; private?: boolean }) =>
    projectFlow('a3t-projects', { action: 'create', ...p }),
  update: (namespace: string, changes: Partial<Pick<ProjectInfo, 'display_name' | 'description' | 'private' | 'members' | 'policy'>>) =>
    projectFlow('a3t-projects', { action: 'update', namespace, ...changes }),
  remove: (namespace: string) => projectFlow('a3t-projects', { action: 'delete', namespace }),
  /** Move a personal project to an org. `dry_run` (the default) reports what
   *  would move and what outside the project refers to it. */
  move: (namespace: string, to_org: string, dry_run = true) =>
    projectFlow('a3t-projects', { action: 'move', namespace, to_org, dry_run }),
};

/// Who else may use a project — read, clone, run, never change. `kind` labels
/// it: community (any org's), or shared / training (the platform orgs').
export type Exposure = { audience: 'none' | 'orgs' | 'public'; orgs?: string[]; kind?: 'community' | 'shared' | 'training' };

/// Publish into an exposed project, or clone something exposed into your own:
/// FlowR copies it (secrets stay server-side) if you may read the source and
/// change the target.
export const shareApi = {
  copy: (p: { kind: 'flow' | 'resource' | 'trigger'; from: string; name?: string; id?: string; to: string }) =>
    projectFlow('a3t-share', p),
};

/// A flow's releases: list them, and roll the active one back or forward.
export const releasesApi = {
  versions: (namespace: string, name: string) => projectFlow('a3t-releases', { action: 'versions', namespace, name }),
  activate: (namespace: string, name: string, version: string) => projectFlow('a3t-releases', { action: 'activate', namespace, name, version }),
};

export const profileApi = {
  get: () => projectFlow('a3t-profile', { action: 'get' }),
  claim: (handle: string, email?: string, name?: string) => projectFlow('a3t-profile', { action: 'claim', handle, email, name }),
};

export const orgsApi = {
  list: () => projectFlow('a3t-orgs', { action: 'list' }),
  /** The creator becomes the org's first admin; email/name label that row. */
  create: (org: string, display_name: string, creator?: { email?: string; name?: string }) =>
    projectFlow('a3t-orgs', { action: 'create', org, display_name, ...creator }),
  remove: (org: string) => projectFlow('a3t-orgs', { action: 'delete', org }),
  setMember: (org: string, user: { user_id: string; email?: string; name?: string }, role: OrgRole) =>
    projectFlow('a3t-orgs', { action: 'set_member', org, role, ...user }),
  removeMember: (org: string, user_id: string) => projectFlow('a3t-orgs', { action: 'remove_member', org, user_id }),
  setRuntime: (org: string, runtime: 'isolated' | 'shared') => projectFlow('a3t-orgs', { action: 'set_runtime', org, runtime }),
};

/// Everyone who can be added to an org or project: local users and, in EE, the
/// identity provider's. Ids are what the server knows a signed-in user as.
export type DirectoryUser = { id: string; email: string; name: string; source: 'local' | 'sso' };
export async function loadDirectory(): Promise<DirectoryUser[]> {
  const out: DirectoryUser[] = [];
  try {
    const local = await execFlow('a3t-users', 'a3t', { input: { action: 'list', org_id: 'org-default' } }, { silent: true });
    const rows = Array.isArray(local.data) ? local.data : (local.data?.rows ?? []);
    for (const u of rows) if (u?.id) out.push({ id: u.id, email: u.email ?? '', name: u.name ?? '', source: 'local' });
  } catch { /* not allowed, or no local users */ }
  try {
    const sso = await idp.users();
    for (const u of sso.ok ? (sso.data?.users ?? []) : []) {
      if (u?.id && !out.some((x) => x.email && x.email === u.email)) out.push({ id: u.id, email: u.email ?? u.username ?? '', name: u.name ?? u.username ?? '', source: 'sso' });
    }
  } catch { /* CE, or not an admin */ }
  return out;
}

// ── Executions (run history — persisted in the storage backend) ──
export const executions = {
  /** Recent runs, newest-first. Optional namespace/name filter + limit. */
  list: (opts: { limit?: number; namespace?: string; name?: string } = {}) =>
    mcpCall('list_executions', {
      ...(opts.limit ? { limit: opts.limit } : {}),
      ...(opts.namespace ? { namespace: opts.namespace } : {}),
      ...(opts.name ? { name: opts.name } : {}),
    }),
  /** Full status + input/output/error for one instance. */
  get: (instanceId: string) => mcpCall('get_execution_status', { instance_id: instanceId }),
};

/** Runs on the durable engine store: list, read, start, debug, re-run. */
export const runsApi = {
  list: (opts: { namespace?: string; namespaces?: string[]; name?: string; status?: string[]; limit?: number; cursor?: string } = {}) =>
    mcpCall('list_runs', opts),
  get: (runId: string) => mcpCall('get_run', { run_id: runId }),
  journal: (runId: string, afterSeq = 0) => mcpCall('get_run_journal', { run_id: runId, after_seq: afterSeq }),
  start: (args: { namespace: string; name: string; input?: any; debug?: boolean; breakpoints?: Breakpoint[]; pause_on_start?: boolean }) =>
    mcpCall('start_run', args),
  debug: (runId: string, command: DebugCommand, stepKey?: string) =>
    mcpCall('debug_run', { run_id: runId, command, ...(stepKey ? { step_key: stepKey } : {}) }),
  setBreakpoints: (runId: string, breakpoints: Breakpoint[]) => mcpCall('set_breakpoints', { run_id: runId, breakpoints }),
  editInput: (runId: string, stepKey: string, input: any) => mcpCall('edit_step_input', { run_id: runId, step_key: stepKey, input }),
  cancel: (runId: string, reason?: string) => mcpCall('cancel_run', { run_id: runId, ...(reason ? { reason } : {}) }),
  rerun: (runId: string, opts: { from_seq?: number; input?: any; debug?: boolean; breakpoints?: Breakpoint[] } = {}) =>
    mcpCall('rerun_run', { run_id: runId, ...opts }),
  /// A CloudEvent into a project: wakes the runs whose `listen` it completes.
  sendEvent: (namespace: string, event: any) => mcpCall('send_event', { namespace, event }),
};
export type Breakpoint = { step_key: string; when?: 'before' | 'after' | 'on_error'; condition?: string; hit?: number; enabled?: boolean };
export type DebugCommand = 'continue' | 'step_over' | 'step_into' | 'step_out' | 'run_to' | 'stop';

// ── Audit / decision log (EE — the Cedar allow/deny record) ──
export const audit = {
  /** Most-recent authorization decisions, newest-first. */
  recent: (limit = 100, denialsOnly = false) =>
    request(`/security/audit?limit=${limit}${denialsOnly ? '&denials_only=true' : ''}`, { method: 'GET' }),
};

export const state = {
  get: (key: string) => mcpCall('get_state', { key }),
  set: (key: string, value: any, ttlSecs?: number) =>
    mcpCall('set_state', { key, value, ...(ttlSecs ? { ttl_secs: ttlSecs } : {}) }),
};

// ── EE security policies (Cedar) ──
// Cedar policies are stored as `type: cedar` registry resources; the operate
// surface (list/reload/test) is served by flowr-server's /security/policies*.
export interface PolicyTestBody {
  principal: { kind: string; user_id?: string; roles?: string[]; agent_id?: string; service_id?: string };
  action: string;
  resource: { type: string; id: string; attributes?: Record<string, any> };
  attributes?: Record<string, any>;
  draft_policies?: string[];
  include_registry?: boolean;
}
/** Public edition probe — {edition:'ce'|'ee', enforcing, policy_hook, features}. */
export const getEdition = () => request('/edition', { method: 'GET' }, false);

export const securityPolicies = {
  /** List saved Cedar policy resources with their source. */
  list: () => request('/security/policies', { method: 'GET' }),
  /** Hot-reload the active policy set — no pod restart. Validate-before-swap. */
  reload: () => request('/security/policies/reload', { method: 'POST' }),
  /** Dry-run a decision against baseline (+registry, +draft). No mutation. */
  test: (body: PolicyTestBody) =>
    request('/security/policies/test', { method: 'POST', body: JSON.stringify(body) }),
  /** Create a new Cedar policy (registry resource, SoD-gated). */
  save: (namespace: string, name: string, cedar: string, description?: string) =>
    request('/registry/resources', {
      method: 'POST',
      body: JSON.stringify({ name, namespace, type: 'cedar', config: { cedar, description } }),
    }),
  /** Update an existing Cedar policy's source (SoD-gated; type keyed off stored resource). */
  update: (namespace: string, name: string, cedar: string, description?: string) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: JSON.stringify({ cedar, description }),
    }),
  /** Delete a Cedar policy (SoD-gated). */
  remove: (namespace: string, name: string) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, { method: 'DELETE' }),
};

// ── Runtime console (deployments, pause/resume, runtime state, metrics) ──
const enc = encodeURIComponent;
export const opsApi = {
  /** Every runtime with its deployments, live activity and host metrics. */
  overview: () => request('/ops/overview', { method: 'GET' }),
  probe: (id: string) => request(`/ops/deployments/${enc(id)}/probe`, { method: 'GET' }),
  /** pause | resume | restart | undeploy. `reason` is shown to whoever finds it paused. */
  deployment: (id: string, action: 'pause' | 'resume' | 'restart' | 'undeploy' | 'terminate', reason?: string) =>
    request(`/ops/deployments/${enc(id)}/${action}`, { method: 'POST', body: JSON.stringify(reason ? { reason } : {}) }),
  /** hibernate | pause | resume — admin only. */
  runtime: (name: string, action: 'hibernate' | 'pause' | 'resume') =>
    request(`/ops/runtimes/${enc(name)}/${action}`, { method: 'POST' }),
  /** What terminating a runtime would do (changes nothing). */
  terminatePlan: (name: string) => request(`/ops/runtimes/${enc(name)}/terminate`, { method: 'GET' }),
  /** Terminate: cancel its runs, remove it, reschedule what was deployed there. */
  terminate: (name: string) =>
    request(`/ops/runtimes/${enc(name)}/terminate`, { method: 'POST', body: JSON.stringify({ confirm: name }) }),
  pausedResponse: (name: string, status: 503 | 404) =>
    request(`/ops/runtimes/${enc(name)}/paused-response`, { method: 'PUT', body: JSON.stringify({ status }) }),
};

// ── Canvas / deployment (flow deployment metadata) ──
export const canvasApi = {
  /** List registered runtime pools (type_ref=runtime resources, typed). */
  runtimes: () => request('/canvas/runtimes', { method: 'GET' }),
  /** Dry-run: resolve a flow's effective canvas + the runtime it maps to. */
  placement: (ns: string, name: string) =>
    request(`/canvas/flows/${encodeURIComponent(ns)}/${encodeURIComponent(name)}/placement`, { method: 'GET' }),
  /** Deploy a flow (CE: logical placement; EE: controller orchestration). */
  deploy: (ns: string, name: string) =>
    request(`/canvas/flows/${encodeURIComponent(ns)}/${encodeURIComponent(name)}/deploy`, { method: 'POST' }),
  /** Deploy a specific canvas resource (an agent's bound canvas) by name. */
  deployResource: (ns: string, name: string) =>
    request(`/canvas/canvases/${encodeURIComponent(ns)}/${encodeURIComponent(name)}/deploy`, { method: 'POST' }),
  /** Live deployment status from the A3T controller (EE); empty in CE. */
  deployments: () => request('/canvas/deployments', { method: 'GET' }),
  /** The published CanvasSpec JSON Schema (single source for authoring/verification). */
  schema: () => request('/canvas/schema', { method: 'GET' }),
  /** Runtime pools are registry resources — reuse registry CRUD to manage them. */
  createRuntime: (namespace: string, name: string, cfg: { labels?: Record<string,string>; namespace?: string; url?: string; default?: boolean }) =>
    request('/registry/resources', { method: 'POST', body: JSON.stringify({ name, namespace, type: 'runtime', config: cfg }) }),

  // ── Canvases are type_ref=canvas registry resources (config = the canvas spec) ──
  /** List saved canvases. */
  list: () => request('/registry/resources?type=canvas', { method: 'GET' }),
  /** Create a canvas. */
  create: (namespace: string, name: string, spec: any) =>
    request('/registry/resources', { method: 'POST', body: JSON.stringify({ name, namespace, type: 'canvas', config: spec }) }),
  /** Update a canvas's spec. */
  update: (namespace: string, name: string, spec: any) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, { method: 'PUT', body: JSON.stringify(spec) }),
  /** Delete a canvas. */
  remove: (namespace: string, name: string) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, { method: 'DELETE' }),
  /** List flows + agents (palette for the designer). */
  flows: () => request('/registry/resources?type=workflow', { method: 'GET' }),
};

// ── EE IdP administration (Keycloak) ──
export interface IdpNewUser {
  username: string; email?: string; password: string; name?: string; temporary?: boolean; roles?: string[];
}
export interface IdpUpdateUser {
  name?: string; email?: string; enabled?: boolean; password?: string; temporary?: boolean; roles?: string[];
}
export const idp = {
  /** Is Keycloak admin configured + reachable? */
  status: () => request('/security/idp/status', { method: 'GET' }),
  /** List realm users. */
  users: () => request('/security/idp/users', { method: 'GET' }),
  /** One user's detail + roles (for edit). */
  getUser: (id: string) => request(`/security/idp/users/${encodeURIComponent(id)}`, { method: 'GET' }),
  /** Create a user in the IdP realm and assign roles. */
  createUser: (u: IdpNewUser) =>
    request('/security/idp/users', { method: 'POST', body: JSON.stringify(u) }),
  /** Update a user (partial: profile / password / roles). */
  updateUser: (id: string, u: IdpUpdateUser) =>
    request(`/security/idp/users/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(u) }),
  /** Delete a user. */
  deleteUser: (id: string) =>
    request(`/security/idp/users/${encodeURIComponent(id)}`, { method: 'DELETE' }),
};

// ── Global progress + notifications ──

import { messages } from '$lib/core/status/messages';

function progressStart() {
  if (typeof window !== 'undefined' && (window as any).__a3t_progress) {
    (window as any).__a3t_progress.start();
  }
}

function progressDone() {
  if (typeof window !== 'undefined' && (window as any).__a3t_progress) {
    (window as any).__a3t_progress.done();
  }
}

// ── Shared exec helper: execute_flow + poll for result ──

export async function execFlow(
  flowName: string,
  namespace: string,
  input: Record<string, any>,
  opts: { maxPolls?: number; pollInterval?: number; silent?: boolean } = {},
): Promise<{ ok: boolean; data?: any; error?: string }> {
  const maxPolls = opts.maxPolls ?? 20;
  const pollInterval = opts.pollInterval ?? 500;
  const silent = opts.silent ?? false;

  // Remap per-action flow names to consolidated CRUD flows
  // e.g. "a3t-templates-list" → flow "a3t-templates" with input.action = "list"
  const actionMatch = flowName.match(/^(a3t-(?:templates|agents|canvas|resources|users|audit))-(list|get|create|update|delete)$/);
  let actualFlowName = flowName;
  let actualInput = input;
  if (actionMatch) {
    actualFlowName = actionMatch[1];
    // Wrap input with action ONLY if not already wrapped
    if (!input?.input?.action) {
      actualInput = { input: { action: actionMatch[2], ...input } };
    }
  }

  progressStart();

  try {
    // Step 1: Execute
    const execRes = await mcpCall('execute_flow', { name: actualFlowName, namespace, input: actualInput });
    if (!execRes.ok) {
      progressDone();
      const err = execRes.error ?? `Failed to execute ${flowName}`;
      if (!silent) messages.error(err);
      return { ok: false, error: err };
    }

    // Step 2: Use the result if the server already has it.
    //
    // execute_flow waits for the flow and returns "Workflow execution completed
    // … Output:\n<json>". This used to be discarded: the client took only the
    // instance id, slept 500 ms, then asked again — paying a fixed half second
    // plus a second round trip on every page load, for an answer it already had.
    const text = typeof execRes.data === 'string' ? execRes.data : '';
    if (/Status:\s*completed/i.test(text)) {
      const out = text.split(/\nOutput:\n/)[1];
      if (out !== undefined) {
        try {
          const parsed = JSON.parse(out);
          // A null output used to yield the whole status object (`d.output ?? d`
          // below); keep that by letting it fall through to the poll.
          if (parsed !== null) {
            progressDone();
            return { ok: true, data: parsed };
          }
        } catch { /* unexpected shape — fall back to polling below */ }
      }
    }
    const match = text.match(/Instance ID:\s*`?([0-9a-f-]+)`?/);
    const instanceId = match?.[1];

    if (!instanceId) {
      progressDone();
      if (execRes.data?.rows || execRes.data?.output) {
        return { ok: true, data: execRes.data };
      }
      const err = 'No instance ID in response';
      if (!silent) messages.error(err);
      return { ok: false, error: err };
    }

    // Step 3: Still running (a long flow outlived the server's wait) — poll.
    // Check first, then wait: sleeping BEFORE the first check added a fixed
    // delay even when the answer was already there.
    for (let i = 0; i < maxPolls; i++) {
      if (i > 0) await new Promise(r => setTimeout(r, pollInterval));
      const statusRes = await mcpCall('get_execution_status', { instance_id: instanceId });
      if (!statusRes.ok) continue;
      const d = statusRes.data;
      if (d?.status === 'Completed' || d?.status === 'completed') {
        progressDone();
        return { ok: true, data: d.output ?? d };
      }
      if (d?.status === 'Failed' || d?.status === 'failed') {
        progressDone();
        const err = d.error ?? `${flowName} failed`;
        if (!silent) messages.error(err);
        return { ok: false, error: err };
      }
    }

    progressDone();
    console.log(`[execFlow] ${flowName} TIMED OUT after ${maxPolls} polls`);
    const err = `${flowName} timed out`;
    if (!silent) messages.error(err);
    return { ok: false, error: err };

  } catch (e) {
    progressDone();
    const err = String(e);
    if (!silent) messages.error(err);
    return { ok: false, error: err };
  }
}


// ── Live agent token stream (SSE over fetch) ──
//
// Uses fetch + ReadableStream rather than EventSource because EventSource cannot
// send an Authorization header, and putting the bearer token in the query string
// would leak it into access logs and browser history.
//
// `afterSeq` resumes an interrupted stream: the server replays anything newer
// before switching to live chunks, so a reconnect loses nothing.
export function streamChat(
  sessionId: string,
  handlers: {
    onToken?: (text: string) => void;
    onTool?: (name: string) => void;
    onDone?: (finalContent: string) => void;
    onSeq?: (seq: number) => void;
  },
  afterSeq = 0,
): { close: () => void } {
  const ctrl = new AbortController();

  (async () => {
    try {
      const headers: Record<string, string> = { Accept: 'text/event-stream' };
      if (accessToken) headers['Authorization'] = `Bearer ${accessToken}`;
      const res = await fetch(
        `${API_BASE}/chat/stream/${encodeURIComponent(sessionId)}?after=${afterSeq}`,
        { headers, signal: ctrl.signal },
      );
      if (!res.ok || !res.body) return;

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buf = '';
      let ev: { id?: string; event?: string; data: string[] } = { data: [] };

      const dispatch = () => {
        if (!ev.event && ev.data.length === 0) return;
        const data = ev.data.join('\n');
        if (ev.id) handlers.onSeq?.(Number(ev.id));
        if (ev.event === 'token') handlers.onToken?.(data);
        else if (ev.event === 'tool') handlers.onTool?.(data);
        else if (ev.event === 'done') handlers.onDone?.(data);
        ev = { data: [] };
      };

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        let nl: number;
        while ((nl = buf.indexOf('\n')) >= 0) {
          const line = buf.slice(0, nl).replace(/\r$/, '');
          buf = buf.slice(nl + 1);
          if (line === '') { dispatch(); continue; }   // blank line ends an event
          if (line.startsWith(':')) continue;          // keep-alive comment
          const c = line.indexOf(':');
          const field = c < 0 ? line : line.slice(0, c);
          const val = c < 0 ? '' : line.slice(c + 1).replace(/^ /, '');
          if (field === 'id') ev.id = val;
          else if (field === 'event') ev.event = val;
          else if (field === 'data') ev.data.push(val);
        }
      }
      dispatch();
    } catch {
      // Aborted or network drop — the caller falls back to polling + rehydration.
    }
  })();

  return { close: () => ctrl.abort() };
}

/// Server-side conversation history for a session (the agent persists it under
/// `agent:session:<id>` with a TTL). Used to rehydrate after a navigation so a
/// reply that landed while the page was unmounted is not lost.
// ── Chat sessions (server-owned, so history follows the user across devices) ──
//
// The transcripts have always been in LanceDB, but the *list* of conversations
// used to live only in one browser's localStorage, so signing in elsewhere showed
// an empty sidebar. These three read the owner-scoped index the agent maintains.

export type ChatSessionSummary = {
  session_id: string;      // full server id, "<agent>:<clientId>"
  title: string;
  message_count: number;
  updated_at: string;
};

/// The signed-in user's conversations, newest first. Never another user's: the
/// server reads only the caller's own index.
export async function fetchChatSessions(): Promise<ChatSessionSummary[]> {
  const res = await request<{ sessions: ChatSessionSummary[] }>('/chat/sessions');
  if (!res.ok || !Array.isArray(res.data?.sessions)) return [];
  // A designer's AI tab talks through the same /chat, but its exchanges are
  // about one edit in one editor — not conversations to list in AI Chat.
  return res.data!.sessions.filter((s) => !s.session_id.includes(':designer-'));
}

/// One transcript as renderable turns. 404 covers both "no such session" and
/// "not yours" — the server does not distinguish, on purpose.
export async function fetchChatSession(
  serverSessionId: string,
): Promise<{ role: string; content: string }[]> {
  const res = await request<{ messages: { role: string; content: string }[] }>(
    `/chat/sessions/${encodeURIComponent(serverSessionId)}`);
  if (!res.ok || !Array.isArray(res.data?.messages)) return [];
  return res.data!.messages;
}

/// Delete a conversation server-side. Sessions have no TTL, so this is the only
/// thing that removes one.
export async function deleteChatSession(serverSessionId: string): Promise<boolean> {
  const res = await request(`/chat/sessions/${encodeURIComponent(serverSessionId)}`,
    { method: 'DELETE' });
  return res.ok;
}

// ── Permissions answered in a conversation ──
//
// What the user allowed or declined when the agent asked. Session-scoped and
// server-owned, so they are the same on every device the conversation opens on.

export interface SessionPermission {
  key: string;
  action: string;
  resource_type: string;
  /// The tool, qualified by MCP server where there is one (`server/tool`).
  resource_id: string;
  verdict: 'granted' | 'denied';
  scope: 'once' | 'session' | 'agent';
  reason: string;
  decided_by: string;
  decided_at_ms: number;
  remaining_uses?: number | null;
}

export async function listSessionPermissions(sessionId: string): Promise<SessionPermission[]> {
  const res = await request<{ permissions: SessionPermission[] }>(
    `/chat/sessions/${encodeURIComponent(sessionId)}/permissions`);
  return res.ok && res.data?.permissions ? res.data.permissions : [];
}

/// Reopen a refusal. This does NOT grant it: the agent asks again next time it
/// needs the tool, so the permission is still given with the context of a real
/// request rather than by a stray click in a list.
export async function liftPermission(sessionId: string, key: string): Promise<boolean> {
  const res = await request<{ lifted: boolean }>(
    `/chat/sessions/${encodeURIComponent(sessionId)}/permissions/lift`,
    { method: 'POST', body: JSON.stringify({ key }) });
  return !!(res.ok && res.data?.lifted);
}

/// Approvals this conversation is currently blocked on.
///
/// The agent streams the question as a data part, but a stream frame is a
/// one-shot: miss it — a reload, a dropped connection, a client that started
/// listening a moment late — and the run stays suspended with nothing on screen
/// to answer. The pending list is server-held state, so it is the reliable
/// source; the stream frame just makes the card appear instantly when it works.
export interface PendingApproval {
  request_id: string;
  tool: string;
  server?: string | null;
  arguments?: any;
  reason?: string;
  scopes?: string[];
}

export async function listPendingApprovals(): Promise<PendingApproval[]> {
  const res = await request<{ inputs: any[] }>('/inputs');
  if (!res.ok || !Array.isArray(res.data?.inputs)) return [];
  return res.data.inputs
    // Deliberately NOT filtered by session. Matching the conversation key meant
    // agreeing on it across the route, the server-side transcript row and the
    // agent-prefixed id the runtime uses — and any mismatch silently hid the
    // card, leaving a suspended agent with no way to be answered. A pending
    // approval always belongs to a run this user started, and answering one they
    // are not the approver for is refused server-side, so showing every pending
    // question is both safe and far harder to get wrong.
    .filter((i) => i?.request_type === 'authz')
    .map((i) => ({
      request_id: i.id,
      tool: i.context?.tool ?? 'a tool',
      server: i.context?.server ?? null,
      arguments: i.context?.arguments,
      reason: i.context?.reason ?? i.message ?? '',
      scopes: Array.isArray(i.options) ? i.options : ['once', 'session'],
    }));
}

/// Answer a pending approval. The agent is suspended on this request id and
/// resumes as soon as the answer lands.
export async function answerApproval(
  requestId: string,
  allow: boolean,
  scope: 'once' | 'session',
): Promise<boolean> {
  const res = await request(`/inputs/${encodeURIComponent(requestId)}`, {
    method: 'POST',
    body: JSON.stringify({ allow, scope }),
  });
  return res.ok;
}

// ── Pins (per user, server-side so they follow you across devices) ──

export type PinMap = Record<string, string[]>;

export async function fetchPins(): Promise<PinMap> {
  const res = await request<{ pins: PinMap }>('/ui/pins');
  return res.ok && res.data?.pins ? res.data.pins : {};
}

/// Pin or unpin one item. Returns the whole updated set, so callers never merge.
export async function setPin(kind: string, id: string, pinned: boolean): Promise<PinMap> {
  const res = await request<{ pins: PinMap }>('/ui/pins', {
    method: 'POST', body: JSON.stringify({ kind, id, pinned }),
  });
  return res.ok && res.data?.pins ? res.data.pins : {};
}

export async function fetchAgentHistory(sessionId: string): Promise<{ role: string; content: string }[]> {
  const res = await mcpCall('get_state', { key: `agent:session:${sessionId}` });
  if (!res.ok) return [];
  let v: any = res.data;
  if (v && typeof v === 'object' && 'value' in v) v = (v as any).value;
  if (typeof v === 'string') { try { v = JSON.parse(v); } catch { return []; } }
  const msgs = v?.messages;
  if (!Array.isArray(msgs)) return [];
  // Only user/assistant turns with text are renderable; tool traffic stays internal.
  return msgs
    .filter((m: any) => (m?.role === 'user' || m?.role === 'assistant') && typeof m?.content === 'string' && m.content.trim())
    .map((m: any) => ({ role: m.role, content: m.content }));
}

// ── Webhook chat: POST to an agent's endpoint (trigger), then poll for the result ──
//
// The webhook is fire-and-forget — it returns a delivery ack with the run instance id
// embedded in targets[0] ("workflow:ns/name (INSTANCE_ID)"). We extract it and poll
// get_execution_status for the final output.
/// Post a payload to a webhook trigger's path (`/webhook/<path>`); the answer
/// lists what it started.
export function fireWebhook(path: string, body: unknown) {
  return request(`/webhook${path.startsWith('/') ? '' : '/'}${path}`, { method: 'POST', body: JSON.stringify(body ?? {}) });
}

export async function webhookChat(
  epPath: string,
  body: Record<string, any>,
  opts: { maxPolls?: number; pollInterval?: number } = {},
): Promise<{ ok: boolean; data?: any; error?: string }> {
  const maxPolls = opts.maxPolls ?? 90;
  const pollInterval = opts.pollInterval ?? 1000;
  progressStart();
  try {
    // Clean path form: /webhook/agent/{id}/chat (epPath begins with "/")
    const url = epPath.startsWith('/') ? `/webhook${epPath}` : `/webhook/${epPath}`;
    const post = await request(url, {
      method: 'POST',
      body: JSON.stringify(body),
    });
    if (!post.ok) { progressDone(); return { ok: false, error: post.error ?? `Webhook ${epPath} failed` }; }

    const targets: string[] = post.data?.targets ?? [];
    const m = (targets[0] || '').match(/\(([0-9a-f-]+)\)/);
    const instanceId = m?.[1];
    if (!instanceId) { progressDone(); return { ok: false, error: `No trigger/endpoint for ${epPath}` }; }

    for (let i = 0; i < maxPolls; i++) {
      await new Promise(r => setTimeout(r, pollInterval));
      const st = await mcpCall('get_execution_status', { instance_id: instanceId });
      if (!st.ok) continue;
      const d = st.data;
      if (d?.status === 'Completed' || d?.status === 'completed') { progressDone(); return { ok: true, data: d.output ?? d }; }
      if (d?.status === 'Failed' || d?.status === 'failed') { progressDone(); return { ok: false, error: d.error ?? 'agent run failed' }; }
    }
    progressDone();
    return { ok: false, error: 'agent run timed out' };
  } catch (e) {
    progressDone();
    return { ok: false, error: String(e) };
  }
}
