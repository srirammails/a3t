/// Operating flows: canvases (attach a flow, placement, deploy) and runtimes
/// (what runs where). Runs show in the Runs panels.
import * as ops from '../adapters/ops';
import { removeResource } from '../adapters/registry';
import { fail, ok } from '../items';
import { fromText, toText } from '../config';
import { FLOW_NAME } from '../flow/model';
import { FORM, resourceConfig } from './registryItems';
import type { ItemKind } from './types';

/// A new canvas: no flow yet, the default runtime pool, running.
const NEW_CANVAS = { flowRef: '', placement: { mode: 'shared', runtimeSelector: {} }, lifecycle: { state: 'running' }, health: { dependsOn: [] } };

export const canvasKind: ItemKind = {
  id: 'canvas', label: 'Canvas', icon: '▦',
  match: (it) => it.kind === 'canvas',
  capabilities: ['edit', 'create', 'delete'],
  /// Attach a flow and deploy it in the center; its placement, lifecycle and
  /// health in the Form panel.
  views: { center: 'canvas', code: 'code', right: FORM },
  console: { section: 'canvas', title: 'Canvas' },
  load: resourceConfig.load,
  masked: resourceConfig.masked,
  create(project, n) {
    return { ref: { project, kind: 'canvas', name: n > 1 ? `new-canvas-${n}` : 'new-canvas' }, text: toText(NEW_CANVAS) };
  },
  /// A new canvas is created under the name chosen in its view; an existing one is updated.
  async push(doc) {
    if (!doc.isNew) return resourceConfig.push!(doc);
    const { value, error } = fromText(doc.text);
    if (error) return fail(`Not valid JSON: ${error}`);
    if (!FLOW_NAME.test(doc.ref.name)) return fail('Name the canvas: lowercase letters, digits and dashes');
    const r = await ops.createCanvas(doc.ref.project, doc.ref.name, value);
    if (!r.ok) return r;
    doc.saved = doc.text;
    return ok({ project: doc.ref.project, kind: 'canvas', name: doc.ref.name });
  },
  dirty: (doc) => !!doc.isNew || (doc.loaded && (doc.text ?? '') !== (doc.saved ?? '')),
  checkName: (n) => (FLOW_NAME.test(n) ? '' : 'Lowercase letters, digits and dashes (not at either end).'),
  remove: (it) => removeResource(it.project, it.name),
};

/// Runtime pools are registry resources (`type: runtime`).
export const runtimeKind: ItemKind = {
  id: 'runtime', label: 'Runtime', icon: '▣',
  match: (it) => it.kind === 'resource' && it.type === 'runtime',
  capabilities: ['edit', 'create', 'delete'],
  /// What is deployed on it and its state in the center; its pool config (labels…) in the Form panel.
  views: { center: 'runtime', code: 'code', right: FORM },
  console: { section: 'runtime', title: 'Runtimes' },
  ...resourceConfig,
  remove: (it) => removeResource(it.project, it.name),
};
