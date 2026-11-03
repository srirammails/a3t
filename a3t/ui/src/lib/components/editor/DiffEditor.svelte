<script lang="ts">
  /// Two texts side by side with their differences marked (CodeMirror's merge
  /// view). The left one may be editable: then each change has an arrow that
  /// takes the right side's version into the left — a manual merge, one change
  /// at a time. Unchanged stretches fold away.
  import { onMount, onDestroy } from 'svelte';
  import { MergeView } from '@codemirror/merge';
  import { EditorView, lineNumbers, highlightActiveLine, keymap, drawSelection } from '@codemirror/view';
  import { EditorState, type Extension } from '@codemirror/state';
  import { yaml as yamlLang } from '@codemirror/lang-yaml';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language';

  let { left, right, editable = false, onchange }: {
    left: string; right: string; editable?: boolean; onchange?: (text: string) => void;
  } = $props();

  let host: HTMLDivElement;
  let view: MergeView | null = null;
  let pushing = false;

  const theme = EditorView.theme({
    '&': { fontSize: '12.5px', fontFamily: 'var(--font-mono, "SF Mono", Menlo, monospace)' },
    '.cm-gutters': { backgroundColor: 'var(--bg-surface, #fff)', color: 'var(--text-5, #c8c3b6)', border: 'none' },
    '.cm-content': { padding: '6px 0' },
  });
  const base = (ro: boolean): Extension[] => [
    lineNumbers(), drawSelection(), highlightActiveLine(), history(), keymap.of([...defaultKeymap, ...historyKeymap]),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }), yamlLang(), theme,
    EditorState.readOnly.of(ro), EditorView.editable.of(!ro),
  ];

  onMount(() => {
    view = new MergeView({
      a: {
        doc: left,
        extensions: [...base(!editable), EditorView.updateListener.of((u) => { if (u.docChanged && !pushing) onchange?.(u.state.doc.toString()); })],
      },
      b: { doc: right, extensions: base(true) },
      parent: host,
      highlightChanges: true,
      gutter: true,
      revertControls: editable ? 'b-to-a' : undefined,
      collapseUnchanged: { margin: 3, minSize: 8 },
    });
  });
  onDestroy(() => view?.destroy());

  // Follow the texts when they change outside (a merge, a reload).
  function sync(ed: EditorView | undefined, text: string) {
    if (!ed) return;
    const cur = ed.state.doc.toString();
    if (cur === text) return;
    pushing = true;
    try { ed.dispatch({ changes: { from: 0, to: cur.length, insert: text } }); } finally { pushing = false; }
  }
  $effect(() => { const t = left; sync(view?.a, t); });
  $effect(() => { const t = right; sync(view?.b, t); });
</script>

<div class="diff" bind:this={host}></div>

<style>
  .diff { height: 100%; min-height: 0; overflow: auto; background: var(--bg-surface, #fff); }
  .diff :global(.cm-mergeView) { min-height: 100%; }
  .diff :global(.cm-merge-revert button) { cursor: pointer; color: var(--spec, #5530a8); border: 1px solid var(--border); background: var(--bg-surface); border-radius: 3px; }
  .diff :global(.cm-merge-revert button:hover) { background: var(--spec-bg); }
</style>
