/// studio-core: the Studio's model in plain TypeScript — items, kinds,
/// workspace, inventory and adapters. No Svelte, DOM or host imports (checked
/// by scripts/check-layers.mjs); the web dash and the VS Code extension both
/// build on it. Design: flowr-system/.ai/A3T_STUDIO_ARCHITECTURE.md.
export * from './items';
export type { StudioHost } from './host';
export { KINDS, kindFor, kindById, can, languagesOf, type ItemKind, type Capability, type PanelDef } from './kinds';
export type { QueryResult } from './adapters/query';
export type { ChatMessage, Tool, Fired } from './adapters/act';
export * as ops from './adapters/ops';
export type { Overview, RuntimeEntry, DeploymentRow, DeploymentAction, RuntimeAction } from './adapters/ops';
export * as flows from './adapters/flows';
export { history, remember, saved, saveQuery, forget, type HistoryEntry, type SavedQuery } from './queries';
export { Workspace, emptyWorkspace, type Doc, type WorkspaceState } from './workspace';
export { maskedPaths, getAt, setAt, removeAt, pathKey, fromText, toText, type Path } from './config';
export { TYPES_BY_CATEGORY, startingConfig, categoryOf, typesLike } from './resourceTypes';
export * as agentModel from './agent/model';
export { PALETTE, STEP_MIME, paletteStep, libraryOf, taskForItem, addTask, type PaletteGroup, type PaletteStep } from './flow/compose';
export { outline, setAt as setSchemaAt, removeAt as removeSchemaAt, appendAt, addKeyAt, renameAt, TASK_TYPES, BUILTIN_CALLS, type ONode, type Outline, type SchemaProblem } from './flow/schema';
export { getTrigger, putTrigger } from './adapters/config';
export { listTemplates } from './adapters/registry';
export type { Visibility, AgentField } from './agent/model';
export { loadInventory, namesIn, type Inventory, type TreeNode } from './inventory';
export { merge3, type Conflict, type Merged } from './merge';
export { versionOf, withVersion, bumpVersion, sortVersions } from './flow/model';
