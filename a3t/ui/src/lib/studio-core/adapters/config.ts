/// Reading and writing the config of registry items: resources (and canvases,
/// which are resources), triggers, templates.
import { mcpCall, resources as registryApi } from '$lib/services/api';
import { fail, ok, type Result } from '../items';
import { listTemplates } from './registry';
import { result, safely } from './util';

export type ResourceRecord = { type: string; config: any; description?: string | null; tags?: string[]; created_at?: string; updated_at?: string };

/// Secrets come back masked (see `maskedPaths`).
export const getResource = (project: string, name: string): Promise<Result<ResourceRecord>> =>
  safely(`load ${name}`, async () => {
    const r = result<any>(await mcpCall('get_resource', { name, namespace: project }), `Load ${name}`);
    if (!r.ok) return r;
    if (!r.data || typeof r.data !== 'object') return fail(`Load ${name}: ${String(r.data)}`);
    return ok({ ...r.data, type: r.data.type ?? r.data.type_ref ?? '', config: r.data.config ?? {} });
  });

export const createResource = (project: string, name: string, type: string, config: unknown): Promise<Result> =>
  safely(`create ${name}`, async () => {
    const r = result(await mcpCall('create_resource', { name, namespace: project, type_ref: type, config }), `Create ${name}`);
    return r.ok ? ok(undefined) : r;
  });

/// The registry replaces the config; masked values sent back unchanged keep what is stored.
export const putResource = (project: string, name: string, config: unknown): Promise<Result> =>
  safely(`save ${name}`, async () => {
    const r = result(await registryApi.update(project, name, config), `Save ${name}`);
    return r.ok ? ok(undefined) : r;
  });

export type TriggerRecord = { id: string; name: string; namespace: string; config: any; targets: any[]; enabled: boolean; status?: string; description?: string | null };

export const getTrigger = (id: string): Promise<Result<TriggerRecord>> =>
  safely('load trigger', async () => {
    const r = result<any>(await mcpCall('get_trigger', { id }), 'Load trigger');
    if (!r.ok) return r;
    return r.data && typeof r.data === 'object' ? ok(r.data) : fail(`Load trigger: ${String(r.data)}`);
  });

/// The platform replaces the trigger with these fields (top level, as its
/// `Trigger` type reads them) under the same id.
export const putTrigger = (id: string, t: Omit<TriggerRecord, 'id' | 'status'>): Promise<Result> =>
  safely('save trigger', async () => {
    const r = result(await mcpCall('update_trigger', { id, ...t }), 'Save trigger');
    return r.ok ? ok(undefined) : r;
  });

export const createTrigger = (t: Omit<TriggerRecord, 'id' | 'status'>): Promise<Result> =>
  safely('create trigger', async () => {
    const r = result(await mcpCall('create_trigger', { ...t }), 'Create trigger');
    return r.ok ? ok(undefined) : r;
  });

export async function getTemplate(id: string): Promise<Result<any>> {
  const r = await listTemplates();
  if (!r.ok) return r;
  const t = r.data.find((x: any) => x.id === id);
  return t ? ok(t) : fail('The template is no longer listed');
}
