/// The CNCF Serverless Workflow DSL 1.0 as structure, for the schema-aware code
/// view: what may go where, what is required, and what each key is for (words
/// and examples from ./swfDocs, the blog's reference tree). From a flow's text
/// it builds an outline — every key the schema allows at each place, the ones
/// the flow sets marked present — and the problems the schema finds, each with
/// its line. Plain TypeScript (studio-core).
///
/// A3T additions: the platform's calls (llm, a3t:agent, adb, lancedb, sql…),
/// with their arguments, and `_`-prefixed engine arguments in any `with`.
import * as YAML from 'yaml';
import { schema as DOCS, type SchemaNode } from './swfDocs';
import { PALETTE } from './compose';

// ── Shapes ────────────────────────────────────────────────────────────
type Val = 'string' | 'expression' | 'integer' | 'number' | 'boolean' | 'duration' | 'directive' | 'any';
export type Shape =
  | { s: 'obj'; f: Record<string, Field>; open?: boolean }
  | { s: 'map'; v: Shape; what: string }         // free keys you name, each a `v`
  | { s: 'list'; v: Shape }                      // a list of `v`
  | { s: 'named'; v: Shape; what: string }       // a list of one-key maps: name → `v`
  | { s: 'tasks' }                               // a list of named tasks
  | { s: 'task' }
  | { s: 'val'; t: Val }
  | { s: 'or'; o: Shape[]; t: string };
/// `r` required; `soft` a requirement the engine fills in (a warning, not an
/// error; a string says why); `alt` another key that does this one's job (A3T's `url` for `endpoint`).
export type Field = { sh: Shape; r?: boolean; soft?: boolean | string; alt?: string; doc?: string; d?: string; ex?: string };

const v = (t: Val): Shape => ({ s: 'val', t });
const obj = (f: Record<string, Field>, open = false): Shape => ({ s: 'obj', f, open });
const F = (sh: Shape, doc?: string, r = false): Field => ({ sh, doc, r });
const R = (sh: Shape, doc?: string): Field => ({ sh, doc, r: true });
const A3T = (sh: Shape, d: string, ex?: string, r = false): Field => ({ sh, d, ex, r });

const DURATION: Shape = { s: 'or', t: 'duration', o: [v('string'), obj({ days: F(v('integer')), hours: F(v('integer')), minutes: F(v('integer')), seconds: F(v('integer')), milliseconds: F(v('integer')) })] };
const EXPR_OR_ANY: Shape = { s: 'or', t: 'map | expression', o: [v('expression'), { s: 'map', v: v('any'), what: 'key' }] };
const SCHEMA = (p: string): Shape => obj({ format: { ...R(v('string'), `${p}/format`), soft: 'the CNCF DSL writes a schema as format: json with the JSON Schema under document (FlowR also reads a bare JSON Schema here)' }, document: F({ s: 'map', v: v('any'), what: 'key' }, `${p}/document`), resource: F(obj({ endpoint: R({ s: 'or', t: 'string | object', o: [v('string'), obj({ uri: R(v('string')), authentication: F(obj({}, true)) })] }) }), `${p}/resource`) }, true);
const TIMEOUT: Shape = { s: 'or', t: 'string | object', o: [v('string'), obj({ after: R(DURATION, 'timeout/after') })] };
const ERROR_DEF: Shape = obj({ type: R(v('string'), 'do/raise/error/type'), status: R(v('integer'), 'do/raise/error/status'), title: F(v('string'), 'do/raise/error/title'), detail: F(v('string'), 'do/raise/error/detail'), instance: F(v('string')) });
const TASKS: Shape = { s: 'tasks' };

