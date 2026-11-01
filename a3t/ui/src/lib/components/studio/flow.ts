/// Flow documents in the studio: what an open flow tab holds, and what the
/// panels read from it (its steps for breakpoints, its input schema, problems).
import * as YAML from 'yaml';

export type OpenItem = { kind: 'flow' | 'resource' | 'trigger' | 'template' | 'canvas'; ns: string; name: string; id?: string; data?: any };

export type Doc = {
  /// `flow:<ns>/<name>`, `resource:<ns>/<name>`, … or `flow:new:<n>`.
  id: string;
  kind: 'flow' | 'resource' | 'trigger' | 'template' | 'canvas';
  ns: string;
  name: string;
  /// Flows: the text being edited and the text last loaded or deployed.
  yaml?: string;
  saved?: string;
  loaded?: boolean;
  isNew?: boolean;
  /// Other items: what the explorer listed.
  data?: any;
};

export function parse(yaml: string | undefined): { doc: any; error: string | null } {
  try {
    return { doc: YAML.parse(yaml ?? '') ?? {}, error: null };
  } catch (e) {
    return { doc: null, error: (e as Error).message };
  }
}

/// The flow's steps as the engine keys them for breakpoints (loop and branch
/// scopes left out): `/a`, `/loop/send`, `/guard/charge`.
export function stepKeys(tasks: any, prefix = ''): string[] {
  const out: string[] = [];
  for (const item of Array.isArray(tasks) ? tasks : []) {
    if (!item || typeof item !== 'object') continue;
    for (const [name, task] of Object.entries<any>(item)) {
      const key = `${prefix}/${name}`;
      out.push(key);
      if (!task || typeof task !== 'object') continue;
      if (Array.isArray(task.do)) out.push(...stepKeys(task.do, key));
      if (task.try) {
        out.push(...stepKeys(task.try.do ?? task.try, key));
        out.push(...stepKeys(task.try.catch?.do ?? task.catch?.do, key));
      }
      // Branch names are not part of the key (the engine runs branch `i` at
      // `fork[i]`): a branch's steps sit directly under the fork.
      for (const b of Array.isArray(task.fork?.branches) ? task.fork.branches : []) {
        for (const inner of Object.values<any>(b ?? {})) out.push(...stepKeys(inner?.do, key));
      }
    }
  }
  return out;
}

export type Problem = { level: 'error' | 'warning'; message: string };

/// What is wrong with a flow before it is deployed.
export function problems(yaml: string | undefined): Problem[] {
  const { doc, error } = parse(yaml);
  if (error) return [{ level: 'error', message: `YAML: ${error}` }];
  const out: Problem[] = [];
  const d = doc?.document;
  if (!d) out.push({ level: 'error', message: 'Missing `document:` (dsl, namespace, name, version).' });
  else {
    for (const k of ['dsl', 'namespace', 'name', 'version']) if (!d[k]) out.push({ level: 'error', message: `document.${k} is missing.` });
  }
  if (!Array.isArray(doc?.do) || !doc.do.length) out.push({ level: 'error', message: 'The flow has no steps (`do:` is empty or missing).' });
  const keys = stepKeys(doc?.do);
  const seen = new Set<string>();
  for (const k of keys) {
    if (seen.has(k)) out.push({ level: 'error', message: `Step ${k} appears twice in the same block.` });
    seen.add(k);
  }
  return out;
}

export function newFlowYaml(ns: string, n: number): string {
  return `document:\n  dsl: 1.0.3\n  namespace: ${ns}\n  name: new-flow${n > 1 ? `-${n}` : ''}\n  version: 1.0.0\ndo:\n- hello:\n    set:\n      message: Hello\n`;
}

/// The line (1-based) where a step is named in the flow's YAML, for "go to
/// code" from the graph: each segment of the key is the nearest such key
/// under the one before it.
export function lineOfStep(yaml: string | undefined, stepKey: string): number | null {
  try { return lineIn(YAML.parseDocument(yaml ?? ''), yaml ?? '', stepKey, true); } catch { return null; }
}

/// Each step key's line, parsing the text once.
export function stepLines(yaml: string | undefined, keys: string[]): Map<string, number> {
  const out = new Map<string, number>();
  let doc: YAML.Document;
  try { doc = YAML.parseDocument(yaml ?? ''); } catch { return out; }
  for (const k of keys) {
    const line = lineIn(doc, yaml ?? '', k, false);
    if (line !== null) out.set(k, line);
  }
  return out;
}

/// `partial`: a key not found all the way down gives its nearest parent's line.
function lineIn(doc: YAML.Document, yaml: string, stepKey: string, partial: boolean): number | null {
  let scope: unknown = doc.get('do', true) ?? doc.contents;
  let at: number | null = null;
  for (const seg of stepKey.split('/').filter(Boolean)) {
    const pair = nearest(scope, seg);
    if (!pair) return at === null || !partial ? null : lineAt(yaml, at);
    at = (pair.key as any)?.range?.[0] ?? at;
    scope = pair.value;
  }
  return at === null ? null : lineAt(yaml, at);
}

/// The shallowest `name:` pair under a node (breadth first).
function nearest(root: unknown, name: string): YAML.Pair | null {
  let level: unknown[] = [root];
  while (level.length) {
    const next: unknown[] = [];
    for (const n of level) {
      if (YAML.isMap(n)) {
        for (const p of n.items) {
          if (YAML.isScalar(p.key) && String(p.key.value) === name) return p;
          next.push(p.value);
        }
      } else if (YAML.isSeq(n)) next.push(...n.items);
    }
    level = next;
  }
  return null;
}

const lineAt = (text: string, offset: number) => text.slice(0, offset).split('\n').length;
