/// Operating: runtimes and what is deployed on them (the deployment ledger),
/// canvases' placement and deploy.
import { canvasApi, opsApi } from '$lib/services/api';
import { ok, type Result } from '../items';
import { result, safely } from './util';

export type DeploymentRow = {
  deployment: {
    id: string; kind: string; target: string; flowRef?: string; agentRef?: string; canvas?: string;
    runtime: string; namespace: string; mode: string; state: string;
    pausedBy?: string; pausedAt?: string; pauseReason?: string; deployedBy: string; deployedAt: string; updatedAt: string;
  };
  serving: string;
  callersGetWhenPaused?: unknown;
  stats?: Record<string, unknown>;
};
export type RuntimeEntry = {
  runtime: { name: string; namespace: string; labels?: Record<string, string>; url?: string | null; default?: boolean; state: string };
  state: string;
  pausedResponse?: unknown;
  deployments: DeploymentRow[];
  host?: Record<string, unknown>;
  canControl?: boolean;
};
export type Overview = { runtimes: RuntimeEntry[]; unplaced: DeploymentRow[]; host?: Record<string, unknown>; controller?: unknown };

/// Every runtime with its deployments, live activity and host metrics.
export const overview = (): Promise<Result<Overview>> =>
  safely('runtimes', async () => {
    const r = result<any>(await opsApi.overview(), 'Runtimes');
    if (!r.ok) return r;
    return ok({ runtimes: r.data?.runtimes ?? [], unplaced: r.data?.unplaced ?? [], host: r.data?.host, controller: r.data?.controller });
  });

/// All deployment rows, placed or not.
export const rowsOf = (o: Overview): DeploymentRow[] => [...o.runtimes.flatMap((r) => r.deployments), ...o.unplaced];

export type DeploymentAction = 'pause' | 'resume' | 'restart' | 'undeploy';
export const deployment = (id: string, action: DeploymentAction, reason?: string): Promise<Result<unknown>> =>
  safely(action, async () => result(await opsApi.deployment(id, action, reason), `${action} deployment`));

/// Admin only; the server says so otherwise.
export type RuntimeAction = 'hibernate' | 'pause' | 'resume';
export const runtime = (name: string, action: RuntimeAction): Promise<Result<unknown>> =>
  safely(action, async () => result(await opsApi.runtime(name, action), `${action} runtime`));

/// Terminate a runtime: its runs are cancelled, it is removed, and what was
/// deployed on it is rescheduled. `plan` says what that would do, changing nothing.
export const terminatePlan = (name: string): Promise<Result<any>> =>
  safely('terminate plan', async () => result(await opsApi.terminatePlan(name), 'Terminate plan'));
export const terminate = (name: string): Promise<Result<any>> =>
  safely('terminate', async () => result(await opsApi.terminate(name), 'Terminate runtime'));

/// Where a flow would be placed, and why (changes nothing).
export const placement = (ns: string, name: string): Promise<Result<any>> =>
  safely('placement', async () => result(await canvasApi.placement(ns, name), 'Placement'));

/// Deploy a flow with its canvas (CE: logical placement; EE: the controller).
export const deploy = (ns: string, name: string): Promise<Result<any>> =>
  safely('deploy', async () => result(await canvasApi.deploy(ns, name), 'Deploy'));

export const createCanvas = (project: string, name: string, spec: unknown): Promise<Result> =>
  safely('create canvas', async () => {
    const r = result(await canvasApi.create(project, name, spec), `Create ${name}`);
    return r.ok ? ok(undefined) : r;
  });
