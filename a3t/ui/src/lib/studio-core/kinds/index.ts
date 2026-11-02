import type { Item } from '../items';
import type { ItemKind } from './types';
import { flowKind } from './flow';
import { canvasKind, resourceKind, templateKind, triggerKind } from './registryItems';

export type { ItemKind, Capability, PanelDef } from './types';

/// First match wins: specific kinds before the ones they refine.
export const KINDS: ItemKind[] = [flowKind, triggerKind, templateKind, canvasKind, resourceKind];

export function kindFor(item: Item): ItemKind {
  return KINDS.find((k) => k.match(item)) ?? resourceKind;
}

export function kindById(id: string): ItemKind {
  return KINDS.find((k) => k.id === id) ?? resourceKind;
}

export const can = (kind: ItemKind, c: ItemKind['capabilities'][number]) => kind.capabilities.includes(c);
