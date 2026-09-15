<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, highlightActiveLine, rectangularSelection, placeholder as cmPlaceholder } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { sql, PostgreSQL, MySQL, StandardSQL } from '@codemirror/lang-sql';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, foldKeymap, indentOnInput } from '@codemirror/language';
  import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
  import { autocompletion, completionKeymap } from '@codemirror/autocomplete';

  let {
    value = '',
    dialect = 'standard',
    placeholder = '',
    onchange,
    onexecute,
  }: {
    value: string;
    dialect?: 'postgres' | 'mysql' | 'standard';
    placeholder?: string;
    onchange?: (val: string) => void;
    onexecute?: () => void;
  } = $props();

  let editorContainer: HTMLDivElement;
  let view: EditorView;
  // See YamlEditor: true only while we push an external `value` in, so the
  // synchronous update listener can tell a programmatic apply from a user edit.
  let applyingExternal = false;

  const dialects: Record<string, any> = {
    postgres: PostgreSQL,
    mysql: MySQL,
    standard: StandardSQL,
  };

  const theme = EditorView.theme({
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
    '.cm-placeholder': { color: 'var(--text-4, #a8a396)', fontStyle: 'italic' },
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
          {
            key: 'Mod-Enter',
            run: () => { onexecute?.(); return true; },
          },
        ]),
        sql({ dialect: dialects[dialect] ?? StandardSQL }),
        cmPlaceholder(placeholder),
        theme,
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !applyingExternal) {
            onchange?.(update.state.doc.toString());
          }
        }),
      ],
    });

    view = new EditorView({ state, parent: editorContainer });
  });

  onDestroy(() => {
    view?.destroy();
  });

  $effect(() => {
    if (!view) return;
    const current = view.state.doc.toString();
    if (value === current) return;
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

<div class="sql-editor" bind:this={editorContainer}></div>

<style>
  .sql-editor {
    height: 100%;
    width: 100%;
    background: var(--bg-surface, #ffffff);
    overflow: hidden;
  }
</style>
