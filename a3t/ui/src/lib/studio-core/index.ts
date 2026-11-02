/// studio-core: the Studio's model in plain TypeScript — items, kinds,
/// workspace, inventory and adapters. No Svelte, DOM or host imports (checked
/// by scripts/check-layers.mjs); the web dash and the VS Code extension both
/// build on it. Design: flowr-system/.ai/A3T_STUDIO_ARCHITECTURE.md.
export * from './items';
export type { StudioHost } from './host';
export { KINDS, kindFor, kindById, can, type ItemKind, type Capability, type PanelDef } from './kinds';
export { Workspace, emptyWorkspace, type Doc, type WorkspaceState } from './workspace';
export { maskedPaths, getAt, setAt, removeAt, pathKey, fromText, toText, type Path } from './config';
export { loadInventory, namesIn, type Inventory, type TreeNode } from './inventory';
