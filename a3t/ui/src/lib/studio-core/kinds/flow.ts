import * as flows from '../adapters/flows';
import { fail, ok } from '../items';
import { FLOW_NAME, newFlowYaml, parse, problems, versionOf, withVersion } from '../flow/model';
import type { ItemKind } from './types';

export const flowKind: ItemKind = {
  id: 'flow',
  label: 'Flow',
  icon: 'ƒ',
  match: (it) => it.kind === 'flow',
  capabilities: ['edit', 'run', 'debug', 'create', 'delete', 'rename', 'duplicate'],
  nameInText: true,
  views: {
    center: 'flow',
    left: [{ id: 'steps', title: 'Steps', component: 'steps' }],
    right: [{ id: 'form', title: 'Form', component: 'flow-form' }, { id: 'ai', title: 'AI', component: 'ai' }],
    bottom: [{ id: 'runs', title: 'Runs' }, { id: 'versions', title: 'Versions' }, { id: 'problems', title: 'Problems', onlyWithBadge: true }],
  },

  /// The run dialog of the Runs panel, opened by the view's Run / Debug.
  initialUi: () => ({ runDialog: false, runDebug: false }),

  async load(doc) {
    const r = await flows.getYaml(doc.ref.project, doc.ref.name);
    doc.text = r.ok ? r.data : '';
    doc.saved = doc.text;
    doc.error = r.ok ? undefined : r.error;
  },

  create(project, n) {
    return { ref: { project, kind: 'flow', name: n > 1 ? `new-flow-${n}` : 'new-flow' }, text: newFlowYaml(project, n) };
  },

  async push(doc) {
    const text = doc.text ?? '';
    const { doc: parsed, error } = parse(text);
    if (error || !parsed) return fail(`Not valid YAML: ${error}`);
    const r = await flows.deploy(text);
    if (!r.ok) return r;
    doc.saved = text;
    return ok({ project: parsed.document?.namespace ?? doc.ref.project, kind: 'flow', name: parsed.document?.name ?? doc.ref.name });
  },

  versions: (doc) => flows.versions(doc.ref.project, doc.ref.name),
  activate: (doc, v) => flows.activate(doc.ref.project, doc.ref.name, v),
  loadVersion: (doc, v) => flows.getYaml(doc.ref.project, doc.ref.name, v),
  versionOf,
  withVersion,

  dirty: (doc) => !!doc.isNew || (doc.loaded && (doc.text ?? '') !== (doc.saved ?? '')),

  badge(doc, panel) {
    if (panel !== 'problems' || !(doc.loaded || doc.isNew)) return null;
    return problems(doc.text).length || null;
  },

  remove: (it) => flows.remove(it.project, it.name),
  rename: (it, to) => flows.rename(it.project, it.name, to),
  duplicate: (it, to) => flows.copy(it.project, it.name, to),
  checkName: (n) => (FLOW_NAME.test(n) ? '' : 'Lowercase letters, digits and dashes (not at either end).'),
  renameNote: 'Saves the flow under the new name, then removes the old one. Past runs stay under the old name; triggers and canvases that name it need rewiring. A deployed flow cannot be renamed.',
};
