<script lang="ts">
  import NewButton from '$lib/components/shared/NewButton.svelte';
  import SubmenuToggle from '$lib/components/shared/SubmenuToggle.svelte';
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  // EE Identity — connect to the SSO IdP (Keycloak) and provision users from
  // A3T. Backed by flowr-server /security/idp/* (Keycloak Admin API). Roles map
  // to A3T canonical roles at login via realm_access.roles → roles::canonicalize.
  import { idp, getEdition, type IdpNewUser } from '$lib/services/api';
  import DesignerPanel from '$lib/components/shared/DesignerPanel.svelte';
  import { onMount } from 'svelte';

  const USER_NODES = [
    { id: 'account', label: 'Account', icon: '<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><circle cx="5.5" cy="7.5" r="1.5"/><path d="M3.5 11c.3-1.2 1.1-1.8 2-1.8s1.7.6 2 1.8"/>' },
    { id: 'credentials', label: 'Credentials', icon: '<path d="M10 6a3 3 0 10-6 0M4 6h8v7H4z"/>' },
    { id: 'roles', label: 'Roles', icon: '<circle cx="8" cy="5" r="2.5"/><path d="M3 13a5 5 0 0110 0"/>' },
  ];
  let userTab = $state('account');

  let edition = $state<'ce' | 'ee' | null>(null);
  let loading = $state(true);
  let loadError = $state('');

  let status = $state<any>(null);
  let users = $state<any[]>([]);

  // User form (create + edit) opened in the shared SlidePanel popin.
  let showCreate = $state(false);
  let editingId = $state<string | null>(null); // null = create mode
  let form = $state<IdpNewUser & { rolesText: string }>({ username: '', email: '', name: '', password: '', temporary: false, roles: [], rolesText: '' });
  let creating = $state(false);
  let createMsg = $state('');

  function openCreate() {
    editingId = null;
    form = { username: '', email: '', name: '', password: '', temporary: false, roles: [], rolesText: '' };
    createMsg = ''; userTab = 'account';
    showCreate = true;
  }

  async function openEdit(u: any) {
    editingId = u.id;
    createMsg = ''; userTab = 'account';
    // Username is the email. Prefill from the row, then refine with full detail.
    form = { username: u.username ?? u.email ?? '', email: u.email ?? '', name: '', password: '', temporary: false, roles: [], rolesText: '' };
    showCreate = true;
    const res = await idp.getUser(u.id);
    if (res.ok && res.data) {
      form.name = res.data.name ?? '';
      form.rolesText = (res.data.roles ?? []).join(', ');
    }
  }

  const isEmail = (s: string) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(s.trim());

  async function del(u: any) {
    if (!confirm(`Delete user "${u.username}"? This cannot be undone.`)) return;
    const res = await idp.deleteUser(u.id);
    if (res.ok) {
      const us = await idp.users();
      users = us.ok ? (us.data?.users ?? []) : users;
    } else {
      alert(res.error || `Delete failed (HTTP ${res.status})`);
    }
  }

  const REALM_ROLE_HINT = 'operator, admin'; // Keycloak realm roles; map to A3T at login.

  async function refresh() {
    loading = true; loadError = '';
    const ed = await getEdition();
    edition = (ed.ok && ed.data?.edition === 'ee') ? 'ee' : 'ce';
    if (edition !== 'ee') { loading = false; return; }
    const st = await idp.status();
    if (st.status === 403) { loadError = 'Identity management requires an Admin or SecurityAdmin role.'; loading = false; return; }
    status = st.ok ? st.data : { error: st.error };
    if (status?.connected) {
      const us = await idp.users();
      users = us.ok ? (us.data?.users ?? []) : [];
    }
    loading = false;
  }

  onMount(refresh);

  async function submit() {
    const isEdit = !!editingId;
    const emailUser = form.username.trim(); // username = email (UI login uses email)
    if (!isEmail(emailUser)) { createMsg = 'Email is required (used as the username to sign in).'; return; }
    if (!isEdit && !form.password) { createMsg = 'Password is required.'; return; }
    creating = true; createMsg = '';
    const roles = form.rolesText.split(',').map(r => r.trim()).filter(Boolean);
    const res = isEdit
      ? await idp.updateUser(editingId!, {
          name: form.name?.trim() || undefined,
          password: form.password || undefined, // blank = unchanged
          temporary: form.temporary,
          roles, // replaces manageable roles
        })
      : await idp.createUser({
          username: emailUser,     // email doubles as the Keycloak username
          email: emailUser,
          name: form.name?.trim() || undefined,
          password: form.password,
          temporary: form.temporary,
          roles,
        });
    if (res.ok) {
      showCreate = false;
      const us = await idp.users();
      users = us.ok ? (us.data?.users ?? []) : users;
    } else {
      createMsg = res.error || `${isEdit ? 'Update' : 'Create'} failed (HTTP ${res.status})`;
    }
    creating = false;
  }
