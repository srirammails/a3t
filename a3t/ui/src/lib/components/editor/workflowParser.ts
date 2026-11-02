/**
 * workflowParser.ts — CNCF Serverless Workflow graph layout engine
 *
 * Parses workflow YAML (flowr steps[] or CNCF DSL do[] format) and
 * produces xyflow Node[] + Edge[] with recursive layout for:
 *   switch (conditional branches), fork (parallel), for (loops),
 *   try/catch (error handling), do (nested sequences), and leaf tasks.
 */

import type { Node, Edge } from '@xyflow/svelte';

// ── Layout constants ──────────────────────────────────────────────
const NODE_W = 210;
const NODE_H = 64;
const GAP_V  = 48;          // vertical gap between nodes
const GAP_H  = 50;          // horizontal gap between branch columns
const COL_W  = NODE_W + GAP_H;  // one column width
const JOIN_SIZE = 30;
const AGENT_NODE_W = 160;
const AGENT_NODE_H = 140;
const SECTION_NODE_W = 200;
const SECTION_NODE_H = 80;
const SECTION_GAP = 20;

// ── Internal task shape (normalised from both formats) ───────────
export interface NTask {
  name: string;
  type: string;            // e.g. 'set', 'call:http', 'switch', 'fork', 'for', 'try', 'do', 'wait', 'raise', 'emit', 'listen', 'run:shell', etc.
  config?: any;
  if?: string;
  timeout?: string | number;
  resource?: string;
  checkpoint?: boolean;
  'input.from'?: string;
  'output.as'?: string;
  'export.as'?: string;
  then?: string;
  metadata?: any;
  /// The step key the engine reports and breakpoints name (`/loop/send`):
  /// parent names joined, loop / branch / try scopes left out.
  key?: string;
}

type Category = 'switch' | 'fork' | 'for' | 'try' | 'do' | 'leaf';

function category(t: string): Category {
  if (t === 'switch') return 'switch';
  if (t === 'fork')   return 'fork';
  if (t === 'for')    return 'for';
  if (t === 'try')    return 'try';
  if (t === 'do')     return 'do';
  return 'leaf';
}

// ── Format detection & normalisation ─────────────────────────────

/** Detect whether parsed YAML is CNCF standard DSL (do: [{name: {call:…}}]) */
export function isCncfDsl(parsed: any): boolean {
  if (parsed?.document?.dsl) return true;
  if (parsed?.do && !parsed?.steps) {
    const first = parsed.do[0];
    if (first && typeof first === 'object' && !first.name && !first.type) return true;
  }
  return false;
}

