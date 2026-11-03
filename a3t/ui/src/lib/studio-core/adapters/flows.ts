/// Flows on the platform: list, read, deploy, delete, copy and rename.
import { flows as flowsApi, mcpCall, releasesApi, resources as registryApi } from '$lib/services/api';
import { fail, ok, type Result } from '../items';
import { withFlowName } from '../flow/model';
import { result, rows, safely } from './util';

export const list = (project: string): Promise<Result<any[]>> =>
  safely(`flows in ${project}`, async () => {
    const r = result(await flowsApi.list(project), `flows in ${project}`);
    return r.ok ? ok(rows(r.data, 'workflows', 'flows')) : r;
  });

/// A flow's YAML in the registry (bare, or fenced in a reply): the active
/// version, or `version`.
export const getYaml = (project: string, name: string, version?: string): Promise<Result<string>> =>
  safely('load flow', async () => {
    const r = version ? await mcpCall('get_flow', { name, namespace: project, version }) : await flowsApi.get(name, project);
    if (!r.ok) return fail(r.error ?? 'Could not load the flow');
    const text = typeof r.data === 'string' ? r.data : r.data?.text ?? '';
    return ok((text.match(/```yaml\n([\s\S]*?)```/)?.[1] ?? text).trim() + '\n');
  });

/// Save and deploy: the platform keys the flow by its `document` namespace and name.
export const deploy = (yaml: string): Promise<Result> =>
  safely('deploy', async () => {
    const r = result(await mcpCall('create_flow', { yaml }), 'Deploy');
    return r.ok ? ok(undefined) : r;
  });

/// Through the registry's delete, which refuses a deployed flow.
export const remove = (project: string, name: string): Promise<Result> =>
  safely('delete flow', async () => {
    const r = result(await registryApi.remove(project, name), `Delete ${name}`);
    return r.ok ? ok(undefined) : r;
  });

/// The flow saved again under another name, in the same project.
export async function copy(project: string, name: string, to: string): Promise<Result> {
  const y = await getYaml(project, name);
  if (!y.ok) return y;
  return deploy(withFlowName(y.data, to));
}

/// No rename on the platform: copy, then delete the old one. Past runs stay
/// under the old name. If the delete fails the copy exists, and the error says so.
export async function rename(project: string, name: string, to: string): Promise<Result> {
  const c = await copy(project, name, to);
  if (!c.ok) return c;
  const d = await remove(project, name);
  return d.ok ? d : fail(`Copied to ${to}, but ${name} could not be removed: ${d.error}`);
}

/// The versions the registry keeps of a flow, and the active one (what runs
/// without a pin).
export const versions = (project: string, name: string): Promise<Result<{ versions: string[]; active: string }>> =>
  safely('versions', async () => {
    const r = result<any>(await releasesApi.versions(project, name), 'Versions');
    if (!r.ok) return r;
    const d = r.data?.result ?? r.data ?? {};
    const vs = (Array.isArray(d.versions) ? d.versions : []).map((v: any) => String(typeof v === 'object' ? v.version ?? '' : v)).filter(Boolean);
    return ok({ versions: vs, active: String(d.active ?? '') });
  });

/// Make `version` the active one (roll back or forward).
export const activate = (project: string, name: string, version: string): Promise<Result> =>
  safely('activate', async () => {
    const r = result(await releasesApi.activate(project, name, version), `Activate ${version}`);
    return r.ok ? ok(undefined) : r;
  });
