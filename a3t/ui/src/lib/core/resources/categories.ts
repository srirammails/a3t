// The fixed resource subtype categories.
//
// Shared because the Resources page filters by them AND the shell's context pane
// lists them: two copies would drift, and the pane must offer exactly the
// categories the page can filter to.

export const TYPE_CATEGORY: Record<string, string> = {
    postgres: 'database', mysql: 'database', lancedb: 'database',
    redis: 'database', qdrant: 'database', clawdb: 'database',
    mcp: 'mcp',
    llm: 'llm', openai: 'llm', anthropic: 'llm', ollama: 'llm', groq: 'llm',
    http: 'other',
    function: 'functions',
    script: 'scripts',
    variable: 'variables',
    credential: 'credentials', secret: 'credentials',
};

/// Bucket a resource `type` into its category. Unknown types land in `other`,
/// which has no tab — deliberately, so a new backend shows up somewhere rather
/// than vanishing.
export function categorize(type: string): string {
  return TYPE_CATEGORY[type] ?? 'other';
}

export const RESOURCE_CATEGORIES = [
    { key: 'all',         label: 'All',          icon: '*', iconStyle: 'background: var(--bg-subtle); color: var(--text-2);' },
    { key: 'flows',       label: 'Flows',        icon: 'F', iconStyle: 'background: var(--runtime-bg); color: var(--runtime-ink);' },
    { key: 'database',    label: 'Databases',    icon: 'D', iconStyle: 'background: var(--bg-sunken); color: var(--text-3);' },
    { key: 'llm',         label: 'LLM',          icon: 'L', iconStyle: 'background: var(--spec-bg); color: var(--spec-ink);' },
    { key: 'mcp',         label: 'MCP Servers',  icon: 'M', iconStyle: 'background: var(--control-bg); color: var(--control-ink);' },
    { key: 'functions',   label: 'Functions',    icon: 'f', iconStyle: 'background: var(--signal-bg); color: var(--signal-ink);' },
    { key: 'scripts',     label: 'Scripts',      icon: 'S', iconStyle: 'background: var(--ephemeral-bg); color: var(--ephemeral-ink);' },
    { key: 'variables',   label: 'Variables',    icon: 'V', iconStyle: 'background: var(--checkpoint-bg); color: var(--checkpoint-ink);' },
    { key: 'credentials', label: 'Secrets',       icon: 'K', iconStyle: 'background: var(--warn-bg); color: var(--warn);' },
] as const;

export const CAT_META: Record<string, { label: string; icon: string; iconStyle: string }> = {};
for (const c of RESOURCE_CATEGORIES) CAT_META[c.key] = c;

export function catMeta(cat: string) {
  return CAT_META[cat] ?? { label: cat, icon: '?', iconStyle: 'background: var(--bg-subtle); color: var(--text-4);' };
}

export const CAT_COLORS: Record<string, string> = {
    flows: '#1a7158', database: '#4a4742', llm: '#5530a8',
    mcp: '#c97c2e', functions: '#5c3589', scripts: '#3a7fff', variables: '#c97c2e', credentials: '#c4243b',
};