export const ROOT: Shape = obj({
  document: R(obj({
    dsl: R(v('string'), 'document/dsl'), namespace: R(v('string'), 'document/namespace'), name: R(v('string'), 'document/name'),
    version: R(v('string'), 'document/version'), title: F(v('string'), 'document/title'), summary: F(v('string'), 'document/summary'),
    tags: F({ s: 'map', v: v('string'), what: 'tag' }, 'document/tags'), metadata: F({ s: 'map', v: v('any'), what: 'key' }, 'document/metadata'),
  }), 'document'),
  input: F(obj({ schema: F(SCHEMA('input/schema'), 'input/schema'), from: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }, 'input/from') }), 'input'),
  use: F(obj({
    authentications: F({ s: 'map', what: 'policy', v: obj({ basic: F(obj({ username: R(v('string')), password: R(v('string')) }), 'use/authentications/basic'), bearer: F(obj({ token: R(v('string')) }), 'use/authentications/bearer'), digest: F(obj({ username: R(v('string')), password: R(v('string')) }), 'use/authentications/digest'), oauth2: F(obj({}, true), 'use/authentications/oauth2'), oidc: F(obj({}, true), 'use/authentications/oidc') }) }, 'use/authentications'),
    catalogs: F({ s: 'map', what: 'catalog', v: obj({ endpoint: R({ s: 'or', t: 'string | object', o: [v('string'), obj({}, true)] }, 'use/catalogs/endpoint') }) }, 'use/catalogs'),
    errors: F({ s: 'map', what: 'error', v: ERROR_DEF }, 'use/errors'),
    extensions: F({ s: 'named', what: 'extension', v: obj({ extend: R(v('string'), 'use/extensions/extend'), when: F(v('expression'), 'use/extensions/when'), before: F(TASKS, 'use/extensions/before'), after: F(TASKS, 'use/extensions/after') }) }, 'use/extensions'),
    functions: F({ s: 'map', what: 'function', v: { s: 'task' } }, 'use/functions'),
    retries: F({ s: 'map', what: 'policy', v: obj({ delay: F(DURATION, 'use/retries/delay'), backoff: F(obj({}, true), 'use/retries/backoff'), limit: F(obj({}, true), 'use/retries/limit'), jitter: F(obj({}, true), 'use/retries/jitter'), when: F(v('expression'), 'use/retries/when'), exceptWhen: F(v('expression'), 'use/retries/exceptWhen') }) }, 'use/retries'),
    secrets: F({ s: 'list', v: v('string') }, 'use/secrets'),
    timeouts: F({ s: 'map', what: 'timeout', v: obj({ after: R(DURATION) }) }, 'use/timeouts'),
  }), 'use'),
  do: R(TASKS, 'do'),
  timeout: F(TIMEOUT, 'timeout'),
  output: F(obj({ as: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }, 'output/as'), schema: F(SCHEMA('output/schema'), 'output/schema') }), 'output'),
  schedule: F(obj({ every: F(DURATION, 'schedule/every'), cron: F(v('string'), 'schedule/cron'), after: F(DURATION, 'schedule/after'), on: F(obj({}, true), 'schedule/on') }), 'schedule'),
  metadata: A3T({ s: 'map', v: v('any'), what: 'key' }, 'A3T: information about the flow for the platform (labels, owner…). The CNCF DSL keeps it under document.metadata.', 'metadata:\n  owner: payments-team'),
  evaluate: F(obj({ language: F(v('string'), 'evaluate/language'), mode: F(v('string'), 'evaluate/mode') }), 'evaluate'),
});

/// Fields every task may have, beside its type.
const COMMON: Record<string, Field> = {
  if: F(v('expression'), 'do/(any task)/if'),
  input: F(obj({ schema: F(SCHEMA('input/schema')), from: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }, 'input/from') }), 'do/(any task)/input'),
  output: F(obj({ as: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }, 'output/as'), schema: F(SCHEMA('output/schema')) }), 'do/(any task)/output'),
  export: F(obj({ as: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }), schema: F(SCHEMA('output/schema')) }), 'do/(any task)/export'),
  timeout: F(TIMEOUT, 'do/(any task)/timeout'),
  then: F(v('directive'), 'do/(any task)/then'),
  metadata: F({ s: 'map', v: v('any'), what: 'key' }, 'do/(any task)/metadata'),
  checkpoint: A3T(v('boolean'), 'A3T: save the run’s state after this task, so a restart resumes here.', 'checkpoint: true'),
};

