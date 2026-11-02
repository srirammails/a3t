/// Registry items edited as config documents (`doc.text` is the config as JSON).
/// Specific resource kinds (database, llm, mcp…) go before `resourceKind` in
/// the registry and take over the types they handle.
import * as registry from '../adapters/registry';
import * as config from '../adapters/config';
import * as act from '../adapters/act';
import { fail, ok } from '../items';
import { fromText, maskedPaths, toText } from '../config';
import type { Doc } from '../workspace';
import type { ItemKind } from './types';

const configViews = { center: 'config' };
const textDirty = (doc: Doc) => doc.loaded && (doc.text ?? '') !== (doc.saved ?? '');

function loaded(doc: Doc, text: string, data: any) {
  doc.text = text;
  doc.saved = text;
  doc.data = data;
  doc.error = undefined;
}

/// A resource: its `config`, saved through the registry.
export const resourceConfig: Pick<ItemKind, 'load' | 'save' | 'dirty' | 'masked'> = {
  async load(doc) {
    const r = await config.getResource(doc.ref.project, doc.ref.name);
    if (!r.ok) { doc.error = r.error; return; }
    doc.type = r.data.type;
    loaded(doc, toText(r.data.config), r.data);
  },
  async save(doc) {
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    const r = await config.putResource(doc.ref.project, doc.ref.name, value);
    if (!r.ok) return r;
    // Read it back as the platform shows it: a replaced secret is masked again.
    const back = await config.getResource(doc.ref.project, doc.ref.name);
    if (back.ok) loaded(doc, toText(back.data.config), back.data);
    else doc.saved = doc.text;
    return ok(doc.ref);
  },
  dirty: textDirty,
  /// From the config as loaded, not as edited: what the server masked.
  masked: (doc) => maskedPaths(doc.type ?? '', fromText(doc.saved).value ?? {}),
};

export const triggerKind: ItemKind = {
  id: 'trigger', label: 'Trigger', icon: '⚡',
  match: (it) => it.kind === 'trigger',
  capabilities: ['edit', 'delete', 'fire'],
  /// Fire it, and watch its target flow's runs below; or edit its config.
  views: {
    center: 'fire',
    modes: [{ id: 'fire', title: 'Fire' }, { id: 'config', title: 'Config' }],
    bottom: [{ id: 'target-runs', title: 'Runs' }],
  },
  initialUi: () => ({ payload: '{\n  "message": "Hello from the Studio"\n}\n', firing: false, fired: null, error: '' }),
  /// As saved (not as being edited): what the platform will fire.
  fire: (doc, payload) => act.fire({ namespace: doc.data?.namespace ?? doc.ref.project, config: doc.data?.config, targets: doc.data?.targets ?? [] }, payload),
  console: { section: 'triggers', title: 'Triggers' },
  /// What is edited: everything but the id and the status the platform keeps.
  async load(doc) {
    const id = doc.ref.id;
    if (!id) { doc.error = 'This trigger has no id'; return; }
    const r = await config.getTrigger(id);
    if (!r.ok) { doc.error = r.error; return; }
    const { name, enabled, description, config: c, targets } = r.data;
    loaded(doc, toText({ name, enabled, description: description ?? '', config: c, targets }), r.data);
  },
  async save(doc) {
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    if (!doc.ref.id) return fail('This trigger has no id');
    const r = await config.putTrigger(doc.ref.id, { ...value, namespace: doc.data?.namespace ?? doc.ref.project });
    if (!r.ok) return r;
    doc.saved = doc.text;
    doc.data = { ...doc.data, ...value };
    return ok({ ...doc.ref, name: String(value.name ?? doc.ref.name) });
  },
  dirty: textDirty,
  remove: (it) => (it.id ? registry.removeTrigger(it.id) : Promise.resolve(fail('This trigger has no id'))),
};

/// Shown read-only here: saving a template changes the agents built from it,
/// which its Console page (and later its own Studio view) handles.
export const templateKind: ItemKind = {
  id: 'template', label: 'Template', icon: '▤',
  match: (it) => it.kind === 'template',
  capabilities: ['delete'],
  views: configViews,
  console: { section: 'templates', title: 'Templates' },
  async load(doc) {
    const r = doc.ref.id ? await config.getTemplate(doc.ref.id) : fail('This template has no id');
    if (!r.ok) { doc.error = r.error; return; }
    loaded(doc, toText(r.data), r.data);
  },
  remove: (it) => (it.id ? registry.removeTemplate(it.id) : Promise.resolve(fail('This template has no id'))),
};

export const canvasKind: ItemKind = {
  id: 'canvas', label: 'Canvas', icon: '▦',
  match: (it) => it.kind === 'canvas',
  capabilities: ['edit', 'delete'],
  views: configViews,
  console: { section: 'canvas', title: 'Canvas' },
  ...resourceConfig,
  remove: (it) => registry.removeResource(it.project, it.name),
};

/// Any resource no more specific kind claims.
export const resourceKind: ItemKind = {
  id: 'resource', label: 'Resource', icon: '◆',
  match: (it) => it.kind === 'resource',
  capabilities: ['edit', 'delete'],
  views: configViews,
  console: { section: 'resources', title: 'Resources' },
  ...resourceConfig,
  remove: (it) => registry.removeResource(it.project, it.name),
};
