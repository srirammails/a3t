<script lang="ts">
  /// Settings → General → Organisations. A superadmin creates and deletes
  /// organisations and manages who is in each (member, admin, security admin);
  /// an org's own admins can manage its members too. Users are independent: one
  /// person can be in several orgs. All of it is the a3t-orgs flow, which checks
  /// the caller on the server.
  import { onMount } from 'svelte';
  import { userRole } from '$lib/stores/auth';
  import { loadProjects } from '$lib/stores/context.svelte.ts';
  import { orgsApi, loadDirectory, type OrgInfo, type OrgRole, type DirectoryUser } from '$lib/services/api';

  const ROLES: OrgRole[] = ['member', 'admin', 'securityadmin'];
  const HANDLE = /^[a-z0-9]([a-z0-9-]{0,22}[a-z0-9])?$/;

  const isSuper = $derived($userRole === 'superadmin');
  let orgs = $state<OrgInfo[]>([]);
  let loading = $state(true);
  let message = $state('');
  let messageOk = $state(false);
  let busy = $state(false);
  let selected = $state('');
  let newHandle = $state('');
  let newName = $state('');
  let directory = $state<DirectoryUser[]>([]);
  let addWho = $state('');
  let addRole = $state<OrgRole>('member');

  const org = $derived(orgs.find((o) => o.handle === selected));

  function say(text: string, ok = false) { message = text; messageOk = ok; }

  async function refresh() {
    loading = true;
    const res = await orgsApi.list();
    loading = false;
    if (!res.ok) { say(res.error ?? 'Could not load organisations'); return; }
    orgs = res.data?.orgs ?? [];
    if (selected && !orgs.some((o) => o.handle === selected)) selected = '';
  }

  async function create() {
    const h = newHandle.trim();
    if (!HANDLE.test(h)) { say('An org handle is 2–24 lowercase letters, digits and dashes.'); return; }
    busy = true;
    const res = await orgsApi.create(h, newName.trim() || h);
    busy = false;
    if (!res.ok) { say(res.error ?? 'Create failed'); return; }
    newHandle = ''; newName = '';
    say(`Created ${h}. Its projects are named ${h}-<project>.`, true);
    await refresh();
    selected = h;
    void loadProjects(true);
  }

  async function remove(o: OrgInfo) {
    if (!confirm(`Delete organisation “${o.handle}”? Its members are removed; it must own no projects.`)) return;
    busy = true;
    const res = await orgsApi.remove(o.handle);
    busy = false;
    if (!res.ok) { say(res.error ?? 'Delete failed'); return; }
    say(`Deleted ${o.handle}.`, true);
    await refresh();
    void loadProjects(true);
  }

  async function addMember() {
    if (!org) return;
    const q = addWho.trim().toLowerCase();
    const u = directory.find((d) => d.email.toLowerCase() === q || d.id === addWho.trim());
    if (!u) { say('Pick someone from the directory — members are matched by account, not by typed email.'); return; }
    busy = true;
    const res = await orgsApi.setMember(org.handle, { user_id: u.id, email: u.email, name: u.name }, addRole);
    busy = false;
    if (!res.ok) { say(res.error ?? 'Could not add the member'); return; }
    addWho = '';
    say('');
    await refresh();
  }

  async function setRole(user_id: string, email: string | undefined, name: string | undefined, role: OrgRole) {
    if (!org) return;
    const res = await orgsApi.setMember(org.handle, { user_id, email, name }, role);
    if (!res.ok) say(res.error ?? 'Could not change the role');
    await refresh();
  }

  async function removeMember(user_id: string) {
    if (!org) return;
    const res = await orgsApi.removeMember(org.handle, user_id);
    if (!res.ok) say(res.error ?? 'Could not remove the member');
    await refresh();
  }

  onMount(async () => {
    await refresh();
    directory = await loadDirectory();
  });
</script>

