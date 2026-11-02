/// Saving agents and agent templates (their Console pages' flows).
import { execFlow } from '$lib/services/api';
import { ok, type Result } from '../items';
import { result, safely } from './util';

const stamp = () => ({ org_id: 'org-default', user_id: 'usr-superadmin', timestamp: new Date().toISOString() });

export const updateAgent = (agent: any, change: Record<string, unknown>): Promise<Result> =>
  safely('save agent', async () => {
    const r = result(await execFlow('a3t-agents-update', 'a3t', {
      id: agent.id, ...stamp(),
      data: {
        name: agent.name, template_id: agent.template_id, template_version: agent.template_version,
        mode: agent.mode, namespace: agent.namespace, status: agent.status,
        resource_bindings: agent.resource_bindings, variables: agent.variables,
        ...change,
      },
    }), 'Save agent');
    return r.ok ? ok(undefined) : r;
  });

export const updateTemplate = (t: any, spec_yaml: string): Promise<Result> =>
  safely('save template', async () => {
    const r = result(await execFlow('a3t-templates-update', 'a3t', {
      id: t.id, ...stamp(),
      data: {
        name: t.name, version: t.version, source: t.source === 'platform' ? 'system' : t.source, spec_yaml,
        tags: Array.isArray(t.tags) ? t.tags.join(',') : (t.tags ?? ''), vertical: t.vertical ?? '',
      },
    }), 'Save template');
    return r.ok ? ok(undefined) : r;
  });
