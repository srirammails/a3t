<script lang="ts">
  import { untrack } from 'svelte';
  // Right-hand pane for any artifact. Flows get the full designer (graph / code /
  // form); everything else gets the shared editor on its JSON.
  //
  // Saving always goes through the kind's MCP write tool with the user's token,
  // so it passes the same Cedar choke point as any other tool call. There is no
  // direct storage path, and a denial is shown rather than swallowed.
  import { mcpCall } from '$lib/services/api';
  import { navigateToDetail } from '$lib/stores/panels.svelte';
  import FlowEditorPane from '$lib/components/editor/FlowEditorPane.svelte';
  import JsonForm from './JsonForm.svelte';
  import YamlEditor from '$lib/components/editor/YamlEditor.svelte';
  import { ARTIFACTS, toEditorText, idArgsFor, type ArtifactKind } from './artifacts';

  let { artifact, revision, onclose, onapplied, onsection, onask, onedit }: {
    artifact: { kind: ArtifactKind; namespace: string; name: string; text: string; id?: string };
    /// A newer version of the *same* artifact produced by the agent. Applied in
    /// place so a revision refreshes this pane instead of requiring the user to
    /// open a second card — unless they have unsaved edits, which are never
    /// silently discarded.
    revision?: { text: string; seq: number } | null;
    onclose?: () => void;
    onapplied?: (summary: string) => void;
    onsection?: (section: string) => void;
    onask?: (prompt: string) => void;
    /// Reports the *live* editor buffer so the chat host can tell the agent what
    /// the user actually has in front of them. Without this the agent only ever
    /// saw the registry copy and would answer about a flow the user had already
    /// changed.
    onedit?: (state: { text: string; dirty: boolean }) => void;
  } = $props();

  const spec = $derived(ARTIFACTS[artifact.kind]);
  let text = $state(artifact.text);
  let section = $state('graph');
  let saving = $state(false);
  let confirmedOverwrite = $state(false);
  let baseText = $state(artifact.text);
  let appliedSeq = $state(0);
  let offered = $state<{ text: string; seq: number } | null>(null);
  const dirty = $derived(text !== baseText);
  let status = $state<{ kind: 'ok' | 'err'; text: string } | null>(null);

  $effect(() => {
    onedit?.({ text, dirty });
  });

  /// Which view the JSON-shaped artifacts show. Flows have their own tab bar.
  let jsonView = $state<'form' | 'code' | 'graph'>('form');

  /// Plain `let`: the effect below would otherwise read the very state it writes
  /// and retrigger itself — the shape that has frozen components in this app.
  let specFetched = false;
  $effect(() => {
    // First mount only; the change-detector further down covers later switches.
    const a = artifact;
    if (specFetched || a.kind !== 'agent') return;
    specFetched = true;
    untrack(() => loadAgentSpec(a.text));
  });

  /// An agent is a template with its inputs frozen, so its graph IS the template's
  /// graph — which is exactly how the Agents page renders one. Fetched on demand
  /// from `config.template_id`; until it arrives the Graph tab says so rather than
  /// showing an empty canvas.
  let agentSpec = $state<string>('');
  let agentSpecState = $state<'idle' | 'loading' | 'none'>('idle');

  async function loadAgentSpec(jsonText: string) {
    agentSpec = ''; agentSpecState = 'loading';
    let tplId = '';
    try {
      const o = JSON.parse(jsonText);
      tplId = o?.config?.template_id ?? o?.template_id ?? '';
    } catch { /* not JSON yet */ }
    if (!tplId) { agentSpecState = 'none'; return; }
    const res = await mcpCall('get_agent_template', { name: tplId });
    const yaml = res.ok ? toEditorText(res.data, 'yaml') : '';
    // Ignore a late reply for an artifact the user has already navigated away from.
    if (artifact.kind !== 'agent') return;
    agentSpec = yaml;
    agentSpecState = yaml ? 'idle' : 'none';
  }

  /// Identity of the artifact currently loaded into the editor.
  ///
  /// A plain `let`, NOT `$state`: the effect below both reads and writes it, and
  /// as reactive state that makes the effect retrigger itself — the shape that
  /// already froze reactivity once in this app, and here stops the pane reacting
  /// when you open a different item from a list. The template keys off `artifact`
  /// directly instead, so it stays reactive without this being state.
  let loadedKey = `${artifact.kind}:${artifact.namespace}/${artifact.name}`;
  $effect(() => {
    const key = `${artifact.kind}:${artifact.namespace}/${artifact.name}`;
    if (key === loadedKey) return;
    loadedKey = key;
    text = artifact.text; baseText = artifact.text;
    offered = null; confirmedOverwrite = false; status = null;
    // JSON artifacts open on their form; flows on the graph.
    jsonView = 'form';
    if (artifact.kind === 'agent') { specFetched = true; loadAgentSpec(artifact.text); }
    else { specFetched = false; agentSpec = ''; agentSpecState = 'idle'; }
    // Local, then assign: reading `section` back inside the effect would track it
    // and retrigger this effect on its own write.
    const next = artifact.kind === 'flow' ? 'graph'
               : (artifact.kind === 'flow_draft' || artifact.kind === 'template') ? 'code'
               : 'form';
    section = next;
    onsection?.(next);
  });

  $effect(() => {
    const rev = revision;
    if (!rev) return;
    // Depend on `revision` ONLY. The body reads and writes `appliedSeq` and
    // `text`, so tracking them would make this effect retrigger itself — the
    // pattern that stops a component reacting at all.
    untrack(() => {
      if (rev.seq === appliedSeq) return;
      appliedSeq = rev.seq;
      if (rev.text === text) return;
      if (dirty) {
        // Editing in the pane wins until the user chooses; losing typed work to a
        // background refresh would be worse than a stale view.
        offered = rev;
      } else {
        text = rev.text; baseText = rev.text; offered = null;
        status = { kind: 'ok', text: 'Updated with the agent’s revision' };
      }
    });
  });

  function takeRevision() {
    if (!offered) return;
    text = offered.text; baseText = offered.text; offered = null;
    status = { kind: 'ok', text: 'Loaded the agent’s revision' };
  }

  async function save() {
    if (!spec.save) { status = { kind: 'err', text: `No write tool for ${spec.label}` }; return; }
    saving = true; status = null;

    // Stale-write guard: the agent writes through the same tools, so without a
    // check a pane save and an agent edit clobber each other, last write wins.
    const cur = await mcpCall(spec.get, idArgsFor(spec, {
      namespace: artifact.namespace, name: artifact.name, id: artifact.id,
    }));
    if (cur.ok) {
      const onServer = toEditorText(cur.data, spec.format);
      if (spec.draft) {
        if (onServer && !confirmedOverwrite) {
          saving = false;
          confirmedOverwrite = true;
          status = { kind: 'err', text: `A ${artifact.namespace}/${artifact.name} already exists. Press Save again to overwrite it.` };
          return;
        }
      } else if (onServer && onServer !== artifact.text.trim() && onServer !== text.trim()) {
        saving = false;
        status = { kind: 'err', text: `This ${spec.label} changed since it was opened — reopen it before saving.` };
        return;
      }
    }

    const payload = spec.format === 'yaml'
      ? { yaml: text }
      : (() => { try { return JSON.parse(text); } catch { return null; } })();
    if (payload === null) {
      saving = false; status = { kind: 'err', text: 'Invalid JSON — fix it before saving.' }; return;
    }
    const res = await mcpCall(spec.save, payload as Record<string, unknown>);
    saving = false;
    if (res.ok) {
      status = { kind: 'ok', text: 'Saved' };
      baseText = text;
      onapplied?.(spec.draft
        ? `I reviewed and saved the draft as ${artifact.namespace}/${artifact.name}. It is now in the registry.`
        : `I updated the ${spec.label} ${artifact.namespace}/${artifact.name} in the editor. Re-read it before making further changes.`);
    } else {
      status = { kind: 'err', text: res.error ?? 'Save failed' };
    }
  }