/** Normalise a single CNCF DSL entry like { taskName: { call: http, with: {…} } } */
function normaliseCncfEntry(entry: any): NTask {
  // Already in flowr format
  if (entry.name && entry.type) return entry as NTask;

  const keys = Object.keys(entry);
  const name = keys[0];
  const def  = entry[name];
  if (!def || typeof def !== 'object') return { name, type: 'set', config: def };

  const common: Partial<NTask> = {};
  if (def.if)          common.if = def.if;
  if (def.timeout)     common.timeout = def.timeout;
  if (def.input?.from) common['input.from'] = def.input.from;
  if (def.output?.as)  common['output.as'] = def.output.as;
  if (def.export?.as)  common['export.as'] = def.export.as;
  if (def.then)        common.then = def.then;
  if (def.metadata)    common.metadata = def.metadata;

  // call: http / grpc / openapi / asyncapi / a2a / custom
  if (def.call !== undefined) {
    return { name, type: `call:${def.call}`, config: def.with ?? {}, resource: def.resource, ...common };
  }
  // set: { key: val }
  if (def.set !== undefined) {
    return { name, type: 'set', config: def.set, ...common };
  }
  // do: [subtasks]
  if (def.do !== undefined) {
    return { name, type: 'do', config: normaliseTasks(def.do), ...common };
  }
  // switch: [cases]
  if (def.switch !== undefined) {
    const cases = (def.switch ?? []).map((c: any) => {
      if (c.then && typeof c.then === 'string') {
        // CNCF goto-style — keep as label reference
        return { when: c.if ?? c.when, then: [], goto: c.then, isDefault: !c.if && !c.when };
      }
      const thenTasks = Array.isArray(c.then) ? normaliseTasks(c.then) : [];
      return { when: c.if ?? c.when, then: thenTasks, isDefault: !c.if && !c.when };
    });
    return { name, type: 'switch', config: cases, ...common };
  }
  // for: { each, in, do }
  if (def.for !== undefined) {
    const forDef = typeof def.for === 'object' ? def.for : {};
    return { name, type: 'for', config: { each: forDef.each ?? def.each, in: forDef.in ?? def.in, at: forDef.at ?? def.at, do: normaliseTasks(forDef.do ?? def.do ?? []) }, ...common };
  }
  // fork: { branches }
  if (def.fork !== undefined) {
    const forkDef = typeof def.fork === 'object' ? def.fork : {};
    // CNCF branches are named tasks (`- a: { do: [...] }`); a branch that is a
    // single task is a one-step branch.
    const branches = (forkDef.branches ?? def.branches ?? []).map((b: any) => {
      if (b?.do && b.name) return { name: b.name, do: normaliseTasks(b.do) };
      const name = Object.keys(b ?? {})[0] ?? 'branch';
      const inner = b?.[name];
      return { name, do: Array.isArray(inner?.do) ? normaliseTasks(inner.do) : normaliseTasks([b]) };
    });
    return { name, type: 'fork', config: { branches }, ...common };
  }
  // try: { try, catch }
  if (def.try !== undefined) {
    // `catch:` is one block (`{ errors, do }`); older flows list several.
    // The body is a list (CNCF) or `{ do, catch }` (FlowR); either may hold the catch.
    const body = Array.isArray(def.try) ? def.try : def.try?.do ?? [];
    const katch = def.catch ?? def.try?.catch;
    const catches = (Array.isArray(katch) ? katch : katch ? [katch] : [])
      .map((c: any) => ({ ...c, errors: c?.errors?.with ? [c.errors.with.type ?? 'error'] : c?.errors, do: normaliseTasks(c?.do ?? []) }));
    return { name, type: 'try', config: { do: normaliseTasks(body), catch: catches }, ...common };
  }
  // wait
  if (def.wait !== undefined) {
    const dur = typeof def.wait === 'string' ? def.wait : def.wait?.duration;
    return { name, type: 'wait', config: { duration: dur }, ...common };
  }
  // raise
  if (def.raise !== undefined) {
    return { name, type: 'raise', config: { error: def.raise?.error ?? def.raise }, ...common };
  }
  // emit
  if (def.emit !== undefined) {
    return { name, type: 'emit', config: { event: def.emit?.event ?? def.emit }, ...common };
  }
  // listen
  if (def.listen !== undefined) {
    return { name, type: 'listen', config: def.listen, ...common };
  }
  // run: shell / script / workflow / container
  if (def.run !== undefined) {
    // `run:` holds one of container / script / shell / workflow (CNCF).
    const runType = typeof def.run === 'string' ? def.run
      : (['workflow', 'container', 'script', 'shell'].find((k) => def.run && k in def.run) ?? 'shell');
    // The form edits the process's own fields (run.workflow.name…).
    const runCfg = def.with ?? (def.run && typeof def.run === 'object' && def.run[runType] && typeof def.run[runType] === 'object' ? def.run[runType] : def.run);
    return { name, type: `run:${runType}`, config: runCfg, ...common };
  }

  // Fallback: treat as set with the whole def as config
  return { name, type: 'set', config: def, ...common };
}

export function normaliseTasks(raw: any[]): NTask[] {
  if (!Array.isArray(raw)) return [];
  return raw.map(entry => {
    if (entry.name && entry.type) return entry as NTask;  // flowr format
    return normaliseCncfEntry(entry);
  });
}

/// Give every task its engine step key (see `NTask.key`).
function assignKeys(tasks: NTask[], prefix = ''): void {
  for (const t of Array.isArray(tasks) ? tasks : []) {
    if (!t || typeof t !== 'object') continue;
    const key = `${prefix}/${t.name}`;
    t.key = key;
    const cat = category(t.type);
    if (cat === 'do') assignKeys(Array.isArray(t.config) ? t.config : [], key);
    else if (cat === 'for') assignKeys(t.config?.do ?? [], key);
    else if (cat === 'try') {
      assignKeys(t.config?.do ?? [], key);
      for (const c of t.config?.catch ?? []) assignKeys(c?.do ?? [], key);
    } else if (cat === 'fork') for (const b of t.config?.branches ?? []) assignKeys(b?.do ?? [], key);
    else if (cat === 'switch') for (const c of Array.isArray(t.config) ? t.config : []) assignKeys(c?.then ?? [], key);
  }
}

// ── Graph builder ────────────────────────────────────────────────
class GraphBuilder {
  nodes: Node[] = [];
  edges: Edge[] = [];
  private counter = 0;

  id(prefix: string): string {
    return `${prefix}-${this.counter++}`;
  }

  node(id: string, type: string, x: number, y: number, data: any) {
    this.nodes.push({ id, type, position: { x, y }, data, draggable: true });
  }

  edge(source: string, target: string, opts: Partial<Edge> = {}) {
    // An agent's next step leaves from its output, not its error or extension connector.
    if (!opts.sourceHandle && this.nodes.find((n) => n.id === source)?.type === 'agent') opts = { ...opts, sourceHandle: 'out' };
    if (!opts.targetHandle && this.nodes.find((n) => n.id === target)?.type === 'agent') opts = { ...opts, targetHandle: 'in' };
    this.edges.push({
      id: `e-${source}-${target}-${this.counter++}`,
      source,
      target,
      type: 'smoothstep',
      animated: false,
      ...opts,
    });
  }
}

// ── Layout result ────────────────────────────────────────────────
interface LR {
  entryId: string;
  exitIds: string[];
  bottom: number;
  left: number;
  right: number;
}

