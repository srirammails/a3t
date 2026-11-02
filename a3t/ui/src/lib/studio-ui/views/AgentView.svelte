<script lang="ts">
  /// An agent: what it runs — its template's workflow, read-only, with this
  /// agent's values filled in. Its settings (as the template exposes them) are
  /// in the Form panel; the template itself opens from here.
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import SaveBar from '../parts/SaveBar.svelte';
  import EditButton from '../parts/EditButton.svelte';
  import { agentModel, fromText, type Doc, type Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const template = $derived(doc.data?.template ?? null);
  const value = $derived((fromText(doc.text).value ?? {}) as any);
  /// The config it runs: the template's, with the agent's own values over it
  /// (the template's LLM picker keys on the bare resource name).
  const seed = $derived.by(() => {
    const cfg: Record<string, unknown> = { ...agentModel.templateConfig(template?.spec_yaml ?? ''), ...(value.settings ?? {}) };
    if (typeof cfg.llm_resource === 'string') cfg.llm_resource = cfg.llm_resource.split('/').pop();
    return cfg;
  });

  $effect(() => {
    if (!doc.loaded && !doc.isNew) void workspace.load(doc);
  });
  function openTemplate() {
    if (template) workspace.open({ project: doc.ref.project, kind: 'template', name: template.name, id: template.id });
  }
</script>

<div class="av">
  <div class="head">
    <span class="kind">{kind.label}</span>
    <span class="mono">{value.name ?? doc.ref.name}</span>
    {#if value.status}<span class="pill {value.status}">{value.status}</span>{/if}
    {#if template}<button class="chip" onclick={openTemplate} title="Open its template">▤ {template.name} v{template.version}</button>{/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    <SaveBar {doc} {workspace} />
  </div>
  {#if !doc.loaded}
    <div class="note">Loading {doc.ref.name}…</div>
  {:else if doc.error}
    <div class="err">Could not load {doc.ref.name}: {doc.error}</div>
  {:else if !template && doc.isNew}
    <div class="note">Pick its template in the Form panel; its workflow shows here, with the agent's values.</div>
  {:else if !template}
    <div class="err">Its template ({doc.data?.agent?.template_id}) is not listed — the agent cannot be shown against it.</div>
  {:else}
    <div class="graph">
      {#key JSON.stringify(seed)}
        <FlowEditorPane part="main" aiKind="agent" aiName={value.name ?? doc.ref.name} initialYaml={template.spec_yaml} readOnly configSeed={seed} />
      {/key}
    </div>
  {/if}
</div>

<style>
  .av { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .pill { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; background: var(--bg-subtle); color: var(--text-2); }
  .pill.active { background: var(--runtime-bg, #e3f4ef); color: var(--runtime, #1a7158); }
  .chip { font-family: var(--font-mono); font-size: 11.5px; padding: 2px 8px; border-radius: 4px; border: 0; background: var(--spec-bg); color: var(--spec-ink, var(--spec)); cursor: pointer; }
  .grow { flex: 1; }
  .graph { flex: 1; min-height: 0; display: flex; }
  .graph > :global(*) { flex: 1; }
  .note { padding: 14px; color: var(--text-4); font-size: 12.5px; }
  .err { margin: 10px 14px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; }
</style>