</script>

<div class="pane">
  <div class="pane-head">
    <div class="pane-id-inline"><span class="pane-kind">{spec.label}</span>{artifact.namespace}/{artifact.name}</div>
    <div class="pane-actions">
      <button class="pbtn" onclick={() => onask?.(`About ${spec.label} ${artifact.namespace}/${artifact.name} (${section}): `)} title="Ask the agent about this">Ask to change…</button>
      {#if spec.save}
        <button class="pbtn primary" onclick={save} disabled={saving}>
          {saving ? 'Saving…' : (spec.draft ? 'Save to registry' : 'Save')}
        </button>
      {/if}
      {#if artifact.kind === 'flow'}
        <button class="pbtn" title="Open in full editor"
          onclick={() => navigateToDetail('flow-editor', artifact.name, `${artifact.namespace}/${artifact.name}`, { namespace: artifact.namespace, name: artifact.name })}>↗</button>
      {/if}
      <button class="pbtn" onclick={() => onclose?.()} title="Close">✕</button>
    </div>
  </div>
  {#if offered}
    <div class="pane-revision">
      The agent revised this while you were editing.
      <button class="pbtn" onclick={takeRevision}>Load theirs</button>
      <button class="pbtn" onclick={() => (offered = null)}>Keep mine</button>
    </div>
  {/if}
  {#if status}<div class="pane-status {status.kind}">{status.text}</div>{/if}
  <div class="pane-editor">
    {#key `${artifact.kind}:${artifact.namespace}/${artifact.name}:${appliedSeq}`}
      {#if artifact.kind === 'flow' || artifact.kind === 'flow_draft' || artifact.kind === 'template'}
        <!-- Templates are agent flows, so they get the designer too — its form
             is the AgentTemplateEditor with the full sub-tab set.
             Wrapped because `.pane-editor` is a flex column: an unwrapped child
             is content-sized (flex-basis auto, min-height auto), so the designer
             grew past the pane and its inner `overflow: auto` never engaged —
             the Code and Form views simply had no scrollbar. -->
        <div class="pane-fill">
        <FlowEditorPane
          initialYaml={text}
          onchange={(v) => (text = v)}
          singleColumn
          showAi={false}
          onsection={(sec) => { section = sec; onsection?.(sec); }}
        />
        </div>
      {:else}
        <!-- No workflow model behind these, so no graph — but the document's own
             fields make a perfectly good form, which beats hand-editing JSON. -->
        <div class="pane-tabs">
          {#if artifact.kind === 'agent'}
            <button class="pane-tab" class:active={jsonView === 'graph'}
                    onclick={() => { jsonView = 'graph'; onsection?.('graph'); }}>Graph</button>
          {/if}
          <button class="pane-tab" class:active={jsonView === 'form'}
                  onclick={() => { jsonView = 'form'; onsection?.('form'); }}>Form</button>
          <button class="pane-tab" class:active={jsonView === 'code'}
                  onclick={() => { jsonView = 'code'; onsection?.('code'); }}>Code</button>
        </div>
        <div class="pane-tabbed">
          {#if jsonView === 'graph'}
            {#if agentSpec}
              <!-- The template's definition, read-only: editing the agent means
                   editing its bindings (Form), not the template it is built from. -->
              <div class="pane-fill">
                <FlowEditorPane initialYaml={agentSpec} singleColumn showAi={false} readOnly />
              </div>
            {:else}
              <div class="pane-hint">
                {agentSpecState === 'loading' ? 'Loading the agent’s template…'
                  : 'This agent has no template to draw — open its template directly.'}
              </div>
            {/if}
          {:else if jsonView === 'form'}
            <JsonForm value={text} onchange={(v) => (text = v)} />
          {:else}
            <YamlEditor value={text} onchange={(v) => (text = v)} />
          {/if}
        </div>
      {/if}
    {/key}
  </div>
</div>

<style>
  /* `min-height: 0` matters as much as min-width: as a grid item this would
     otherwise be sized by its content, pushing the row past the viewport. */
  .pane { display: flex; flex-direction: column; height: 100%;
          min-width: 0; min-height: 0; overflow: hidden;
          border-left: 1px solid var(--border); background: var(--bg-surface); }
  .pane-head { display: flex; align-items: center; justify-content: space-between;
               gap: 8px; padding: 8px 10px; border-bottom: 1px solid var(--border); }
  .pane-id-inline { display: flex; align-items: center; gap: 7px; min-width: 0;
                    font-family: var(--font-mono); font-size: 12px; color: var(--text-2);
                    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pane-kind.draft { border-color: var(--ephemeral, #7c5cff); color: var(--ephemeral, #7c5cff); }
  .pane-unsaved { font-family: var(--font-sans); font-size: 10px; color: var(--ephemeral, #7c5cff);
                  border: 1px dashed var(--ephemeral, #7c5cff); border-radius: 4px; padding: 1px 5px; }
  .pane-kind { font-size: 9.5px; text-transform: uppercase; letter-spacing: 0.05em;
               color: var(--text-3); border: 1px solid var(--border); border-radius: 4px;
               padding: 1px 5px; font-family: var(--font-sans); }
  .pane-actions { display: flex; gap: 5px; flex-shrink: 0; }
  .pbtn { font-size: 11px; padding: 3px 8px; cursor: pointer; border-radius: 5px;
          border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); }
  .pbtn:hover { color: var(--text-1); border-color: var(--border-strong); }
  .pbtn.primary { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }
  .pbtn:disabled { opacity: 0.5; cursor: not-allowed; }
  .pane-revision { display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
                   padding: 6px 10px; font-size: 11.5px; color: var(--text-2);
                   background: var(--bg-subtle); border-bottom: 1px solid var(--border); }
  .pane-status { padding: 5px 10px; font-size: 11.5px; }
  .pane-status.ok { color: var(--text-2); }
  .pane-status.err { color: var(--err); }
  .pane-editor { flex: 1; min-height: 0; overflow: hidden; background: var(--bg-page);
                 display: flex; flex-direction: column; }
  .pane-tabs {
    flex: 0 0 auto;
    display: flex; gap: 2px; padding: 6px 8px;
    border-bottom: 1px solid var(--border); background: var(--bg-surface);
  }
  .pane-tab {
    padding: 4px 10px; font-size: 11px;
    border: 1px solid transparent; border-radius: var(--r-sm);
    background: transparent; color: var(--text-3); cursor: pointer;
  }
  .pane-tab:hover { color: var(--text-1); }
  .pane-tab.active { color: var(--text-1); background: var(--bg-subtle); border-color: var(--border); }
  .pane-tabbed { flex: 1; min-height: 0; overflow: hidden; }
  .pane-hint { padding: 20px; font-size: 12px; color: var(--text-4); }
  /* Fills the flex column and is allowed to shrink, so whatever it holds gets a
     bounded height and can scroll. `min-height: 0` is the part that matters:
     without it a flex item never shrinks below its content. */
  .pane-fill { flex: 1; min-height: 0; height: 100%; overflow: hidden; }
</style>
