/// Registry items other than flows: resources (and canvases, which are
/// resources of type `canvas`), triggers and agent templates.
import { resources as registryApi, mcpCall, execFlow } from '$lib/services/api';
import { ok, type Result } from '../items';
import { result, rows, safely } from './util';

export const listResources = (project: string): Promise<Result<any[]>> =>
  safely(`resources in ${project}`, async () => {
    const r = result(await mcpCall('list_resources', { namespace: project }), `resources in ${project}`);
    return r.ok ? ok(rows(r.data, 'resources')) : r;
  });

/// A runtime with deployments answers 409 with its reschedule plan (the error says so).
export const removeResource = (project: string, name: string): Promise<Result> =>
  safely('delete resource', async () => {
    const r = result(await registryApi.remove(project, name), `Delete ${name}`);
    return r.ok ? ok(undefined) : r;
  });

export const listTriggers = (): Promise<Result<any[]>> =>
  safely('triggers', async () => {
    const r = result(await mcpCall('list_triggers', {}), 'triggers');
    return r.ok ? ok(rows(r.data, 'triggers')) : r;
  });

export const removeTrigger = (id: string): Promise<Result> =>
  safely('delete trigger', async () => {
    const r = result(await mcpCall('delete_trigger', { id }), 'Delete trigger');
    return r.ok ? ok(undefined) : r;
  });

export const listTemplates = (): Promise<Result<any[]>> =>
  safely('templates', async () => {
    const r = result(await execFlow('a3t-templates-list', 'a3t', { org_id: 'org-default' }, { silent: true }), 'templates');
    return r.ok ? ok(rows(r.data, 'rows')) : r;
  });

export const listAgents = (): Promise<Result<any[]>> =>
  safely('agents', async () => {
    const r = result(await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' }, { silent: true }), 'agents');
    return r.ok ? ok(rows(r.data, 'rows', 'output')) : r;
  });

/// The server refuses while agents are built from the template.
export const removeTemplate = (id: string): Promise<Result> =>
  safely('delete template', async () => {
    const r = result(await execFlow('a3t-templates-delete', 'a3t', { id, org_id: 'org-default' }), 'Delete template');
    return r.ok ? ok(undefined) : r;
  });
