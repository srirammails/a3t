/// What the editor's graph and the Runs panel share about one flow: your
/// breakpoints (kept per flow in this browser) and the run shown in the Runs
/// panel — each node's state in it and where it stands — so the graph can
/// mark breakpoints and the run's position, and its context menu can drive the
/// run (console spec §5).
import { runsApi, type Breakpoint } from '$lib/services/api';

/// A node's state in the shown run.
export type StepMark = 'done' | 'failed' | 'running' | 'waiting' | 'paused';

/// A run's step key as the editor names the node: loop and branch indices,
/// revisits and `try`/`catch`/`for`/`fork` scopes left out
/// (`/loop/for[3]/send#2` → `/loop/send`) — the engine's `node_key`.
export function nodeKey(key: string): string {
  let out = '';
  for (const raw of key.split('/')) {
    if (!raw) continue;
    const seg = raw.split('#')[0];
    const base = seg.split('[')[0];
    if (seg.includes('[') && (base === 'for' || base === 'fork')) continue;
    if (seg === 'try' || seg === 'catch') continue;
    out += `/${seg}`;
  }
  return out;
}

const RANK: Record<StepMark, number> = { done: 1, waiting: 2, failed: 3, running: 4, paused: 5 };
const LIVE = ['pending', 'running', 'waiting', 'suspended'];

export class FlowDebug {
  readonly ns: string;
  readonly name: string;
  breakpoints = $state<Breakpoint[]>([]);
  /// The run shown in the Runs panel, or null.
  runId = $state<string | null>(null);
  phase = $state('');
  debug = $state(false);
  /// Node key → its state in that run (the most telling over all iterations).
  marks = $state<Record<string, StepMark>>({});
  /// The node the run is paused at, or running, while it is live.
  current = $state<string | null>(null);
  /// A node picked on the graph for the Runs panel to show.
  focus = $state<string | null>(null);

  constructor(ns: string, name: string) {
    this.ns = ns;
    this.name = name;
    try { this.breakpoints = JSON.parse(localStorage.getItem(this.storeKey) ?? '[]') ?? []; } catch { this.breakpoints = []; }
  }

  get storeKey() { return `a3t.run.breakpoints.${this.ns}/${this.name}`; }
  get live() { return LIVE.includes(this.phase); }
  /// A debug run stopped at a breakpoint or step: the graph can drive it.
  get paused() { return this.debug && this.phase === 'suspended'; }

  has(key: string) { return this.breakpoints.some((b) => b.step_key === key); }

  save() {
    try { localStorage.setItem(this.storeKey, JSON.stringify(this.breakpoints)); } catch { /* private window */ }
  }

  /// Set or clear a breakpoint; a live debug run picks the change up at once.
  async toggle(key: string, when: Breakpoint['when'] = 'before'): Promise<string | null> {
    const at = this.breakpoints.find((b) => b.step_key === key);
    if (at && at.when === when) this.breakpoints = this.breakpoints.filter((b) => b !== at);
    else if (at) at.when = when;
    else this.breakpoints = [...this.breakpoints, { step_key: key, when }];
    return this.changed();
  }

  async clear(): Promise<string | null> {
    this.breakpoints = [];
    return this.changed();
  }

  async changed(): Promise<string | null> {
    this.save();
    if (!(this.runId && this.debug && this.live)) return null;
    const r = await runsApi.setBreakpoints(this.runId, $state.snapshot(this.breakpoints));
    return r.ok ? null : r.error ?? 'Could not update the run';
  }

  /// Publish the shown run (from `get_run`), or nothing.
  show(run: any | null) {
    if (!run) {
      this.runId = null; this.phase = ''; this.debug = false; this.marks = {}; this.current = null;
      return;
    }
    const marks: Record<string, StepMark> = {};
    const put = (key: string, m: StepMark) => {
      const k = nodeKey(key);
      if (!marks[k] || RANK[m] > RANK[marks[k]]) marks[k] = m;
    };
    let running: string | null = null;
    for (const s of run.steps ?? []) {
      const m: StepMark | null = s.status === 'completed' ? 'done' : s.status === 'failed' ? 'failed'
        : s.status === 'started' ? 'running' : s.status === 'waiting' ? 'waiting' : null;
      if (!m) continue;
      put(s.step_key, m);
      if (m === 'running' || m === 'waiting') running = nodeKey(s.step_key);
    }
    const paused = run.phase === 'suspended' ? run.paused?.step_key : null;
    if (paused) put(paused, 'paused');
    this.runId = run.run_id ?? null;
    this.phase = run.phase ?? '';
    this.debug = !!run.debug;
    this.marks = marks;
    this.current = paused ? nodeKey(paused) : LIVE.includes(run.phase) ? running : null;
  }
}

const flows = new Map<string, FlowDebug>();

/// The one shared state for a flow.
export function flowDebug(ns: string, name: string): FlowDebug {
  const id = `${ns}/${name}`;
  let fd = flows.get(id);
  if (!fd) flows.set(id, (fd = new FlowDebug(ns, name)));
  return fd;
}
