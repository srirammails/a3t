/// Composing a flow by dropping things on it: a step from the palette, or an
/// item from the Explorer — a resource becomes the step that uses it, a flow a
/// sub-flow, an agent a call to its chat flow. New tasks go at the end of the
/// flow's `do` (CNCF DSL 1.0), named uniquely; the form edits them from there.
import * as YAML from 'yaml';
import type { Item } from '../items';
import { write } from '../yamlText';

export const STEP_MIME = 'application/x-a3t-step';

type Body = Record<string, unknown>;
export type PaletteStep = { id: string; label: string; icon: string; base: string; task: () => Body };
export type PaletteGroup = { title: string; steps: PaletteStep[] };

const call = (fn: string, w: Body): Body => ({ call: fn, with: w });

/// The step palette: CNCF DSL tasks and the platform's calls.
export const PALETTE: PaletteGroup[] = [
  { title: 'Control', steps: [
    { id: 'set', label: 'Set', icon: '=', base: 'set', task: () => ({ set: { key: 'value' } }) },
    { id: 'switch', label: 'Switch', icon: '⑂', base: 'decide', task: () => ({ switch: [{ yes: { when: '${ . }', then: 'continue' } }, { otherwise: { then: 'end' } }] }) },
    { id: 'for', label: 'For each', icon: '↻', base: 'each', task: () => ({ for: { each: 'item', in: '${ .items }' }, do: [{ handle: { set: { item: '${ $item }' } } }] }) },
    { id: 'fork', label: 'Fork', icon: '⫛', base: 'parallel', task: () => ({ fork: { branches: [{ a: { set: { a: true } } }, { b: { set: { b: true } } }] } }) },
    { id: 'try', label: 'Try / catch', icon: '⚑', base: 'guard', task: () => ({ try: [{ attempt: { set: { ok: true } } }], catch: { errors: { with: { type: 'https://serverlessworkflow.io/spec/1.0.0/errors/runtime' } }, do: [{ recover: { set: { ok: false } } }] } }) },
    { id: 'do', label: 'Do (group)', icon: '▤', base: 'group', task: () => ({ do: [{ first: { set: { step: 1 } } }] }) },
    { id: 'wait', label: 'Wait', icon: '⏱', base: 'wait', task: () => ({ wait: { seconds: 5 } }) },
    { id: 'raise', label: 'Raise error', icon: '⚠', base: 'fail', task: () => ({ raise: { error: { type: 'https://serverlessworkflow.io/spec/1.0.0/errors/runtime', status: 500, title: 'Failed' } } }) },
  ] },
  { title: 'Calls', steps: [
    { id: 'http', label: 'HTTP', icon: '⇄', base: 'fetch', task: () => call('http', { method: 'get', endpoint: { uri: 'https://example.com' } }) },
    { id: 'llm', label: 'LLM', icon: '✦', base: 'ask', task: () => call('llm', { prompt: '', _resource: '' }) },
    { id: 'agent', label: 'Agent', icon: '◉', base: 'agent', task: () => call('a3t:agent', { config: {} }) },
    { id: 'mcp', label: 'MCP tool', icon: '⚙', base: 'tool', task: () => call('mcp', { tool: '', arguments: {}, _resource: '' }) },
    { id: 'sql', label: 'SQL', icon: '⛁', base: 'query', task: () => call('sql', { query: 'SELECT 1', _resource: '' }) },
    { id: 'lancedb', label: 'LanceDB', icon: '⛁', base: 'search', task: () => call('lancedb', { operation: 'query', table: '', limit: 10, _resource: '' }) },
    { id: 'adb', label: 'ADB (AQL)', icon: '⛁', base: 'recall', task: () => call('adb', { query: 'RECALL FROM WORKING LIMIT 5', _resource: '' }) },
    { id: 'redis', label: 'Redis', icon: '⛁', base: 'cache', task: () => call('redis', { command: 'GET', args: ['${ .key }'], _resource: '' }) },
    { id: 'human', label: 'Ask a person', icon: '☺', base: 'approve', task: () => call('human', { prompt: 'Approve?' }) },
  ] },
  { title: 'Run', steps: [
    { id: 'workflow', label: 'Sub-flow', icon: 'ƒ', base: 'subflow', task: () => ({ run: { workflow: { namespace: '', name: '', version: 'latest', input: {} } } }) },
    { id: 'script', label: 'Script', icon: '{}', base: 'script', task: () => ({ run: { script: { language: 'js', code: 'return {};' } } }) },
    { id: 'shell', label: 'Shell', icon: '$', base: 'shell', task: () => ({ run: { shell: { command: 'echo hello' } } }) },
  ] },
  { title: 'Events', steps: [
    { id: 'emit', label: 'Emit event', icon: '↗', base: 'emit', task: () => ({ emit: { event: { with: { type: 'my.event', source: 'flow' } } } }) },
    { id: 'listen', label: 'Listen', icon: '↘', base: 'listen', task: () => ({ listen: { to: { one: { with: { type: 'my.event' } } } } }) },
  ] },
  { title: 'State', steps: [
    { id: 'state-get', label: 'Get state', icon: '⇣', base: 'load', task: () => call('state:get', { key: '' }) },
    { id: 'state-set', label: 'Set state', icon: '⇡', base: 'save', task: () => call('state:set', { key: '', value: '${ . }' }) },
  ] },
];

/// A palette step by id; `fn:<name>` is a call of a function the flow declares
/// in `use.functions` (the designer's Library group).
export function paletteStep(id: string): PaletteStep | null {
  if (id.startsWith('fn:')) {
    const name = id.slice(3);
    return { id, label: name, icon: 'ƒ', base: name, task: () => ({ call: name }) };
  }
  return PALETTE.flatMap((g) => g.steps).find((s) => s.id === id) ?? null;
}