</script>

<div class="identity">
  <header class="screen-header">
    <div class="breadcrumb">Security <span class="sep">/</span> Identity</div>
    <div class="screen-title-row">
      <div class="title-block"><SubmenuToggle appType="security" /><h1 class="screen-title">Identity</h1></div>
      <div class="screen-actions">
        <button class="btn" onclick={refresh} disabled={loading}>↻ Refresh</button>
        {#if edition === 'ee' && status?.connected}
          <NewButton label="New user" accent="spec" onclick={openCreate} />
        {/if}
      </div>
    </div>
  </header>
  <div class="banner-wrap">
    <PageBanner
      headline="Decide who gets in."
      highlightWord="who"
      subtitle="Connect your SSO provider and provision users."
      description="Realm roles map to A3T roles at login, and those roles are what the Cedar policies decide on. Manage the rules themselves under Policies."
      accent="spec"
    />
  </div>

  {#if loading}
    <div class="state">Loading…</div>
  {:else if edition === 'ce'}
    <div class="state">
      <p><strong>Enterprise</strong> feature. Identity federation and user provisioning are available on an EE runtime.</p>
      <p class="sub">You're connected to a Community (CE) runtime. Connect to EE — Settings → Runtime — to manage identity.</p>
    </div>
  {:else if loadError}
    <div class="state err">{loadError}</div>
  {:else}
    <div class="body">
      <!-- Connection status -->
      <section class="card">
        <h2>SSO connection</h2>
        {#if !status?.configured}
          <div class="conn none">
            <span class="dot"></span> Not configured
          </div>
          <p class="hint">{status?.reason || 'Set security.oidc_issuer to a Keycloak realm URL and provide the admin password (FLOWR_SECURITY_OIDC_ADMIN_PASSWORD).'}</p>
        {:else if status?.connected}
          <div class="conn ok"><span class="dot"></span> Connected</div>
          <dl class="kv">
            <div><dt>Provider</dt><dd>{status.base_url}</dd></div>
            <div><dt>Realm</dt><dd>{status.realm}</dd></div>
          </dl>
        {:else}
          <div class="conn bad"><span class="dot"></span> Configured, unreachable</div>
          <p class="hint">{status?.error || 'Could not obtain an admin token.'}</p>
        {/if}
      </section>

      {#if status?.connected}
        <!-- User list -->
        <section class="card">
          <h2>Users <span class="count">{users.length}</span></h2>
          {#if users.length === 0}
            <p class="hint">No users returned.</p>
          {:else}
            <table class="users">
              <thead><tr><th>Username</th><th>Email</th><th>Enabled</th><th class="act-col">Actions</th></tr></thead>
              <tbody>
                {#each users as u}
                  <tr>
                    <td>{u.username ?? '—'}</td>
                    <td>{u.email ?? '—'}</td>
                    <td>{u.enabled ? '✓' : '✗'}</td>
                    <td class="act-col">
                      <button class="row-btn" onclick={() => openEdit(u)}>Edit</button>
                      <button class="row-btn danger" onclick={() => del(u)}>Delete</button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </section>
      {/if}
    </div>
  {/if}
</div>

<!-- Add new: shared SlidePanel popin (same as Resources / Agents / Policies). -->
<!-- Canonical designer popin: tree on the left, form on the right. -->
<DesignerPanel
  open={showCreate}
  onclose={() => (showCreate = false)}
  icon="U"
  name={form.username || (editingId ? 'Edit User' : 'New User')}
  meta={editingId ? 'edit' : 'user'}
  nodes={USER_NODES}
  activeNode={userTab}
  onselect={(id) => (userTab = id)}
>
  {#if userTab === 'account'}
    <div class="pf">
      <h3>Account</h3>
      <label class="cf-field"><span>Email (used to sign in)</span><input type="email" bind:value={form.username} placeholder="jane@company.com" readonly={!!editingId} /></label>
      <label class="cf-field"><span>Full name</span><input bind:value={form.name} placeholder="Jane Doe" /></label>
    </div>
  {:else if userTab === 'credentials'}
    <div class="pf">
      <h3>Credentials</h3>
      <label class="cf-field"><span>Password{#if editingId} (leave blank to keep){/if}</span><input type="password" bind:value={form.password} /></label>
      <label class="cf-field cf-check"><input type="checkbox" bind:checked={form.temporary} /> <span>Require password reset on {editingId ? 'next' : 'first'} login</span></label>
    </div>
  {:else}
    <div class="pf">
      <h3>Roles</h3>
      <p class="pf-hint">Keycloak realm roles; mapped to A3T roles at login.</p>
      <label class="cf-field"><span>Realm roles</span><input bind:value={form.rolesText} placeholder={REALM_ROLE_HINT} /></label>
      <div class="create-actions">
        <button class="btn" onclick={() => (showCreate = false)}>Cancel</button>
        <button class="btn spec" onclick={submit} disabled={creating || !form.username.trim() || (!editingId && !form.password)}>{creating ? 'Saving…' : (editingId ? 'Save Changes' : 'Create User')}</button>
        {#if createMsg}<span class="cf-msg err">{createMsg}</span>{/if}
      </div>
    </div>
  {/if}
</DesignerPanel>

<style>
  /* Page header — the same shape as every other section. */
  .screen-header { padding: 18px 32px 12px; border-bottom: 1px solid var(--border); background: var(--bg-page); flex: none; }
  .breadcrumb { font-size: 12px; color: var(--text-3); margin-bottom: 8px; font-family: var(--font-mono); }
  .breadcrumb .sep { color: var(--text-5); margin: 0 2px; }
  .screen-title-row { display: flex; align-items: center; gap: 18px; }
  .title-block { flex: 1; display: flex; align-items: center; gap: 10px; min-width: 0; }
  .screen-title { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; margin: 0; }
  .screen-actions { display: flex; align-items: center; gap: 8px; }
  .banner-wrap { padding: 20px 32px 0; flex: none; }
  .act-col { text-align: right; white-space: nowrap; }
  .row-btn { padding: 4px 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg-surface); color: var(--text-2); font-size: 12px; cursor: pointer; margin-left: 6px; }
  .row-btn:hover { background: var(--bg-subtle); }
  .row-btn.danger { color: var(--warn, #d66); border-color: var(--warn, #d66); }
  .identity { display: flex; flex-direction: column; height: 100%; overflow: hidden; color: var(--text-1); }
  .sub { font-size: 13px; color: var(--text-3); margin-top: 4px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .state { padding: 40px; text-align: center; color: var(--text-4); }
  .state.err { color: var(--warn, #d66); }
  .body { overflow-y: auto; padding: 20px 32px; display: flex; flex-direction: column; gap: 18px; max-width: 780px; }
  .card { border: 1px solid var(--border); border-radius: 8px; padding: 18px; background: var(--bg-surface); }
  .card h2 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
  .count { font-size: 12px; color: var(--text-4); font-weight: 400; }
  .conn { display: flex; align-items: center; gap: 8px; font-size: 14px; font-weight: 600; }
  .conn .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .conn.ok .dot { background: #3cb46e; } .conn.ok { color: #3cb46e; }
  .conn.bad .dot { background: #d66; } .conn.bad { color: #d66; }
  .conn.none .dot { background: var(--text-4); } .conn.none { color: var(--text-3); }
  .hint { font-size: 12px; color: var(--text-3); margin-top: 8px; }
  .kv { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .kv div { display: flex; gap: 10px; font-size: 13px; }
  .kv dt { color: var(--text-4); width: 70px; }
  .kv dd { color: var(--text-1); font-family: var(--font-mono, monospace); }
  .users { width: 100%; border-collapse: collapse; font-size: 13px; }
  .users th { text-align: left; color: var(--text-4); font-weight: 500; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .users td { padding: 7px 8px; border-bottom: 1px solid var(--border); }
</style>
