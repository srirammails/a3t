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

/** Discover every server's tools in parallel, in the order given. */
export async function discoverServerTools(servers: string[], ns = 'a3t'): Promise<ServerTools[]> {
  return Promise.all(
    servers.map(async (s): Promise<ServerTools> => {
      const ref = serverRef(s, ns);
      const i = ref.indexOf('/');
      const [namespace, name] = [ref.slice(0, i), ref.slice(i + 1)];
      const res = await mcpCall('list_mcp_tools', { namespace, name });
      if (!res.ok) return { server: ref, name, tools: [], local: false, error: res.error ?? 'list_mcp_tools failed' };
      let raw: any = res.data;
      if (typeof raw === 'string') { try { raw = JSON.parse(raw); } catch { raw = {}; } }
      const tools = [...new Set<string>((raw?.tools ?? []).map(String))].sort();
      return {
        server: ref,
        name,
        tools,
        local: raw?.transport === 'in-process',
        error: raw?.error ?? (tools.length === 0 ? 'the server reported no tools' : undefined),
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
