/// Resources you talk to: LLMs (a Chat view) and MCP servers (a Tools view).
/// Both keep their config in the Form panel.
import * as act from '../adapters/act';
import { removeResource } from '../adapters/registry';
import { FORM, resourceConfig } from './registryItems';
import type { ItemKind } from './types';

const LLM_TYPES = new Set(['llm', 'openai', 'anthropic', 'ollama', 'groq']);

export const llmKind: ItemKind = {
  id: 'llm', label: 'LLM', icon: '✦',
  match: (it) => it.kind === 'resource' && LLM_TYPES.has(it.type ?? ''),
  capabilities: ['edit', 'delete', 'chat'],
  views: { center: 'chat', right: FORM },
  console: { section: 'resources', title: 'Resources' },
  initialUi: () => ({ messages: [], draft: '', sending: false, error: '' }),
  ...resourceConfig,
  chat: (doc, messages) => act.chat(doc.ref.project, doc.ref.name, messages),
  remove: (it) => removeResource(it.project, it.name),
};

export const mcpKind: ItemKind = {
  id: 'mcp', label: 'MCP server', icon: '⚙',
  match: (it) => it.kind === 'resource' && it.type === 'mcp',
  capabilities: ['edit', 'delete', 'tools'],
  views: { center: 'tools', right: FORM },
  console: { section: 'resources', title: 'Resources' },
  initialUi: () => ({ tools: null, listing: false, error: '', tool: '', args: '{}', calling: false, result: null }),
  ...resourceConfig,
  tools: (doc) => act.listTools(doc.ref.project, doc.ref.name),
  callTool: (doc, tool, args) => act.callTool(doc.ref.project, doc.ref.name, tool, args),
  remove: (it) => removeResource(it.project, it.name),
};