// ── Width in columns (pre-pass) ─────────────────────────────────
function colWidth(tasks: NTask[]): number {
  let w = 1;
  for (const t of tasks) {
    const cat = category(t.type);
    if (cat === 'switch') {
      const cases: any[] = Array.isArray(t.config) ? t.config : [];
      let total = 0;
      for (const c of cases) {
        const steps = c.then ?? (c.default ? c.default : []);
        total += colWidth(Array.isArray(steps) ? steps : []);
      }
      w = Math.max(w, Math.max(total, 1));
    } else if (cat === 'fork') {
      const branches: any[] = t.config?.branches ?? [];
      let total = 0;
      for (const b of branches) total += colWidth(b.do ?? []);
      w = Math.max(w, Math.max(total, 1));
    } else if (cat === 'for') {
      w = Math.max(w, colWidth(t.config?.do ?? []));
    } else if (cat === 'try') {
      const tw = colWidth(t.config?.do ?? []);
      const catches: any[] = t.config?.catch ?? [];
      let cw = 0;
      for (const c of catches) cw = Math.max(cw, colWidth(c.do ?? []));
      w = Math.max(w, tw + (cw > 0 ? cw : 0));
    } else if (cat === 'do') {
      w = Math.max(w, colWidth(Array.isArray(t.config) ? t.config : []));
    }
  }
  return w;
}

// ── Recursive layout ─────────────────────────────────────────────

function layoutSequence(g: GraphBuilder, tasks: NTask[], centerX: number, startY: number): LR | null {
  if (tasks.length === 0) return null;

  let y = startY;
  let entryId = '';
  let prevExitIds: string[] = [];
  let gLeft  = centerX - NODE_W / 2;
  let gRight = centerX + NODE_W / 2;

  for (let i = 0; i < tasks.length; i++) {
    const task = tasks[i];
    const cat  = category(task.type);
    let r: LR | null = null;

    switch (cat) {
      case 'leaf':   r = layoutLeaf(g, task, centerX, y);   break;
      case 'switch': r = layoutSwitch(g, task, centerX, y); break;
      case 'fork':   r = layoutFork(g, task, centerX, y);   break;
      case 'for':    r = layoutFor(g, task, centerX, y);     break;
      case 'try':    r = layoutTry(g, task, centerX, y);     break;
      case 'do':     r = layoutDo(g, task, centerX, y);      break;
    }

    if (!r) continue;
    // The task's own node (leaf, or the header of a block) carries its key.
    const own = g.nodes.find((n) => n.id === r!.entryId);
    if (own && task.key) own.data = { ...own.data, stepKey: task.key };

    // Connect previous → current entry
    for (const pid of prevExitIds) {
      g.edge(pid, r.entryId);
    }

    if (!entryId) entryId = r.entryId;
    prevExitIds = r.exitIds;
    y = r.bottom + GAP_V;
    gLeft  = Math.min(gLeft, r.left);
    gRight = Math.max(gRight, r.right);
  }

  return { entryId, exitIds: prevExitIds, bottom: y - GAP_V, left: gLeft, right: gRight };
}

function isAgentCall(type: string): boolean {
  return type === 'call:a3t:agent' || type === 'call:agent' || type === 'call:a3t:agent-engine';
}

const CHAT_FLOWS = new Set(['a3t-chat-engine', 'a3t-chat-agent']);

/// What the graph shows of an agent step: a `call: a3t:agent`, or a run of an
/// agent's chat flow (an agent dropped from the Explorer). Null for other steps.
function agentOf(task: NTask): Record<string, unknown> | null {
  if (isAgentCall(task.type)) {
    const cfg = task.config ?? {};
    const inner = typeof cfg.config === 'object' && cfg.config ? cfg.config : cfg;
    return {
      kindLabel: 'agent', version: '', model: inner.llm_resource ?? '',
      extensionCount: (cfg._extensions ?? []).length, errorCount: 0,
    };
  }
  // `run:workflow` carries `run.workflow` as its config.
  const wf = task.type === 'run:workflow' ? (task.config?.workflow ?? task.config) : null;
  if (wf && CHAT_FLOWS.has(wf.name) && wf.input && typeof wf.input === 'object' && 'agent_id' in wf.input) {
    return { kindLabel: 'agent', version: wf.version && wf.version !== 'latest' ? `v${wf.version}` : 'latest', model: '', extensionCount: 0, errorCount: 0 };
  }
  return null;
}

function layoutLeaf(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  const id = g.id('s');

  // An agent in a flow is one unit: a square block with its connectors —
  // input, output, errors, extensions — that the steps around it join to.
  const agent = agentOf(task);
  if (agent) {
    const x = cx - AGENT_NODE_W / 2;
    g.node(id, 'agent', x, y, { label: task.name, ...agent, detailed: false });
    return { entryId: id, exitIds: [id], bottom: y + AGENT_NODE_H, left: x, right: x + AGENT_NODE_W };
  }

  // Regular workflow node with bottom ports
  const x = cx - NODE_W / 2;
  const bottomPorts: any[] = [];

  // Add resource as closed coupling port
  if (task.resource) {
    bottomPorts.push({ id: `bp-res-${id}`, label: task.resource, kind: 'closed' });
  }

  g.node(id, 'workflow', x, y, {
    label:      task.name,
    callType:   task.type,
    subtitle:   task.resource ?? '',
    checkpoint: task.checkpoint ?? false,
    hasIf:      !!task.if,
    hasError:   task.type === 'raise' || !!task.metadata?.error,
    bottomPorts,
  });
  return { entryId: id, exitIds: [id], bottom: y + NODE_H, left: x, right: x + NODE_W };
}