/// Each task type: the key that names it, and the fields that come with it at
/// the task's level (its own key first).
export const TASK_TYPES: Record<string, Record<string, Field>> = {
  call: { call: R(v('string'), 'do/call/call'), with: F({ s: 'map', v: v('any'), what: 'argument' }, 'do/call/with') },
  set: { set: R(EXPR_OR_ANY, 'do/set') },
  switch: { switch: R({ s: 'named', what: 'case', v: obj({ when: F(v('expression'), 'do/switch/when'), then: R(v('directive'), 'do/switch/then') }) }, 'do/switch') },
  for: { for: R(obj({ in: R({ s: 'or', t: 'expression | array', o: [v('expression'), { s: 'list', v: v('any') }] }, 'do/for/for.in'), each: F(v('string'), 'do/for/for.each'), at: F(v('string'), 'do/for/for.at') }), 'do/for'), while: F(v('expression'), 'do/for/while'), do: R(TASKS, 'do/for/do') },
  fork: { fork: R(obj({ branches: R(TASKS, 'do/fork/branches'), compete: F(v('boolean'), 'do/fork/compete') }), 'do/fork') },
  try: { try: R(TASKS, 'do/try'), catch: R(obj({ errors: F(obj({ with: F(obj({ type: F(v('string')), status: F(v('integer')), instance: F(v('string')), title: F(v('string')), detail: F(v('string')) }), 'do/try/catch/errors.with') })), as: F(v('string'), 'do/try/catch/as'), when: F(v('expression'), 'do/try/catch/when'), exceptWhen: F(v('expression'), 'do/try/catch/exceptWhen'), retry: F({ s: 'or', t: 'string | object', o: [v('string'), obj({}, true)] }, 'do/try/catch/retry'), do: F(TASKS, 'do/try/catch/do'), then: F(v('directive'), 'do/try/catch/then') }), 'do/try/catch') },
  emit: { emit: R(obj({ event: R(obj({ with: R(obj({ source: R(v('string'), 'do/emit/event.with/source'), type: R(v('string'), 'do/emit/event.with/type'), data: F(v('any'), 'do/emit/event.with/data'), subject: F(v('string'), 'do/emit/event.with/subject') }, true), 'do/emit/event.with') })) }), 'do/emit') },
  listen: { listen: R(obj({ to: R(obj({ one: F(obj({ with: F(obj({}, true)) }), 'do/listen/to/one'), all: F({ s: 'list', v: obj({ with: F(obj({}, true)) }) }, 'do/listen/to/all'), any: F({ s: 'list', v: obj({ with: F(obj({}, true)) }) }, 'do/listen/to/any'), until: F({ s: 'or', t: 'expression | object', o: [v('expression'), obj({}, true)] }, 'do/listen/to/until') }), 'do/listen/to'), read: F(v('string'), 'do/listen/read') }), 'do/listen') },
  wait: { wait: R(DURATION, 'do/wait') },
  raise: { raise: R(obj({ error: R({ s: 'or', t: 'string | object', o: [v('string'), ERROR_DEF] }, 'do/raise/error') }), 'do/raise') },
  run: { run: R(obj({ container: F(obj({ image: R(v('string')) }, true), 'do/run/container'), script: F(obj({ language: R(v('string')), code: F(v('string')), source: F(obj({}, true)) }, true), 'do/run/script'), shell: F(obj({ command: R(v('string')) }, true), 'do/run/shell'), workflow: F(obj({ namespace: R(v('string')), name: R(v('string')), version: { ...R(v('string')), soft: true }, input: F(v('any')) }), 'do/run/workflow'), await: F(v('boolean'), 'do/run/await'), return: F(v('string'), 'do/run/return') }), 'do/run') },
  do: { do: R(TASKS, 'do/do') },
};
export const TASK_TYPE_KEYS = Object.keys(TASK_TYPES);

