<script lang="ts">
  /// The code view's schema outline: the flow as the CNCF Workflow DSL sees it.
  /// Every key the DSL allows at each place is listed — the ones the flow sets
  /// in full, the rest greyed out, ready to add. Lists end in a +; a key opens to
  /// its own keys; what the schema finds wrong is marked red (errors) or amber
  /// (warnings). Edits go back into the text, which stays the source.
  import * as YAML from 'yaml';
  import { untrack } from 'svelte';
  import { outline, setAt, removeAt, renameAt, appendAt, addKeyAt, TASK_CHOICES, type ONode, type Path } from '$lib/studio-core/flow/schema';

  let { text, onchange, readOnly = false, onreveal }: {
    text: string;
    onchange: (text: string) => void;
    readOnly?: boolean;
    /// A row was picked: show its line in the text.
    onreveal?: (line: number) => void;
  } = $props();

  const o = $derived(outline(text));

  // ── What is open ──
  let open = $state<Record<string, boolean>>({});
  let showGhosts = $state((() => { try { return localStorage.getItem('a3t.outline.ghosts') !== 'off'; } catch { return true; } })());
  $effect(() => { try { localStorage.setItem('a3t.outline.ghosts', showGhosts ? 'on' : 'off'); } catch { /* private window */ } });
  /// At first: the flow's own top keys, its tasks, and the way to each problem.
  let seeded = false;
  $effect(() => {
    const nodes = o.nodes;
    if (seeded || !nodes.length) return;
    seeded = true;
    untrack(() => {
      const next: Record<string, boolean> = {};
      for (const n of nodes) if (n.present) next[n.id] = true;
      const mark = (ns: ONode[], up: string[]): boolean => {
        let any = false;
        for (const n of ns) {
          const bad = n.problems.some((p) => p.level === 'error');
          const below = mark(n.children, [...up, n.id]);
          if (bad || below) { for (const u of up) next[u] = true; any = true; }
        }
        return any;
      };
      mark(nodes, []);
      open = next;
    });
  });
  const isOpen = (n: ONode) => open[n.id] ?? false;
  const toggle = (n: ONode) => (open = { ...open, [n.id]: !isOpen(n) });
  const openPath = (p: Path) => {
    const next = { ...open };
    for (let i = 1; i <= p.length; i++) next[p.slice(0, i).map(String).join('/')] = true;
    open = next;
  };

  let selected = $state<string | null>(null);
  function pick(n: ONode) {
    selected = n.id;
    if (n.line) onreveal?.(n.line);
  }

  // ── Edits ──
  function apply(next: string, reveal?: Path) {
    if (next === text) return;
    onchange(next);
    if (reveal) { openPath(reveal); selected = reveal.map(String).join('/'); }
  }
  /// A greyed key: add it, with its example's value.
  function addGhost(n: ONode) {
    if (readOnly) return;
    apply(setAt(text, n.path, n.starter ?? ''), n.path);
  }
  /// The + at a row's end.
  let taskMenu = $state<{ node: ONode; x: number; y: number } | null>(null);
  function plus(e: MouseEvent, n: ONode) {
    e.stopPropagation();
    if (readOnly || !n.add) return;
    const a = n.add;
    if (a.what === 'task') { taskMenu = { node: n, x: e.clientX, y: e.clientY }; return; }
    if (a.what === 'named') { const r = appendAt(text, n.path, a.value, a.name); apply(r.text, r.path); return; }
    if (a.what === 'key') { const r = addKeyAt(text, n.path, a.name, a.value); apply(r.text, r.path); return; }
    const r = appendAt(text, n.path, a.value); apply(r.text, r.path);
  }
  function addTaskOf(id: string) {
    const m = taskMenu; taskMenu = null;
    if (!m) return;
    const step = TASK_CHOICES.flatMap((g) => g.steps).find((s) => s.id === id);
    if (!step) return;
    const r = appendAt(text, m.node.path, step.task(), step.base);
    apply(r.text, r.path);
  }

  // ── A scalar's value, edited in place ──
  let editing = $state<string | null>(null);
  let draft = $state('');
  function startEdit(e: MouseEvent, n: ONode) {
    if (readOnly || !n.scalar) return;
    e.stopPropagation();
    editing = n.id; draft = n.value ?? '';
  }
  /// Typed text as YAML would read it (numbers, true/false), else as a string.
  function scalarOf(s: string): unknown {
    if (s.trim() === '') return '';
    if (/^\s*\$\{/.test(s)) return s;
    try { const v = YAML.parse(s); return v === null || typeof v === 'object' ? s : v; } catch { return s; }
  }
  function commit(n: ONode) {
    if (editing !== n.id) return;
    editing = null;
    if (draft !== (n.value ?? '')) apply(setAt(text, n.path, scalarOf(draft)));
  }

  // ── Row menu ──
  let menu = $state<{ node: ONode; x: number; y: number } | null>(null);
  function onmenu(e: MouseEvent, n: ONode) {
    e.preventDefault(); e.stopPropagation();
    menu = { node: n, x: e.clientX, y: e.clientY };
  }
  /// Tasks, cases and map entries are named by you; schema keys are not renamed.
  const renamable = (n: ONode) => n.present && (typeof n.path[n.path.length - 2] === 'number' || n.type.startsWith('task') || /^(map|array) ·/.test(parentType(n)));
  function parentType(n: ONode): string {
    const pid = n.path.slice(0, -1).map(String).join('/');
    const find = (ns: ONode[]): ONode | null => { for (const x of ns) { if (x.id === pid) return x; const f = find(x.children); if (f) return f; } return null; };
    return find(o.nodes)?.type ?? '';
  }
  function rename(n: ONode) {
    menu = null;
    const to = prompt(`Rename ${n.key} to`, n.key)?.trim();
    if (to && to !== n.key) apply(renameAt(text, n.path, to), [...n.path.slice(0, -1), to]);
  }
  function remove(n: ONode) { menu = null; apply(removeAt(text, n.path)); }

  const lvl = (n: ONode) => (n.problems.some((p) => p.level === 'error') ? 'error' : n.problems.length ? 'warning' : '');
  /// A collapsed row whose inside has a problem says so.
  const inner = (n: ONode): string => {
    for (const c of n.children) { const l = lvl(c) || inner(c); if (l === 'error') return 'error'; if (l) return l; }
    return '';
  };
  const visible = (ns: ONode[]) => (showGhosts ? ns : ns.filter((n) => n.present));
  const errors = $derived(o.problems.filter((p) => p.level === 'error').length);
  const warnings = $derived(o.problems.length - errors);
</script>

<div class="so" role="tree">
  <div class="bar">
    <span class="title">CNCF DSL</span>
    {#if errors}<span class="cnt err" title="Errors">✗ {errors}</span>{/if}
    {#if warnings}<span class="cnt warn" title="Warnings">! {warnings}</span>{/if}
    {#if !errors && !warnings && !o.error}<span class="cnt ok">✓ valid</span>{/if}
    <span class="grow"></span>
    <label class="tg" title="Show the keys the flow does not set, greyed out"><input type="checkbox" bind:checked={showGhosts} /> all keys</label>
  </div>
  {#if o.error}
    <button class="parse" onclick={() => o.errorLine && onreveal?.(o.errorLine)}>✗ The YAML does not parse{o.errorLine ? ` (line ${o.errorLine})` : ''}: {o.error}</button>
  {:else}
    <div class="rows">
      {#each visible(o.nodes) as n (n.id)}{@render row(n, 0)}{/each}
    </div>
  {/if}
</div>

{#snippet row(n: ONode, depth: number)}
  {@const kids = visible(n.children)}
  {@const l = lvl(n)}
  {@const il = !isOpen(n) ? inner(n) : ''}
  <div class="row {l}" class:ghost={!n.present} class:sel={selected === n.id} style="padding-left: {6 + depth * 14}px"
    role="treeitem" aria-selected={selected === n.id} aria-expanded={kids.length ? isOpen(n) : undefined} tabindex="-1"
    onclick={() => (n.present ? pick(n) : toggle(n))} ondblclick={() => !n.present && addGhost(n)} oncontextmenu={(e) => onmenu(e, n)}
    title={[n.d, ...n.problems.map((p) => `${p.level === 'error' ? '✗' : '!'} ${p.message}`)].filter(Boolean).join('\n\n')}>
    <button class="tw" class:none={!kids.length} onclick={(e) => { e.stopPropagation(); toggle(n); }} aria-label={isOpen(n) ? 'Collapse' : 'Expand'}>{#if kids.length}<svg width="8" height="8" viewBox="0 0 8 8" class:down={isOpen(n)}><path d="M2.5 1l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>{/if}</button>
    <span class="k" class:req={n.required}>{n.key}</span>
    {#if n.task}<span class="task">{n.task}</span>{/if}
    {#if n.present && n.value !== undefined}
      {#if editing === n.id}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="ed" bind:value={draft} autofocus onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => { if (e.key === 'Enter') commit(n); else if (e.key === 'Escape') editing = null; }} onblur={() => commit(n)} />
      {:else}
        <span class="v" class:edit={n.scalar && !readOnly} class:coll={!n.scalar} onclick={(e) => startEdit(e, n)} role="presentation">{n.value}</span>
      {/if}
    {/if}
    <span class="grow"></span>
    {#if l}<span class="mk {l}">{l === 'error' ? '✗' : '!'}</span>{:else if il}<span class="mk {il} dim">{il === 'error' ? '✗' : '!'}</span>{/if}
    <span class="t">{n.type}</span>
    {#if !n.present && !readOnly}
      <button class="add ghost-add" onclick={(e) => { e.stopPropagation(); addGhost(n); }} title="Add {n.key}">+</button>
    {:else if n.add && !readOnly}
      <button class="add" onclick={(e) => plus(e, n)} title={n.add.what === 'task' ? 'Add a task' : n.add.what === 'named' ? `Add a ${n.add.name}` : n.add.what === 'key' ? `Add a ${n.add.name}` : 'Add an item'}>+</button>
    {:else}
      <span class="add-sp"></span>
    {/if}
  </div>
  {#if selected === n.id && (n.d || n.problems.length)}
    <div class="note" style="margin-left: {26 + depth * 14}px">
      {#each n.problems as p}<div class="np {p.level}">{p.level === 'error' ? '✗' : '!'} {p.message}</div>{/each}
      {#if n.d}<div class="nd">{n.d}</div>{/if}
      {#if n.ex}<pre class="nx">{n.ex}</pre>{/if}
    </div>
  {/if}
  {#if isOpen(n)}
    {#each kids as c (c.id)}{@render row(c, depth + 1)}{/each}
  {/if}
{/snippet}

{#if taskMenu}
  <div class="ov" role="presentation" onclick={() => (taskMenu = null)} oncontextmenu={(e) => { e.preventDefault(); taskMenu = null; }}></div>
  <div class="menu tasks" style="left: {Math.min(taskMenu.x, window.innerWidth - 300)}px; top: {Math.min(taskMenu.y, window.innerHeight - 360)}px" role="menu">
    {#each TASK_CHOICES as g (g.title)}
      <div class="mh">{g.title}</div>
      <div class="mg">
        {#each g.steps as s (s.id)}
          <button role="menuitem" onclick={() => addTaskOf(s.id)}><span class="mi">{s.icon}</span>{s.label}</button>
        {/each}
      </div>
    {/each}
  </div>
{/if}

{#if menu}
  {@const n = menu.node}
  <div class="ov" role="presentation" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
  <div class="menu" style="left: {Math.min(menu.x, window.innerWidth - 220)}px; top: {Math.min(menu.y, window.innerHeight - 160)}px" role="menu">
    <div class="mt">{n.key} <small>{n.type}</small></div>
    {#if n.present}
      {#if n.line}<button role="menuitem" onclick={() => { menu = null; pick(n); }}>Go to line {n.line}</button>{/if}
      {#if n.add && !readOnly}<button role="menuitem" onclick={(e) => { menu = null; plus(e, n); }}>Add {n.add.what === 'task' ? 'a task' : n.add.what === 'item' ? 'an item' : `a ${n.add.name}`}</button>{/if}
      {#if !readOnly && renamable(n)}<button role="menuitem" onclick={() => rename(n)}>Rename…</button>{/if}
      {#if !readOnly}<button role="menuitem" class="danger" onclick={() => remove(n)}>Remove {n.key}</button>{/if}
    {:else if !readOnly}
      <button role="menuitem" onclick={() => { menu = null; addGhost(n); }}>Add {n.key}</button>
    {/if}
  </div>
{/if}

<style>
  .so { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-surface); font-size: 12px; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 5px 8px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .title { font-size: 10px; font-weight: 600; letter-spacing: 0.06em; color: var(--spec, #5530a8); text-transform: uppercase; }
  .cnt { font: 600 10.5px var(--font-mono); padding: 1px 6px; border-radius: 8px; }
  .cnt.err { color: var(--err, #b42318); background: var(--err-bg, #fde8e8); }
  .cnt.warn { color: var(--warn, #b45309); background: var(--warn-bg, #fdf1e3); }
  .cnt.ok { color: var(--ok, #15803d); background: rgba(21, 128, 61, 0.08); }
  .grow { flex: 1; }
  .tg { display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--text-3); cursor: pointer; }
  .tg input { width: auto; margin: 0; }
  .parse { margin: 10px; padding: 8px 10px; border: 0; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font: 11.5px var(--font-mono); text-align: left; cursor: pointer; }
  .rows { flex: 1; overflow: auto; padding: 4px 0 24px; }

  .row { display: flex; align-items: center; gap: 6px; height: 22px; padding-right: 6px; cursor: default; white-space: nowrap; border-left: 2px solid transparent; }
  .row:hover { background: var(--bg-subtle); }
  .row.sel { background: var(--spec-bg, #f3effc); }
  .row.error { border-left-color: var(--err, #d92d20); background: rgba(217, 45, 32, 0.05); }
  .row.warning { border-left-color: var(--warn, #b45309); }
  .row.ghost .k, .row.ghost .t { opacity: 0.42; }
  .row.ghost .k { font-style: italic; }
  .row.ghost.error .k { opacity: 1; color: var(--err, #b42318); font-style: normal; }
  .tw { width: 14px; flex-shrink: 0; border: 0; background: none; padding: 0; color: var(--text-4); font-size: 10px; cursor: pointer; }
  .tw.none { cursor: default; }
  .tw svg { transition: transform 0.1s; }
  .tw svg.down { transform: rotate(90deg); }
  .k { font-family: var(--font-mono); color: var(--text-1); font-weight: 500; }
  .k.req::after { content: '*'; color: var(--err, #d92d20); margin-left: 1px; font-weight: 400; }
  .row.ghost .k.req::after { opacity: 1; }
  .task { font: 10.5px var(--font-mono); color: var(--spec, #5530a8); background: var(--spec-bg, #f3effc); padding: 0 5px; border-radius: 3px; }
  .v { font-family: var(--font-mono); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; max-width: 55%; padding: 0 3px; border-radius: 3px; }
  .v.edit { cursor: text; }
  .v.edit:hover { background: var(--bg-surface); box-shadow: inset 0 0 0 1px var(--border); }
  .v.coll { color: var(--text-4); }
  .ed { font: 12px var(--font-mono); padding: 0 4px; height: 18px; flex: 1; min-width: 80px; width: auto; }
  .t { font: 10px var(--font-mono); color: var(--text-4); flex-shrink: 0; }
  .mk { font-weight: 700; font-size: 11px; }
  .mk.error { color: var(--err, #d92d20); }
  .mk.warning { color: var(--warn, #b45309); }
  .mk.dim { opacity: 0.6; }
  .add, .add-sp { width: 18px; height: 18px; flex-shrink: 0; }
  .add { border: 1px solid var(--border); background: var(--bg-surface); border-radius: 4px; color: var(--spec, #5530a8); font-weight: 600; font-size: 13px; line-height: 1; padding: 0; cursor: pointer; }
  .add:hover { border-color: var(--spec); background: var(--spec-bg); }
  .ghost-add { opacity: 0; }
  .row:hover .ghost-add, .row.sel .ghost-add { opacity: 1; }

  .note { margin: 2px 8px 6px; padding: 6px 8px; border-left: 2px solid var(--border); background: var(--bg-subtle); border-radius: 0 4px 4px 0; font-size: 11.5px; color: var(--text-2); white-space: normal; }
  .np.error { color: var(--err, #b42318); }
  .np.warning { color: var(--warn, #b45309); }
  .nd { line-height: 1.45; }
  .nx { margin: 5px 0 0; padding: 5px 7px; background: var(--bg-surface); border-radius: 3px; font: 11px var(--font-mono); color: var(--text-2); white-space: pre; overflow: auto; }

  .ov { position: fixed; inset: 0; z-index: 90; }
  .menu { position: fixed; z-index: 91; min-width: 190px; padding: 4px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); box-shadow: var(--shadow-lg); display: flex; flex-direction: column; }
  .menu.tasks { width: 290px; max-height: 360px; overflow: auto; }
  .menu button { display: flex; align-items: center; gap: 6px; border: 0; background: none; padding: 5px 8px; border-radius: 4px; text-align: left; font-size: 12px; color: var(--text-1); cursor: pointer; }
  .menu button:hover { background: var(--bg-subtle); }
  .menu .danger { color: var(--err, #b42318); }
  .mt { padding: 4px 8px 6px; font: 600 12px var(--font-mono); border-bottom: 1px solid var(--border); margin-bottom: 3px; }
  .mt small { font-weight: 400; color: var(--text-4); margin-left: 4px; }
  .mh { font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-4); padding: 6px 8px 2px; }
  .mg { display: grid; grid-template-columns: 1fr 1fr; }
  .mi { width: 16px; text-align: center; color: var(--spec, #5530a8); font-family: var(--font-mono); font-size: 11px; }
</style>
