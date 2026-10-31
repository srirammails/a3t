<script lang="ts">
  /// Projects: the namespaces people work in, who is in each, and the rules it
  /// runs under. A project is a namespace plus a `type: project` record in it
  /// (services/api.ts `projectsApi`); `default` and other namespaces from before
  /// projects have no record and stay open to everyone.
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import FoldSection from '$lib/components/shared/FoldSection.svelte';
  import EntityCard from '$lib/components/shared/EntityCard.svelte';
  import SlidePanel from '$lib/components/shared/SlidePanel.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import { onMount, untrack } from 'svelte';
  import { getActivePanel, navigateToDetail, navigateTo } from '$lib/stores/panels.svelte.ts';
  import { bumpContext } from '$lib/stores/apps.svelte.ts';
  import { currentUser, userRole } from '$lib/stores/auth';
  import {
    getProjects, getShowPlatform, loadProjects, setProject, canSee, roleIn, type Project,
  } from '$lib/stores/context.svelte.ts';
  import {
    projectsApi, idp, type ProjectRecord, type ProjectMember, type ProjectRole,
  } from '$lib/services/api';

  const ROLES: ProjectRole[] = ['owner', 'developer', 'operator', 'viewer'];
  const ROLE_HINT: Record<ProjectRole, string> = {
    owner: 'Everything, including members and policy',
    developer: 'Build and run flows, agents and resources',
    operator: 'Deploy and run; no authoring',
    viewer: 'Read only',
  };
  /// Rules a project can add on top of the org baseline. Saved with the project;
  /// enforced once project-scoped Cedar is on.
  const POLICY_PRESETS: { key: string; label: string; hint: string }[] = [
    { key: 'prod_deploy_owner_only', label: 'Only owners deploy to prod', hint: 'Developers and operators deploy to dev and staging.' },
    { key: 'destructive_owner_only', label: 'Destructive tools need an owner', hint: 'Deletes and drops run only when an owner asks.' },
    { key: 'agent_memory_per_user', label: 'Agents remember per user', hint: "An agent's memory of one member isn't visible to another." },
    { key: 'members_only', label: 'Members only', hint: 'Hide this project from non-members (admins still see it). Applies in the dash now.' },
  ];
  const ENVIRONMENTS = ['dev', 'staging', 'prod'];
  const SLUG = /^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$/;

  const isAdmin = $derived($userRole === 'superadmin' || $userRole === 'admin');
  const visible = $derived(getProjects().filter((p) => (p.platform ? getShowPlatform() : canSee(p))));
  const mine = $derived(visible.filter((p) => roleIn(p) != null));
  const shared = $derived(visible.filter((p) => roleIn(p) == null));

  // ── Panel state ──
  let panel = $state<'closed' | 'create' | 'detail'>('closed');
  let tab = $state<'overview' | 'members' | 'policy' | 'environments'>('overview');
  let selectedName = $state('');
  let draft = $state<ProjectRecord>({});
  let newSlug = $state('');
  let saving = $state(false);
  let message = $state('');
  let knownUsers = $state<{ id: string; label: string }[]>([]);
  let addKind = $state<'user' | 'group'>('user');
  let addId = $state('');
  let addRole = $state<ProjectRole>('developer');

  const selected = $derived(getProjects().find((p) => p.name === selectedName));
  const myRole = $derived(selected ? roleIn(selected) : null);
  /// Owners and admins change a project; everyone else reads it.
  const canEdit = $derived(isAdmin || myRole === 'owner');
  const resourceCount = (p: Project) => Math.max(0, (p.resources ?? 0) - (p.record ? 1 : 0));
  const isEmpty = (p: Project) => !p.flows && !p.triggers && resourceCount(p) === 0;

  function card(p: Project) {
    const role = roleIn(p);
    return {
      name: p.record?.display_name || p.name,
      description: p.record?.description || (p.platform ? 'Platform namespace' : p.record ? '' : 'Open to everyone — no members yet'),
      tags: [role ?? (p.record ? 'not a member' : 'open'), ...(p.platform ? ['platform'] : [])],
      meta: `${p.flows ?? 0} flows · ${resourceCount(p)} resources · ${p.triggers ?? 0} triggers`,
    };
  }

  function openDetail(name: string) {
    navigateToDetail('projects', 'Projects', name);
  }

  function showDetail(name: string) {
    const p = getProjects().find((x) => x.name === name);
    if (!p) return;
    selectedName = name;
    draft = structuredClone($state.snapshot(p.record) ?? {}) as ProjectRecord;
    draft.members ??= [];
    draft.environments ??= [...ENVIRONMENTS];
    message = '';
    tab = 'overview';
    panel = 'detail';
  }

  function openCreate() {
    newSlug = '';
    draft = { display_name: '', description: '', members: [], environments: [...ENVIRONMENTS] };
    message = '';
    tab = 'overview';
    panel = 'create';
  }

  function closePanel() {
    panel = 'closed';
    if (getActivePanel()?.props?.selectedId || getActivePanel()?.props?.create) navigateTo('projects', 'Projects');
  }

  // The URL drives the panel: /projects/<name> opens it, the switcher's
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

  function explain(res: { status?: number; error?: string }, what: string): string {
    if (res.status === 403) return `${what} needs the Admin role.`;
    if (res.status === 409) return 'That namespace already has a project.';
    return res.error || `${what} failed (HTTP ${res.status ?? '?'})`;
  }

  async function create() {
    const slug = newSlug.trim();
    if (!SLUG.test(slug)) { message = 'Use lowercase letters, digits and dashes (2–40), starting and ending with a letter or digit.'; return; }
    if (getProjects().some((p) => p.name === slug)) { message = `“${slug}” is already in use.`; return; }
    const me = $currentUser;
    if (!me) return;
    saving = true; message = '';
    const record: ProjectRecord = {
      ...draft,
      display_name: draft.display_name?.trim() || slug,
      owner: { id: me.id, email: me.email, name: me.name },
      created_at: new Date().toISOString(),
    };
    const res = await projectsApi.create(slug, record);
    saving = false;
    if (!res.ok) { message = explain(res, 'Creating a project'); return; }
    await loadProjects(true);
    setProject(slug);
    navigateToDetail('projects', 'Projects', slug);
    tab = 'members';
  }

  async function save() {
    if (!selected || !canEdit) return;
    saving = true; message = '';
    const record = $state.snapshot(draft) as ProjectRecord;
    // A namespace from before projects gets its first record, owned by whoever saves it.
    const res = selected.record
      ? await projectsApi.update(selected.name, record)
      : await projectsApi.create(selected.name, { ...record, owner: record.owner ?? { id: $currentUser!.id, email: $currentUser!.email, name: $currentUser!.name }, created_at: new Date().toISOString() });
    saving = false;
    if (!res.ok) { message = explain(res, 'Saving the project'); return; }
    message = 'Saved.';
    await loadProjects(true);
    bumpContext();
  }

  async function remove() {
    if (!selected || !canEdit || !isEmpty(selected)) return;
    if (!confirm(`Delete project “${selected.name}”? Its members and rules are removed; the namespace is already empty.`)) return;
    const res = await projectsApi.remove(selected.name);
    if (!res.ok) { message = explain(res, 'Deleting the project'); return; }
    await loadProjects(true);
    setProject('');
    closePanel();
  }

  function addMember() {
    const id = addId.trim();
    if (!id) return;
    draft.members ??= [];
    if (draft.members.some((m) => m.kind === addKind && m.id === id)) { message = `${id} is already a member.`; return; }
    const known = knownUsers.find((u) => u.id === id);
    draft.members = [...draft.members, { kind: addKind, id, name: known?.label, role: addRole }];
    addId = '';
    message = '';
  }

  function removeMember(m: ProjectMember) {
    draft.members = (draft.members ?? []).filter((x) => !(x.kind === m.kind && x.id === m.id));
  }

  onMount(async () => {
    await loadProjects(true);
    // The identity provider's users, for the member picker (EE, admins). Without
    // it, members are added by email.
    try {
      const us = await idp.users();
      if (us.ok) knownUsers = (us.data?.users ?? []).map((u: any) => ({ id: u.email ?? u.username ?? u.id, label: u.name || u.username || u.email }));
    } catch { /* CE, or not an admin */ }
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
      <NewButton label="New project" accent="spec" onclick={openCreate} disabled={!isAdmin}
                 title={isAdmin ? '' : 'Creating a project needs the Admin role'} />
    </div>
  </div>
</div>

<div class="screen-body">
  <PageBanner
    headline="Organise work into projects."
    highlightWord="projects"
    subtitle="A project is a namespace with its members and rules."
    description="Flows, agents, resources and triggers live in a project. Pick one in the top bar and every page shows only its work. Add people with a role, and set the rules the project runs under."
    accent="spec"
  />
  <AppHome appType="projects" embedded />

  {#snippet projectCards(list: Project[])}
    <div class="card-grid">
      {#each list as p (p.name)}
        {@const c = card(p)}
        <EntityCard
          icon={(p.record?.display_name || p.name).slice(0, 2).toUpperCase()}
          color={p.platform ? '#777' : '#5530a8'}
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

  <FoldSection title="My Projects" count={mine.length} hint="You own or are a member of">
    {@render projectCards(mine)}
  </FoldSection>
  <FoldSection title="Shared Projects" count={shared.length} hint="Projects you can see but aren't a member of">
    {@render projectCards(shared)}
  </FoldSection>
</div>

<SlidePanel open={panel !== 'closed'} onclose={closePanel} onsave={panel === 'create' ? create : (canEdit ? save : undefined)}>
  <div class="pj">
    <div class="pj-head">
      <div class="pj-icon">{(panel === 'create' ? (newSlug || 'NP') : (selected?.record?.display_name || selectedName)).slice(0, 2).toUpperCase()}</div>
      <div>
        <h2 class="pj-title">{panel === 'create' ? 'New project' : (selected?.record?.display_name || selectedName)}</h2>
        <div class="pj-sub">
          {#if panel === 'create'}A namespace, its members and its rules{:else}
            <span class="mono">{selectedName}</span> · {myRole ?? (selected?.record ? 'not a member' : 'open to everyone')}
          {/if}
        </div>
      </div>
    </div>

    {#if panel === 'detail'}
      <div class="pj-tabs" role="tablist">
        {#each [['overview', 'Overview'], ['members', `Members (${draft.members?.length ?? 0})`], ['policy', 'Policy'], ['environments', 'Environments']] as [key, label]}
          <button class="pj-tab" class:active={tab === key} role="tab" aria-selected={tab === key} onclick={() => (tab = key as any)}>{label}</button>
        {/each}
      </div>
    {/if}

    {#if message}<div class="pj-msg" class:ok={message === 'Saved.'}>{message}</div>{/if}

    {#if panel === 'create' || tab === 'overview'}
      <div class="pj-form">
        {#if panel === 'create'}
          <label><span>Namespace</span>
            <input class="mono" bind:value={newSlug} placeholder="orders-app" autocomplete="off" />
            <em>Lowercase letters, digits and dashes. It can't be changed later.</em>
          </label>
        {/if}
        <label><span>Display name</span>
          <input bind:value={draft.display_name} placeholder={panel === 'create' ? (newSlug || 'Orders app') : selectedName} disabled={panel === 'detail' && !canEdit} />
        </label>
        <label><span>Description</span>
          <textarea rows="3" bind:value={draft.description} placeholder="What this project delivers" disabled={panel === 'detail' && !canEdit}></textarea>
        </label>
        {#if panel === 'detail' && selected}
          <div class="pj-facts">
            <div><span>Owner</span>{selected.record?.owner?.name || selected.record?.owner?.email || '— (open project)'}</div>
            <div><span>Created</span>{selected.record?.created_at ? new Date(selected.record.created_at).toLocaleDateString() : '—'}</div>
            <div><span>Holds</span>{card(selected).meta}</div>
          </div>
          <div class="pj-row">
            <button class="pj-btn" onclick={() => setProject(selected.name)}>Work in this project</button>
            {#if canEdit && selected.record}
              <button class="pj-btn danger" onclick={remove} disabled={!isEmpty(selected)}
                      title={isEmpty(selected) ? '' : 'Move or delete its flows, resources and triggers first'}>Delete project</button>
            {/if}
          </div>
          {#if !selected.record && canEdit}
            <p class="pj-note">This namespace has no project record yet, so it's open to everyone. Save to make it a project you own, then add members.</p>
          {/if}
        {:else}
          <p class="pj-note">You become the project's owner. Add members next.</p>
          <div class="pj-row"><button class="pj-btn primary" onclick={create} disabled={saving || !newSlug.trim()}>{saving ? 'Creating…' : 'Create project'}</button></div>
        {/if}
      </div>
    {:else if tab === 'members'}
      <div class="pj-form">
        <table class="pj-table">
          <thead><tr><th>Member</th><th>Kind</th><th>Role</th><th></th></tr></thead>
          <tbody>
            {#if selected?.record?.owner}
              <tr><td>{selected.record.owner.name || selected.record.owner.email}<div class="dim mono">{selected.record.owner.email ?? selected.record.owner.id}</div></td><td>user</td><td>owner</td><td></td></tr>
            {/if}
            {#each draft.members ?? [] as m (m.kind + m.id)}
              <tr>
                <td>{m.name || m.id}{#if m.name}<div class="dim mono">{m.id}</div>{/if}</td>
                <td>{m.kind}</td>
                <td>
                  <select bind:value={m.role} disabled={!canEdit}>
                    {#each ROLES as r}<option value={r}>{r}</option>{/each}
                  </select>
                </td>
                <td>{#if canEdit}<button class="pj-x" onclick={() => removeMember(m)} title="Remove">×</button>{/if}</td>
              </tr>
            {:else}
              {#if !selected?.record?.owner}<tr><td colspan="4" class="dim">No members yet.</td></tr>{/if}
            {/each}
          </tbody>
        </table>

        {#if canEdit}
          <div class="pj-add">
            <select bind:value={addKind}><option value="user">User</option><option value="group">Group</option></select>
            <input bind:value={addId} list="pj-users" placeholder={addKind === 'user' ? 'email' : 'group name (Keycloak)'} onkeydown={(e) => { if (e.key === 'Enter') addMember(); }} />
            <datalist id="pj-users">{#each knownUsers as u}<option value={u.id}>{u.label}</option>{/each}</datalist>
            <select bind:value={addRole} title={ROLE_HINT[addRole]}>{#each ROLES as r}<option value={r}>{r}</option>{/each}</select>
            <button class="pj-btn" onclick={addMember}>Add</button>
          </div>
          <ul class="pj-roles">{#each ROLES as r}<li><b>{r}</b> — {ROLE_HINT[r]}</li>{/each}</ul>
        {/if}
        <p class="pj-note">Membership decides “My” and “Shared” and which projects you're offered today. Permissions per role apply once project-scoped Cedar is on (next step); until then the org roles still decide what anyone may do. Save to keep changes.</p>
      </div>
    {:else if tab === 'policy'}
      <div class="pj-form">
        <h4>Project rules</h4>
        {#each POLICY_PRESETS as rule}
          <label class="pj-check">
            <input type="checkbox" disabled={!canEdit}
                   checked={!!draft.policy?.[rule.key]}
                   onchange={(e) => { draft.policy = { ...(draft.policy ?? {}), [rule.key]: e.currentTarget.checked }; }} />
            <span><b>{rule.label}</b><em>{rule.hint}</em></span>
          </label>
        {/each}
        <p class="pj-note">Saved with the project, enforced once project-scoped Cedar is on (next step). Always in force, from the org baseline: deny by default, agents act only on behalf of a user, destructive tools need an admin, prod deploys need an admin, and InfoSec owns Cedar policy resources — see Security → Policies.</p>
      </div>
    {:else if tab === 'environments'}
      <div class="pj-form">
        <div class="pj-envs">{#each draft.environments ?? ENVIRONMENTS as env}<span class="pj-env {env}">{env}</span>{/each}</div>
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
  .pj-form .mono, .mono { font-family: var(--font-mono); }
  .dim { color: var(--text-4); font-size: 11px; }
  .pj-facts { display: grid; gap: 6px; font-size: 12.5px; color: var(--text-2); }
  .pj-facts span { display: inline-block; width: 80px; color: var(--text-4); }
  .pj-row { display: flex; gap: 8px; flex-wrap: wrap; }
  .pj-btn { padding: 7px 14px; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--bg-surface); font-size: 12.5px; color: var(--text-1); cursor: pointer; }
  .pj-btn:hover:not(:disabled) { border-color: var(--spec); color: var(--spec); }
  .pj-btn.primary { background: var(--spec); border-color: var(--spec); color: white; }
  .pj-btn.danger:hover:not(:disabled) { border-color: var(--err, #d44); color: var(--err, #d44); }
  .pj-btn:disabled { opacity: 0.5; cursor: default; }
  .pj-note { margin: 0; font-size: 11.5px; color: var(--text-4); line-height: 1.5; }
  .pj-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .pj-table th { text-align: left; font-weight: 550; color: var(--text-4); font-size: 11px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .pj-table td { padding: 8px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: middle; }
  .pj-x { border: none; background: none; color: var(--text-4); font-size: 16px; cursor: pointer; }
  .pj-x:hover { color: var(--err, #d44); }
  .pj-add { display: grid; grid-template-columns: 90px 1fr 120px auto; gap: 8px; }
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