const RES = A3T(v('string'), 'A3T: the resource this call uses, as project/name (from the Explorer).', '_resource: my-project/qwen');
/// The arguments (`with`) of each built-in call: the DSL's own and A3T's.
const CALL_WITH: Record<string, Record<string, Field>> = {
  http: { method: R(v('string'), 'do/call/call: http/method'), endpoint: { ...R({ s: 'or', t: 'string | object', o: [v('string'), obj({ uri: R(v('string')), authentication: F(obj({}, true)) })] }, 'do/call/call: http/endpoint'), alt: 'url' }, url: A3T(v('string'), 'A3T: the URL, as FlowR also accepts it (the DSL writes endpoint).'), headers: F({ s: 'map', v: v('string'), what: 'header' }, 'do/call/call: http/headers'), body: F(v('any'), 'do/call/call: http/body'), query: F({ s: 'map', v: v('string'), what: 'parameter' }, 'do/call/call: http/query'), output: F(v('string'), 'do/call/call: http/output'), redirect: F(v('boolean'), 'do/call/call: http/redirect') },
  grpc: { proto: R(obj({}, true), 'do/call/call: grpc/proto'), service: R(obj({}, true), 'do/call/call: grpc/service'), method: R(v('string'), 'do/call/call: grpc/method'), arguments: F({ s: 'map', v: v('any'), what: 'argument' }, 'do/call/call: grpc/arguments') },
  openapi: { document: R(obj({}, true), 'do/call/call: openapi/document'), operationId: R(v('string'), 'do/call/call: openapi/operationId'), parameters: F({ s: 'map', v: v('any'), what: 'parameter' }, 'do/call/call: openapi/parameters'), authentication: F(obj({}, true), 'do/call/call: openapi/authentication'), output: F(v('string'), 'do/call/call: openapi/output') },
  asyncapi: { document: R(obj({}, true), 'do/call/call: asyncapi/document'), operation: R(v('string'), 'do/call/call: asyncapi/operation'), protocol: F(v('string'), 'do/call/call: asyncapi/protocol'), message: F(obj({}, true), 'do/call/call: asyncapi/message'), subscription: F(obj({}, true), 'do/call/call: asyncapi/subscription') },
  a2a: { method: R(v('string'), 'do/call/call: a2a/method'), agentCard: F(obj({}, true), 'do/call/call: a2a/agentCard'), server: F({ s: 'or', t: 'string | object', o: [v('string'), obj({}, true)] }, 'do/call/call: a2a/server'), parameters: F({ s: 'map', v: v('any'), what: 'parameter' }, 'do/call/call: a2a/parameters') },
  mcp: { tool: A3T(v('string'), 'A3T: the tool to call on the MCP server.', 'tool: search'), arguments: A3T({ s: 'map', v: v('any'), what: 'argument' }, 'A3T: the tool’s arguments.', 'arguments:\n  query: ${ .message }'), _resource: RES, method: F(v('string'), 'do/call/call: mcp/method'), transport: F(obj({}, true), 'do/call/call: mcp/transport'), parameters: F({ s: 'map', v: v('any'), what: 'parameter' }, 'do/call/call: mcp/parameters') },
  llm: { prompt: A3T(v('string'), 'A3T: the message for the model. Usually an expression over the input.', 'prompt: ${ .message }', true), system_prompt: A3T(v('string'), 'A3T: instructions the model follows for this call.', 'system_prompt: You are brief.'), messages: A3T({ s: 'or', t: 'array | expression', o: [v('expression'), { s: 'list', v: v('any') }] }, 'A3T: the conversation before the prompt.', 'messages: ${ .messages // [] }'), temperature: A3T(v('number'), 'A3T: sampling temperature, 0 to 1.', 'temperature: 0.3'), max_tokens: A3T(v('integer'), 'A3T: the longest answer, in tokens.', 'max_tokens: 512'), _resource: { ...RES, r: true } },
  'a3t:agent': { config: A3T({ s: 'or', t: 'object | expression', o: [v('expression'), obj({}, true)] }, 'A3T: the agent’s settings — system_prompt, llm_resource, mcp_servers, memory… (the template form edits them).', 'config:\n  system_prompt: You help with orders.\n  llm_resource: my-project/qwen'), message: A3T(v('string'), 'A3T: the user’s message for this turn.', 'message: ${ .message }'), session_id: A3T(v('string'), 'A3T: keeps a conversation together across runs.', 'session_id: ${ .session_id }') },
  adb: { query: A3T(v('string'), 'A3T: an AQL statement — RECALL, STORE, LINK… against agent memory.', 'query: RECALL FROM WORKING LIMIT 5', true), _resource: RES },
  clawdb: { query: A3T(v('string'), 'A3T: an AQL statement against agent memory.', 'query: RECALL FROM WORKING LIMIT 5', true), _resource: RES },
  sql: { query: A3T(v('string'), 'A3T: the SQL to run.', 'query: SELECT 1', true), params: A3T({ s: 'list', v: v('any') }, 'A3T: values for the query’s placeholders.', 'params: [${ .id }]'), _resource: { ...RES, r: true } },
  lancedb: { operation: A3T(v('string'), 'A3T: query, insert, upsert, delete…', 'operation: query', true), table: A3T(v('string'), 'A3T: the table.', 'table: docs'), limit: A3T(v('integer'), 'A3T: the most rows to return.', 'limit: 10'), _resource: RES },
  redis: { command: A3T(v('string'), 'A3T: the Redis command — GET, SET, DEL, HGET…', 'command: GET', true), args: A3T({ s: 'list', v: v('any') }, 'A3T: the command’s arguments, in order.', 'args:\n  - ${ .key }'), _resource: { ...RES, r: true } },
  'state:get': { key: A3T(v('string'), 'A3T: the state key to read.', 'key: counter', true) },
  'state:set': { key: A3T(v('string'), 'A3T: the state key to write.', 'key: counter', true), value: A3T(v('any'), 'A3T: the value to store.', 'value: ${ . }') },
  human: { prompt: A3T(v('string'), 'A3T: what the person is asked.', 'prompt: Approve the refund?', true) },
};
export const BUILTIN_CALLS = Object.keys(CALL_WITH);

// ── Words and examples from the reference tree ─────────────────────────
function docNode(path: string | undefined): SchemaNode | null {
  if (!path) return null;
  let list: SchemaNode[] | undefined = DOCS, node: SchemaNode | null = null;
  for (const seg of path.split('/')) {
    node = list?.find((n) => n.k === seg) ?? null;
    if (!node) return null;
    list = node.c;
  }
  return node;
}

