// The Studio's layer rules (flowr-system/.ai/A3T_STUDIO_ARCHITECTURE.md §4):
//   studio-core  plain TypeScript: no Svelte, DOM components, stores or host code,
//                so the VS Code extension can run it;
//   studio-ui    components: no direct network calls (they go through studio-core).
// Run by the Vite build (a violation fails it) and by `npm run check:layers`.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const RULES = [
  {
    dir: 'src/lib/studio-core',
    allow: [/^\.\.?\//, /^yaml$/, /^\$lib\/services\/api$/, /^\$lib\/core\//],
    why: 'studio-core may import only itself, yaml, the API client and $lib/core',
  },
  {
    dir: 'src/lib/studio-ui',
    deny: [/^\$lib\/services\//],
    why: 'studio-ui makes no network calls; use a studio-core adapter',
  },
];

const IMPORT = /\bimport\s+(?:type\s+)?(?:[^'"]*?\sfrom\s+)?['"]([^'"]+)['"]|\bimport\(\s*['"]([^'"]+)['"]\s*\)/g;

function files(dir) {
  let out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out = out.concat(files(p));
    else if (/\.(ts|js|svelte)$/.test(name)) out.push(p);
  }
  return out;
}

export function checkLayers(root) {
  const problems = [];
  for (const rule of RULES) {
    let list;
    try { list = files(join(root, rule.dir)); } catch { continue; }
    for (const f of list) {
      const src = readFileSync(f, 'utf8');
      for (const m of src.matchAll(IMPORT)) {
        const spec = m[1] ?? m[2];
        const bad = rule.allow ? !rule.allow.some((r) => r.test(spec)) : rule.deny.some((r) => r.test(spec));
        if (bad) problems.push(`${relative(root, f)}: imports '${spec}' — ${rule.why}`);
      }
    }
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const problems = checkLayers(join(fileURLToPath(new URL('.', import.meta.url)), '..'));
  if (problems.length) { console.error(problems.join('\n')); process.exit(1); }
  console.log('layers ok');
}