function layoutSwitch(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  const switchId = g.id('sw');
  const x = cx - NODE_W / 2;

  const cases: any[] = Array.isArray(task.config) ? task.config : [];
  const branches = cases.map(c => {
    const steps: NTask[] = c.then ?? (c.default !== undefined ? c.default : []);
    const label = c.when ?? (c.default !== undefined || c.isDefault ? 'default' : '?');
    return { steps: Array.isArray(steps) ? steps : [], label: String(label), isDefault: c.default !== undefined || c.isDefault };
  });

  // Ensure at least one branch to avoid empty layout
  if (branches.length === 0) {
    g.node(switchId, 'control', x, y, { label: task.name, taskType: 'switch', branchCount: 0, hasIf: !!task.if });
    return { entryId: switchId, exitIds: [switchId], bottom: y + NODE_H, left: x, right: x + NODE_W };
  }

  g.node(switchId, 'control', x, y, {
    label: task.name, taskType: 'switch', branchCount: branches.length, hasIf: !!task.if,
  });

  const branchY = y + NODE_H + GAP_V;

  // Column widths per branch
  const bWidths = branches.map(b => Math.max(1, colWidth(b.steps)));
  const totalCols = bWidths.reduce((a, b) => a + b, 0);
  const totalPx   = totalCols * COL_W - GAP_H; // remove trailing gap

  let bx = cx - totalPx / 2;
  let maxBottom = branchY;
  const exitIds: string[] = [];
  let gLeft  = x;
  let gRight = x + NODE_W;

  for (let i = 0; i < branches.length; i++) {
    const br = branches[i];
    const bCenterX = bx + (bWidths[i] * COL_W - GAP_H) / 2 + NODE_W / 2;

    if (br.steps.length > 0) {
      const r = layoutSequence(g, br.steps, bCenterX, branchY);
      if (r) {
        const edgeLabel = br.isDefault ? 'default' : truncExpr(br.label);
        g.edge(switchId, r.entryId, {
          label: edgeLabel,
          animated: true,
          style: br.isDefault ? 'stroke-dasharray: 5 5;' : undefined,
        });
        exitIds.push(...r.exitIds);
        maxBottom = Math.max(maxBottom, r.bottom);
        gLeft  = Math.min(gLeft, r.left);
        gRight = Math.max(gRight, r.right);
      }
    } else {
      // Empty branch — add a pass-through marker
      const passId = g.id('pass');
      g.node(passId, 'step', bCenterX - NODE_W / 2, branchY, {
        label: br.isDefault ? '(default)' : '(empty)', callType: 'pass', server: '',
      });
      g.edge(switchId, passId, { label: br.isDefault ? 'default' : truncExpr(br.label), animated: true });
      exitIds.push(passId);
      maxBottom = Math.max(maxBottom, branchY + NODE_H);
      gLeft  = Math.min(gLeft, bCenterX - NODE_W / 2);
      gRight = Math.max(gRight, bCenterX + NODE_W / 2);
    }

    bx += bWidths[i] * COL_W;
  }

  // Join node
  if (exitIds.length > 1) {
    const jy = maxBottom + GAP_V;
    const jId = g.id('j');
    g.node(jId, 'join', cx - JOIN_SIZE / 2, jy, {});
    for (const eid of exitIds) g.edge(eid, jId);
    return { entryId: switchId, exitIds: [jId], bottom: jy + JOIN_SIZE, left: gLeft, right: gRight };
  }

  return { entryId: switchId, exitIds: exitIds.length ? exitIds : [switchId], bottom: maxBottom, left: gLeft, right: gRight };
}

