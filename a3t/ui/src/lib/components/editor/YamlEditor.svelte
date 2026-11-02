<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { gutter, GutterMarker, Decoration } from '@codemirror/view';
  import { StateField, StateEffect, RangeSet, type Range } from '@codemirror/state';
  import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, highlightActiveLine, rectangularSelection } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { yaml as yamlLang } from '@codemirror/lang-yaml';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, foldKeymap, indentOnInput } from '@codemirror/language';
  import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
  import { autocompletion, completionKeymap } from '@codemirror/autocomplete';

  /// A step's line, as the host marks it: its breakpoint (`before` | `after` |
  /// `on_error`), its state in the shown run, whether the run stands there.
  type LineMark = { line: number; bp?: string; state?: string; current?: boolean };

  /// `reveal`: a line to select and scroll to; `n` makes a repeat request a change.
  /// `marks`: step lines — with them comes a breakpoint gutter (`ongutter` on a
  /// click), line tints, and the current line kept in view. `oncontext`: a
  /// right-click on a line; return true to take it over from the browser menu.
  /// `issues`: problems the host found (the CNCF schema's), tinted on their
  /// lines with the message on hover.
  type Issue = { line: number; level: 'error' | 'warning'; message: string };
  let { value = '', onchange, readOnly = false, reveal = null, marks = null, ongutter, oncontext, issues = null }: {
    value: string; onchange?: (val: string) => void; readOnly?: boolean; reveal?: { line: number; n: number } | null;
    marks?: LineMark[] | null; ongutter?: (line: number) => void; oncontext?: (e: MouseEvent, line: number) => boolean;
    issues?: Issue[] | null;
  } = $props();

  // ── Problems ──
  const setIssues = StateEffect.define<Issue[]>();
  const issuesField = StateField.define<Issue[]>({
    create: () => [],
    update(v, tr) {
      for (const e of tr.effects) if (e.is(setIssues)) return e.value;
      return v;
    },
  });
  const issueLines = EditorView.decorations.compute([issuesField], (state) => {
    const by = new Map<number, Issue[]>();
    for (const i of state.field(issuesField)) if (i.line >= 1 && i.line <= state.doc.lines) by.set(i.line, [...(by.get(i.line) ?? []), i]);
    const out: Range<Decoration>[] = [...by].sort((a, b) => a[0] - b[0]).map(([line, is]) => {
      const err = is.some((i) => i.level === 'error');
      return Decoration.line({ class: `cm-issue ${err ? 'cm-issue-error' : 'cm-issue-warning'}`, attributes: { title: is.map((i) => `${i.level === 'error' ? '✗' : '!'} ${i.message}`).join('\n') } }).range(lineOf(state, line).from);
    });
    return Decoration.set(out);
  });

  // ── Step marks ──
  const setMarks = StateEffect.define<LineMark[]>();
  const marksField = StateField.define<LineMark[]>({
    create: () => [],
    update(v, tr) {
      for (const e of tr.effects) if (e.is(setMarks)) return e.value;
      return v;
    },
  });
  class BpMarker extends GutterMarker {
    kind: string;
    constructor(kind: string) { super(); this.kind = kind; }
    eq(o: BpMarker) { return o.kind === this.kind; }
    toDOM() {
      const el = document.createElement('span');
      el.className = `cm-bp cm-bp-${this.kind}`;
      el.title = this.kind === 'step' ? 'Add a breakpoint' : 'Breakpoint — click to remove';
      return el;
    }
  }
  const lineOf = (state: EditorState, n: number) => state.doc.line(Math.max(1, Math.min(n, state.doc.lines)));
  const sorted = (ms: LineMark[]) => [...ms].filter((m) => m.line >= 1).sort((a, b) => a.line - b.line).filter((m, i, a) => i === 0 || a[i - 1].line !== m.line);
  const stepGutter = gutter({
    class: 'cm-bp-gutter',
    markers: (v) => RangeSet.of(sorted(v.state.field(marksField)).filter((m) => m.line <= v.state.doc.lines)
      .map((m) => new BpMarker(m.bp ?? 'step').range(lineOf(v.state, m.line).from))),
    initialSpacer: () => new BpMarker('spacer'),
    domEventHandlers: {
      mousedown(view, block) {
        const n = view.state.doc.lineAt(block.from).number;
        if (!view.state.field(marksField).some((m) => m.line === n)) return false;
        ongutter?.(n);
        return true;
      },
    },
  });
  const stepLines = EditorView.decorations.compute([marksField], (state) => {
    const out: Range<Decoration>[] = [];
    for (const m of sorted(state.field(marksField))) {
      if (m.line > state.doc.lines || !(m.state || m.current)) continue;
      out.push(Decoration.line({ class: `cm-st cm-st-${m.state ?? 'none'}${m.current ? ' cm-here' : ''}` }).range(lineOf(state, m.line).from));
    }
    return Decoration.set(out);
  });
  const contextMenu = EditorView.domEventHandlers({
    contextmenu(e, view) {
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos === null || !oncontext) return false;
      if (!oncontext(e, view.state.doc.lineAt(pos).number)) return false;
      e.preventDefault();
      return true;
    },
  });

  let editorContainer: HTMLDivElement;
  let view: EditorView;
  // True only while *we* are pushing an external `value` into the document.
  // CodeMirror calls the update listener synchronously from `view.dispatch`, so
  // without this the programmatic sync looks like a user edit — the old code set
  // its skip flag from inside the listener and never cleared it, which swallowed
  // every *other* external update (form edits stopped reaching the Code view
  // whenever both panes were mounted side by side).
  let applyingExternal = false;

  // Light theme matching A3T UI
  const a3tTheme = EditorView.theme({
    '&': { height: '100%', fontSize: '12.5px', fontFamily: 'var(--font-mono, "SF Mono", Menlo, monospace)' },
    '.cm-content': { padding: '10px 0', caretColor: 'var(--text-1, #1a1a1a)' },
    '.cm-cursor': { borderLeftColor: 'var(--text-1, #1a1a1a)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: 'rgba(85, 48, 168, 0.1)' },
    '.cm-activeLine': { backgroundColor: 'rgba(85, 48, 168, 0.04)' },
    '.cm-activeLineGutter': { backgroundColor: 'rgba(85, 48, 168, 0.04)' },
    '.cm-gutters': { backgroundColor: 'var(--bg-surface, #ffffff)', color: 'var(--text-5, #c8c3b6)', border: 'none', borderRight: '1px solid var(--divider, #f0ece3)', paddingRight: '4px' },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 8px 0 12px', minWidth: '28px' },
    '.cm-foldGutter .cm-gutterElement': { padding: '0 4px' },
    '.cm-line': { padding: '0 12px' },
    '.cm-scroller': { overflow: 'auto' },
  });

  onMount(() => {
    const state = EditorState.create({
      doc: value,
      extensions: [
        marksField,
        issuesField,
        issueLines,
        stepGutter,
        lineNumbers(),
        highlightActiveLineGutter(),
        stepLines,
        contextMenu,
        highlightSpecialChars(),
        history(),
        foldGutter(),
        drawSelection(),
        indentOnInput(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        bracketMatching(),
        autocompletion(),
        rectangularSelection(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...foldKeymap,
          ...searchKeymap,
          ...completionKeymap,
          indentWithTab,
        ]),
        yamlLang(),
        a3tTheme,
        EditorState.readOnly.of(readOnly),
        EditorView.editable.of(!readOnly),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !applyingExternal) {
            onchange?.(update.state.doc.toString());
          }
        }),
      ],
    });

    view = new EditorView({ state, parent: editorContainer });
    ready = true;
    goto(reveal);
  });

  let ready = $state(false);
  // Marks follow the host; the run's current line is kept in view (without
  // moving the caret).
  let lastCurrent: number | null = null;
  $effect(() => {
    if (!ready) return;
    const ms = marks ?? [];
    const cur = ms.find((m) => m.current)?.line ?? null;
    const effects: StateEffect<unknown>[] = [setMarks.of($state.snapshot(ms) as LineMark[])];
    if (cur && cur !== lastCurrent && cur <= view.state.doc.lines) effects.push(EditorView.scrollIntoView(lineOf(view.state, cur).from, { y: 'nearest', yMargin: 60 }));
    lastCurrent = cur;
    view.dispatch({ effects });
  });

  $effect(() => {
    if (!ready) return;
    view.dispatch({ effects: setIssues.of($state.snapshot(issues ?? []) as Issue[]) });
  });

  function goto(r: { line: number } | null) {
    if (!view || !r) return;
    const line = view.state.doc.line(Math.max(1, Math.min(r.line, view.state.doc.lines)));
    view.dispatch({ selection: { anchor: line.from, head: line.to }, effects: EditorView.scrollIntoView(line.from, { y: 'center' }) });
    view.focus();
  }
  $effect(() => goto(reveal));

  onDestroy(() => {
    view?.destroy();
  });

  // Sync external value changes into the editor. `value` is the only tracked
  // dependency; a no-op when the document already matches, so typing in here
  // never round-trips through a full-document replace (which would drop the
  // caret).
  $effect(() => {
    if (!view) return;
    const current = view.state.doc.toString();
    if (value === current) return;
    // Keep the caret where it was rather than collapsing it to the start.
    const anchor = Math.min(view.state.selection.main.anchor, value.length);
    const head = Math.min(view.state.selection.main.head, value.length);
    applyingExternal = true;
    try {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
        selection: { anchor, head },
      });
    } finally {
      applyingExternal = false;
    }
  });
