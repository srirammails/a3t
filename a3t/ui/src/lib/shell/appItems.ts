// One implementation of "what are this app's items", shared by the context list
// and the app home page.
//
// These lived inside ContextList. The home page needs exactly the same rows, and
// a second copy would drift — which is the failure mode that already produced two
// disagreeing resource-category counts.

import { mcpCall, fetchChatSessions, opsApi, canvasApi } from '$lib/services/api';
import { RESOURCE_CATEGORIES, categorize } from '$lib/core/resources/categories';

export type AppItem = {
  /// What navigation addresses. For chat this is the *client* conversation id,
  /// which is what `/chat/<id>` uses.
  id: string;
  label: string;
  sub?: string;
  /// The full server key, kept for chat so delete hits the right transcript.
  serverId?: string;
  /// Single-letter badge + its colours, for the fixed resource categories.
  badge?: string;
  badgeStyle?: string;
  /// A conversation that exists only in this browser so far.
  pending?: boolean;
  /// Registry namespace. The list GROUPS by this, so it is kept as data rather
  /// than baked into `sub` — repeating it on every row under its own heading
  /// was most of the visual noise.
  namespace?: string;
  /// Short state shown as a coloured dot (execution status, agent status).
  status?: string;
  /// Tree position for hierarchical lists (host → runtime → deployment). Items
  /// arrive in depth-first order; a row's children are the following rows with a
  /// greater depth. Absent for flat lists.
  depth?: number;
};

/// The page merges these namespaces, so anything counting or listing across the
/// registry must use the same set — querying without one returns only `default`.
const NAMESPACES = ['default', 'a3t', 'system'];

/// Server session ids are "<agent>:<clientId>"; the agent half is often empty.
/// Never drop an entry over a missing prefix — that bug hid every conversation.
export function splitServerId(sid: string): { agent: string; id: string } {
  const i = sid.indexOf(':');
  if (i < 0) return { agent: '', id: sid };
  const id = sid.slice(i + 1);
  if (!id) return { agent: '', id: sid };
  return { agent: sid.slice(0, i), id };
}