function layoutFork(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  const forkId = g.id('fk');
  const x = cx - NODE_W / 2;

  const branches: any[] = task.config?.branches ?? [];
  if (branches.length === 0) {
    g.node(forkId, 'control', x, y, { label: task.name, taskType: 'fork', branchCount: 0 });
    return { entryId: forkId, exitIds: [forkId], bottom: y + NODE_H, left: x, right: x + NODE_W };
  }

  g.node(forkId, 'control', x, y, {
    label: task.name, taskType: 'fork', branchCount: branches.length,
  });

  const branchY = y + NODE_H + GAP_V;
  const bWidths = branches.map((b: any) => Math.max(1, colWidth(b.do ?? [])));
  const totalCols = bWidths.reduce((a: number, b: number) => a + b, 0);
  const totalPx   = totalCols * COL_W - GAP_H;

  let bx = cx - totalPx / 2;
  let maxBottom = branchY;
  const exitIds: string[] = [];
  let gLeft = x, gRight = x + NODE_W;

  for (let i = 0; i < branches.length; i++) {
    const br = branches[i];
    const steps: NTask[] = br.do ?? [];
    const bCenterX = bx + (bWidths[i] * COL_W - GAP_H) / 2 + NODE_W / 2;

    if (steps.length > 0) {
      const r = layoutSequence(g, steps, bCenterX, branchY);
      if (r) {
        g.edge(forkId, r.entryId, { label: br.name, animated: true, style: 'stroke: var(--runtime, #1a7158);' });
        exitIds.push(...r.exitIds);
        maxBottom = Math.max(maxBottom, r.bottom);
        gLeft  = Math.min(gLeft, r.left);
        gRight = Math.max(gRight, r.right);
      }
    }

    bx += bWidths[i] * COL_W;
  }

  // Join
  if (exitIds.length > 1) {
    const jy = maxBottom + GAP_V;
    const jId = g.id('j');
    g.node(jId, 'join', cx - JOIN_SIZE / 2, jy, {});
    for (const eid of exitIds) g.edge(eid, jId);
    return { entryId: forkId, exitIds: [jId], bottom: jy + JOIN_SIZE, left: gLeft, right: gRight };
  }

  return { entryId: forkId, exitIds: exitIds.length ? exitIds : [forkId], bottom: maxBottom, left: gLeft, right: gRight };
}

function layoutFor(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  const forId = g.id('fr');
  const x = cx - NODE_W / 2;

  g.node(forId, 'control', x, y, {
    label:    task.name,
    taskType: 'for',
    each:     task.config?.each ?? 'item',
    inExpr:   task.config?.in ?? '',
    hasIf:    !!task.if,
  });

  const bodyTasks: NTask[] = task.config?.do ?? [];
  if (bodyTasks.length === 0) {
    return { entryId: forId, exitIds: [forId], bottom: y + NODE_H, left: x, right: x + NODE_W };
  }

  const bodyY = y + NODE_H + GAP_V;
  const r = layoutSequence(g, bodyTasks, cx, bodyY);
  if (!r) return { entryId: forId, exitIds: [forId], bottom: y + NODE_H, left: x, right: x + NODE_W };

  // for header → body entry
  g.edge(forId, r.entryId, { animated: true });

  // Loop-back edge from body exit → for header
  for (const eid of r.exitIds) {
    g.edge(eid, forId, {
      animated: true,
      style: 'stroke: var(--checkpoint, #c47028); stroke-dasharray: 6 3;',
      type: 'smoothstep',
    });
  }

  return { entryId: forId, exitIds: [forId], bottom: r.bottom, left: Math.min(x, r.left), right: Math.max(x + NODE_W, r.right) };
}

function layoutTry(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  const tryId = g.id('tr');
  const x = cx - NODE_W / 2;

  const trySteps:  NTask[] = task.config?.do ?? [];
  const catches:   any[]   = task.config?.catch ?? [];

  g.node(tryId, 'control', x, y, {
    label:      task.name,
    taskType:   'try',
    hasCatch:   catches.length > 0,
  });

  const bodyY = y + NODE_H + GAP_V;
  let gLeft = x, gRight = x + NODE_W;
  let maxBottom = y + NODE_H;
  const allExitIds: string[] = [];

  // Layout try-body (center)
  if (trySteps.length > 0) {
    const tw = Math.max(1, colWidth(trySteps));
    // If there are catch blocks, shift try body left
    const hasCatch = catches.length > 0 && catches.some((c: any) => (c.do ?? []).length > 0);
    const tryCx = hasCatch ? cx - COL_W / 2 : cx;

    const r = layoutSequence(g, trySteps, tryCx, bodyY);
    if (r) {
      g.edge(tryId, r.entryId, { label: 'try', animated: true });
      allExitIds.push(...r.exitIds);
      maxBottom = Math.max(maxBottom, r.bottom);
      gLeft  = Math.min(gLeft, r.left);
      gRight = Math.max(gRight, r.right);
    }

    // Layout catch branches (right of try body)
    if (hasCatch) {
      let catchCx = cx + COL_W / 2;
      for (const c of catches) {
        const cSteps: NTask[] = c.do ?? [];
        if (cSteps.length === 0) continue;
        const cr = layoutSequence(g, cSteps, catchCx, bodyY);
        if (cr) {
          const errLabel = Array.isArray(c.errors) ? c.errors.join(', ') : 'catch';
          g.edge(tryId, cr.entryId, {
            label: `catch: ${errLabel}`, animated: true,
            style: 'stroke: var(--err, #e53e3e); stroke-dasharray: 5 3;',
          });
          allExitIds.push(...cr.exitIds);
          maxBottom = Math.max(maxBottom, cr.bottom);
          gLeft  = Math.min(gLeft, cr.left);
          gRight = Math.max(gRight, cr.right);
        }
        catchCx += COL_W;
      }
    }
  }

  // Join if multiple exits
  if (allExitIds.length > 1) {
    const jy = maxBottom + GAP_V;
    const jId = g.id('j');
    g.node(jId, 'join', cx - JOIN_SIZE / 2, jy, {});
    for (const eid of allExitIds) g.edge(eid, jId);
    return { entryId: tryId, exitIds: [jId], bottom: jy + JOIN_SIZE, left: gLeft, right: gRight };
  }

  return { entryId: tryId, exitIds: allExitIds.length ? allExitIds : [tryId], bottom: maxBottom, left: gLeft, right: gRight };
}

