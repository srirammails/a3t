// A3T Domain Service — wraps FlowR consolidated CRUD flow executions
//
// Each entity has a single consolidated flow (e.g. a3t-templates, a3t-agents)
// that routes by input.action (list, get, create, update, delete).
// Flow input lands at `.` (not `.input`), so we pass { input: { action, ...data } }.
// Auth context (org_id, user_id) is injected from the session store.

import { flows } from './api';
import { getSession } from '$lib/stores/auth';

// ── Helpers ──

function ctx() {
  const session = getSession();
  return {
    org_id: session?.user?.org_id ?? 'org-default',
    user_id: session?.user?.id ?? 'unknown',
    timestamp: new Date().toISOString(),
  };
}

function genId(prefix: string) {
  return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
}

async function exec(flowName: string, input: Record<string, any>) {
  const res = await flows.execute(flowName, 'a3t', input);
  console.log(`[a3t] exec ${flowName}:`, res);
  if (!res.ok) throw new Error(res.error ?? `Flow ${flowName} failed`);

  // execute_flow returns async — extract instance_id and poll for completion
  const text = typeof res.data === 'string' ? res.data : res.data?.text ?? '';
  const idMatch = typeof text === 'string' ? text.match(/Instance ID:\s*`?([0-9a-f-]+)`?/) : null;
  const instanceId = idMatch?.[1] ?? res.data?.instance_id;

  if (!instanceId) {
    return res.data;
  }

  // Poll for completion (max 15s)
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 500));
    const status = await flows.status(instanceId);
    if (!status.ok) continue;
    const d = typeof status.data === 'string' ? JSON.parse(status.data) : status.data;
    if (d?.status === 'completed' || d?.status === 'Completed') return d?.output ?? d;
    if (d?.status === 'failed' || d?.status === 'Failed') throw new Error(d?.error ?? 'Flow failed');
  }
  throw new Error(`Flow ${flowName} timed out`);
}

// ── Templates ──

export const templates = {
  async list() {
    const { org_id } = ctx();
    return exec('a3t-templates', { input: { action: 'list', org_id } });
  },

  async get(id: string) {
    const { org_id } = ctx();
    return exec('a3t-templates', { input: { action: 'get', id, org_id } });
  },

  async create(data: {
    id?: string;
    name: string;
    version: string;
    source?: string;
    vertical?: string;
    tags?: string;
    spec_yaml?: string;
  }) {
    const { org_id, user_id, timestamp } = ctx();
    return exec('a3t-templates', {
      input: {
        action: 'create',
        org_id, user_id, timestamp,
        data: { id: data.id ?? genId('tpl'), ...data },
      },
    });
  },

  async update(id: string, data: Record<string, any>) {
    const { org_id, timestamp } = ctx();
    return exec('a3t-templates', {
      input: { action: 'update', id, org_id, timestamp, data },
    });
  },

  async delete(id: string) {
    const { org_id } = ctx();
    return exec('a3t-templates', { input: { action: 'delete', id, org_id } });
  },
};

// ── Agents ──

export const agents = {
  async list() {
    const { org_id } = ctx();
    return exec('a3t-agents', { input: { action: 'list', org_id } });
  },

  async get(id: string) {
    const { org_id } = ctx();
    return exec('a3t-agents', { input: { action: 'get', id, org_id } });
  },

  async create(data: {
    id?: string;
    name: string;
    template_id: string;
    template_version: string;
    namespace?: string;
    mode?: string;
    variables?: string;
    resource_bindings?: string;
  }) {
    const { org_id, user_id, timestamp } = ctx();
    return exec('a3t-agents', {
      input: {
        action: 'create',
        org_id, user_id, timestamp,
        data: {
          id: data.id ?? genId('agt'),
          namespace: data.namespace ?? 'default',
          mode: data.mode ?? 'reactive',
          variables: data.variables ?? '{}',
          resource_bindings: data.resource_bindings ?? '{}',
          ...data,
        },
      },
    });
  },

  async update(id: string, data: Record<string, any>) {
    const { org_id, timestamp } = ctx();
    return exec('a3t-agents', {
      input: { action: 'update', id, org_id, timestamp, data },
    });
  },

  async delete(id: string) {
    const { org_id } = ctx();
    return exec('a3t-agents', { input: { action: 'delete', id, org_id } });
  },
};

