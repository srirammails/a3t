// Covers the working context (stores/context.svelte.ts): which namespaces a list
// loads for a project, and the `/p/<project>/…` URL prefix.
//
// Mirrors the store's pure parts. The risks being guarded: platform flows leaking
// into a project's lists, shared platform resources (LLMs, ADB, MCP) vanishing from
// pickers when the platform is hidden, and a project URL being misread as an app.

const PROJECTS = [
  { name: 'default', platform: false },
  { name: 'orders', platform: false },
  { name: 'a3t', platform: true },
  { name: 'system', platform: true },
];

function namespacesFor(kind, { project, showPlatform }, projects = PROJECTS) {
  const user = projects.filter((p) => !p.platform).map((p) => p.name);
  const platform = projects.filter((p) => p.platform).map((p) => p.name);
  const base = project ? [project] : user;
  return kind === 'shared' || showPlatform ? [...base, ...platform.filter((n) => !base.includes(n))] : base;
}

function inContext(ns, kind, ctx, projects = PROJECTS) {
  if (!ns) return true;
  return namespacesFor(kind, ctx, projects).includes(ns) || (!ctx.project && !projects.some((p) => p.name === ns));
}

function splitContextPath(path) {
  const m = path.match(/^\/p\/([^/]+)(\/.*)?$/);
  if (!m) return { project: null, rest: path };
  return { project: decodeURIComponent(m[1]), rest: m[2] || '/' };
}

let pass = 0, fail = 0;
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const check = (n, c, extra = '') => c ? (pass++, console.log(`  PASS  ${n}`))
                                      : (fail++, console.log(`  FAIL  ${n}  ${extra}`));

// 1. A project shows only itself; platform hidden.
{
  const ctx = { project: 'orders', showPlatform: false };
  check('project: owned lists load only the project', eq(namespacesFor('owned', ctx), ['orders']));
  check('project: shared lists add the platform', eq(namespacesFor('shared', ctx), ['orders', 'a3t', 'system']));
  check('  a3t flow hidden', !inContext('a3t', 'owned', ctx));
  check('  default flow hidden', !inContext('default', 'owned', ctx));
  check('  a3t LLM still pickable', inContext('a3t', 'shared', ctx));
  check('  conversation (no namespace) shown', inContext(undefined, 'owned', ctx));
}
// 2. All projects: every user project, not the platform.
{
  const ctx = { project: '', showPlatform: false };
  check('all: owned lists every user project', eq(namespacesFor('owned', ctx), ['default', 'orders']));
  check('  system flow hidden', !inContext('system', 'owned', ctx));
  check('  unknown namespace shown (created since the list loaded)', inContext('team-x', 'owned', ctx));
}
// 3. Show platform brings a3t/system into owned lists, once each.
{
  const ctx = { project: 'a3t', showPlatform: true };
  check('platform project: no duplicate', eq(namespacesFor('owned', ctx), ['a3t', 'system']));
  check('show platform + all', eq(namespacesFor('owned', { project: '', showPlatform: true }), ['default', 'orders', 'a3t', 'system']));
}
// 4. URL prefix.
{
  check('/p/orders/chat/x', eq(splitContextPath('/p/orders/chat/x'), { project: 'orders', rest: '/chat/x' }));
  check('/p/orders', eq(splitContextPath('/p/orders'), { project: 'orders', rest: '/' }));
  check('/chat/x has no project', eq(splitContextPath('/chat/x'), { project: null, rest: '/chat/x' }));
  check('/p-ish app path not misread', eq(splitContextPath('/pins'), { project: null, rest: '/pins' }));
  check('encoded project', splitContextPath('/p/team%3Aa/flows').project === 'team:a');
}

console.log(`\n# pass ${pass}\n# fail ${fail}`);
if (fail) process.exit(1);