function layoutDo(g: GraphBuilder, task: NTask, cx: number, y: number): LR {
  // "do" is a named grouping — place a header node, then layout children
  const doId = g.id('do');
  const x = cx - NODE_W / 2;
  g.node(doId, 'control', x, y, { label: task.name, taskType: 'do', hasIf: !!task.if });

  const subtasks: NTask[] = Array.isArray(task.config) ? task.config : [];
  if (subtasks.length === 0) {
    return { entryId: doId, exitIds: [doId], bottom: y + NODE_H, left: x, right: x + NODE_W };
  }

  const bodyY = y + NODE_H + GAP_V;
  const r = layoutSequence(g, subtasks, cx, bodyY);
  if (!r) return { entryId: doId, exitIds: [doId], bottom: y + NODE_H, left: x, right: x + NODE_W };

  g.edge(doId, r.entryId, { animated: true });
  return { entryId: doId, exitIds: r.exitIds, bottom: r.bottom, left: Math.min(x, r.left), right: Math.max(x + NODE_W, r.right) };
}

// ── Helpers ──────────────────────────────────────────────────────

/** Truncate jq expressions for edge labels */
function truncExpr(s: string): string {
  if (!s) return '';
  // Strip ${ } wrapper
  let clean = s.replace(/^\$\{\s*/, '').replace(/\s*\}$/, '').trim();
  if (clean.length > 40) clean = clean.slice(0, 37) + '…';
  return clean;
}

// ── Public API ───────────────────────────────────────────────────

export interface WorkflowGraph {
  nodes: Node[];
  edges: Edge[];
  /** Parsed & normalised document metadata */
  meta: {
    name: string;
    namespace: string;
    version: string;
    dsl?: string;
    title?: string;
    description?: string;
    hasExtensions: boolean;
    extensionCount: number;
    taskCount: number;
    format: 'flowr' | 'cncf-dsl' | 'unknown';
  };
}

/** Check if parsed YAML represents an agent template */
export function isAgentTemplateYaml(parsed: any): boolean {
  if (!parsed) return false;
  if (parsed.kind === 'AgentTemplate' || parsed.type === 'agent') return true;
  // Single-step workflow wrapping call:a3t:agent
  const tasks = parsed.do ?? parsed.steps ?? [];
  if (Array.isArray(tasks) && tasks.length === 1) {
    const entry = tasks[0];
    const def = typeof entry === 'object' ? (entry[Object.keys(entry)[0]] ?? entry) : null;
    if (def?.call === 'a3t:agent' || def?.call === 'agent' || def?.type === 'call:a3t:agent' || def?.type === 'call:agent') return true;
  }
  return false;
}

/** Extract agent config from parsed YAML (works for both CRD and workflow wrapper formats) */
function extractAgentConfig(parsed: any): any {
  // CRD format: spec.brain, spec.tools, etc.
  if (parsed.spec) return parsed.spec;
  // Workflow wrapper: first task's call.with
  const tasks = parsed.do ?? parsed.steps ?? [];
  if (Array.isArray(tasks) && tasks.length >= 1) {
    const entry = tasks[0];
    const key = Object.keys(entry)[0];
    const def = entry[key];
    if (def?.with) return def.with;
    if (def?.config) return def.config;
  }
  return parsed.config ?? parsed;
}

