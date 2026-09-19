// One table describing every artifact kind the chat can surface.
//
// Adding a kind means adding a row here and a row in the server's widget table —
// not another component. `get`/`save` name the MCP tools, so every mutation keeps
// going through the Cedar-gated tool path with the user's own token.
export type ArtifactKind = 'flow' | 'flow_draft' | 'agent' | 'template' | 'resource' | 'trigger' | 'canvas';

export interface ArtifactSpec {
  label: string;
  /// Tool that fetches one item.
  get: string;
  /// Tool that writes one item, or null where the platform exposes no writer.
  save: string | null;
  /// Which field of a list row identifies the item.
  idField: 'name' | 'id';
  /// Argument name(s) the get/save tools expect it under. Several where a
  /// tool's schema and implementation disagree (get_agent declares `agent_id`
  /// but reads `name`), so we send both rather than pick a side.
  idArgs: string[];
  /// Flows carry YAML; everything else is JSON.
  format: 'yaml' | 'json';
  /// A draft exists only in the browser until saved — nothing to re-read, and
  /// no stale-write check to run.
  draft?: boolean;
}

export const ARTIFACTS: Record<ArtifactKind, ArtifactSpec> = {
  // Proposed by the agent, not in the registry. Saving is the moment it becomes
  // real, and that save is the Cedar-gated `create_flow` like any other write.
  flow_draft: { label: 'draft flow', get: 'get_flow', save: 'create_flow', idField: 'name', idArgs: ['name'], format: 'yaml', draft: true },
  flow:     { label: 'flow',     get: 'get_flow',           save: 'create_flow',           idField: 'name', idArgs: ['name'],             format: 'yaml' },
  agent:    { label: 'agent',    get: 'get_agent',          save: 'create_agent',          idField: 'name', idArgs: ['name', 'agent_id'], format: 'json' },
  template: { label: 'template', get: 'get_agent_template', save: 'create_agent_template', idField: 'name', idArgs: ['name'],             format: 'yaml' },
  resource: { label: 'resource', get: 'get_resource',       save: 'create_resource',       idField: 'name', idArgs: ['name'],             format: 'json' },
  // get_trigger takes the trigger UUID, not its name.
  trigger:  { label: 'trigger',  get: 'get_trigger',        save: 'create_trigger',        idField: 'id',   idArgs: ['id'],               format: 'json' },
  // A flow's deployment metadata. Saving goes through update_canvas, which
  // refuses shared presets (clone those instead) and validates the spec.
  canvas:   { label: 'canvas',   get: 'get_canvas',         save: 'update_canvas',         idField: 'name', idArgs: ['name'],             format: 'json' },
};

/// Build the get/save arguments for one item.
export function idArgsFor(spec: ArtifactSpec, item: { namespace?: string; name?: string; id?: string }) {
  const value = spec.idField === 'id' ? (item.id ?? item.name ?? '') : (item.name ?? '');
  const args: Record<string, string> = {};
  if (item.namespace) args.namespace = item.namespace;
  for (const k of spec.idArgs) args[k] = value;
  return args;
}

/// Plural widget kind (`data-flows`) → singular artifact kind (`flow`).
export const LIST_KIND_TO_ITEM: Record<string, ArtifactKind> = {
  flows: 'flow', agents: 'agent', templates: 'template',
  resources: 'resource', triggers: 'trigger',
  // Runtime pools are registry resources.
  runtimes: 'resource',
};

/// `get_flow` answers in markdown; everything else in JSON. Normalise both to
/// the text the editor should show.
export function toEditorText(raw: unknown, format: 'yaml' | 'json'): string {
  // Registry rows carry the definition in `spec_yaml` alongside their metadata
  // (id, version, tags, …). The designer wants the YAML, not the row — handing it
  // the row is what left agent templates with no graph, code or form view.
  if (format === 'yaml' && raw && typeof raw === 'object' && !Array.isArray(raw)) {
    const spec = (raw as Record<string, unknown>).spec_yaml;
    if (typeof spec === 'string' && spec.trim()) return spec.trim();
  }
  let text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
  if (!text) return '';
  // Legacy messages persisted before the server stopped JSON-encoding tool text.
  if (!text.includes('\n') && /\\n/.test(text)) {
    try { text = JSON.parse(text.trim().startsWith('"') ? text : `"${text.replace(/"/g, '\\"')}"`); }
    catch { text = text.replace(/\\n/g, '\n').replace(/\\t/g, '\t').replace(/\\"/g, '"'); }
  }
  if (format === 'yaml' && text.includes('```')) {
    const body = text.includes('```yaml') ? text.split('```yaml')[1] : text.split('```')[1];
    return (body ?? '').split('```')[0].replace(/^\n/, '').trimEnd();
  }
  if (format === 'json') {
    try { return JSON.stringify(JSON.parse(text), null, 2); } catch { /* already text */ }
  }
  return text.trim();
}

/// A one-line summary for a list row, from whatever fields the kind provides.
export function rowMeta(kind: ArtifactKind, meta: Record<string, any> = {}): string {
  const bits: string[] = [];
  if (meta.namespace) bits.push(meta.namespace);
  if (meta.type) bits.push(String(meta.type));
  if (meta.version) bits.push(`v${meta.version}`);
  if (meta.step_count) bits.push(`${meta.step_count} steps`);
  if (kind === 'trigger' && meta.config?.type) bits.push(String(meta.config.type));
  if (meta.enabled === false) bits.push('disabled');
  if (meta.description) bits.push(String(meta.description).slice(0, 60));
  return bits.join(' · ');
}