export function relTime(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return '';
  const mins = Math.floor((Date.now() - t) / 60000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return days < 30 ? `${days}d ago` : new Date(t).toLocaleDateString();
}

/// Pull an array out of whatever shape a tool returns, without assuming one.
function rows(data: any, ...keys: string[]): any[] {
  if (Array.isArray(data)) return data;
  for (const k of keys) if (Array.isArray(data?.[k])) return data[k];
  for (const v of Object.values(data ?? {})) if (Array.isArray(v)) return v as any[];
  return [];
}

const named = (r: any): AppItem => ({
  id: r.name ?? r.id ?? String(r),
  label: r.name ?? r.id ?? String(r),
  namespace: r.namespace || undefined,
  // Namespace is the group heading now, so the subtitle carries what differs
  // between siblings: version, then type.
  sub: [r.version ? `v${r.version}` : '', r.type_ref ?? r.type ?? ''].filter(Boolean).join(' · '),
  status: r.status || undefined,
});

/// Load one app's items. Throws with a readable message so callers can surface a
/// failure rather than rendering an empty list that looks like "nothing here".
/// `document.namespace` from a CNCF workflow's YAML, without a YAML parser: the
/// first `namespace:` inside the top-level `document:` block.
function specNamespace(yaml: unknown): string | undefined {
  if (typeof yaml !== 'string') return undefined;
  const doc = yaml.split(/\n(?=\S)/).find((block) => block.startsWith('document:'));
  return doc?.match(/^\s+namespace:\s*['"]?([\w.-]+)/m)?.[1];
}

/// Group items by namespace for display, keeping each group's original order.
///
/// Namespaces in `PREFERRED` come first in that order (the platform's own
/// namespace before user ones), the rest alphabetically; items without a
/// namespace collect under `''`, rendered last. Returns `null` when nothing in
/// the list has a namespace — grouping a flat list would only add a heading.
const PREFERRED = ['a3t', 'default', 'system'];
export function groupByNamespace(items: AppItem[]): { ns: string; items: AppItem[] }[] | null {
  if (!items.some((i) => i.namespace)) return null;
  const map = new Map<string, AppItem[]>();
  for (const it of items) {
    const k = it.namespace ?? '';
    (map.get(k) ?? map.set(k, []).get(k)!).push(it);
  }
  const rank = (ns: string) => {
    if (!ns) return [2, ''];
    const i = PREFERRED.indexOf(ns);
    return i >= 0 ? [0, String(i)] : [1, ns];
  };
  return [...map.entries()]
    .sort(([a], [b]) => {
      const [ra, ka] = rank(a), [rb, kb] = rank(b);
      return ra !== rb ? (ra as number) - (rb as number) : (ka as string).localeCompare(kb as string);
    })
    .map(([ns, items]) => ({ ns, items }));
}

export async function loadAppItems(source: string): Promise<AppItem[]> {
  if (source === 'sessions') {
    const remote = await fetchChatSessions();
    return remote.map((s) => ({
      id: splitServerId(s.session_id).id,
      label: s.title || 'Conversation',
      sub: `${relTime(s.updated_at)}${s.message_count ? ` · ${s.message_count} msgs` : ''}`,
      serverId: s.session_id,
    }));
  }

  if (source === 'flows') {
    const out: AppItem[] = [];
    for (const ns of NAMESPACES) {
      const res = await mcpCall('list_flows', { namespace: ns });
      if (!res.ok) continue;
      for (const r of rows(res.data, 'workflows', 'flows')) {
        out.push({
          // "~" not "/": the id becomes a single URL path segment.
          id: `${r.namespace ?? ns}~${r.name}`,
          label: r.name,
          namespace: r.namespace ?? ns,
          sub: [r.version ? `v${r.version}` : '',
                r.step_count != null ? `${r.step_count} step${r.step_count === 1 ? '' : 's'}` : '']
            .filter(Boolean).join(' · '),
          status: r.enabled === false ? 'disabled' : undefined,
        });
      }
    }
    return out;
  }

  if (source === 'resource-categories') {
    // Fixed categories, always all shown — a zero is information. Counts come from
    // the same two sources the Resources page merges.
    const counts: Record<string, number> = {};
    for (const ns of NAMESPACES) {
      const res = await mcpCall('list_resources', { namespace: ns });
      if (!res.ok) continue;
      for (const r of rows(res.data, 'resources')) {
        const c = categorize(r.type ?? '');
        counts[c] = (counts[c] ?? 0) + 1;
      }
    }
    let flowCount = 0;
    for (const ns of NAMESPACES) {
      const fl = await mcpCall('list_flows', { namespace: ns });
      if (fl.ok) flowCount += rows(fl.data, 'workflows', 'flows').length;
    }
    counts.flows = flowCount;
    counts.all = Object.entries(counts).filter(([k]) => k !== 'all')
      .reduce((n, [, v]) => n + v, 0);
    return RESOURCE_CATEGORIES.map((c) => ({
      id: c.key, label: c.label, sub: `${counts[c.key] ?? 0}`,
      badge: c.icon, badgeStyle: c.iconStyle,
    }));
  }

  if (source === 'agents') {
    const res = await mcpCall('list_agents', {});
    if (!res.ok) throw new Error(res.error ?? 'list_agents failed');
    return rows(res.data, 'agents').map(named);
  }
  if (source === 'templates') {
    const res = await mcpCall('list_agent_templates', {});
    if (!res.ok) throw new Error(res.error ?? 'list_agent_templates failed');
    // Template rows have no namespace column — it lives in the definition
    // (`document.namespace` in spec_yaml). Read it from there so templates group
    // like everything else, and show where the template came from.
    return rows(res.data, 'templates').map((r: any) => ({
      ...named(r),
      namespace: r.namespace || specNamespace(r.spec_yaml),
      sub: [r.version ? `v${r.version}` : '', r.source === 'system' ? 'built-in' : r.source === 'my' ? 'yours' : r.source ?? '']
        .filter(Boolean).join(' · '),
    }));
  }
  if (source === 'canvases') {
    const res = await canvasApi.list();
    if (!res.ok) throw new Error(res.error || 'Could not load canvases');
    return rows(res.data, 'resources').map((r: any) => {
      const c = r.config ?? {};
      return {
        id: r.name, label: r.name, namespace: r.namespace || undefined,
        sub: c.flowRef ? `deploys ${c.flowRef}` : (c.shared === false ? 'your copy · unbound' : 'preset'),
        status: c.flowRef ? 'active' : undefined,
      };
    });
  }
  if (source === 'runtimes') {
    return runtimeTree();
  }
  if (source === 'triggers') {
    const res = await mcpCall('list_triggers', {});
    if (!res.ok) throw new Error(res.error ?? 'list_triggers failed');
    return rows(res.data, 'triggers').map((r: any) => ({
      id: r.id ?? r.name, label: r.name ?? r.id, sub: r.config?.type ?? r.type ?? '',
      namespace: r.namespace || undefined, status: r.status || undefined,
    }));
  }
  if (source === 'executions') {
    const res = await mcpCall('list_executions', { limit: 100 });
    if (!res.ok) throw new Error(res.error ?? 'list_executions failed');
    return rows(res.data, 'executions').map((r: any) => ({
      id: r.instance_id, label: r.name ?? r.instance_id,
      namespace: r.namespace || undefined,
      sub: [r.status, r.started_at ? relTime(r.started_at) : ''].filter(Boolean).join(' · '),
      status: r.status ? String(r.status).toLowerCase() : undefined,
    }));
  }
  return [];
}

/// The Runtime console's drill-down: host → runtime → deployed agents and flows.
///
/// Ids are typed (`host:` / `rt:` / `dep:`) because the page shows a different
/// view for each level. In CE every runtime lives in the one flowr pod, so there is
/// a single host; with the EE controller each runtime gets its own pods, and the
/// same tree will show them under their own hosts.
const DEP_STATUS: Record<string, string> = {
  serving: 'active', paused: 'paused', 'runtime-paused': 'paused', hibernated: 'hibernated', unplaced: 'degraded',
};
const BADGE = {
  host: 'background:#e8eef6;color:#3a5a85',
  runtime: 'background:var(--runtime-bg,#e6f4f1);color:var(--runtime,#0f766e)',
  flow: 'background:var(--spec-bg,#ede7ff);color:var(--spec,#5530a8)',
  agent: 'background:var(--ephemeral-bg,#fff4e5);color:var(--ephemeral,#b45309)',
};
async function runtimeTree(): Promise<AppItem[]> {
  const res = await opsApi.overview();
  if (!res.ok) throw new Error(res.error || 'Could not load runtimes');
  const d = res.data;
  const host = d.host?.host || 'flowr';
  const out: AppItem[] = [{
    id: `host:${host}`, label: host, badge: 'H', badgeStyle: BADGE.host, depth: 0,
    sub: `shared host · ${(d.runtimes ?? []).length} runtimes`,
  }];
  const dep = (row: any, depth: number): AppItem => ({
    id: `dep:${row.deployment.id}`,
    label: row.deployment.target,
    badge: row.deployment.kind === 'agent' ? 'A' : 'F',
    badgeStyle: row.deployment.kind === 'agent' ? BADGE.agent : BADGE.flow,
    sub: `${row.deployment.kind} · ${row.serving.replace('-', ' ')}`,
    status: DEP_STATUS[row.serving],
    depth,
  });
  for (const r of d.runtimes ?? []) {
    const labels = Object.entries(r.runtime.labels ?? {}).map(([k, v]) => `${k}=${v}`).join(' ');
    out.push({
      id: `rt:${r.runtime.name}`, label: r.runtime.name, badge: 'R', badgeStyle: BADGE.runtime, depth: 1,
      sub: [`${r.deployments.length} deployed`, labels].filter(Boolean).join(' · '),
      status: r.state,
    });
    for (const row of r.deployments) out.push(dep(row, 2));
  }
  if ((d.unplaced ?? []).length) {
    out.push({ id: 'rt:__unplaced', label: 'Unplaced', badge: '?', badgeStyle: BADGE.host, depth: 1, sub: 'runtime was removed', status: 'degraded' });
    for (const row of d.unplaced) out.push(dep(row, 2));
  }
  return out;
}