export function parseWorkflowToGraph(parsed: any): WorkflowGraph {
  const empty: WorkflowGraph = {
    nodes: [], edges: [],
    meta: { name: '', namespace: '', version: '', hasExtensions: false, extensionCount: 0, taskCount: 0, format: 'unknown' },
  };

  if (!parsed || typeof parsed !== 'object') return empty;

  // Extensions from use block
  const extensions = parsed.use?.extensions ?? [];

  // ── An agent template opened on its own: the agent block, and each of its
  //    connectors as a node — input, output, every error, every extension. ──
  if (isAgentTemplateYaml(parsed)) {
    const g = new GraphBuilder();
    const agentCfg = extractAgentConfig(parsed);
    const cfgInner = agentCfg.config ?? agentCfg;
    const name = parsed.metadata?.name ?? parsed.document?.name ?? parsed.name ?? 'agent';
    const version = parsed.document?.version ?? parsed.version ?? '';
    const errors = parsed.use?.errors && typeof parsed.use.errors === 'object' ? Object.entries<any>(parsed.use.errors) : [];
    const exts = (Array.isArray(extensions) ? extensions : []).map((e: any, i: number) => {
      const n = e && typeof e === 'object' ? (e.name ?? Object.keys(e)[0] ?? `ext-${i}`) : `ext-${i}`;
      const def = e && typeof e === 'object' ? (e.name ? e : e[n] ?? {}) : {};
      return { name: String(n), extend: def.extend ?? 'all' };
    });

    const CX = 360, AY = 120;
    const agentId = g.id('agent');
    g.node(agentId, 'agent', CX - AGENT_NODE_W / 2, AY, {
      label: name, version: version ? `v${version}` : '', kindLabel: 'agent template', detailed: true,
      model: typeof cfgInner === 'object' ? (cfgInner.llm_resource ?? cfgInner.brain?.llmResourceRef ?? '') : '',
      errorCount: errors.length, extensionCount: exts.length,
    });
    const port = (port: string, label: string, sub: string, detail: string, x: number, y: number, section: string) => {
      const pid = g.id(port);
      g.node(pid, 'agentPort', x, y, { port, label, sub, detail, _portChild: true, _portSection: section });
      return pid;
    };
    const keysOf = (sch: any) => Object.keys(sch?.document?.properties ?? sch?.properties ?? {});
    const SQ = AY + 30;  // the square's top, under the label
    if (parsed.input) {
      const ks = keysOf(parsed.input.schema);
      const pid = port('input', 'input', ks.length ? ks.join(', ') : (parsed.input.from ? String(parsed.input.from) : 'schema'), 'What the agent is started with (input:)', CX - AGENT_NODE_W / 2 - 230, SQ + 30, 'identity');
      g.edge(pid, agentId, { targetHandle: 'in', style: 'stroke: #0891b2;' });
    }
    if (parsed.output) {
      const ks = keysOf(parsed.output.schema);
      const pid = port('output', 'output', ks.length ? ks.join(', ') : (parsed.output.as ? String(parsed.output.as) : 'schema'), 'What the agent returns (output:)', CX + AGENT_NODE_W / 2 + 80, SQ, 'identity');
      g.edge(agentId, pid, { sourceHandle: 'out', style: 'stroke: #059669;' });
    }
    errors.forEach(([k, e], i) => {
      const sub = [e?.status, e?.title].filter(Boolean).join(' · ') || String(e?.type ?? '').split('/').pop() || 'error';
      const pid = port('error', k, sub, `use.errors.${k}${e?.type ? ` — ${e.type}` : ''}`, CX + AGENT_NODE_W / 2 + 80, SQ + 60 + i * 52, 'extensions');
      g.edge(agentId, pid, { sourceHandle: 'err', style: 'stroke: #dc2626; stroke-dasharray: 4 3;' });
    });
    // Extensions under the agent, three to a row.
    const extW = 200, PER = 3;
    exts.forEach((x, i) => {
      const inRow = Math.min(PER, exts.length - Math.floor(i / PER) * PER);
      const col = i % PER, row = Math.floor(i / PER);
      const pid = port('extension', x.name, `extend: ${x.extend}`, `use.extensions.${x.name}`, CX - (inRow * extW) / 2 + col * extW + 10, AY + AGENT_NODE_H + 70 + row * 64, 'extensions');
      g.edge(agentId, pid, { sourceHandle: 'ext', targetHandle: 'in', style: 'stroke: #b45309; stroke-dasharray: 4 3;' });
    });

    return {
      nodes: g.nodes, edges: g.edges,
      meta: {
        name, namespace: parsed.document?.namespace ?? 'system',
        version, dsl: parsed.document?.dsl, hasExtensions: exts.length > 0,
        extensionCount: exts.length, taskCount: 1, format: 'cncf-dsl',
      },
    };
  }

  // ── Regular workflow parsing ──
  const isCncf = isCncfDsl(parsed);
  const rawTasks = parsed.steps ?? parsed.do ?? [];
  if (!Array.isArray(rawTasks) || rawTasks.length === 0) return empty;

  const tasks: NTask[] = isCncf ? normaliseTasks(rawTasks) : rawTasks;

  // Inject extensions into agent call tasks so they can access them
  if (extensions.length > 0) {
    for (const t of tasks) {
      if (isAgentCall(t.type) && t.config) {
        t.config._extensions = extensions;
      }
    }
  }

  assignKeys(tasks);
  const g = new GraphBuilder();

  const startId = g.id('start');
  const startX  = 300;
  g.node(startId, 'flowMarker', startX - 20, 10, { label: parsed.name ?? parsed.document?.name ?? 'start', marker: 'start' });

  const result = layoutSequence(g, tasks, startX, 80);

  if (result) {
    g.edge(startId, result.entryId, { animated: true });
    const endId = g.id('end');
    g.node(endId, 'flowMarker', startX - 20, result.bottom + GAP_V, { label: 'end', marker: 'end' });
    for (const eid of result.exitIds) {
      g.edge(eid, endId, { animated: true });
    }
  }

  return {
    nodes: g.nodes,
    edges: g.edges,
    meta: {
      name:        parsed.name ?? parsed.document?.name ?? '',
      namespace:   parsed.namespace ?? parsed.document?.namespace ?? 'default',
      version:     parsed.version ?? parsed.document?.version ?? '',
      dsl:         parsed.dsl ?? parsed.document?.dsl,
      title:       parsed.title,
      description: parsed.description,
      hasExtensions:  extensions.length > 0,
      extensionCount: extensions.length,
      taskCount:      tasks.length,
      format:         isCncf ? 'cncf-dsl' : (parsed.steps ? 'flowr' : 'unknown'),
    },
  };
}

