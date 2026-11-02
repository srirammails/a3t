/// Resource types the Studio can create, by category, with the config a new one
/// starts from (the shapes existing resources of that type use). The first
/// type in a category is its default.
import { categorize } from '$lib/core/resources/categories';

export const TYPES_BY_CATEGORY: Record<string, { type: string; label: string }[]> = {
  database: [
    { type: 'postgres', label: 'PostgreSQL' }, { type: 'mysql', label: 'MySQL' }, { type: 'lancedb', label: 'LanceDB' },
    { type: 'redis', label: 'Redis / Dragonfly' }, { type: 'qdrant', label: 'Qdrant' }, { type: 'adb', label: 'ADB (agent memory)' },
  ],
  llm: [{ type: 'openai', label: 'OpenAI-compatible' }, { type: 'anthropic', label: 'Anthropic' }, { type: 'groq', label: 'Groq' }, { type: 'ollama', label: 'Ollama' }],
  mcp: [{ type: 'mcp', label: 'MCP server' }],
  credentials: [{ type: 'secret', label: 'Secret' }, { type: 'credential', label: 'Credential' }],
  functions: [{ type: 'function', label: 'Function' }],
  scripts: [{ type: 'script', label: 'Script' }],
  variables: [{ type: 'variable', label: 'Variable' }],
  runtimes: [{ type: 'runtime', label: 'Runtime pool' }],
};

const LLM = (api_url: string) => ({ api_url, model: '', max_tokens: 1024, temperature: 0.7, api_key_ref: '' });

export const STARTING_CONFIG: Record<string, object> = {
  postgres: { host: 'localhost', port: 5432, database: 'postgres', user: 'postgres', password: '' },
  mysql: { host: 'localhost', port: 3306, database: '', user: '', password: '' },
  lancedb: { db_path: '/data/a3t/my-db.lance' },
  redis: { url: 'redis://localhost:6379' },
  qdrant: { url: 'http://localhost:6333' },
  adb: { url: 'http://adb.a3t.svc.cluster.local:3000' },
  openai: LLM('https://api.openai.com/v1'),
  anthropic: LLM('https://api.anthropic.com'),
  groq: LLM('https://api.groq.com/openai/v1'),
  ollama: { api_url: 'http://localhost:11434/v1', model: '' },
  mcp: { url: 'http://localhost:3002/message', transport: 'http' },
  secret: { value: '' },
  runtime: { labels: { class: 'cpu' }, default: false },
};

export const startingConfig = (type: string) => STARTING_CONFIG[type] ?? {};

/// The category a type belongs to, with runtime pools their own.
export const categoryOf = (type: string) => (type === 'runtime' ? 'runtimes' : categorize(type));
export const typesLike = (type: string) => TYPES_BY_CATEGORY[categoryOf(type)] ?? [{ type, label: type }];
