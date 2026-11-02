import type { Item } from '../items';
import type { ItemKind } from './types';
import { flowKind } from './flow';
import { agentKind, resourceKind, templateKind, triggerKind } from './registryItems';
import { canvasKind, runtimeKind } from './operate';
import { databaseKind } from './database';
import { llmKind, mcpKind } from './act';

export type { ItemKind, Capability, PanelDef } from './types';
export { languagesOf } from './database';

/// First match wins: specific kinds before the ones they refine.
export const KINDS: ItemKind[] = [flowKind, triggerKind, templateKind, agentKind, canvasKind, databaseKind, llmKind, mcpKind, runtimeKind, resourceKind];

export function kindFor(item: Item): ItemKind {
  return KINDS.find((k) => k.match(item)) ?? resourceKind;
}

export function kindById(id: string): ItemKind {
  return KINDS.find((k) => k.id === id) ?? resourceKind;
}

export const can = (kind: ItemKind, c: ItemKind['capabilities'][number]) => kind.capabilities.includes(c);
