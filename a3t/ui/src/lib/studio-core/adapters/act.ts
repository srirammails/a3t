/// Acting on resources: chat with an LLM, list and call an MCP server's tools,
/// fire a trigger. One implementation each; Console pages may use them too.
import { execFlow, fireWebhook, runsApi } from '$lib/services/api';
import { fail, ok, type Result } from '../items';
import { result, rows, safely } from './util';

export type ChatMessage = { role: 'user' | 'assistant'; content: string };

/// Through the platform's resource test flow: the new message as `message`, the
/// conversation before it as `messages` (the `llm` call puts the prompt after
/// them; used once the flow passes them on).
export const chat = (project: string, name: string, messages: ChatMessage[]): Promise<Result<string>> =>
  safely('chat', async () => {
    const last = messages[messages.length - 1]?.content ?? '';
    const r = result<any>(await execFlow('a3t-test-resource', 'a3t', {
      input: { test_type: 'llm', resource: `${project}/${name}`, message: last, messages: messages.slice(0, -1) },
    }), 'Chat');
    if (!r.ok) return r;
    const d = r.data;
    return ok(typeof d === 'string' ? d : d?.content ?? d?.text ?? d?.response ?? JSON.stringify(d, null, 2));
  });

export type Tool = { name: string; description: string; inputSchema?: any };

export const listTools = (project: string, name: string): Promise<Result<Tool[]>> =>
  safely('list tools', async () => {
    const r = result<any>(await execFlow('a3t-test-resource', 'a3t', { input: { test_type: 'mcp', resource: `${project}/${name}` } }), 'List tools');
    if (!r.ok) return r;
    // `{ tools: [...] }`, or the list itself; anything else is the server's message.
    const list = rows(r.data, 'tools');
    if (!list.length && r.data && !Array.isArray(r.data) && !Array.isArray(r.data?.tools))
      return fail(typeof r.data === 'string' ? r.data : JSON.stringify(r.data));
    return ok(list.map((t: any) => ({ name: String(t.name ?? ''), description: String(t.description ?? ''), inputSchema: t.inputSchema ?? t.input_schema })));
  });

/// Calling a tool runs it for real: it may change things on the server it belongs to.
export const callTool = (project: string, name: string, tool: string, args: unknown): Promise<Result<unknown>> =>
  safely('call tool', async () => {
    const r = result<any>(await execFlow('a3t-test-resource', 'a3t', {
      input: { test_type: 'mcp_call', resource: `${project}/${name}`, tool, arguments: args ?? {} },
    }), `Call ${tool}`);
    return r;
  });

export type Fired = { how: string; detail: unknown };

/// Fire a trigger with a payload, as its type would: a webhook posts to its
/// path; an event trigger gets a CloudEvent of its `source` type in its
/// project; any other (cron…) starts its workflow targets with the payload.
export const fire = (t: { namespace: string; config: any; targets: any[] }, payload: unknown): Promise<Result<Fired>> =>
  safely('fire', async () => {
    const type = t.config?.type;
    if (type === 'webhook') {
      if (!t.config.path) return fail('This webhook trigger has no path');
      const r = result<any>(await fireWebhook(t.config.path, payload), 'Webhook');
      return r.ok ? ok({ how: `POST /webhook${t.config.path}`, detail: r.data }) : r;
    }
    if (type === 'event') {
      if (!t.config.source) return fail('This event trigger has no source');
      const r = result<any>(await runsApi.sendEvent(t.namespace, { type: t.config.source, data: payload }), 'Send event');
      return r.ok ? ok({ how: `event ${t.config.source} in ${t.namespace}`, detail: r.data }) : r;
    }
    const flows = (t.targets ?? []).filter((x: any) => x.type === 'workflow' && x.name);
    if (!flows.length) return fail('This trigger has no workflow to start');
    const started = [];
    for (const f of flows) {
      const r = result<any>(await runsApi.start({ namespace: f.namespace ?? t.namespace, name: f.name, input: payload }), `Start ${f.name}`);
      if (!r.ok) return r;
      started.push({ flow: `${f.namespace ?? t.namespace}/${f.name}`, run: r.data });
    }
    return ok({ how: `started ${flows.length} flow${flows.length > 1 ? 's' : ''}`, detail: started });
  });