/// The value a newly added key starts with: its example's, when the example is
/// that key (`format: json` for format), else empty of its shape.
export function starter(key: string, f: Field): unknown {
  const ex = f.ex ?? docNode(f.doc)?.ex;
  if (ex) {
    try {
      const v = YAML.parse(ex);
      const last = key.split('.').pop()!;
      if (v && typeof v === 'object' && !Array.isArray(v) && last in v) return v[last];
    } catch { /* not YAML: fall through */ }
  }
  return empty(f.sh);
}
function empty(sh: Shape): unknown {
  switch (sh.s) {
    case 'obj': {
      const o: Record<string, unknown> = {};
      for (const [k, f] of Object.entries(sh.f)) if (f.r) o[k] = starter(k, f);
      return o;
    }
    case 'map': return {};
    case 'list': case 'named': return [];
    case 'tasks': return [{ step: { set: { key: 'value' } } }];
    case 'task': return { set: { key: 'value' } };
    case 'or': return empty(sh.o[0]);
    case 'val': return sh.t === 'boolean' ? false : sh.t === 'integer' || sh.t === 'number' ? 0 : sh.t === 'directive' ? 'continue' : '';
  }
}

export function typeLabel(sh: Shape): string {
  switch (sh.s) {
    case 'obj': return 'object';
    case 'map': return `map · ${sh.what}`;
    case 'list': return `array · ${typeLabel(sh.v)}`;
    case 'named': return `array · ${sh.what}`;
    case 'tasks': return 'array · task';
    case 'task': return 'task';
    case 'or': return sh.t;
    case 'val': return sh.t;
  }
}

// ── The outline ───────────────────────────────────────────────────────
export type Path = (string | number)[];
export type Level = 'error' | 'warning';
export type SchemaProblem = { level: Level; message: string; path: Path; line: number | null };
/// What a "+" at the end of a row adds.
export type Adds = { what: 'task' } | { what: 'named'; name: string; value: unknown } | { what: 'key'; name: string; value: unknown } | { what: 'item'; value: unknown };

export type ONode = {
  id: string;
  path: Path;
  key: string;
  type: string;
  d?: string;
  ex?: string;
  required: boolean;
  present: boolean;
  /// A scalar value's text (present scalars), or `[3]` / `{2}` for collections.
  value?: string;
  scalar: boolean;
  /// The task type of a task node (call, set…), and for calls what it calls.
  task?: string;
  add?: Adds;
  line: number | null;
  problems: { level: Level; message: string }[];
  children: ONode[];
  /// The starting value when a ghost (absent) key is added.
  starter?: unknown;
};

export type Outline = { nodes: ONode[]; problems: SchemaProblem[]; error: string | null; errorLine: number | null };

const id = (p: Path) => p.map(String).join('/') || '$';
const isExpr = (x: unknown) => typeof x === 'string' && /^\s*\$\{[\s\S]*\}\s*$/.test(x);

function preview(n: unknown): { value?: string; scalar: boolean } {
  if (YAML.isScalar(n)) { const v = n.value; return { value: v === null ? '' : String(v), scalar: true }; }
  if (YAML.isSeq(n)) return { value: `[${n.items.length}]`, scalar: false };
  if (YAML.isMap(n)) return { value: `{${n.items.length}}`, scalar: false };
  return { scalar: false };
}

const keyOf = (p: YAML.Pair) => String((p.key as any)?.value ?? p.key);

class Builder {
  problems: SchemaProblem[] = [];
  taskNames = new Set<string>();
  functions = new Set<string>();
  constructor(private lc: YAML.LineCounter) {}
  line(node: any): number | null {
    const r = node?.range;
    return r ? this.lc.linePos(r[0]).line : null;
  }
  flag(n: ONode, level: Level, message: string) {
    n.problems.push({ level, message });
    this.problems.push({ level, message: `${id(n.path) === '$' ? '' : n.path.join('.') + ': '}${message}`, path: n.path, line: n.line });
  }

  /// One key's row: present (its node and value) or a ghost (absent: what could go there).
  field(path: Path, key: string, f: Field, pair: YAML.Pair | null, depth: number): ONode {
    const doc = docNode(f.doc);
    const node: ONode = {
      id: id(path), path, key, type: typeLabel(f.sh), d: f.d ?? doc?.d, ex: f.ex ?? doc?.ex, required: !!f.r, present: !!pair,
      scalar: false, line: pair ? this.line(pair.key) : null, problems: [], children: [],
    };
    if (!pair) {
      node.starter = starter(key, f);
      // A ghost shows the keys inside it, a level or two down (never tasks).
      if (depth < 2) node.children = this.ghosts(path, f.sh, depth + 1);
      return node;
    }
    Object.assign(node, preview(pair.value));
    this.value(node, f.sh, pair.value, depth);
    return node;
  }

