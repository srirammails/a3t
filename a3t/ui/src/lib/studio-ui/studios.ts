/// What a Studio is: configuration. Each host shows one of these (the web dash
/// one per app page; VS Code one with all scopes).
import type { StudioLayout } from './layout.svelte.ts';

export type StudioConfig = {
  /// Keys its layout, open tabs and recent items (`a3t.studio.<app>…`).
  app: string;
  /// The kind whose panels show while nothing (or a kind without panels) is open.
  homeKind: string;
  /// Kinds "+ New" creates.
  create: string[];
  layout?: Partial<StudioLayout>;
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

export const STUDIOS = {
  flows: {
    app: 'flows',
    homeKind: 'flow',
    create: ['flow'],
    layout: {
      left: { size: 280, minimized: false, active: 'explorer' },
      right: { size: 380, minimized: false, active: 'form' },
      bottom: { size: 320, minimized: true, active: 'runs' },
    },
    home: {
      headline: 'Author a flow.',
      highlight: 'flow',
      subtitle: 'CNCF Serverless Workflow definitions, edited as a graph, as YAML, or as a form.',
      description: 'Open a flow from the Explorer or start a new one. Check it under Problems, deploy it, then run or debug it from Runs — all without leaving the editor.',
      accent: 'spec',
      docsPath: '/concepts/workflows/',
    },
  },
} satisfies Record<string, StudioConfig>;