// ── Port expansion — spawn sub-nodes below a port ───────────────

interface ExpandResult {
  nodes: Node[];
  edges: Edge[];
}

/**
 * Expand a bottom port — spawn sub-nodes below the parent node.
 * Each port section (Config, Extensions, Integrations) produces its own sub-nodes
 * connected from the parent's bottom port handle.
 */
export function expandPort(
  parentId: string,
  parentPos: { x: number; y: number },
  parentData: any,
  portId: string,
): ExpandResult {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  let counter = 0;
  const sid = (prefix: string) => `${parentId}-${prefix}-${counter++}`;

  // Position card centered below the parent toolbar (one at a time)
  const CARD_W = 240;
  const subY = parentPos.y + AGENT_NODE_H + 60;
  const baseX = parentPos.x + (AGENT_NODE_W / 2) - (CARD_W / 2);

  if (portId === 'bp-config') {
    const cfg = parentData.agentConfig?.config ?? parentData.agentConfig ?? {};
    const nId = sid('cfg-card');
    nodes.push({
      id: nId, type: 'portCard', position: { x: baseX - CARD_W / 2, y: subY },
      data: {
        label: 'Config Schema', section: 'config', _portChild: true,
        addLabel: '+ Add Field',
        items: [
          { name: 'model', type: 'string' },
          { name: 'temperature', type: 'number' },
          { name: 'max_tokens', type: 'number' },
          { name: 'tools', type: 'array' },
          { name: 'system_prompt', type: 'string' },
          { name: 'max_iterations', type: 'number' },
          { name: 'memory_namespace', type: 'string' },
          { name: 'llm_resource', type: 'string' },
          { name: 'mcp_servers', type: 'array' },
          { name: 'tool_flows', type: 'object' },
          { name: 'memory_config', type: 'object' },
          { name: 'ttl', type: 'number' },
        ],
      },
      draggable: false,
    });
    // No connector edge — card appears directly below toolbar

  } else if (portId === 'bp-integrations') {
    const mcps: any[] = parentData.mcpServers ?? [];
    const toolFlows: Record<string, any> = parentData.toolFlows ?? {};
    const intgItems: any[] = [];
    if (parentData.model) intgItems.push({ name: parentData.model || 'LLM', type: 'LLM', typeColor: '#8b5cf6' });
    for (const mcp of mcps) {
      const ref = typeof mcp === 'string' ? mcp : mcp?.ref ?? 'mcp';
      intgItems.push({ name: ref, type: 'MCP', typeColor: '#1a7158' });
    }
    for (const [name, ref] of Object.entries(toolFlows)) {
      intgItems.push({ name, type: 'FLOW', typeColor: '#c47028' });
    }

    const nId = sid('intg-card');
    nodes.push({
      id: nId, type: 'portCard', position: { x: baseX - CARD_W / 2, y: subY },
      data: {
        label: 'Integrations', section: 'integrations', _portChild: true,
        addLabel: '+ Add Integration',
        items: intgItems,
      },
      draggable: false,
    });
    // No connector edge — card appears directly below toolbar

  } else if (portId === 'bp-ext') {
    const exts: any[] = parentData.extensions ?? [];
    const extItems = exts.map((ext: any, i: number) => {
      const extName = typeof ext === 'object' ? (Object.keys(ext)[0] ?? `ext-${i}`) : `ext-${i}`;
      return { name: extName, type: 'function', typeColor: '#5530a8' };
    });

    const nId = sid('ext-card');
    nodes.push({
      id: nId, type: 'portCard', position: { x: baseX - CARD_W / 2, y: subY },
      data: {
        label: 'Extensions', section: 'extensions', _portChild: true,
        addLabel: '+ Add Extension Point',
        items: extItems,
      },
      draggable: false,
    });
    // No connector edge — card appears directly below toolbar

  } else if (portId === 'bp-events') {
    const nId = sid('evt-card');
    nodes.push({
      id: nId, type: 'portCard', position: { x: baseX - CARD_W / 2, y: subY },
      data: {
        label: 'Events', section: 'events', _portChild: true,
        addLabel: '+ Add Event',
        items: [
          { name: 'session.started', type: 'SessionStarted', typeColor: '#5530a8' },
          { name: 'session.message', type: 'MessageEvent', typeColor: '#5530a8' },
          { name: 'session.ended', type: 'SessionEnded', typeColor: '#5530a8' },
          { name: 'session.error', type: 'ErrorEvent', typeColor: '#dc2626' },
        ],
      },
      draggable: false,
    });
    // No connector edge — card appears directly below toolbar
  }

  return { nodes, edges };
}
