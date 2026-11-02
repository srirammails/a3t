/// Agents and their templates (AGENT_TEMPLATE_SPEC §2). A template is a
/// workflow whose agent step (`call: a3t:agent`) carries a `config`; for each
/// config field the template says what an agent may do with it:
///
///   required  the template's value is a placeholder (for testing the template);
///             every agent must set its own
///   optional  the template's value is the default; an agent may override it
///   locked    the template's value is final; agents see it read-only
///   hidden    the template's value is final; agents do not see it
///
/// Kept in the template's workflow metadata: `document.metadata.a3t.fields`
/// (`{ <config key>: <visibility> }`). A field without one is `optional` — what
/// agents could change before visibility existed.
///
/// An agent stores only its own values: `resource_bindings.llm` for the LLM,
/// `variables` for the rest, plus where it is served (`target`, `ep_path`).
import * as YAML from 'yaml';

export type Visibility = 'required' | 'optional' | 'locked' | 'hidden';
export const VISIBILITIES: Visibility[] = ['required', 'optional', 'locked', 'hidden'];

/// The engine's settings an agent may have even when a template's config does
/// not list them (the native engine reads them all).
export const ENGINE_FIELDS = ['llm_resource', 'system_prompt', 'max_iterations', 'temperature', 'mcp_servers', 'enabled_tools', 'memory_resource', 'memory_namespace', 'memory_config'];

/// The agent step's config: a literal object, or a `${ {…} }` expression
/// whose body is JSON (as the shipped templates write it).
export function templateConfig(specYaml: string): Record<string, unknown> {
  let doc: any;
  try { doc = YAML.parse(specYaml) ?? {}; } catch { return {}; }
  for (const step of Array.isArray(doc.do) ? doc.do : []) {
    const task: any = Object.values(step ?? {})[0];
    if (!task?.call || !String(task.call).includes('a3t:agent')) continue;
    const cfg = task.with?.config;
    if (cfg && typeof cfg === 'object') return cfg;
    if (typeof cfg === 'string') {
      const body = cfg.trim().match(/^\$\{([\s\S]*)\}$/)?.[1]?.trim();
      if (body) { try { return JSON.parse(body); } catch { /* an expression, not data */ } }
    }
    return {};
  }
  return {};
}

export function visibilities(specYaml: string): Record<string, Visibility> {
  try {
    const f = YAML.parse(specYaml)?.document?.metadata?.a3t?.fields;
    return f && typeof f === 'object' ? f : {};
  } catch { return {}; }
}

/// The template with one field's visibility set (layout and comments kept).
export function withVisibility(specYaml: string, key: string, v: Visibility): string {
  const d = YAML.parseDocument(specYaml);
  if (v === 'optional') d.deleteIn(['document', 'metadata', 'a3t', 'fields', key]);
  else d.setIn(['document', 'metadata', 'a3t', 'fields', key], v);
  return d.toString();
}

/// The fields of a template: its config's, in order, then the rest of the
/// engine's settings (the engine reads them all, whatever the config lists —
/// and a config worked out at run time lists none).
export function templateFields(specYaml: string): { key: string; visibility: Visibility; value: unknown }[] {
  const cfg = templateConfig(specYaml);
  const vis = visibilities(specYaml);
  const keys = [...new Set([...Object.keys(cfg), ...ENGINE_FIELDS])];
  return keys.map((key) => ({ key, visibility: vis[key] ?? 'optional', value: cfg[key] }));
}

const parse = (v: unknown): Record<string, any> => {
  if (v && typeof v === 'object') return v as Record<string, any>;
  try { return JSON.parse(String(v ?? '{}')) ?? {}; } catch { return {}; }
};

/// What an agent sets itself, by config key (`llm_resource` from its bindings).
export function agentValues(agent: any): Record<string, unknown> {
  const vars = parse(agent?.variables);
  const rb = parse(agent?.resource_bindings);
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(vars)) if (!['target', 'ep_path', 'system_default'].includes(k)) out[k] = v;
  if (rb.llm) out.llm_resource = rb.llm;
  return out;
}

export type AgentField = { key: string; visibility: Visibility; template: unknown; own: unknown; set: boolean };

/// The agent's fields as its template exposes them (hidden ones left out).
export function agentFields(specYaml: string, agent: any): AgentField[] {
  const own = agentValues(agent);
  const fields = templateFields(specYaml);
  // Values the agent has for fields the template does not list stay visible (optional).
  for (const k of Object.keys(own)) if (!fields.some((f) => f.key === k)) fields.push({ key: k, visibility: 'optional', value: undefined });
  return fields
    .filter((f) => f.visibility !== 'hidden')
    .map((f) => ({ key: f.key, visibility: f.visibility, template: f.value, own: own[f.key], set: f.key in own }));
}

/// What a save writes: only the agent's own values for fields it may set; a
/// locked or hidden field never carries an agent value.
export function toRecord(agent: any, specYaml: string, own: Record<string, unknown>, serving: { target?: string; ep_path?: string }) {
  const vis = visibilities(specYaml);
  const vars = parse(agent?.variables);
  const rb = parse(agent?.resource_bindings);
  for (const k of Object.keys(vars)) if (!['target', 'ep_path', 'system_default'].includes(k)) delete vars[k];
  delete rb.llm;
  for (const [k, v] of Object.entries(own)) {
    const visibility = vis[k] ?? 'optional';
    if (visibility === 'locked' || visibility === 'hidden' || v === undefined) continue;
    if (k === 'llm_resource') rb.llm = v;
    else vars[k] = v;
  }
  if (serving.target !== undefined) vars.target = serving.target;
  if (serving.ep_path !== undefined) vars.ep_path = serving.ep_path;
  return { resource_bindings: JSON.stringify(rb), variables: JSON.stringify(vars) };
}

/// The chat flow an agent of this template is served by: the native engine's
/// wrapper for engine templates, the SWF wrapper otherwise (never the
/// template itself, which is not deployed as a flow).
export function chatFlowFor(specYaml: string): string {
  let name = '';
  try { name = YAML.parse(specYaml)?.document?.name ?? ''; } catch { /* keep '' */ }
  if (name === 'agent-engine' || /a3t:agent/.test(specYaml)) return 'a3t/a3t-chat-engine';
  return 'a3t/a3t-chat-agent';
}

/// Required fields the agent has not set.
export function missing(specYaml: string, own: Record<string, unknown>): string[] {
  return templateFields(specYaml).filter((f) => f.visibility === 'required' && (own[f.key] === undefined || own[f.key] === '')).map((f) => f.key);
}