/// The flow's own reusable tasks (`use.functions`), as palette steps.
export function libraryOf(yaml: string): PaletteStep[] {
  try {
    const fns = YAML.parse(yaml ?? '')?.use?.functions;
    return fns && typeof fns === 'object' ? Object.keys(fns).map((n) => paletteStep(`fn:${n}`)!) : [];
  } catch { return []; }
}

const LLM = new Set(['llm', 'openai', 'anthropic', 'ollama', 'groq']);

/// The step an Explorer item becomes in `project`'s flow, or null when it is not
/// a step (a trigger starts flows; it is wired to the flow instead).
export function taskForItem(it: Item): { base: string; task: Body } | null {
  const ref = `${it.project}/${it.name}`;
  if (it.kind === 'flow') return { base: it.name, task: { run: { workflow: { namespace: it.project, name: it.name, version: 'latest', input: {} } } } };
  if (it.kind === 'agent') {
    const target = (() => { try { return JSON.parse(it.data?.variables ?? '{}').target; } catch { return ''; } })() || 'a3t/a3t-chat-engine';
    const [ns, ...name] = String(target).split('/');
    return { base: it.name, task: { run: { workflow: { namespace: ns, name: name.join('/'), version: 'latest', input: { message: '${ .message }', agent_id: it.id ?? it.name } } } } };
  }
  if (it.kind !== 'resource') return null;
  const t = it.type ?? '';
  if (LLM.has(t)) return { base: `ask-${it.name}`, task: call('llm', { prompt: '${ .message }', _resource: ref }) };
  if (t === 'mcp') return { base: `tool-${it.name}`, task: call('mcp', { tool: '', arguments: {}, _resource: ref }) };
  if (t === 'postgres' || t === 'mysql') return { base: `query-${it.name}`, task: call('sql', { query: 'SELECT 1', _resource: ref }) };
  if (t === 'lancedb') return { base: `search-${it.name}`, task: call('lancedb', { operation: 'query', table: '', limit: 10, _resource: ref }) };
  if (t === 'adb' || t === 'clawdb') return { base: `recall-${it.name}`, task: call('adb', { query: 'RECALL FROM WORKING LIMIT 5', _resource: ref }) };
  if (t === 'redis') return { base: `cache-${it.name}`, task: call('redis', { command: 'GET', args: ['${ .key }'], _resource: ref }) };
  if (t === 'qdrant') return { base: `search-${it.name}`, task: call('qdrant', { operation: 'search', collection: '', limit: 10, _resource: ref }) };
  if (t === 'secret' || t === 'credential') return { base: `use-${it.name}`, task: { set: { [it.name.replace(/[^a-z0-9_]/gi, '_')]: `\${ $secrets["${ref}"] }` } } };
  return { base: `use-${it.name}`, task: { set: { resource: ref } } };
}

type Path = (string | number)[];
const keyOf = (p: YAML.Pair) => String((p.key as any)?.value ?? p.key);
/// Task names already used in a list.
function names(seq: unknown): Set<string> {
  const out = new Set<string>();
  if (YAML.isSeq(seq)) for (const e of seq.items) if (YAML.isMap(e)) for (const p of e.items) out.add(keyOf(p));
  return out;
}
const unique = (base: string, taken: Set<string>) => {
  const clean = base.toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '') || 'step';
  let name = clean;
  for (let i = 2; taken.has(name); i++) name = `${clean}-${i}`;
  return name;
};

/// The flow with a task put into the list at `list` (`['do']`, a loop's
/// `do`…) at `index` (past the end: appended), under a name not yet used in
/// that list (`base`, `base-2`…); layout and comments kept.
export function insertTask(yaml: string, list: Path, index: number, base: string, task: Body): { yaml: string; name: string; path: Path } {
  const doc = YAML.parseDocument(yaml || 'do: []\n');
  let seq = doc.getIn(list, true);
  if (!YAML.isSeq(seq)) { doc.setIn(list, doc.createNode([])); seq = doc.getIn(list, true); }
  const s = seq as YAML.YAMLSeq;
  const name = unique(base, names(s));
  const at = Math.max(0, Math.min(index, s.items.length));
  s.items.splice(at, 0, doc.createNode({ [name]: task }) as any);
  return { yaml: write(doc, yaml), name, path: [...list, at, name] };
}

/// …at the end of the flow's `do`.
export function addTask(yaml: string, base: string, task: Body): { yaml: string; name: string } {
  return insertTask(yaml, ['do'], Number.MAX_SAFE_INTEGER, base, task);
}

/// A task's `then:` — where the flow goes after it (a task in the same list,
/// or continue / exit / end).
export function setThen(yaml: string, task: Path, to: string): string {
  const doc = YAML.parseDocument(yaml);
  if (to === 'continue') doc.deleteIn([...task, 'then']);
  else doc.setIn([...task, 'then'], to);
  return write(doc, yaml);
}

/// A new case on a switch, going to `to` (its condition left for the user).
export function addCase(yaml: string, task: Path, to: string): { yaml: string; name: string } {
  const doc = YAML.parseDocument(yaml);
  const cases = doc.getIn([...task, 'switch'], true);
  const name = unique(`to-${to}`, names(cases));
  const entry = { [name]: { when: '${ true }', then: to } };
  if (YAML.isSeq(cases)) cases.items.push(doc.createNode(entry) as any);
  else doc.setIn([...task, 'switch'], doc.createNode([entry]));
  return { yaml: write(doc, yaml), name };
}
