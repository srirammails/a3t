/// Registry items edited as config documents (`doc.text` is the config as JSON).
/// Specific resource kinds (database, llm, mcp…) go before `resourceKind` in
/// the registry and take over the types they handle.
import * as registry from '../adapters/registry';
import * as config from '../adapters/config';
import * as act from '../adapters/act';
import { fail, ok } from '../items';
import { fromText, maskedPaths, toText } from '../config';
import { listTriggers } from '../adapters/registry';
import { startingConfig, categoryOf } from '../resourceTypes';
import type { Doc } from '../workspace';
import type { ItemKind, PanelDef } from './types';

/// The right region for config documents: the Form (studio-ui `config`) and
/// the AI assistant (`ai`), which works on the document's text.
export const FORM: PanelDef[] = [{ id: 'form', title: 'Form', component: 'config' }, { id: 'ai', title: 'AI', component: 'ai' }];
/// No action of its own yet: the config as code in the center, as a form beside it.
const configViews = { center: 'code', code: 'code', right: FORM };

const NAME = /^[a-z0-9](-?[a-z0-9])*$/;
export const checkName = (n: string) => (NAME.test(n) ? '' : 'Lowercase letters, digits and dashes (not at either end).');
const textDirty = (doc: Doc) => doc.loaded && (doc.text ?? '') !== (doc.saved ?? '');

function loaded(doc: Doc, text: string, data: any) {
  doc.text = text;
  doc.saved = text;
  doc.data = data;
  doc.error = undefined;
}

/// A resource: its `config`, saved through the registry. New ones start from
/// their type's usual config and are created under the name given in the view.
export const resourceConfig: Pick<ItemKind, 'load' | 'push' | 'dirty' | 'masked' | 'create' | 'checkName'> = {
  create(project, n, type = 'secret') {
    const base = categoryOf(type) === 'credentials' ? 'secret' : type;
    return { ref: { project, kind: 'resource', name: n > 1 ? `new-${base}-${n}` : `new-${base}` }, text: toText(startingConfig(type)), type };
  },
  checkName,
  async load(doc) {
    const r = await config.getResource(doc.ref.project, doc.ref.name);
    if (!r.ok) { doc.error = r.error; return; }
    doc.type = r.data.type;
    loaded(doc, toText(r.data.config), r.data);
  },
  async push(doc) {
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    if (doc.isNew) {
      if (!NAME.test(doc.ref.name)) return fail(`Name it: ${checkName(doc.ref.name)}`);
      const c = await config.createResource(doc.ref.project, doc.ref.name, doc.type ?? 'secret', value);
      if (!c.ok) return c;
      doc.saved = doc.text;
      return ok({ project: doc.ref.project, kind: 'resource', name: doc.ref.name });
    }
    const r = await config.putResource(doc.ref.project, doc.ref.name, value);
    if (!r.ok) return r;
    // Read it back as the platform shows it: a replaced secret is masked again.
    const back = await config.getResource(doc.ref.project, doc.ref.name);
    if (back.ok) loaded(doc, toText(back.data.config), back.data);
    else doc.saved = doc.text;
    return ok(doc.ref);
  },
  dirty: (doc) => !!doc.isNew || textDirty(doc),
  /// From the config as loaded, not as edited: what the server masked.
  masked: (doc) => maskedPaths(doc.type ?? '', fromText(doc.saved).value ?? {}),
};

export const triggerKind: ItemKind = {
  id: 'trigger', label: 'Trigger', icon: '⚡',
  match: (it) => it.kind === 'trigger',
  capabilities: ['edit', 'create', 'delete', 'fire'],
  nameInText: true,
  /// Its designer (channel, targets as a graph, test by firing) in the center;
  /// the runs of its target flow below.
  views: { center: 'trigger', code: 'code', right: FORM, bottom: [{ id: 'target-runs', title: 'Runs' }] },
  create(project, n) {
    const name = n > 1 ? `new-trigger-${n}` : 'new-trigger';
    return {
      ref: { project, kind: 'trigger', name },
      text: toText({ name, enabled: true, description: '', config: { type: 'webhook', path: `/${name}`, method: 'POST' }, targets: [] }),
    };
  },
  checkName,
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
  async push(doc) {
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    if (doc.isNew) {
      if (!NAME.test(String(value.name ?? ''))) return fail(`Name it: ${checkName(String(value.name ?? ''))}`);
      const c = await config.createTrigger({ ...value, namespace: doc.ref.project });
      if (!c.ok) return c;
      // The platform gives it an id; find it by name to keep the tab on it.
      const all = await listTriggers();
      const made = all.ok ? all.data.find((t: any) => t.name === value.name && (t.namespace ?? doc.ref.project) === doc.ref.project) : null;
      doc.saved = doc.text;
      doc.data = made ?? { ...value, namespace: doc.ref.project };
      return ok({ project: doc.ref.project, kind: 'trigger', name: String(value.name), ...(made?.id ? { id: made.id } : {}) });
    }
    if (!doc.ref.id) return fail('This trigger has no id');
    const r = await config.putTrigger(doc.ref.id, { ...value, namespace: doc.data?.namespace ?? doc.ref.project });
    if (!r.ok) return r;
    doc.saved = doc.text;
    doc.data = { ...doc.data, ...value };
    return ok({ ...doc.ref, name: String(value.name ?? doc.ref.name) });
  },
  dirty: (doc) => !!doc.isNew || textDirty(doc),
  remove: (it) => (it.id ? registry.removeTrigger(it.id) : Promise.resolve(fail('This trigger has no id'))),
};

/// Any resource no more specific kind claims.
export const resourceKind: ItemKind = {
  id: 'resource', label: 'Resource', icon: '◆',
  match: (it) => it.kind === 'resource',
  capabilities: ['edit', 'create', 'delete'],
  views: configViews,
  console: { section: 'resources', title: 'Resources' },
  ...resourceConfig,
  remove: (it) => registry.removeResource(it.project, it.name),
};
