// toEditorText must hand the designer YAML, not the registry row that wraps it.
//
// The agent/template tools return a row: { id, name, version, tags, spec_yaml }.
// Passing that row through as JSON is what left agent templates with no graph,
// code or form view — FlowEditorPane parses YAML and got a JSON blob.

function toEditorText(raw, format) {
  if (format === 'yaml' && raw && typeof raw === 'object' && !Array.isArray(raw)) {
    const spec = raw.spec_yaml;
    if (typeof spec === 'string' && spec.trim()) return spec.trim();
  }
  let text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
  if (!text) return '';
  if (format === 'yaml' && text.includes('```')) {
    const body = text.includes('```yaml') ? text.split('```yaml')[1] : text.split('```')[1];
    return (body ?? '').split('```')[0].replace(/^\n/, '').trimEnd();
  }
  if (format === 'json') { try { return JSON.stringify(JSON.parse(text), null, 2); } catch {} }
  return text.trim();
}

const SPEC = "document:\n  dsl: '1.0.3'\n  name: agent-template\ndo:\n- step:\n    set:\n      a: 1";
let pass = 0, fail = 0;
const check = (n, c, extra='') => c ? (pass++, console.log(`  PASS  ${n}`))
                                    : (fail++, console.log(`  FAIL  ${n}  ${extra}`));

// 1. The regression this guards.
{
  const row = { id: 'tpl-agent-template', name: 'agent-template', version: '2.0.0',
                tags: 'core', spec_yaml: SPEC };
  const out = toEditorText(row, 'yaml');
  check('registry row yields its spec_yaml', out === SPEC, JSON.stringify(out.slice(0,40)));
  check('  starts as YAML, not JSON', out.startsWith('document:'), out.slice(0,20));
  check('  metadata is not leaked into the editor', !out.includes('tpl-agent-template'));
}
// 2. Plain YAML still passes through.
{
  check('plain yaml string untouched', toEditorText(SPEC, 'yaml') === SPEC);
}
// 3. A fenced markdown answer (get_flow) still unwraps.
{
  const md = "# Workflow\n\n```yaml\n" + SPEC + "\n```\n";
  check('fenced yaml still unwrapped', toEditorText(md, 'yaml').startsWith('document:'));
}
// 4. A row WITHOUT spec_yaml falls back rather than returning nothing.
{
  const row = { id: 'x', name: 'y' };
  const out = toEditorText(row, 'yaml');
  check('row without a spec falls back to JSON', out.includes('"name"'), out.slice(0,40));
}
// 5. JSON artifacts (agents, resources) are unaffected.
{
  const row = { agent_id: 'agt-1', config: { template_id: 'tpl-agent-engine' }, spec_yaml: SPEC };
  const out = toEditorText(row, 'json');
  check('json format ignores spec_yaml', out.includes('"agent_id"'), out.slice(0,40));
}
// 6. An empty spec_yaml is not treated as a definition.
{
  const row = { name: 'n', spec_yaml: '   ' };
  check('blank spec_yaml falls back', toEditorText(row, 'yaml').includes('"name"'));
}
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
