// One implementation of "what are this app's items", shared by the context list
// and the app home page.
//
// These lived inside ContextList. The home page needs exactly the same rows, and
// a second copy would drift — which is the failure mode that already produced two
// disagreeing resource-category counts.

import { mcpCall, fetchChatSessions } from '$lib/services/api';
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
  sub: r.namespace ? `${r.namespace}${r.version ? ` · v${r.version}` : ''}` : (r.type_ref ?? r.type ?? ''),
});

/// Load one app's items. Throws with a readable message so callers can surface a
/// failure rather than rendering an empty list that looks like "nothing here".
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
          sub: `${r.namespace ?? ns}${r.version ? ` · v${r.version}` : ''}${
            r.step_count != null ? ` · ${r.step_count} steps` : ''}`,
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
    return rows(res.data, 'templates').map(named);
  }
  if (source === 'triggers') {
    const res = await mcpCall('list_triggers', {});
    if (!res.ok) throw new Error(res.error ?? 'list_triggers failed');
    return rows(res.data, 'triggers').map((r: any) => ({
      id: r.id ?? r.name, label: r.name ?? r.id, sub: r.config?.type ?? r.type ?? '',
    }));
  }
  if (source === 'executions') {
    const res = await mcpCall('list_executions', { limit: 100 });
    if (!res.ok) throw new Error(res.error ?? 'list_executions failed');
    return rows(res.data, 'executions').map((r: any) => ({
      id: r.instance_id, label: `${r.namespace}/${r.name}`,
      sub: `${r.status}${r.started_at ? ` · ${relTime(r.started_at)}` : ''}`,
    }));
  }
  return [];
}
