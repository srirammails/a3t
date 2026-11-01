<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, highlightActiveLine, rectangularSelection } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { yaml as yamlLang } from '@codemirror/lang-yaml';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, foldKeymap, indentOnInput } from '@codemirror/language';
  import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
  import { autocompletion, completionKeymap } from '@codemirror/autocomplete';

  /// `reveal`: a line to select and scroll to; `n` makes a repeat request a change.
  let { value = '', onchange, readOnly = false, reveal = null }:
    { value: string; onchange?: (val: string) => void; readOnly?: boolean; reveal?: { line: number; n: number } | null } = $props();

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
        lineNumbers(),
        highlightActiveLineGutter(),
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
    goto(reveal);
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
</style>
