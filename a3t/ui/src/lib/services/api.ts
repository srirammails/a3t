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
      error: res.ok ? undefined : (data?.error || `HTTP ${res.status}`),
      status: res.status,
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
  /** Delete a resource (registry REST DELETE — the MCP delete_resource tool is Cedar-gated in EE). */
  remove: (namespace: string, name: string) =>
    request(`/registry/resources/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),
};

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

    // Step 2: Extract instance ID
    const text = typeof execRes.data === 'string' ? execRes.data : '';
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

    // Step 3: Poll for completion
    for (let i = 0; i < maxPolls; i++) {
      await new Promise(r => setTimeout(r, pollInterval));
      const statusRes = await mcpCall('get_execution_status', { instance_id: instanceId });
      console.log(`[execFlow] poll ${i}/${maxPolls} for ${instanceId}:`, statusRes.ok, typeof statusRes.data, statusRes.data?.status);
      if (!statusRes.ok) continue;
      const d = statusRes.data;
      if (d?.status === 'Completed' || d?.status === 'completed') {
        console.log(`[execFlow] ${flowName} completed, output type:`, typeof d.output, Array.isArray(d.output) ? `array(${d.output.length})` : d.output);
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
  return res.data!.sessions;
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
