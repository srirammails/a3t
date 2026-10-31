<script lang="ts">
  /// Projects: namespaces owned by you (<handle>-<name>) or by an org
  /// (<org>-<name>), who is in each, and the rules it runs under. Everything goes
  /// through the a3t-projects / a3t-profile flows, which decide on the server who
  /// may see and change what; this page only asks. Namespaces from before
  /// projects (such as `default`) have no owner and stay open to everyone.
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import { onMount, untrack } from 'svelte';
  import { getActivePanel, navigateToDetail, navigateTo } from '$lib/stores/panels.svelte.ts';
  import { currentUser } from '$lib/stores/auth';
  import {
    getProjects, getShowPlatform, loadProjects, setProject, roleIn, isMine, getProfile, getMyOrgs, type Project,
  } from '$lib/stores/context.svelte.ts';
  import {
    projectsApi, profileApi, loadDirectory, releasesApi, mcpCall, type ProjectMember, type ProjectRole, type DirectoryUser,
  } from '$lib/services/api';

  const ROLES: ProjectRole[] = ['owner', 'developer', 'operator', 'viewer'];
  const ROLE_HINT: Record<ProjectRole, string> = {
    owner: 'Everything, including members and rules',
    developer: 'Build and run flows, agents and resources',
    operator: 'Deploy and run; no authoring',
    viewer: 'Read only',
  };
  /// Rules a project adds on top of the org baseline. Saved with the project;
  /// enforced once project-scoped Cedar is on.
  const POLICY_PRESETS: { key: string; label: string; hint: string }[] = [
    { key: 'prod_deploy_owner_only', label: 'Only owners deploy to prod', hint: 'Developers and operators deploy to dev and staging.' },
    { key: 'destructive_owner_only', label: 'Destructive tools need an owner', hint: 'Deletes and drops run only when an owner asks.' },
    { key: 'agent_memory_per_user', label: 'Agents remember per user', hint: "An agent's memory of one member isn't visible to another." },
  ];
  const ENVIRONMENTS = ['dev', 'staging', 'prod'];
  const SLUG = /^[a-z0-9]([a-z0-9-]{0,36}[a-z0-9])?$/;
  const HANDLE = /^[a-z0-9]([a-z0-9-]{0,22}[a-z0-9])?$/;

  const visible = $derived(getProjects().filter((p) => !p.platform || getShowPlatform()));
  const mine = $derived(visible.filter((p) => isMine(p)));
  const shared = $derived(visible.filter((p) => !isMine(p)));
  const profile = $derived(getProfile());
  const orgs = $derived(getMyOrgs());

  // ── Panel state ──
  let panel = $state<'closed' | 'create' | 'detail'>('closed');
  let tab = $state<'overview' | 'members' | 'policy' | 'releases' | 'environments'>('overview');
  // Releases: each flow in the project, its versions and the active one.
  let releases = $state<{ name: string; versions: string[]; active: string }[]>([]);
  let releasesLoading = $state(false);
  async function loadReleases(ns: string) {
    releasesLoading = true;
    const r = await mcpCall('list_flows', { namespace: ns });
    const flows: any[] = r.ok ? (r.data?.workflows ?? []) : [];
    const out = [];
    for (const f of flows) {
      const v = await releasesApi.versions(ns, f.name);
      out.push({ name: f.name, versions: v.ok ? (v.data?.versions ?? []) : [f.version], active: v.ok ? v.data?.active : f.version });
    }
    releases = out;
    releasesLoading = false;
  }
  async function activate(name: string, version: string) {
    if (!info) return;
    const r = await releasesApi.activate(info.namespace, name, version);
    say(r.ok ? `${name}: callers without a pin now get ${version}.` : (r.error ?? 'Could not activate'), r.ok);
    await loadReleases(info.namespace);
  }
  $effect(() => { if (tab === 'releases' && info) void loadReleases(info.namespace); });
  let selectedName = $state('');
  let message = $state('');
  let messageOk = $state(false);
  let saving = $state(false);
  // Create
  let owner = $state('');            // "user:<handle>" or "org:<handle>"
  let newName = $state('');
  let newDisplay = $state('');
  let newDesc = $state('');
  let newPrivate = $state(false);
  let handleDraft = $state('');
  // Detail drafts
  let dName = $state('');
  let dDesc = $state('');
  let dPrivate = $state(false);
  let dMembers = $state<ProjectMember[]>([]);
  let dPolicy = $state<Record<string, any>>({});
  // Members
  let directory = $state<DirectoryUser[]>([]);
  let addWho = $state('');
  let addRole = $state<ProjectRole>('developer');
  // Move
  let moveOrg = $state('');
  let moveReport = $state<any>(null);

  const selected = $derived(getProjects().find((p) => p.name === selectedName));
  /// Exposure: the system org exposes to a3t only; the platform orgs label it
  /// shared or training, every other org's is community.
  const isSystem = $derived(selected?.info?.owner_kind === 'org' && selected?.info?.owner === 'system');
  const isPlatformOrg = $derived(selected?.info?.owner_kind === 'org' && (selected?.info?.owner === 'a3t' || isSystem));
  const ex = $derived<any>(dPolicy.exposure ?? { audience: 'none' });
  const info = $derived(selected?.info);
  const canEdit = $derived(!!info && (info.can_admin || info.role === 'owner'));
  const adminOrgs = $derived(orgs.filter((o) => o.role === 'admin'));
  const ownerHandle = $derived(owner.split(':')[1] ?? '');
  const previewNs = $derived(ownerHandle && newName ? `${ownerHandle}-${newName}` : '');

  function say(text: string, ok = false) { message = text; messageOk = ok; }

  function card(p: Project) {
    const role = roleIn(p);
    return {
      name: p.info?.display_name || p.name,
      description: p.info?.description || (p.platform ? 'Platform namespace' : p.info ? `${p.info.owner_kind === 'org' ? 'Org' : 'Personal'} project of ${p.info.owner}` : 'From before projects — open to everyone'),
      tags: [role ?? (p.platform ? 'platform' : 'open'), ...(p.info?.private ? ['private'] : [])],
      meta: `${p.flows ?? 0} flows · ${p.resources ?? 0} resources · ${p.triggers ?? 0} triggers`,
    };
  }

  function openDetail(name: string) { navigateToDetail('projects', 'Projects', name); }

  function showDetail(name: string) {
    const p = getProjects().find((x) => x.name === name);
    if (!p) return;
    selectedName = name;
    dName = p.info?.display_name ?? '';
    dDesc = p.info?.description ?? '';
    dPrivate = !!p.info?.private;
    dMembers = structuredClone($state.snapshot(p.info?.members ?? [])) as ProjectMember[];
    dPolicy = { ...(p.info?.policy ?? {}) };
    moveOrg = ''; moveReport = null;
    say('');
    tab = 'overview';
    panel = 'detail';
  }

  function openCreate() {
    owner = profile ? `user:${profile.handle}` : (orgs[0] ? `org:${orgs[0].handle}` : '');
    newName = ''; newDisplay = ''; newDesc = ''; newPrivate = false;
    handleDraft = profile ? '' : (($currentUser?.email ?? '').split('@')[0] ?? '').toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24);
    say('');
    panel = 'create';
  }

  function closePanel() {
    panel = 'closed';
    if (getActivePanel()?.props?.selectedId || getActivePanel()?.props?.create) navigateTo('projects', 'Projects');
  }

  // The URL drives the panel: /projects/<name> opens it; the switcher's
  // "+ New project" arrives with { create: true }.
  $effect(() => {
    const props = getActivePanel()?.props ?? {};
    const id = props.selectedId as string | undefined;
    const create = !!props.create;
    const n = getProjects().length;
    untrack(() => {
      if (create && panel !== 'create') openCreate();
      else if (id && n && (panel !== 'detail' || selectedName !== id)) showDetail(id);
      else if (!id && !create && panel !== 'closed') panel = 'closed';
    });
  });

  async function claimHandle() {
    const h = handleDraft.trim();
    if (!HANDLE.test(h)) { say('A handle is 2–24 lowercase letters, digits and dashes.'); return; }
    saving = true;
    const res = await profileApi.claim(h, $currentUser?.email, $currentUser?.name);
    saving = false;
    if (!res.ok) { say(res.error ?? 'Could not claim the handle'); return; }
    await loadProjects(true);
    owner = `user:${h}`;
    say(`Your handle is @${h}.`, true);
  }

  async function create() {
    const [kind, handle] = owner.split(':') as ['user' | 'org', string];
    if (!kind || !handle) { say(profile ? 'Choose an owner.' : 'Choose your handle first.'); return; }
    if (!SLUG.test(newName.trim())) { say('A project name is 2–38 lowercase letters, digits and dashes.'); return; }
    saving = true; say('');
    const res = await projectsApi.create({
      owner_kind: kind, owner: handle, name: newName.trim(),
      display_name: newDisplay.trim() || newName.trim(), description: newDesc.trim(), private: newPrivate,
    });
    saving = false;
    if (!res.ok) { say(res.error ?? 'Create failed'); return; }
    const ns = res.data?.project?.namespace ?? `${handle}-${newName.trim()}`;
    await loadProjects(true);
    setProject(ns);
    navigateToDetail('projects', 'Projects', ns);
    tab = 'members';
  }

  async function save() {
    if (!info || !canEdit) return;
    saving = true; say('');
    const res = await projectsApi.update(info.namespace, {
      display_name: dName.trim() || info.name, description: dDesc, private: dPrivate,
      members: $state.snapshot(dMembers) as ProjectMember[], policy: { ...dPolicy },
    });
    saving = false;
    if (!res.ok) { say(res.error ?? 'Save failed'); return; }
    await loadProjects(true);
    say('Saved.', true);
  }

  async function remove() {
    if (!info || !canEdit) return;
    if (!confirm(`Delete project “${info.namespace}”? Its members and rules are removed.`)) return;
    const res = await projectsApi.remove(info.namespace);
    if (!res.ok) { say(res.error ?? 'Delete failed'); return; }
    await loadProjects(true);
    setProject('');
    closePanel();
  }

  async function previewMove() {
    if (!info || !moveOrg) return;
    saving = true; say(''); moveReport = null;
    const res = await projectsApi.move(info.namespace, moveOrg, true);
    saving = false;
    if (!res.ok) { say(res.error ?? 'Preview failed'); return; }
    moveReport = res.data?.report ?? {};
  }

  async function doMove() {
    if (!info || !moveOrg || !moveReport) return;
    saving = true; say('');
    const res = await projectsApi.move(info.namespace, moveOrg, false);
    saving = false;
    if (!res.ok) { say(res.error ?? 'Move failed'); return; }
    const ns = res.data?.project?.namespace ?? moveReport.to;
    await loadProjects(true);
    setProject(ns);
    navigateToDetail('projects', 'Projects', ns);
  }

  function addMember() {
    const q = addWho.trim().toLowerCase();
    const u = directory.find((d) => d.email.toLowerCase() === q || d.id === addWho.trim());
    if (!u) { say('Pick someone from the directory — members are matched by account, not by typed email.'); return; }
    if (dMembers.some((m) => m.id === u.id)) { say(`${u.email || u.id} is already a member.`); return; }
    dMembers = [...dMembers, { kind: 'user', id: u.id, email: u.email, name: u.name, role: addRole }];
    addWho = '';
    say('');
  }

  function removeMember(m: ProjectMember) {
    dMembers = dMembers.filter((x) => x.id !== m.id);
  }

  onMount(async () => {
    await loadProjects(true);
    directory = await loadDirectory();
  });