  ghosts(path: Path, sh: Shape, depth: number): ONode[] {
    const o = sh.s === 'or' ? sh.o.find((x) => x.s === 'obj') : sh;
    if (!o || o.s !== 'obj') return [];
    return Object.entries(o.f).map(([k, f]) => this.field([...path, k], k, f, null, depth));
  }

  /// Check a present value against its shape, and fill the row's children.
  value(node: ONode, sh: Shape, n: unknown, depth: number) {
    const path = node.path;
    const scalarVal = YAML.isScalar(n) ? n.value : undefined;
    if (sh.s === 'or') {
      // The first option the value fits; an expression fits any.
      if (YAML.isScalar(n) && isExpr(scalarVal)) return;
      const pick = sh.o.find((o) => (YAML.isMap(n) ? o.s === 'obj' || o.s === 'map' : YAML.isSeq(n) ? o.s === 'list' || o.s === 'named' || o.s === 'tasks' : o.s === 'val'));
      if (!pick) { this.flag(node, 'error', `expected ${sh.t}`); return; }
      this.value(node, pick, n, depth);
      return;
    }
    switch (sh.s) {
      case 'val': {
        if (sh.t === 'any') return;
        if (!YAML.isScalar(n)) { if (n != null) this.flag(node, 'error', `expected ${sh.t}, found ${YAML.isSeq(n) ? 'a list' : 'a map'}`); return; }
        if (isExpr(scalarVal)) return;
        if ((sh.t === 'integer' || sh.t === 'number') && typeof scalarVal !== 'number') this.flag(node, 'error', `expected a number`);
        else if (sh.t === 'boolean' && typeof scalarVal !== 'boolean') this.flag(node, 'error', `expected true or false`);
        else if (sh.t === 'expression' && typeof scalarVal === 'string' && /\$\{/.test(scalarVal) && !isExpr(scalarVal)) this.flag(node, 'warning', 'an expression is written as ${ … } around the whole value');
        return;
      }
      case 'obj': {
        if (!YAML.isMap(n)) { if (!(YAML.isScalar(n) && (n.value === null || isExpr(n.value)))) this.flag(node, 'error', 'expected a map of keys'); return; }
        node.children = this.object(path, sh, n, depth);
        return;
      }
      case 'map': {
        if (!YAML.isMap(n)) { if (!(YAML.isScalar(n) && (n.value === null || isExpr(n.value)))) this.flag(node, 'error', `expected a map (${sh.what}: value)`); return; }
        node.add = { what: 'key', name: sh.what, value: empty(sh.v) };
        node.children = n.items.map((p) => {
          const k = keyOf(p);
          const c: ONode = { id: id([...path, k]), path: [...path, k], key: k, type: typeLabel(sh.v), required: false, present: true, scalar: false, line: this.line(p.key), problems: [], children: [] };
          Object.assign(c, preview(p.value));
          if (path.length === 2 && path[0] === 'use' && path[1] === 'functions') this.functions.add(k);
          if (sh.v.s === 'task') this.task(c, p.value, depth + 1);
          else this.value(c, sh.v, p.value, depth + 1);
          return c;
        });
        return;
      }
      case 'list': {
        if (!YAML.isSeq(n)) { if (!(YAML.isScalar(n) && isExpr(n.value))) this.flag(node, 'error', 'expected a list'); return; }
        node.add = { what: 'item', value: empty(sh.v) };
        node.children = n.items.map((it, i) => {
          const c: ONode = { id: id([...path, i]), path: [...path, i], key: `#${i + 1}`, type: typeLabel(sh.v), required: false, present: true, scalar: false, line: this.line(it), problems: [], children: [] };
          Object.assign(c, preview(it));
          this.value(c, sh.v, it, depth + 1);
          return c;
        });
        return;
      }
      case 'named': case 'tasks': {
        if (!YAML.isSeq(n)) { if (!(YAML.isScalar(n) && n.value === null)) this.flag(node, 'error', `expected a list of ${sh.s === 'tasks' ? 'tasks' : sh.what + 's'} (- name: …)`); return; }
        node.add = sh.s === 'tasks' ? { what: 'task' } : { what: 'named', name: sh.what, value: empty(sh.v) };
        node.children = n.items.map((it, i) => {
          if (!YAML.isMap(it) || it.items.length !== 1) {
            const c: ONode = { id: id([...path, i]), path: [...path, i], key: `#${i + 1}`, type: sh.s === 'tasks' ? 'task' : sh.what, required: false, present: true, scalar: false, line: this.line(it), problems: [], children: [] };
            this.flag(c, 'error', `each item is one name with the ${sh.s === 'tasks' ? 'task' : sh.what} under it (- name: …)`);
            return c;
          }
          const p = it.items[0];
          const k = keyOf(p);
          const c: ONode = { id: id([...path, i, k]), path: [...path, i, k], key: k, type: sh.s === 'tasks' ? 'task' : sh.what, required: false, present: true, scalar: false, line: this.line(p.key), problems: [], children: [] };
          if (sh.s === 'tasks') { this.taskNames.add(k); this.task(c, p.value, depth + 1); }
          else { Object.assign(c, preview(p.value)); this.value(c, sh.v, p.value, depth + 1); }
          return c;
        });
        return;
      }
      case 'task': this.task(node, n, depth); return;
    }
  }

  /// A map's keys, in the schema's order (present or ghost), then any it does not know.
  object(path: Path, sh: Extract<Shape, { s: 'obj' }>, n: YAML.YAMLMap, depth: number, extra: Record<string, Field> = {}): ONode[] {
    const fields = { ...sh.f, ...extra };
    const out: ONode[] = [];
    const pairs = new Map(n.items.map((p) => [keyOf(p), p] as const));
    for (const [k, f] of Object.entries(fields)) {
      const pair = pairs.get(k) ?? null;
      const c = this.field([...path, k], k, f, pair, depth);
      if (!pair && f.r && !(f.alt && pairs.has(f.alt))) this.flag(c, f.soft ? 'warning' : 'error', typeof f.soft === 'string' ? f.soft : f.soft ? `${k} is required by the CNCF DSL (the engine assumes a default)` : `${k} is required`);
      out.push(c);
    }
    for (const [k, p] of pairs) {
      if (k in fields) continue;
      const c: ONode = { id: id([...path, k]), path: [...path, k], key: k, type: '?', required: false, present: true, scalar: false, line: this.line(p.key), problems: [], children: [] };
      Object.assign(c, preview(p.value));
      if (!sh.open && !k.startsWith('_')) this.flag(c, 'warning', `${k} is not a key here in the CNCF DSL`);
      out.push(c);
    }
    return out;
  }

  /// A task: its type's keys first, then the fields every task has.
  task(node: ONode, n: unknown, depth: number) {
    if (!YAML.isMap(n)) { this.flag(node, 'error', 'a task is a map: its type (call, set, switch…) and fields'); return; }
    const keys = n.items.map(keyOf);
    const types = keys.filter((k) => k in TASK_TYPES && !(k === 'do' && keys.includes('for')));
    if (types.length === 0) {
      node.task = '?';
      this.flag(node, 'error', `no task type — add one of ${TASK_TYPE_KEYS.join(', ')}`);
      node.children = this.object(node.path, obj(COMMON) as any, n, depth);
      return;
    }
    if (types.length > 1 && !(types.includes('try') && types.length === 2 && types.includes('do'))) this.flag(node, 'error', `one task type per task, found ${types.join(' and ')}`);
    const type = types[0];
    let fields: Record<string, Field> = { ...TASK_TYPES[type] };
    if (type === 'call') {
      const callNode = n.items.find((p) => keyOf(p) === 'call');
      const callee = YAML.isScalar(callNode?.value) ? String(callNode!.value.value) : '';
      node.task = callee ? `call ${callee}` : 'call';
      const known = CALL_WITH[callee];
      if (known) fields.with = { ...fields.with, sh: obj(known, true), r: Object.values(known).some((f) => f.r) };
    } else node.task = type;
    // FlowR also reads `do` and `while` inside `for`, and `try` as a map holding
    // `do` and `catch`; accepted, with a note that the DSL writes them otherwise.
    const at = (k: string) => n.items.find((p) => keyOf(p) === k)?.value;
    const forNode = at('for'), tryNode = at('try');
    if (type === 'for' && YAML.isMap(forNode) && forNode.items.some((p) => ['do', 'while'].includes(keyOf(p)))) {
      const inner = (fields.for.sh as Extract<Shape, { s: 'obj' }>).f;
      fields = { ...fields, for: { ...fields.for, sh: obj({ ...inner, while: fields.while, do: fields.do }) }, do: { ...fields.do, r: false } };
      this.flag(node, 'warning', 'the CNCF DSL puts do and while beside for, not inside it (FlowR reads both)');
    }
    if (type === 'try' && YAML.isMap(tryNode)) {
      fields = { ...fields, try: { ...fields.try, sh: obj({ do: R(TASKS, 'do/try'), catch: fields.catch }) }, catch: { ...fields.catch, r: false } };
      this.flag(node, 'warning', 'the CNCF DSL writes try as a list of tasks with catch beside it (FlowR reads both)');
    }
    node.type = 'task';
    node.value = undefined;
    node.children = this.object(node.path, obj({ ...fields, ...COMMON }) as any, n, depth);
    // A task offers its own fields; the other task types are not ghosts.
  }
}

/// The flow's outline against the schema, and its problems.
export function outline(text: string): Outline {
  const lc = new YAML.LineCounter();
  let doc: YAML.Document.Parsed;
  try { doc = YAML.parseDocument(text ?? '', { lineCounter: lc, keepSourceTokens: false }); } catch (e) { return { nodes: [], problems: [], error: (e as Error).message, errorLine: null }; }
  if (doc.errors.length) {
    const e = doc.errors[0];
    const line = e.linePos?.[0]?.line ?? null;
    return { nodes: [], problems: [{ level: 'error', message: `YAML: ${e.message.split('\n')[0]}`, path: [], line }], error: e.message.split('\n')[0], errorLine: line };
  }
  const b = new Builder(lc);
  const root = YAML.isMap(doc.contents) ? doc.contents : new YAML.YAMLMap();
  const nodes = b.object([], ROOT as any, root, 0);
  // `then:` names a task, or continue / exit / end.
  const walk = (ns: ONode[]) => {
    for (const n of ns) {
      if (n.key === 'then' && n.present && n.scalar && n.value && !['continue', 'exit', 'end'].includes(n.value) && !isExpr(n.value) && !b.taskNames.has(n.value)) b.flag(n, 'error', `no task named ${n.value}`);
      walk(n.children);
    }
  };
  walk(nodes);
  return { nodes, problems: b.problems, error: null, errorLine: null };
}

// ── Edits ─────────────────────────────────────────────────────────────
function edit(text: string, f: (d: YAML.Document) => void): string {
  const d = YAML.parseDocument(text || '{}');
  f(d);
  return d.toString();
}

/// Set a key (adding it if absent), layout and comments kept.
export function setAt(text: string, path: Path, value: unknown): string {
  return edit(text, (d) => d.setIn(path, d.createNode(value)));
}
export function removeAt(text: string, path: Path): string {
  return edit(text, (d) => {
    // A named item (`[…, i, name]`) goes with its list entry.
    const last = path[path.length - 1];
    const before = path[path.length - 2];
    if (typeof last === 'string' && typeof before === 'number') d.deleteIn(path.slice(0, -1));
    else d.deleteIn(path);
  });
}
/// Rename a key in place (a map key, or a task's / case's name).
export function renameAt(text: string, path: Path, to: string): string {
  return edit(text, (d) => {
    const parent = path.length > 1 ? d.getIn(path.slice(0, -1), true) : d.contents;
    if (!YAML.isMap(parent)) return;
    const p = parent.items.find((x) => keyOf(x) === String(path[path.length - 1]));
    if (p) p.key = d.createNode(to) as any;
  });
}
/// Add a value at the end of a list; for a list of named items, under a name
/// not yet used there (`base`, `base-2`…). Returns the new item's path.
export function appendAt(text: string, path: Path, value: unknown, base?: string): { text: string; path: Path } {
  let at: Path = path;
  const out = edit(text, (d) => {
    let seq = d.getIn(path, true);
    if (!YAML.isSeq(seq)) { d.setIn(path, d.createNode([])); seq = d.getIn(path, true); }
    const s = seq as YAML.YAMLSeq;
    if (base === undefined) { s.items.push(d.createNode(value)); at = [...path, s.items.length - 1]; return; }
    const taken = new Set(s.items.flatMap((it) => (YAML.isMap(it) ? it.items.map(keyOf) : [])));
    const clean = base.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'item';
    let name = clean;
    for (let i = 2; taken.has(name); i++) name = `${clean}-${i}`;
    s.items.push(d.createNode({ [name]: value }));
    at = [...path, s.items.length - 1, name];
  });
  return { text: out, path: at };
}
/// Add a key to a map under a name not yet used there.
export function addKeyAt(text: string, path: Path, base: string, value: unknown): { text: string; path: Path } {
  let at: Path = path;
  const out = edit(text, (d) => {
    const m = path.length ? d.getIn(path, true) : d.contents;
    const taken = new Set(YAML.isMap(m) ? m.items.map(keyOf) : []);
    let name = base;
    for (let i = 2; taken.has(name); i++) name = `${base}${i}`;
    d.setIn([...path, name], d.createNode(value));
    at = [...path, name];
  });
  return { text: out, path: at };
}

/// The task types a "+" on a task list offers (the palette's steps).
export const TASK_CHOICES = PALETTE;
