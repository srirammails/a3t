/// Agent templates and agents (AGENT_TEMPLATE_SPEC; model in ../agent/model.ts).
/// A template is edited as a workflow, with each config field's visibility in
/// its Agent fields panel. An agent is edited through what its template
/// exposes: required and optional fields, locked ones read-only, hidden ones not
/// at all.
import * as registry from '../adapters/registry';
import * as agents from '../adapters/agents';
import { getTemplate } from '../adapters/config';
import { fail, ok } from '../items';
import { fromText, toText } from '../config';
import { agentValues, missing, toRecord } from '../agent/model';
import type { Doc } from '../workspace';
import type { ItemKind } from './types';

const AI = { id: 'ai', title: 'AI', component: 'ai' };
const textDirty = (doc: Doc) => doc.loaded && (doc.text ?? '') !== (doc.saved ?? '');

export const templateKind: ItemKind = {
  id: 'template', label: 'Agent template', icon: '▤',
  match: (it) => it.kind === 'template',
  capabilities: ['edit'],
  nameInText: true,
  /// The template's workflow (graph / code); its form, which fields agents may
  /// set (Agent fields) and AI on the right.
  views: {
    center: 'template',
    right: [{ id: 'form', title: 'Form', component: 'flow-form' }, { id: 'fields', title: 'Agent fields', component: 'template-fields' }, AI],
  },
  console: { section: 'templates', title: 'Templates' },
  async load(doc) {
    const r = doc.ref.id ? await getTemplate(doc.ref.id) : fail('This template has no id');
    if (!r.ok) { doc.error = r.error; return; }
    doc.text = r.data.spec_yaml ?? '';
    doc.saved = doc.text;
    doc.data = { template: r.data, agents: [] };
    const a = await registry.listAgents();
    doc.data.agents = a.ok ? a.data.filter((x: any) => x.template_id === r.data.id) : [];
  },
  /// Saving changes what every agent built from it runs.
  async beforeSave(doc) {
    const used = (doc.data?.agents ?? []) as any[];
    if (!used.length) return null;
    const names = used.map((a) => a.name ?? a.id).slice(0, 10).join(', ');
    return `This template is used by ${used.length} agent(s): ${names}${used.length > 10 ? '…' : ''}.\n\nSaving changes what those agents run. Continue?`;
  },
  async save(doc) {
    const r = await agents.updateTemplate(doc.data.template, doc.text ?? '');
    if (!r.ok) return r;
    doc.saved = doc.text;
    doc.data.template = { ...doc.data.template, spec_yaml: doc.text };
    return ok(doc.ref);
  },
  dirty: textDirty,
};

/// The agent document: what the agent sets itself, as JSON —
/// `{ name, mode, status, settings: { <config key>: value }, serving: { target, ep_path } }`.
export const agentKind: ItemKind = {
  id: 'agent', label: 'Agent', icon: '◉',
  match: (it) => it.kind === 'agent',
  capabilities: ['edit'],
  /// What it runs (its template's workflow with its values) in the center; its
  /// settings, as its template exposes them, in the Form panel.
  views: { center: 'agent', code: 'code', right: [{ id: 'form', title: 'Form', component: 'agent-form' }, AI] },
  console: { section: 'agents', title: 'Agents' },
  async load(doc) {
    const [a, t] = await Promise.all([registry.listAgents(), registry.listTemplates()]);
    if (!a.ok) { doc.error = a.error; return; }
    const agent = a.data.find((x: any) => x.id === doc.ref.id);
    if (!agent) { doc.error = 'The agent is no longer listed'; return; }
    const template = t.ok ? t.data.find((x: any) => x.id === agent.template_id) ?? null : null;
    const vars = (() => { try { return JSON.parse(agent.variables ?? '{}'); } catch { return {}; } })();
    doc.text = toText({
      name: agent.name, mode: agent.mode, status: agent.status,
      settings: agentValues(agent),
      serving: { target: vars.target ?? '', ep_path: vars.ep_path ?? '' },
    });
    doc.saved = doc.text;
    doc.data = { agent, template };
  },
  async save(doc) {
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    const spec = doc.data?.template?.spec_yaml ?? '';
    const need = missing(spec, value.settings ?? {});
    if (need.length) return fail(`Set what the template requires: ${need.join(', ')}`);
    const agent = doc.data.agent;
    const r = await agents.updateAgent(agent, {
      name: value.name ?? agent.name, mode: value.mode ?? agent.mode, status: value.status ?? agent.status,
      ...toRecord(agent, spec, value.settings ?? {}, value.serving ?? {}),
    });
    if (!r.ok) return r;
    doc.saved = doc.text;
    return ok(doc.ref);
  },
  dirty: textDirty,
};