</script>

<div class="screen-header">
  <div class="breadcrumb">Projects <span class="sep">/</span> All</div>
  <div class="screen-title-row">
    <div class="title-block">
      <SubmenuToggle appType="projects" />
      <h1 class="screen-title">Projects</h1>
    </div>
    <div class="screen-actions">
      <NewButton label="New project" accent="spec" onclick={openCreate} />
    </div>
  </div>
</div>

<div class="screen-body">
  <PageBanner
    headline="Organise work into projects."
    highlightWord="projects"
    subtitle="Yours, or your organisation's — each one a namespace with its members and rules."
    description="A project you create lives under your handle (you-orders); an organisation's under its name (acme-orders). Make it private to keep it to yourself — or, in an org, to its members and admins. Pick one in the top bar and every page shows only its work."
    accent="spec"
  />
  <AppHome appType="projects" embedded />

  {#snippet projectCards(list: Project[])}
    <div class="card-grid">
      {#each list as p (p.name)}
        {@const c = card(p)}
        <EntityCard
          icon={(p.info?.display_name || p.name).slice(0, 2).toUpperCase()}
          color={p.platform ? '#777' : p.info?.owner_kind === 'org' ? '#1a7158' : '#5530a8'}
          name={c.name}
          description={c.description}
          tags={c.tags}
          meta={c.meta}
          project={p.name}
          actionLabel="Work in this project"
          onclick={() => openDetail(p.name)}
          onaction={() => setProject(p.name)}
        />
      {/each}
    </div>
  {/snippet}

  <FoldSection title="My Projects" count={mine.length} hint={profile ? `Yours (@${profile.handle}) and the ones you're a member of` : "Yours and the ones you're a member of"}>
    {@render projectCards(mine)}
  </FoldSection>
  <FoldSection title="Shared Projects" count={shared.length} hint="Your orgs' projects, and namespaces open to everyone">
    {@render projectCards(shared)}
  </FoldSection>
</div>

<SlidePanel open={panel !== 'closed'} onclose={closePanel} onsave={panel === 'detail' && canEdit ? save : undefined}>
  <div class="pj">
    <div class="pj-head">
      <div class="pj-icon">{(panel === 'create' ? (newName || 'NP') : (info?.display_name || selectedName)).slice(0, 2).toUpperCase()}</div>
      <div>
        <h2 class="pj-title">{panel === 'create' ? 'New project' : (info?.display_name || selectedName)}</h2>
        <div class="pj-sub">
          {#if panel === 'create'}Yours or an organisation's{:else}
            <span class="mono">{selectedName}</span> · {info ? `${info.owner_kind === 'org' ? 'org' : 'personal'} · ${info.role}${info.private ? ' · private' : ''}` : (selected?.platform ? 'platform' : 'open to everyone')}
          {/if}
        </div>
      </div>
    </div>

    {#if panel === 'detail' && info}
      <div class="pj-tabs" role="tablist">
        {#each [['overview', 'Overview'], ['members', `Members (${dMembers.length})`], ['policy', 'Policy'], ['releases', 'Releases'], ['environments', 'Environments']] as [key, label]}
          <button class="pj-tab" class:active={tab === key} role="tab" aria-selected={tab === key} onclick={() => (tab = key as any)}>{label}</button>
        {/each}
      </div>
    {/if}

    {#if message}<div class="pj-msg" class:ok={messageOk}>{message}</div>{/if}

    {#if panel === 'create'}
      <div class="pj-form">
        {#if !profile}
          <div class="pj-box">
            <h4>Choose your handle</h4>
            <p class="pj-note">Your personal projects are named after it: <span class="mono">{handleDraft || 'you'}-orders</span>. It's yours for good, shared with no org.</p>
            <div class="pj-row">
              <input class="mono" bind:value={handleDraft} placeholder="your-handle" />
              <button class="pj-btn primary" onclick={claimHandle} disabled={saving || !handleDraft.trim()}>Claim</button>
            </div>
          </div>
        {/if}
        <label><span>Owner</span>
          <select bind:value={owner}>
            {#if profile}<option value={`user:${profile.handle}`}>Me — @{profile.handle}</option>{/if}
            {#each orgs as o}<option value={`org:${o.handle}`}>{o.handle} (org, {o.role})</option>{/each}
          </select>
        </label>
        <label><span>Project name</span>
          <input class="mono" bind:value={newName} placeholder="orders" autocomplete="off" />
          <em>Namespace: <span class="mono">{previewNs || '<owner>-<name>'}</span> — lowercase letters, digits and dashes.</em>
        </label>
        <label><span>Display name</span><input bind:value={newDisplay} placeholder={newName || 'Orders'} /></label>
        <label><span>Description</span><textarea rows="3" bind:value={newDesc} placeholder="What this project delivers"></textarea></label>
        <label class="pj-check">
          <input type="checkbox" bind:checked={newPrivate} />
          <span><b>Private</b><em>{owner.startsWith('org:') ? "Only the project's members and the org's admins and security admins see it." : 'Only you see it.'}</em></span>
        </label>
        <div class="pj-row"><button class="pj-btn primary" onclick={create} disabled={saving || !owner || !newName.trim()}>{saving ? 'Creating…' : 'Create project'}</button></div>
      </div>

    {:else if !info}
      <div class="pj-form">
        <p class="pj-note">{selected?.platform
          ? 'A platform namespace: the shared LLMs, memory and MCP servers every project uses, and A3T’s own flows.'
          : 'This namespace is from before projects, so it has no owner and is open to everyone. New work belongs in a project of yours or your organisation’s.'}</p>
        <div class="pj-facts"><div><span>Holds</span>{selected ? card(selected).meta : ''}</div></div>
        <div class="pj-row"><button class="pj-btn" onclick={() => setProject(selectedName)}>Work in this namespace</button></div>
      </div>

    {:else if tab === 'overview'}
      <div class="pj-form">
        <label><span>Display name</span><input bind:value={dName} disabled={!canEdit} /></label>
        <label><span>Description</span><textarea rows="3" bind:value={dDesc} disabled={!canEdit}></textarea></label>
        <label class="pj-check">
          <input type="checkbox" bind:checked={dPrivate} disabled={!canEdit} />
          <span><b>Private</b><em>{info.owner_kind === 'org' ? "Only its members and the org's admins and security admins see it." : 'Only you see it.'}</em></span>
        </label>
        <div class="pj-facts">
          <div><span>Owner</span>{info.owner_kind === 'org' ? `org ${info.owner}` : `@${info.owner}`}</div>
          <div><span>Created</span>{info.created_at ? new Date(info.created_at).toLocaleDateString() : '—'}</div>
          <div><span>Holds</span>{card(selected!).meta}</div>
        </div>
        <div class="pj-row">
          <button class="pj-btn" onclick={() => setProject(info.namespace)}>Work in this project</button>
          {#if canEdit}<button class="pj-btn danger" onclick={remove}>Delete project</button>{/if}
        </div>

        {#if info.owner_kind === 'user' && info.role === 'owner'}
          <div class="pj-box">
            <h4>Move to an organisation</h4>
            {#if adminOrgs.length === 0}
              <p class="pj-note">You need the admin role in an organisation to move a project into it.</p>
            {:else}
              <p class="pj-note">The namespace becomes <span class="mono">&lt;org&gt;-{info.name}</span>. Flows, resources and triggers move with it, and references inside the project are rewritten. Run history stays under the old name.</p>
              <div class="pj-row">
                <select bind:value={moveOrg} onchange={() => (moveReport = null)}>
                  <option value="">Choose an organisation…</option>
                  {#each adminOrgs as o}<option value={o.handle}>{o.handle}</option>{/each}
                </select>
                <button class="pj-btn" onclick={previewMove} disabled={!moveOrg || saving}>Preview</button>
              </div>
              {#if moveReport}
                <div class="pj-report">
                  <div><b>{moveReport.from}</b> → <b>{moveReport.to}</b></div>
                  <div>{moveReport.flows?.length ?? 0} flows, {moveReport.resources?.length ?? 0} resources, {moveReport.triggers?.length ?? 0} triggers move.</div>
                  {#if moveReport.external_references?.length}
                    <div class="warn">These refer to it from outside and will break: {moveReport.external_references.join(', ')}</div>
                  {:else}
                    <div class="dim">Nothing outside the project refers to it.</div>
                  {/if}
                  <button class="pj-btn primary" onclick={doMove} disabled={saving}>{saving ? 'Moving…' : `Move to ${moveOrg}`}</button>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </div>

    {:else if tab === 'members'}
      <div class="pj-form">
        <table class="pj-table">
          <thead><tr><th>Member</th><th>Role</th><th></th></tr></thead>
          <tbody>
            {#each dMembers as m (m.id)}
              <tr>
                <td>{m.name || m.email || m.id}<div class="dim mono">{m.email || m.id}</div></td>
                <td><select bind:value={m.role} disabled={!canEdit}>{#each ROLES as r}<option value={r}>{r}</option>{/each}</select></td>
                <td>{#if canEdit}<button class="pj-x" onclick={() => removeMember(m)} title="Remove">×</button>{/if}</td>
              </tr>
            {:else}
              <tr><td colspan="3" class="dim">{info.owner_kind === 'org' ? "No named members — the org's admins (and, unless private, all its members) see it." : 'Only you so far.'}</td></tr>
            {/each}
          </tbody>
        </table>
        {#if canEdit}
          <div class="pj-add">
            <input bind:value={addWho} list="pj-users" placeholder="name or email" onkeydown={(e) => { if (e.key === 'Enter') addMember(); }} />
            <datalist id="pj-users">{#each directory as u}<option value={u.email || u.id}>{u.name}{u.source === 'sso' ? ' (SSO)' : ''}</option>{/each}</datalist>
            <select bind:value={addRole} title={ROLE_HINT[addRole]}>{#each ROLES as r}<option value={r}>{r}</option>{/each}</select>
            <button class="pj-btn" onclick={addMember}>Add</button>
          </div>
          <ul class="pj-roles">{#each ROLES as r}<li><b>{r}</b> — {ROLE_HINT[r]}</li>{/each}</ul>
        {/if}
        <p class="pj-note">Members decide who sees the project now. What each role may do inside it takes effect once project-scoped Cedar is on (next step); until then the org-wide roles decide. Save to keep changes.</p>
      </div>

    {:else if tab === 'policy'}
      <div class="pj-form">
        <h4>Runtime</h4>
        <label><span>Where this project's flows run</span>
          <select value={dPolicy.runtime ?? ''} disabled={!canEdit}
                  onchange={(e) => { const v = e.currentTarget.value; dPolicy = { ...dPolicy }; if (v) dPolicy.runtime = v; else delete dPolicy.runtime; }}>
            <option value="">{info.owner_kind === 'org' ? "The organisation's setting" : 'Shared (default)'}</option>
            <option value="isolated">Isolated — only on {info.owner_kind === 'org' ? `${info.owner}'s own runtimes` : 'its own runtime'}</option>
            <option value="shared">Shared runtimes</option>
          </select>
          <em>{info.owner_kind === 'org'
            ? `Isolated runs only on runtimes labelled a3t.io/org=${info.owner}; never on another org's.`
            : `Isolated runs only on a runtime labelled a3t.io/project=${info.namespace}.`} Enforced by FlowR when a flow is deployed.</em>
        </label>
        <h4>Exposure</h4>
        <label><span>Who else may use this project — read, clone and run its flows, resources, triggers and templates; never change them</span>
          <select value={ex.audience ?? 'none'} disabled={!canEdit}
                  onchange={(e) => { const a = e.currentTarget.value; dPolicy = { ...dPolicy, exposure: a === 'none' ? { audience: 'none' }
                    : { ...ex, audience: a, orgs: isSystem ? ['a3t'] : (ex.orgs ?? []), kind: ex.kind ?? (isPlatformOrg ? 'shared' : 'community') } }; }}>
            <option value="none">Not exposed</option>
            <option value="orgs">{isSystem ? 'Org a3t' : 'Selected orgs'}</option>
            {#if !isSystem}<option value="public">All orgs</option>{/if}
          </select>
          {#if isSystem}<em>The system org exposes to a3t only; a3t decides what it passes on.</em>{/if}
        </label>
        {#if ex.audience === 'orgs' && !isSystem}
          <label><span>Org handles</span>
            <input class="mono" value={(ex.orgs ?? []).join(', ')} disabled={!canEdit} placeholder="acme, globex"
                   onchange={(e) => { dPolicy = { ...dPolicy, exposure: { ...ex, orgs: e.currentTarget.value.split(',').map((x) => x.trim()).filter(Boolean) } }; }} />
          </label>
        {/if}
        {#if ex.audience !== 'none' && isPlatformOrg}
          <label><span>Shown to them as</span>
            <select value={ex.kind ?? 'shared'} disabled={!canEdit}
                    onchange={(e) => { dPolicy = { ...dPolicy, exposure: { ...ex, kind: e.currentTarget.value } }; }}>
              <option value="shared">Shared — to use or wire</option>
              <option value="training">Training — examples to learn from and clone</option>
            </select>
          </label>
        {/if}
        <p class="pj-note">{ex.audience === 'none' ? 'Only this project\'s members and its org see it.' : 'Publish items into this project from their cards (Publish); others see them under Community, or From A3T / Training for the platform. Save to apply.'}</p>
        <h4>Project rules</h4>
        {#each POLICY_PRESETS as rule}
          <label class="pj-check">
            <input type="checkbox" disabled={!canEdit} checked={!!dPolicy[rule.key]}
                   onchange={(e) => { dPolicy = { ...dPolicy, [rule.key]: e.currentTarget.checked }; }} />
            <span><b>{rule.label}</b><em>{rule.hint}</em></span>
          </label>
        {/each}
        <p class="pj-note">Saved with the project, enforced once project-scoped Cedar is on (next step). Always in force, from the org baseline: deny by default, agents act only on behalf of a user, destructive tools need an admin, prod deploys need an admin, and InfoSec owns Cedar policy resources — see Security → Policies.</p>
      </div>

    {:else if tab === 'releases'}
      <div class="pj-form">
        <p class="pj-note">Each flow keeps every version released here. Callers that pin one (<span class="mono">run: workflow … version: 1.0.0</span>) keep it; everyone else gets the active one. Activate rolls that back or forward — one release at a time, never all callers' pins.</p>
        {#if releasesLoading}<p class="dim">Loading…</p>
        {:else if releases.length === 0}<p class="dim">No flows in this project yet.</p>
        {:else}
          <table class="pj-table">
            <thead><tr><th>Flow</th><th>Releases</th><th>Active</th></tr></thead>
            <tbody>
              {#each releases as r (r.name)}
                <tr>
                  <td class="mono">{r.name}</td>
                  <td class="mono dim">{r.versions.join(', ')}</td>
                  <td>
                    <select value={r.active} disabled={!canEdit || r.versions.length < 2} onchange={(e) => activate(r.name, e.currentTarget.value)}>
                      {#each r.versions as v}<option value={v}>{v}</option>{/each}
                    </select>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {:else if tab === 'environments'}
      <div class="pj-form">
        <div class="pj-envs">{#each info.environments?.length ? info.environments : ENVIRONMENTS as env}<span class="pj-env {env}">{env}</span>{/each}</div>
        <p class="pj-note">Environments arrive with the next stage: runtimes labelled per environment, the environment switcher in the top bar, per-environment resource values and promotion from dev to prod.</p>
      </div>
    {/if}
  </div>
</SlidePanel>

<style>
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; }
  .screen-actions { display: flex; gap: 8px; }
  .screen-body { flex: 1; overflow-y: auto; padding: 20px 32px 40px; }
  .card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }

  .pj { max-width: 760px; margin: 0 auto; padding: 8px 28px 40px; }
  .pj-head { display: flex; align-items: center; gap: 14px; margin-bottom: 18px; }
  .pj-icon { width: 44px; height: 44px; border-radius: var(--r-sm); background: #5530a8; color: white; display: grid; place-items: center; font: 700 13px var(--font-mono); }
  .pj-title { margin: 0; font-family: var(--font-serif); font-size: 22px; font-weight: 500; }
  .pj-sub { font-size: 12px; color: var(--text-4); margin-top: 2px; }
  .pj-tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); margin-bottom: 18px; }
  .pj-tab { padding: 8px 12px; border: none; background: none; font-size: 13px; color: var(--text-3); cursor: pointer; border-bottom: 2px solid transparent; margin-bottom: -1px; }
  .pj-tab.active { color: var(--spec); border-bottom-color: var(--spec); font-weight: 550; }
  .pj-msg { padding: 8px 12px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12.5px; margin-bottom: 14px; }
  .pj-msg.ok { background: var(--spec-bg); color: var(--spec); }
  .pj-form { display: flex; flex-direction: column; gap: 14px; }
  .pj-form h4 { margin: 0; font-size: 13px; font-weight: 600; color: var(--text-2); }
  .pj-form label { display: flex; flex-direction: column; gap: 5px; font-size: 12px; color: var(--text-3); }
  .pj-form label > span { font-weight: 550; color: var(--text-2); }
  .pj-form label em { font-style: normal; font-size: 11px; color: var(--text-4); }
  .pj-form input, .pj-form textarea, .pj-form select {
    padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--bg-surface); color: var(--text-1); font-size: 13px; font-family: inherit;
  }
  .mono { font-family: var(--font-mono); }
  .dim { color: var(--text-4); font-size: 11px; }
  .warn { color: var(--warn, #b45309); font-size: 12px; }
  .pj-box { border: 1px solid var(--border); border-radius: var(--r); padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; background: var(--bg-surface); }
  .pj-report { display: flex; flex-direction: column; gap: 6px; font-size: 12.5px; color: var(--text-2); }
  .pj-report .pj-btn { align-self: flex-start; }
  .pj-facts { display: grid; gap: 6px; font-size: 12.5px; color: var(--text-2); }
  .pj-facts span { display: inline-block; width: 80px; color: var(--text-4); }
  .pj-row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .pj-btn { padding: 7px 14px; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--bg-surface); font-size: 12.5px; color: var(--text-1); cursor: pointer; }
  .pj-btn:hover:not(:disabled) { border-color: var(--spec); color: var(--spec); }
  .pj-btn.primary { background: var(--spec); border-color: var(--spec); color: white; }
  .pj-btn.primary:hover:not(:disabled) { color: white; opacity: 0.9; }
  .pj-btn.danger:hover:not(:disabled) { border-color: var(--err, #d44); color: var(--err, #d44); }
  .pj-btn:disabled { opacity: 0.5; cursor: default; }
  .pj-note { margin: 0; font-size: 11.5px; color: var(--text-4); line-height: 1.5; }
  .pj-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .pj-table th { text-align: left; font-weight: 550; color: var(--text-4); font-size: 11px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .pj-table td { padding: 8px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: middle; }
  .pj-x { border: none; background: none; color: var(--text-4); font-size: 16px; cursor: pointer; }
  .pj-x:hover { color: var(--err, #d44); }
  .pj-add { display: grid; grid-template-columns: 1fr 120px auto; gap: 8px; }
  .pj-roles { margin: 0; padding-left: 18px; font-size: 11.5px; color: var(--text-3); line-height: 1.6; }
  .pj-check { flex-direction: row !important; align-items: flex-start; gap: 10px !important; }
  .pj-check span { display: flex; flex-direction: column; gap: 2px; font-size: 13px; color: var(--text-1); }
  .pj-check em { font-style: normal; font-size: 11.5px; color: var(--text-4); }
  .pj-envs { display: flex; gap: 8px; }
  .pj-env { padding: 4px 12px; border-radius: 12px; font: 12px var(--font-mono); border: 1px solid var(--border); }
  .pj-env.dev { color: #15803d; border-color: #86efac; }
  .pj-env.staging { color: #b45309; border-color: #fcd34d; }
  .pj-env.prod { color: #b91c1c; border-color: #fca5a5; }
</style>
