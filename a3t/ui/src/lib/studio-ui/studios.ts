/// What a Studio is: configuration. Each host shows one of these (the web dash
/// one per app page; VS Code one with all scopes).
import type { StudioLayout } from './layout.svelte.ts';

export type StudioConfig = {
  /// Keys its layout, open tabs and recent items (`a3t.studio.<app>…`).
  app: string;
  /// The kind whose panels show while nothing (or a kind without panels) is open.
  homeKind: string;
  /// What "+ New" creates: a kind (and resource type), with its label.
  create: { kind: string; type?: string; label: string }[];
  layout?: Partial<StudioLayout>;
  /// A resource category the Explorer shows first, open (e.g. `database`).
  pin?: string;
  /// The home's banner, shown with nothing open.
  home: {
    headline: string;
    highlight?: string;
    subtitle?: string;
    description?: string;
    accent?: 'spec' | 'runtime' | 'control' | 'ephemeral';
    docsPath?: string;
  };
};

/// Everything the Studio can create, for its "+ New" menu.
const NEW_ANYTHING: StudioConfig['create'] = [
  { kind: 'flow', label: 'workflow' },
  { kind: 'database', type: 'postgres', label: 'database' },
  { kind: 'llm', type: 'openai', label: 'LLM' },
  { kind: 'mcp', type: 'mcp', label: 'MCP server' },
  { kind: 'resource', type: 'secret', label: 'secret' },
  { kind: 'trigger', label: 'trigger' },
  { kind: 'canvas', label: 'canvas' },
  { kind: 'runtime', type: 'runtime', label: 'runtime pool' },
];

export const STUDIOS = {
  flows: {
    app: 'flows',
    homeKind: 'flow',
    create: NEW_ANYTHING,
    layout: {
      left: { size: 280, minimized: false, active: 'explorer' },
      right: { size: 380, minimized: false, active: 'form' },
      bottom: { size: 320, minimized: true, active: 'runs' },
    },
    home: {
      headline: 'Build in the Studio.',
      highlight: 'Studio',
      subtitle: 'Flows, databases, LLMs, MCP servers, triggers and canvases — one editor for all of them.',
      description: 'Open anything from the Explorer: edit a flow as a graph, YAML or form and run or debug it; query a database; chat with an LLM; call an MCP server’s tools; fire a trigger. Each item’s config is one click away.',
      accent: 'spec',
      docsPath: '/getting-started/introduction/',
    },
  },
  db: {
    app: 'db',
    homeKind: 'database',
    create: [{ kind: 'database', type: 'postgres', label: 'database' }],
    pin: 'database',
    layout: {
      left: { size: 280, minimized: false, active: 'explorer' },
      right: { size: 380, minimized: false, active: 'form' },
      bottom: { size: 320, minimized: false, active: 'results' },
    },
    home: {
      headline: 'Query your data.',
      highlight: 'data',
      subtitle: 'SQL, Redis and AQL against the databases in your projects — answers as tables.',
      description: 'Pick a database in the Explorer. Write a query and run it with Ctrl/⌘+Enter; runs are kept under History, and Save query keeps one by name. Config shows the database resource itself.',
      accent: 'control',
      docsPath: '/concepts/resources/',
    },
  },
} satisfies Record<string, StudioConfig>;
