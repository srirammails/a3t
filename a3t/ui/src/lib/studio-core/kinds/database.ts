/// Database resources: a Query view (run statements in the database's own
/// language, results below) and a Config view (the resource's config form).
import * as query from '../adapters/query';
import { removeResource } from '../adapters/registry';
import { resourceConfig } from './registryItems';
import type { ItemKind } from './types';

/// Each database type's languages, the first being the default. `dialect` is the
/// editor's SQL flavour.
export const LANGUAGES: Record<string, { id: string; label: string; dialect: 'postgres' | 'mysql' | 'standard' }[]> = {
  postgres: [{ id: 'sql', label: 'SQL', dialect: 'postgres' }],
  mysql: [{ id: 'sql', label: 'SQL', dialect: 'mysql' }],
  lancedb: [{ id: 'sql', label: 'SQL', dialect: 'standard' }],
  redis: [{ id: 'redis', label: 'Redis', dialect: 'standard' }],
  qdrant: [{ id: 'aql', label: 'AQL', dialect: 'standard' }],
  adb: [{ id: 'aql', label: 'AQL', dialect: 'standard' }, { id: 'sql', label: 'SQL', dialect: 'standard' }],
  clawdb: [{ id: 'aql', label: 'AQL', dialect: 'standard' }, { id: 'sql', label: 'SQL', dialect: 'standard' }],
};

export const languagesOf = (type: string | undefined) => LANGUAGES[type ?? ''] ?? [{ id: 'query', label: 'Query', dialect: 'standard' as const }];

export const databaseKind: ItemKind = {
  id: 'database', label: 'Database', icon: '⛁',
  match: (it) => it.kind === 'resource' && !!it.type && it.type in LANGUAGES,
  capabilities: ['edit', 'delete', 'query'],
  views: {
    center: 'query',
    modes: [{ id: 'query', title: 'Query' }, { id: 'config', title: 'Config' }],
    bottom: [{ id: 'results', title: 'Results' }, { id: 'history', title: 'History' }, { id: 'saved', title: 'Saved' }],
  },
  console: { section: 'resources', title: 'Resources' },
  /// The query being written and the last result are view state; the
  /// document's text is the config (Config view), saved like any resource.
  initialUi: () => ({ query: '', lang: '', result: null, error: '', running: false }),
  ...resourceConfig,
  async query(doc, text, lang) {
    return query.run({ project: doc.ref.project, name: doc.ref.name, type: doc.type ?? '' }, text, lang);
  },
  remove: (it) => removeResource(it.project, it.name),
};
