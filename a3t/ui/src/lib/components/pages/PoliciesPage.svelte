<script lang="ts">
  import NewButton from '$lib/components/shared/NewButton.svelte';
  // EE Policies — a first-class surface for Cedar authorization policies:
  // list / author / hot-reload (no restart) / dry-run test before you commit.
  // Backed by flowr-server /security/policies* + the SoD-gated registry CRUD.
  import { securityPolicies, getEdition, type PolicyTestBody } from '$lib/services/api';
  import DesignerPanel from '$lib/components/shared/DesignerPanel.svelte';
  import { BASELINE_POLICY } from '$lib/data/baselinePolicy';
  import { onMount } from 'svelte';

  const CREATE_NODES = [
    { id: 'details', label: 'Details', icon: '<rect x="2" y="2" width="12" height="12" rx="2"/><path d="M5 6h6M5 9h4"/>' },
    { id: 'policy', label: 'Cedar Policy', icon: '<path d="M8 1l5.5 2.2v4c0 3.5-2.3 6.4-5.5 7.6C4.8 13.6 2.5 10.7 2.5 7.2v-4L8 1z"/>' },
    { id: 'review', label: 'Review & Create', icon: '<path d="M4 8l3 3 5-6"/><circle cx="8" cy="8" r="7"/>' },
  ];
  let createTab = $state('details');

  interface Policy { namespace: string; name: string; cedar: string; description?: string | null; readOnly?: boolean; }

  // The deny-by-default pack compiled into the EE binary (include_str! baseline.cedar).
  // Always applies beneath the registry policies; shown read-only so operators can
  // see the guardrails they can't edit here.
  const BASELINE_ENTRY: Policy = {
    namespace: 'system',
    name: 'baseline (compiled default)',
    cedar: BASELINE_POLICY,
    description: 'Deny-by-default guardrails compiled into the EE runtime. Always active; forbid overrides permit. Read-only.',
    readOnly: true,
  };

  const NEW_CEDAR = '// Cedar policy — forbid overrides permit; deny by default.\n// Example: block a tool by name for everyone.\nforbid(principal, action, resource)\nwhen { resource is Tool && resource has name && resource.name == "some_tool" };\n';

  let loading = $state(true);
  let loadError = $state('');
  let policies = $state<Policy[]>([]);
  let edition = $state<'ce' | 'ee' | null>(null);

  // Baseline is compiled into the EE runtime — surface it (read-only) above the
  // editable registry policies.
  let displayPolicies = $derived(edition === 'ee' ? [BASELINE_ENTRY, ...policies] : policies);

  // Editor state. `selected` is an existing policy being viewed/edited.
  let selected = $state<Policy | null>(null);
  let saveMsg = $state('');
  let saving = $state(false);

  // Create popin (shared SlidePanel — same "add new" experience as elsewhere).
  let showCreate = $state(false);
  let draft = $state({ namespace: 'default', name: '', description: '', cedar: NEW_CEDAR });
  let creating = $state(false);
  let createMsg = $state('');

  // Reload state.
  let reloadMsg = $state('');
  let reloading = $state(false);

  // Test panel state.
  let testRole = $state('Viewer');
  let testAction = $state('mcp:call');
  let testResType = $state('Tool');
  let testResId = $state('list_flows');
  let testUseDraft = $state(true);
  let testIncludeRegistry = $state(true);
  let testResult = $state<any>(null);
  let testing = $state(false);

  const ROLES = ['SuperAdmin', 'SecurityAdmin', 'Admin', 'Operator', 'Viewer'];
  const ACTIONS = ['mcp:call', 'tool:invoke', 'resource:create', 'resource:update', 'resource:delete', 'llm:call', 'flow:start'];
  const RES_TYPES = ['Tool', 'Resource', 'Llm', 'Flow'];

  async function load() {
    loading = true; loadError = '';
    const ed = await getEdition();
    edition = (ed.ok && ed.data?.edition === 'ee') ? 'ee' : 'ce';
    const res = await securityPolicies.list();
    if (res.ok) {
      policies = (res.data?.policies ?? []) as Policy[];
    } else {
      loadError = res.status === 403
        ? 'Policy administration requires an Admin or SecurityAdmin role.'
        : (res.error || 'Failed to load policies');
    }
    loading = false;
  }

  onMount(load);

  function selectPolicy(p: Policy) {
    selected = { ...p };
    saveMsg = '';
  }

  function openCreate() {
    draft = { namespace: 'default', name: '', description: '', cedar: NEW_CEDAR };
    createMsg = ''; createTab = 'details';
    showCreate = true;
  }

  async function createPolicy() {
    if (!draft.name.trim()) { createMsg = 'Name is required'; return; }
    creating = true; createMsg = '';
    const res = await securityPolicies.save(draft.namespace, draft.name.trim(), draft.cedar, draft.description || undefined);
    if (res.ok) {
      showCreate = false;
      await load();
      const s = policies.find(p => p.namespace === draft.namespace && p.name === draft.name.trim());
      if (s) { selected = { ...s }; saveMsg = 'Created — click Reload to apply.'; }
    } else {
      createMsg = res.error || `Create failed (HTTP ${res.status})`;
    }
    creating = false;
  }

  async function save() {
    if (!selected) return;
    saving = true; saveMsg = '';
    const res = await securityPolicies.update(selected.namespace, selected.name, selected.cedar, selected.description || undefined);
    saveMsg = res.ok ? 'Saved — click Reload to apply.' : (res.error || `Save failed (HTTP ${res.status})`);
    saving = false;
  }

  async function remove() {
    if (!selected) return;
    if (!confirm(`Delete policy "${selected.name}"? Reload after to apply.`)) return;
    const res = await securityPolicies.remove(selected.namespace, selected.name);
    if (res.ok) { selected = null; await load(); }
    else saveMsg = res.error || `Delete failed (HTTP ${res.status})`;
  }

  async function reload() {
    reloading = true; reloadMsg = '';
    const res = await securityPolicies.reload();
    if (res.ok && res.data?.reloaded) {
      reloadMsg = `Applied — ${res.data.registry_policies} policy(ies) active.`;
    } else if (res.ok) {
      reloadMsg = res.data?.reason || res.data?.error || 'Not reloaded.';
    } else {
      reloadMsg = res.error || `Reload failed (HTTP ${res.status})`;
    }
    reloading = false;
  }

  async function runTest() {
    testing = true; testResult = null;
    const body: PolicyTestBody = {
      principal: { kind: 'human', user_id: 'test-principal', roles: [testRole] },
      action: testAction,
      resource: { type: testResType, id: testResId },
      include_registry: testIncludeRegistry,
      draft_policies: (testUseDraft && !selected?.readOnly && selected?.cedar?.trim()) ? [selected.cedar] : [],
    };
    const res = await securityPolicies.test(body);
    testResult = res.ok ? res.data : { error: res.error || `HTTP ${res.status}` };
    testing = false;
  }
