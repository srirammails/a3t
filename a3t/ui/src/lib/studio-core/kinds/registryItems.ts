/// Registry items the Studio previews for now: managed in full on their Console
/// pages. Specific resource kinds (database, llm, mcp…) go before `resourceKind`
/// in the registry and take over the types they handle.
import * as registry from '../adapters/registry';
import { fail } from '../items';
import type { ItemKind } from './types';

const preview = { center: 'preview' };

export const triggerKind: ItemKind = {
  id: 'trigger', label: 'Trigger', icon: '⚡',
  match: (it) => it.kind === 'trigger',
  capabilities: ['delete'],
  views: preview,
  console: { section: 'triggers', title: 'Triggers' },
  remove: (it) => (it.id ? registry.removeTrigger(it.id) : Promise.resolve(fail('This trigger has no id'))),
};

export const templateKind: ItemKind = {
  id: 'template', label: 'Template', icon: '▤',
  match: (it) => it.kind === 'template',
  capabilities: ['delete'],
  views: preview,
  console: { section: 'templates', title: 'Templates' },
  remove: (it) => (it.id ? registry.removeTemplate(it.id) : Promise.resolve(fail('This template has no id'))),
};

export const canvasKind: ItemKind = {
  id: 'canvas', label: 'Canvas', icon: '▦',
  match: (it) => it.kind === 'canvas',
  capabilities: ['delete'],
  views: preview,
  console: { section: 'canvas', title: 'Canvas' },
  remove: (it) => registry.removeResource(it.project, it.name),
};

/// Any resource no more specific kind claims.
export const resourceKind: ItemKind = {
  id: 'resource', label: 'Resource', icon: '◆',
  match: (it) => it.kind === 'resource',
  capabilities: ['delete'],
  views: preview,
  console: { section: 'resources', title: 'Resources' },
  remove: (it) => registry.removeResource(it.project, it.name),
};
