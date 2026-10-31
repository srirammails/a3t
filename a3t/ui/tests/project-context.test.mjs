// Covers the working context (stores/context.svelte.ts): which namespaces a list
// loads for a project, and the `/p/<project>/…` URL prefix.
//
// Mirrors the store's pure parts. With a project selected, every list shows that
// project only; pickers still offer the platform's LLMs, MCP servers and flows.
// The risks being guarded: another project's or the platform's items leaking into
// a selected project's lists, platform resources vanishing from pickers, and a
// project URL being misread as an app.

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
  const withPlatform = kind === 'picker' || (showPlatform && !project);
  return withPlatform ? [...base, ...platform.filter((n) => !base.includes(n))] : base;
}

function inContext(ns, kind, ctx, projects = PROJECTS) {
  if (!ns) return true;
  return namespacesFor(kind, ctx, projects).includes(ns) || (!ctx.project && !projects.some((p) => p.name === ns));
}

function itemProject(_kind, ns) { return ns; }
const projectOf = (ns, createdBy) => itemProject('agent', ns, { created_by: createdBy });

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
  check('project: shared lists show the project only', eq(namespacesFor('shared', ctx), ['orders']));
  check('project: pickers still offer the platform', eq(namespacesFor('picker', ctx), ['orders', 'a3t', 'system']));
  check('  a3t flow hidden', !inContext('a3t', 'owned', ctx));
  check('  default flow hidden', !inContext('default', 'owned', ctx));
  check('  a3t resource hidden from the list', !inContext('a3t', 'shared', ctx));
  check('  a3t LLM still pickable', inContext('a3t', 'picker', ctx));
  check('  conversation (no namespace) shown', inContext(undefined, 'owned', ctx));
}
// 2. All projects: every user project, not the platform.
{
  const ctx = { project: '', showPlatform: false };
  check('all: owned lists every user project', eq(namespacesFor('owned', ctx), ['default', 'orders']));
  check('  system flow hidden', !inContext('system', 'owned', ctx));
  check('  built-in templates hidden under all projects without Show platform', !inContext('system', 'shared', ctx));
  check('  ...and shown with it', inContext('system', 'shared', { project: '', showPlatform: true }));
  check('  unknown namespace shown (created since the list loaded)', inContext('team-x', 'owned', ctx));
}
// 3. Show platform brings a3t/system into owned lists, once each.
{
  const ctx = { project: 'a3t', showPlatform: true };
  check('selecting a platform namespace shows only it', eq(namespacesFor('owned', ctx), ['a3t']));
  check('show platform never mixes platform items into a project', eq(namespacesFor('shared', { project: 'orders', showPlatform: true }), ['orders']));
  check('show platform + all', eq(namespacesFor('owned', { project: '', showPlatform: true }), ['default', 'orders', 'a3t', 'system']));
}
// 3b. Items belong where they are stored: users' work saved in a3t is a3t's.
{
  check('user template in a3t stays a3t', projectOf('a3t', 'usr-superadmin') === 'a3t');
  check('user flow in a3t stays a3t', itemProject('flow', 'a3t', { name: 'ops-ping' }) === 'a3t');
  check('a3t items hidden from a selected project', !inContext(itemProject('agent', 'a3t'), 'shared', { project: 'orders', showPlatform: false }));
  check('a3t items not mixed into a project even with Show platform', !inContext(itemProject('agent', 'a3t'), 'shared', { project: 'orders', showPlatform: true }));
  check('a3t items shown under All projects with Show platform', inContext(itemProject('agent', 'a3t'), 'shared', { project: '', showPlatform: true }));
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