</script>

<div class="yaml-editor" bind:this={editorContainer}></div>

<style>
  .yaml-editor {
    height: 100%;
    width: 100%;
    /* As a flex/grid child it must be allowed to shrink, or it is sized by the
       document and the editor never gets a bounded box to scroll inside. */
    min-height: 0;
    background: var(--bg-surface, #ffffff);
    border-radius: var(--r-sm, 6px);
    overflow: hidden;
  }

  /* CodeMirror's own scrolling.
     The height in `EditorView.theme({'&': …})` is injected at runtime as a
     StyleModule, which is easy to lose to whatever the host lays out around it —
     the Code view ended up sized by its content, so `.cm-scroller` had nothing to
     scroll and the text was simply clipped. Pin it from the component, where the
     rule is deterministic. `:global` because this DOM belongs to CodeMirror and
     carries no Svelte scope class. */
  .yaml-editor :global(.cm-editor) { height: 100%; max-height: 100%; }
  .yaml-editor :global(.cm-scroller) { overflow: auto; min-height: 0; }

  /* Breakpoint gutter: a hollow dot on hover for any step line, red when set. */
  .yaml-editor :global(.cm-bp-gutter .cm-gutterElement) { width: 16px; display: flex; align-items: center; justify-content: center; cursor: pointer; }
  .yaml-editor :global(.cm-bp) { display: inline-block; width: 9px; height: 9px; border-radius: 50%; }
  .yaml-editor :global(.cm-bp-step) { border: 1px solid transparent; }
  .yaml-editor :global(.cm-gutterElement:hover .cm-bp-step) { border-color: var(--err, #d92d20); opacity: 0.6; }
  .yaml-editor :global(.cm-bp-before) { background: var(--err, #d92d20); }
  .yaml-editor :global(.cm-bp-after) { border: 1.5px solid var(--err, #d92d20); background: linear-gradient(90deg, transparent 50%, var(--err, #d92d20) 50%); }
  .yaml-editor :global(.cm-bp-on_error) { background: var(--err, #d92d20); border-radius: 1px; transform: rotate(45deg) scale(0.85); }
  .yaml-editor :global(.cm-bp-spacer) { visibility: hidden; }

  /* Problems: a tint and a mark at the line's start; the message on hover. */
  .yaml-editor :global(.cm-issue) { position: relative; }
  .yaml-editor :global(.cm-issue-error) { background: rgba(217, 45, 32, 0.07); box-shadow: inset 3px 0 0 var(--err, #d92d20); }
  .yaml-editor :global(.cm-issue-warning) { background: rgba(180, 83, 9, 0.06); box-shadow: inset 3px 0 0 var(--warn, #b45309); }

  /* The shown run: each step line's state, and where the run stands. */
  .yaml-editor :global(.cm-st) { box-shadow: inset 3px 0 0 var(--st, transparent); }
  .yaml-editor :global(.cm-st-done) { --st: var(--ok, #15803d); }
  .yaml-editor :global(.cm-st-failed) { --st: var(--err, #d92d20); background: rgba(217, 45, 32, 0.07); }
  .yaml-editor :global(.cm-st-waiting) { --st: var(--warn, #b45309); }
  .yaml-editor :global(.cm-st-running) { --st: var(--spec, #5530a8); background: rgba(85, 48, 168, 0.08); }
  .yaml-editor :global(.cm-st-paused) { --st: var(--warn, #b45309); background: rgba(180, 83, 9, 0.12); }
  .yaml-editor :global(.cm-here::after) { content: '◀ paused here'; margin-left: 12px; font-size: 10.5px; font-weight: 600; color: var(--warn, #b45309); }
  .yaml-editor :global(.cm-here.cm-st-running::after) { content: '◀ running'; color: var(--spec, #5530a8); }
  .yaml-editor :global(.cm-here.cm-st-waiting::after) { content: '◀ waiting'; }
</style>