<div class="card">
  <div class="card-head"><h2>Organisations</h2></div>
  <p class="card-sub">An organisation owns projects named <span class="mono">&lt;org&gt;-&lt;project&gt;</span>. People are independent of it: add and remove them here, as member, admin or security admin. Admins and security admins see the org's private projects; members see the rest. {isSuper ? '' : 'Creating and deleting organisations needs the SuperAdmin role.'}</p>

  {#if message}<div class="msg" class:ok={messageOk}>{message}</div>{/if}

  {#if isSuper}
    <div class="row">
      <input class="mono" bind:value={newHandle} placeholder="handle, e.g. acme" />
      <input bind:value={newName} placeholder="Display name" />
      <button class="btn primary" onclick={create} disabled={busy || !newHandle.trim()}>Create organisation</button>
    </div>
  {/if}

  {#if loading}
    <p class="dim">Loading…</p>
  {:else if orgs.length === 0}
    <p class="dim">{isSuper ? 'No organisations yet.' : "You aren't in any organisation."}</p>
  {:else}
    <table class="tbl">
      <thead><tr><th>Organisation</th><th>Members</th><th>Your role</th><th></th></tr></thead>
      <tbody>
        {#each orgs as o (o.id)}
          <tr class:sel={o.handle === selected}>
            <td><button class="link" onclick={() => (selected = o.handle === selected ? '' : o.handle)}>{o.display_name}</button><div class="dim mono">{o.handle}</div></td>
            <td>{o.members.length}</td>
            <td>{o.my_role ?? (isSuper ? 'superadmin' : '—')}</td>
            <td class="right">
              {#if o.can_admin}<button class="btn" onclick={() => (selected = o.handle)}>Members</button>{/if}
              {#if isSuper}<button class="btn danger" onclick={() => remove(o)} disabled={busy || o.id === 'org-default'} title={o.id === 'org-default' ? "The default organisation can't be deleted" : ''}>Delete</button>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if org}
  <div class="card">
    <div class="card-head"><h2>{org.display_name} — members</h2></div>
    <table class="tbl">
      <thead><tr><th>Person</th><th>Role</th><th></th></tr></thead>
      <tbody>
        {#each org.members as m (m.user_id)}
          <tr>
            <td>{m.name || m.email || m.user_id}<div class="dim mono">{m.email || m.user_id}</div></td>
            <td>
              <select value={m.role} disabled={!org.can_admin} onchange={(e) => setRole(m.user_id, m.email, m.name, e.currentTarget.value as OrgRole)}>
                {#each ROLES as r}<option value={r}>{r}</option>{/each}
              </select>
            </td>
            <td class="right">{#if org.can_admin}<button class="btn danger" onclick={() => removeMember(m.user_id)}>Remove</button>{/if}</td>
          </tr>
        {:else}
          <tr><td colspan="3" class="dim">No members yet.</td></tr>
        {/each}
      </tbody>
    </table>
    {#if org.can_admin}
      <div class="row">
        <input bind:value={addWho} list="org-users" placeholder="name or email" onkeydown={(e) => { if (e.key === 'Enter') addMember(); }} />
        <datalist id="org-users">{#each directory as u}<option value={u.email || u.id}>{u.name}{u.source === 'sso' ? ' (SSO)' : ''}</option>{/each}</datalist>
        <select bind:value={addRole}>{#each ROLES as r}<option value={r}>{r}</option>{/each}</select>
        <button class="btn primary" onclick={addMember} disabled={busy || !addWho.trim()}>Add</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r); padding: 18px 20px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 12px; }
  .card-head h2 { margin: 0; font-family: var(--font-serif); font-size: 18px; font-weight: 500; }
  .card-sub { margin: 0; font-size: 12.5px; color: var(--text-3); line-height: 1.5; }
  .mono { font-family: var(--font-mono); }
  .dim { color: var(--text-4); font-size: 11.5px; margin: 0; }
  .msg { padding: 8px 12px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12.5px; }
  .msg.ok { background: var(--spec-bg); color: var(--spec); }
  .row { display: flex; gap: 8px; flex-wrap: wrap; }
  .row input, .row select, .tbl select { padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-page); color: var(--text-1); font-size: 13px; }
  .row input { flex: 1; min-width: 160px; }
  .btn { padding: 6px 12px; border-radius: var(--r-sm); border: 1px solid var(--border); background: var(--bg-surface); font-size: 12px; color: var(--text-1); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--spec); color: var(--spec); }
  .btn.primary { background: var(--spec); border-color: var(--spec); color: white; }
  .btn.danger:hover:not(:disabled) { border-color: var(--err, #d44); color: var(--err, #d44); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .tbl { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .tbl th { text-align: left; font-weight: 550; color: var(--text-4); font-size: 11px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .tbl td { padding: 8px; border-bottom: 1px solid var(--divider, var(--border)); vertical-align: middle; }
  .tbl tr.sel td { background: var(--spec-bg); }
  .right { text-align: right; white-space: nowrap; }
  .right .btn + .btn { margin-left: 6px; }
  .link { border: none; background: none; padding: 0; color: var(--text-1); font-size: 13px; cursor: pointer; text-align: left; }
  .link:hover { color: var(--spec); }
</style>
