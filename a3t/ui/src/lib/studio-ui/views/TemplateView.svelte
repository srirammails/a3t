<script lang="ts">
  /// An agent template: its workflow as a graph or as code, edited and saved.
  /// The Form panel edits it through the agent template editor; Agent fields
  /// sets what agents may do with each config field; its agents are in the
  /// Agents panel below. Saving asks first when agents are built from it.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import type { Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
</script>

<div class="tpl">
  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else}
    <FlowEditorPane part="main" aiKind="template" aiName={`${doc.ref.project}/${doc.ref.name}`} initialYaml={doc.text ?? ''} gotoLine={doc.ui.gotoLine ?? null}
      onchange={(v: string) => (doc.text = v)} showCode={doc.ui.codeAsked}>
      {#snippet toolbar()}
        <div class="title">
          <span class="kind">{kind.label}</span>
          <span class="mono">{doc.ref.name}{doc.data?.template?.version ? ` v${doc.data.template.version}` : ''}</span>
        </div>
        <span class="grow"></span>
        <SaveBar {doc} {workspace} />
      {/snippet}
    </FlowEditorPane>
  {/if}
</div>

<style>
  .tpl { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .tpl > :global(*) { flex: 1; min-height: 0; }
  .title { display: flex; align-items: center; gap: 6px; min-width: 0; margin-left: 8px; }
  .kind { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); white-space: nowrap; }
  .mono { font-family: var(--font-mono); font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grow { flex: 1; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 10px 14px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
