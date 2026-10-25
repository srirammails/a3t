// Tool discovery for the Permissions UIs (agent and template): which tools each
// selected MCP server exposes, asked of FlowR's list_mcp_tools (which queries
// remote servers with tools/list and serves its own tools in-process).

import { mcpCall } from '$lib/services/api';

export interface ServerTools {
  /** `ns/name` of the MCP server resource */
  server: string;
  /** Resource name, for headings */
  name: string;
  tools: string[];
  /** FlowR's own MCP surface (agent-flows and friends), served in-process */
  local: boolean;
  /** Why the tools couldn't be listed, if they couldn't */
  error?: string;
}

/** `ns/name` for a saved server reference (the UI used to store bare names). */
export function serverRef(s: string, ns = 'a3t'): string {
  const v = String(s).trim();
  return v.includes('/') ? v : `${ns}/${v}`;
}

async function listTools(namespace: string, name: string): Promise<{ ok: boolean; raw: any; error?: string }> {
  const res = await mcpCall('list_mcp_tools', { namespace, name });
  if (!res.ok) return { ok: false, raw: {}, error: res.error ?? 'list_mcp_tools failed' };
  let raw: any = res.data;
  if (typeof raw === 'string') { try { raw = JSON.parse(raw); } catch { raw = {}; } }
  return { ok: true, raw };
}

/**
 * Discover every server's tools in parallel, in the order given. A bare name is
 * looked up in `ns` (the agent's or template's namespace), then `a3t` (where A3T's
 * resources live), then `default` — the order FlowR's agent runtime resolves them in.
 */
export async function discoverServerTools(servers: string[], ns = 'a3t'): Promise<ServerTools[]> {
  return Promise.all(
    servers.map(async (s): Promise<ServerTools> => {
      const v = String(s).trim();
      const slash = v.indexOf('/');
      const name = slash >= 0 ? v.slice(slash + 1) : v;
      const namespaces = slash >= 0 ? [v.slice(0, slash)] : [...new Set([ns, 'a3t', 'default'])];
      let found = { ok: false, raw: {} as any, error: undefined as string | undefined };
      let namespace = namespaces[0];
      for (const n of namespaces) {
        namespace = n;
        found = await listTools(n, name);
        // Not this namespace: try the next. Anything else (tools, or a real error) is the answer.
        if (!(found.ok && /no MCP server resource/.test(found.raw?.error ?? ''))) break;
      }
      const ref = `${namespace}/${name}`;
      if (!found.ok) return { server: ref, name, tools: [], local: false, error: found.error };
      const raw = found.raw;
      const tools = [...new Set<string>((raw?.tools ?? []).map(String))].sort();
      const notFound = /no MCP server resource/.test(raw?.error ?? '');
      return {
        server: ref,
        name,
        tools,
        local: raw?.transport === 'in-process',
        error: notFound
          ? `no MCP server resource '${name}' in ${namespaces.join(', ')}`
          : raw?.error ?? (tools.length === 0 ? 'the server reported no tools' : undefined),
      };
    }),
  );
}

/** Every tool name across servers, de-duplicated and sorted. */
export function allToolNames(servers: ServerTools[]): string[] {
  return [...new Set(servers.flatMap(s => s.tools))].sort();
}

export const isDestructive = (t: string) => /delete|remove|drop|purge|destroy|clear|wipe|forget/i.test(t);

/**
 * Sections for one server's tools. FlowR's own tools are many, so they keep the
 * Read / Run / Author / Destructive split; any other server is one section.
 */
export function toolSections(s: ServerTools): { group: string; tools: string[] }[] {
  if (s.tools.length === 0) return [];   // shown as the server's error instead
  if (!s.local) return [{ group: s.name, tools: s.tools }];
  const groups: Record<string, string[]> = { Read: [], Run: [], Author: [], Destructive: [] };
  for (const t of s.tools) {
    if (isDestructive(t)) groups.Destructive.push(t);
    else if (/^(create|update|attach|detach|enable|pause|install|register)_/.test(t)) groups.Author.push(t);
    else if (/^(execute|send|set|run|call|trigger|publish|resume|cancel)_/.test(t)) groups.Run.push(t);
    else groups.Read.push(t);
  }
  return Object.entries(groups)
    .filter(([, tools]) => tools.length > 0)
    .map(([group, tools]) => ({ group: `${s.name} · ${group}`, tools }));
}