</script>

<div class="policies">
  <header class="head">
    <div>
      <h1>Policies</h1>
      <p class="sub">Cedar authorization policies. Edit, then <strong>Reload</strong> to apply live — no restart.</p>
    </div>
    <div class="head-actions">
      {#if reloadMsg}<span class="reload-msg">{reloadMsg}</span>{/if}
      <button class="btn" onclick={reload} disabled={reloading}>{reloading ? 'Reloading…' : '↻ Reload'}</button>
      <NewButton label="New policy" accent="spec" onclick={openCreate} />
    </div>
  </header>

  {#if loading}
    <div class="state">Loading…</div>
  {:else if loadError}
    <div class="state err">{loadError}</div>
  {:else if edition === 'ce'}
    <div class="state">
      <p>Cedar policy management is an <strong>Enterprise</strong> feature.</p>
      <p class="sub">You're connected to a Community (CE) runtime, which runs permissively (allow-all). Connect to an EE runtime — Settings → Runtime — to author and enforce policies.</p>
    </div>
  {:else}
    <div class="body">
      <aside class="list">
        {#if policies.length === 0}
          <div class="empty">No custom policies yet — the compiled baseline below still applies.</div>
        {/if}
        {#each displayPolicies as p}
          <button class="pol-card" class:active={selected && selected.namespace === p.namespace && selected.name === p.name} class:baseline={p.readOnly} onclick={() => selectPolicy(p)}>
            <span class="pol-name">{p.name}{#if p.readOnly}<span class="ro-badge">read-only</span>{/if}</span>
            <span class="pol-ns">{p.namespace}</span>
            {#if p.description}<span class="pol-desc">{p.description}</span>{/if}
          </button>
        {/each}
      </aside>

      <main class="main">
        {#if !selected}
          <div class="state">Select a policy, or create a new one.</div>
        {:else}
          <div class="editor">
            <div class="ed-row">
              <label>Name
                <input class="inp" value={selected.name} readonly />
              </label>
              <label>Namespace
                <input class="inp" value={selected.namespace} readonly />
              </label>
            </div>
            <label>Description
              {#if selected.readOnly}
                <input class="inp" value={selected.description} readonly />
              {:else}
                <input class="inp" bind:value={selected.description} placeholder="what this policy enforces" />
              {/if}
            </label>
            <label>Cedar source
              {#if selected.readOnly}
                <textarea class="code" value={selected.cedar} readonly spellcheck="false" rows="18"></textarea>
              {:else}
                <textarea class="code" bind:value={selected.cedar} spellcheck="false" rows="12"></textarea>
              {/if}
            </label>
            {#if selected.readOnly}
              <div class="ed-actions">
                <span class="save-msg">Compiled into the runtime — edit via the source tree, then rebuild. Layer overrides with a <strong>New Policy</strong>.</span>
              </div>
            {:else}
              <div class="ed-actions">
                <button class="btn spec" onclick={save} disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
                <button class="btn danger" onclick={remove}>Delete</button>
                {#if saveMsg}<span class="save-msg">{saveMsg}</span>{/if}
              </div>
            {/if}
          </div>

          <div class="test">
            <h2>Test a decision</h2>
            <p class="sub">Dry-run against the baseline + saved policies{testUseDraft ? ' + the draft above' : ''}. Nothing is mutated.</p>
            <div class="test-grid">
              <label>Principal role
                <select class="inp" bind:value={testRole}>{#each ROLES as r}<option>{r}</option>{/each}</select>
              </label>
              <label>Action
                <select class="inp" bind:value={testAction}>{#each ACTIONS as a}<option>{a}</option>{/each}</select>
              </label>
              <label>Resource type
                <select class="inp" bind:value={testResType}>{#each RES_TYPES as t}<option>{t}</option>{/each}</select>
              </label>
              <label>Resource id
                <input class="inp" bind:value={testResId} placeholder="list_flows" />
              </label>
            </div>
            <div class="test-opts">
              <label class="chk"><input type="checkbox" bind:checked={testUseDraft} /> include the draft above</label>
              <label class="chk"><input type="checkbox" bind:checked={testIncludeRegistry} /> include saved policies</label>
              <button class="btn" onclick={runTest} disabled={testing}>{testing ? 'Testing…' : 'Run Test'}</button>
            </div>
            {#if testResult}
              {#if testResult.error}
                <div class="result err">⚠ {testResult.error}</div>
              {:else}
                <div class="result" class:allow={testResult.allowed} class:deny={!testResult.allowed}>
                  <span class="verdict">{testResult.allowed ? '✓ ALLOW' : '✗ DENY'}</span>
                  {#if testResult.reason}<span class="reason">{testResult.reason}</span>{/if}
                  {#if testResult.policy_ids?.length}<span class="pids">policies: {testResult.policy_ids.join(', ')}</span>{/if}
                  <span class="pids">evaluated {testResult.tested_policies} extra policy(ies)</span>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </main>
    </div>
  {/if}
</div>

<!-- Canonical designer popin: tree on the left, form on the right. -->
<DesignerPanel
  open={showCreate}
  onclose={() => (showCreate = false)}
  icon="P"
  name={draft.name || 'New Policy'}
  meta="cedar"
  nodes={CREATE_NODES}
  activeNode={createTab}
  onselect={(id) => (createTab = id)}
>
  {#if createTab === 'details'}
    <div class="pf">
      <h3>Details</h3>
      <div class="cf-grid">
        <label class="cf-field"><span>Name</span><input bind:value={draft.name} placeholder="deny-list-flows" /></label>
        <label class="cf-field"><span>Namespace</span><input bind:value={draft.namespace} /></label>
      </div>
      <label class="cf-field"><span>Description</span><input bind:value={draft.description} placeholder="what this policy enforces" /></label>
    </div>
  {:else if createTab === 'policy'}
    <div class="pf">
      <h3>Cedar source</h3>
      <p class="pf-hint">forbid overrides permit; deny by default. Test it before saving on the Review tab or the page's Test panel.</p>
      <label class="cf-field"><textarea class="code" bind:value={draft.cedar} spellcheck="false" rows="16"></textarea></label>
    </div>
  {:else}
    <div class="pf">
      <h3>Review &amp; create</h3>
      <div class="pf-row"><span class="pf-k">Name</span><span class="pf-v">{draft.name || '—'}</span></div>
      <div class="pf-row"><span class="pf-k">Namespace</span><span class="pf-v">{draft.namespace}</span></div>
      <div class="pf-row"><span class="pf-k">Description</span><span class="pf-v">{draft.description || '—'}</span></div>
      <pre class="pf-code">{draft.cedar}</pre>
      <div class="create-actions">
        <button class="btn" onclick={() => (showCreate = false)}>Cancel</button>
        <button class="btn spec" onclick={createPolicy} disabled={creating || !draft.name.trim()}>{creating ? 'Creating…' : 'Create Policy'}</button>
        {#if createMsg}<span class="cf-msg err">{createMsg}</span>{/if}
      </div>
    </div>
  {/if}
</DesignerPanel>

<style>
  .policies { display: flex; flex-direction: column; height: 100%; overflow: hidden; color: var(--text-1); }
  .head { display: flex; justify-content: space-between; align-items: flex-start; padding: 20px 24px 16px; border-bottom: 1px solid var(--border); }
  .head h1 { font-family: var(--font-serif, serif); font-size: 24px; font-weight: 500; color: var(--text-1); }
  .sub { font-size: 13px; color: var(--text-3); margin-top: 4px; }
  .head-actions { display: flex; align-items: center; gap: 10px; }
  .reload-msg { font-size: 12px; color: var(--text-3); }
  .btn { padding: 7px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); color: var(--text-2); font-size: 13px; cursor: pointer; }
  .btn:hover { background: var(--bg-subtle); }
  .btn.spec { background: var(--spec-bg, #2b2f45); color: var(--spec-ink, #cdd3ff); border-color: transparent; }
  .btn.danger { color: var(--warn, #d66); border-color: var(--warn, #d66); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  /* Create popin content (inside the shared SlidePanel) */
  .state { padding: 40px; text-align: center; color: var(--text-4); }
  .state.err { color: var(--warn, #d66); }
  .body { display: grid; grid-template-columns: 260px 1fr; flex: 1; overflow: hidden; }
  .list { border-right: 1px solid var(--border); overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 6px; }
  .empty { font-size: 12px; color: var(--text-4); padding: 12px; }
  .pol-card { text-align: left; padding: 10px 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-surface); cursor: pointer; display: flex; flex-direction: column; gap: 2px; }
  .pol-card:hover { background: var(--bg-subtle); }
  .pol-card.active { border-color: var(--spec-ink, #6a75c0); background: var(--bg-subtle); }
  .pol-name { font-weight: 600; font-size: 13px; }
  .pol-ns { font-size: 11px; color: var(--text-4); }
  .pol-desc { font-size: 11px; color: var(--text-3); margin-top: 2px; }
  .pol-card.baseline { border-color: var(--spec-ink, #6a75c0); border-style: dashed; }
  .pol-name { display: flex; align-items: center; gap: 6px; }
  .ro-badge { font-size: 9.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  textarea.code[readonly] { opacity: 0.85; }
  .main { overflow-y: auto; padding: 20px 24px; display: flex; flex-direction: column; gap: 24px; }
  .editor { display: flex; flex-direction: column; gap: 12px; max-width: 820px; }
  .ed-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-3); }
  .inp { padding: 8px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, var(--bg-surface)); color: var(--text-1); font-size: 13px; }
  .inp[readonly] { opacity: 0.7; }
  .code { padding: 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-sunken, #12141c); color: var(--text-1); font-family: var(--font-mono, monospace); font-size: 12.5px; line-height: 1.5; resize: vertical; }
  .ed-actions { display: flex; align-items: center; gap: 10px; }
  .save-msg { font-size: 12px; color: var(--text-3); }
  .test { border-top: 1px solid var(--border); padding-top: 20px; max-width: 820px; }
  .test h2 { font-size: 15px; font-weight: 600; }
  .test-grid { display: grid; grid-template-columns: 1fr 1fr 1fr 1fr; gap: 12px; margin: 12px 0; }
  .test-opts { display: flex; align-items: center; gap: 16px; margin-bottom: 12px; }
  .chk { flex-direction: row; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); }
  .result { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; padding: 12px 14px; border-radius: 6px; font-size: 13px; border: 1px solid var(--border); }
  .result.allow { background: rgba(60,180,110,0.12); border-color: rgba(60,180,110,0.4); }
  .result.deny { background: rgba(210,90,90,0.12); border-color: rgba(210,90,90,0.4); }
  .result.err { background: rgba(210,160,60,0.12); border-color: rgba(210,160,60,0.4); }
  .verdict { font-weight: 700; }
  .reason { color: var(--text-2); }
  .pids { font-size: 11px; color: var(--text-4); font-family: var(--font-mono, monospace); }
</style>
