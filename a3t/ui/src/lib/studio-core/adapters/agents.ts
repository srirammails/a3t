/// Saving agents and agent templates (their Console pages' flows).
import { execFlow, mcpCall, webhookChat } from '$lib/services/api';
import { fail, ok, type Result } from '../items';
import { result, safely } from './util';

const stamp = () => ({ org_id: 'org-default', user_id: 'usr-superadmin', timestamp: new Date().toISOString() });

export const updateAgent = (agent: any, change: Record<string, unknown>): Promise<Result> =>
  safely('save agent', async () => {
    const r = result(await execFlow('a3t-agents-update', 'a3t', {
      id: agent.id, ...stamp(),
      data: {
        name: agent.name, template_id: agent.template_id, template_version: agent.template_version,
        mode: agent.mode, namespace: agent.namespace, status: agent.status,
        resource_bindings: agent.resource_bindings, variables: agent.variables,
        ...change,
      },
    }), 'Save agent');
    return r.ok ? ok(undefined) : r;
  });

export const updateTemplate = (t: any, spec_yaml: string): Promise<Result> =>
  safely('save template', async () => {
    const r = result(await execFlow('a3t-templates-update', 'a3t', {
      id: t.id, ...stamp(),
      data: {
        name: t.name, version: t.version, source: t.source === 'platform' ? 'system' : t.source, spec_yaml,
        tags: Array.isArray(t.tags) ? t.tags.join(',') : (t.tags ?? ''), vertical: t.vertical ?? '',
      },
    }), 'Save template');
    return r.ok ? ok(undefined) : r;
  });

/// Create an agent and its chat endpoint (a webhook trigger → its chat flow).
export const createAgent = (a: { id: string; name: string; namespace: string; template_id: string; template_version: string; mode: string; resource_bindings: string; variables: string }, target: string, ep_path: string): Promise<Result> =>
  safely('create agent', async () => {
    const r = result(await execFlow('a3t-agents-create', 'a3t', { ...stamp(), data: a }), 'Create agent');
    if (!r.ok) return r;
    const [tns, ...tname] = target.includes('/') ? target.split('/') : ['a3t', target];
    const t = result(await mcpCall('create_trigger', {
      name: `agent-${a.id}-chat`, namespace: a.namespace, config: { type: 'webhook', path: ep_path },
      targets: [{ type: 'workflow', namespace: tns, name: tname.join('/') }], enabled: true,
    }), 'Create its endpoint');
    return t.ok ? ok(undefined) : fail(`Agent created, but its endpoint was not: ${t.error}`);
  });

/// Delete an agent and its chat endpoint.
export const deleteAgent = (id: string): Promise<Result> =>
  safely('delete agent', async () => {
    const r = result(await execFlow('a3t-agents-delete', 'a3t', { id, org_id: 'org-default' }), 'Delete agent');
    if (!r.ok) return r;
    const tr = await mcpCall('list_triggers', {});
    const list = Array.isArray(tr.data) ? tr.data : tr.data?.triggers ?? [];
    const trig = list.find((t: any) => t?.config?.path === `/agent/${id}/chat`);
    if (trig?.id) await mcpCall('delete_trigger', { id: trig.id });
    return ok(undefined);
  });

/// One message to an agent through its endpoint; its answer's text.
export const chat = (epPath: string, body: Record<string, unknown>): Promise<Result<string>> =>
  safely('chat', async () => {
    const r = await webhookChat(epPath, body, { maxPolls: 90, pollInterval: 1000 });
    if (!r.ok) return fail(r.error ?? 'The agent did not answer');
    const d = r.data;
    const raw = d?.content ?? d?.output?.content ?? d?.output ?? d?.agent_output ?? d;
    const text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
    if (text.trim()) return ok(text);
    return ok(d?.stopped_reason === 'max_iterations' ? `(No answer — it stopped at its step limit after ${d?.tool_calls_made ?? 0} tool calls.)` : '(No answer.)');
  });