// ── Canvas ──
// Retired: canvases are now `type_ref=canvas` registry resources with the
// a3t.io/v1alpha1 CanvasSpec (see A3T_CANVAS_SPEC.md §8). Use `canvasApi` in
// services/api.ts (registry CRUD + /canvas/* placement). The old `a3t-canvas`
// (canvas_specs) CRUD flow is deprecated.

// ── Resources ──

export const resources = {
  async list() {
    const { org_id } = ctx();
    return exec('a3t-resources', { input: { action: 'list', org_id } });
  },

  async create(data: {
    id?: string;
    name: string;
    type: string;
    config?: string;
  }) {
    const { org_id, user_id, timestamp } = ctx();
    return exec('a3t-resources', {
      input: {
        action: 'create',
        org_id, user_id, timestamp,
        data: {
          id: data.id ?? genId('res'),
          config: data.config ?? '{}',
          ...data,
        },
      },
    });
  },
};

// ── Users ──

export const users = {
  async list() {
    const { org_id } = ctx();
    return exec('a3t-users', { input: { action: 'list', org_id } });
  },
};

// ── Audit ──

export const audit = {
  async list() {
    const { org_id } = ctx();
    return exec('a3t-audit', { input: { action: 'list', org_id } });
  },
};

// ── Knowledge Graph (RotatE / ConceptNet over LanceDB) ──
//
// Backed by the a3t-knowledge flow → call:rotate handler → a3t-kg-db resource.
// Each triple is { head, relation, tail, weight?, head_type?, tail_type? }.

export type Triple = {
  head: string;
  relation: string;
  tail: string;
  weight?: number;
  head_type?: string;
  tail_type?: string;
};

export const knowledge = {
  /** Add ConceptNet triples; entities/relations are created on the fly. */
  async ingest(triples: Triple[], dim = 256) {
    return exec('a3t-knowledge', { input: { action: 'ingest', triples, dim } });
  },

  /** Train RotatE embeddings over the stored triples. */
  async train(opts: { epochs?: number; lr?: number; gamma?: number; neg_samples?: number } = {}) {
    return exec('a3t-knowledge', { input: { action: 'train', ...opts } });
  },

  /** Score a single (head, relation, tail) fact. */
  async score(head: string, relation: string, tail: string, gamma?: number) {
    return exec('a3t-knowledge', { input: { action: 'score', head, relation, tail, gamma } });
  },

  /** Rank candidate tails for (head, relation) via RotatE link prediction. */
  async predictTail(head: string, relation: string, limit = 10, gamma?: number) {
    return exec('a3t-knowledge', { input: { action: 'predict_tail', head, relation, limit, gamma } });
  },

  /** Rank candidate heads for (relation, tail). */
  async predictHead(relation: string, tail: string, limit = 10, gamma?: number) {
    return exec('a3t-knowledge', { input: { action: 'predict_head', relation, tail, limit, gamma } });
  },

  /** Nearest entities to a given entity in embedding space. */
  async neighbors(entity: string, limit = 10) {
    return exec('a3t-knowledge', { input: { action: 'neighbors', entity, limit } });
  },

  /** Filter stored triples by any of head / relation / tail. */
  async lookup(filter: { head?: string; relation?: string; tail?: string; limit?: number } = {}) {
    return exec('a3t-knowledge', { input: { action: 'lookup', ...filter } });
  },

  /** Paginated entity list with vector norms + previews (Vectors tab). */
  async entities(opts: { query?: string; limit?: number; offset?: number } = {}) {
    return exec('a3t-knowledge', { input: { action: 'entities', ...opts } });
  },

  /** Relation list with per-relation triple counts. */
  async relations() {
    return exec('a3t-knowledge', { input: { action: 'relations' } });
  },

  /** Graph summary + 3D projection of entities + edges (3D Graph tab). */
  async stats(opts: { sample?: number; max_edges?: number } = {}) {
    return exec('a3t-knowledge', { input: { action: 'stats', ...opts } });
  },
};
