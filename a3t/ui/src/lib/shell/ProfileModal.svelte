<script lang="ts">
  import { currentUser } from '$lib/stores/auth';
  import { mcpCall, execFlow, login as apiLogin } from '$lib/services/api';

  let { open = $bindable(false) } = $props();

  type Node = 'personal' | 'preferences' | 'security' | 'account';
  let propNode = $state<Node>('personal');
  let panelClosing = $state(false);

  function closePanel() {
    panelClosing = true;
    setTimeout(() => { open = false; panelClosing = false; }, 200);
  }

  const initials = $derived(($currentUser?.name ?? '?').split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2));

  // ── Personal details ──
  const nameParts = () => ($currentUser?.name ?? '').trim().split(/\s+/);
  let firstName = $state('');
  let lastName = $state('');
  let email = $state('');
  let address = $state('');
  let personalSaving = $state(false);
  let personalMsg = $state('');

  $effect(() => {
    if (open) {
      const p = nameParts();
      firstName = p[0] ?? '';
      lastName = p.slice(1).join(' ');
      email = $currentUser?.email ?? '';
      personalMsg = ''; pwMsg = '';
    }
  });

  async function savePersonal(e: Event) {
    e.preventDefault();
    personalSaving = true; personalMsg = '';
    try {
      const res = await mcpCall('execute_flow', {
        name: 'a3t-users', namespace: 'a3t',
        input: { input: { action: 'update', id: $currentUser?.id, name: `${firstName} ${lastName}`.trim(), email, address } }
      });
      personalMsg = res.ok ? 'Saved' : (res.error ?? 'Save failed');
    } catch (err) {
      personalMsg = err instanceof Error ? err.message : 'Save failed';
    } finally { personalSaving = false; }
  }

  // ── Preferences ──
  let theme = $state('system');
  let emailNotifications = $state(true);
  let prefMsg = $state('');
  function savePrefs() {
    try { localStorage.setItem('a3t.prefs', JSON.stringify({ theme, emailNotifications })); } catch {}
    prefMsg = 'Saved';
  }

  // ── Security ──
  let securityTab = $state<'password' | 'apikeys'>('password');
  let currentPw = $state('');
  let newPw = $state('');
  let confirmPw = $state('');
  let pwSaving = $state(false);
  let pwMsg = $state('');

  async function changePassword(e: Event) {
    e.preventDefault();
    pwMsg = '';
    if (newPw.length < 8) { pwMsg = 'New password must be at least 8 characters'; return; }
    if (newPw !== confirmPw) { pwMsg = 'Passwords do not match'; return; }
    pwSaving = true;
    try {
      // 1. Verify the current password server-side (Argon2) without touching the
      //    active session — this is the raw api login, not the auth-store one.
      const verify = await apiLogin($currentUser?.email ?? '', currentPw);
      if (!verify.ok) { pwMsg = 'Current password is incorrect'; return; }

      // 2. Set the new password via the users CRUD flow (waits for completion).
      const res = await execFlow('a3t-users', 'a3t', {
        input: {
          action: 'change-password',
          id: $currentUser?.id,
          org_id: $currentUser?.org_id,
          new_password: newPw,
          timestamp: new Date().toISOString()
        }
      }, { silent: true });

      pwMsg = res.ok ? 'Password updated' : (res.error ?? 'Update failed');
      if (res.ok) { currentPw = newPw = confirmPw = ''; }
    } catch (err) {
      pwMsg = err instanceof Error ? err.message : 'Update failed';
    } finally { pwSaving = false; }
  }

  interface ApiKey { id: string; name: string; prefix: string; created: string; }
  let apiKeys = $state<ApiKey[]>([]);
  let newKeyName = $state('');
  let createdKey = $state('');
  function createKey() {
    if (!newKeyName.trim()) return;
    const rand = Array.from({ length: 32 }, () =>
      'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'[Math.floor(Math.random() * 62)]).join('');
    const token = `a3t_${rand}`;
    apiKeys = [...apiKeys, { id: crypto.randomUUID(), name: newKeyName.trim(), prefix: token.slice(0, 12), created: new Date().toISOString().slice(0, 10) }];
    createdKey = token; newKeyName = '';
  }
  function revokeKey(id: string) { apiKeys = apiKeys.filter(k => k.id !== id); }

  // ── Account ──
  let accountTab = $state<'payment' | 'bills'>('payment');
</script>

{#if open}
  <div class="slide-overlay" class:closing={panelClosing} onclick={closePanel}></div>
  <div class="slide-panel" class:closing={panelClosing}>
    <div class="sp-layout">
      <!-- LEFT: Tree menu -->
      <div class="sp-tree-panel">
        <div class="sp-tree-head">
          <div class="sp-tree-icon" style="background: linear-gradient(135deg, #e8c9a3, #d19863)">{initials}</div>
          <div class="sp-tree-info">
            <div class="sp-tree-name">{$currentUser?.name ?? 'Profile'}</div>
            <div class="sp-tree-meta">{$currentUser?.role ?? ''}</div>
          </div>
        </div>
        <nav class="sp-tree">
          <button class="sp-tree-node" class:active={propNode === 'personal'} onclick={() => propNode = 'personal'}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="5" r="3"/><path d="M2 14a6 6 0 0112 0"/></svg>
            Personal Details
          </button>
          <button class="sp-tree-node" class:active={propNode === 'preferences'} onclick={() => propNode = 'preferences'}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="2"/><path d="M8 1v2M8 13v2M15 8h-2M3 8H1M12.7 3.3l-1.4 1.4M4.7 11.3l-1.4 1.4M12.7 12.7l-1.4-1.4M4.7 4.7L3.3 3.3"/></svg>
            Preferences
          </button>
          <button class="sp-tree-node" class:active={propNode === 'security'} onclick={() => propNode = 'security'}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 1L2 4v4c0 4 3 6.5 6 7.5 3-1 6-3.5 6-7.5V4z"/></svg>
            Security
          </button>
          <button class="sp-tree-node" class:active={propNode === 'account'} onclick={() => propNode = 'account'}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="3" width="14" height="10" rx="2"/><path d="M1 6h14"/></svg>
            Account
          </button>
        </nav>
      </div>

      <!-- CENTER: Content + close -->
      <div class="sp-main">
        <div class="sp-close-bar">
          <div class="sp-btn-wrap">
            <button class="sp-close" onclick={closePanel}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8"/></svg>
            </button>
            <span class="sp-tooltip">Close</span>
          </div>
        </div>
        <div class="sp-content">
          {#if propNode === 'personal'}
            <div class="sp-form">
              <h3>Personal Details</h3>
              <form onsubmit={savePersonal}>
                <div class="pm-row">
                  <div class="pm-field"><label for="pm-fn">First Name</label><input id="pm-fn" bind:value={firstName} placeholder="Jane" /></div>
                  <div class="pm-field"><label for="pm-ln">Last Name</label><input id="pm-ln" bind:value={lastName} placeholder="Doe" /></div>
                </div>
                <div class="pm-field"><label for="pm-email">Email</label><input id="pm-email" type="email" bind:value={email} placeholder="jane@company.com" /></div>
                <div class="pm-field"><label for="pm-addr">Address</label><textarea id="pm-addr" rows="3" bind:value={address} placeholder="Street, City, Country"></textarea></div>
                <div class="pm-actions">
                  <button type="submit" class="pm-save" disabled={personalSaving}>{personalSaving ? 'Saving…' : 'Save changes'}</button>
                  {#if personalMsg}<span class="pm-msg" class:ok={personalMsg === 'Saved'}>{personalMsg}</span>{/if}
                </div>
              </form>
            </div>

          {:else if propNode === 'preferences'}
            <div class="sp-form">
              <h3>Preferences</h3>
              <div class="pm-field"><label for="pm-theme">Theme</label>
                <select id="pm-theme" bind:value={theme}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select>
              </div>
              <label class="pm-check"><input type="checkbox" bind:checked={emailNotifications} /> Email notifications</label>
              <div class="pm-actions">
                <button type="button" class="pm-save" onclick={savePrefs}>Save preferences</button>
                {#if prefMsg}<span class="pm-msg ok">{prefMsg}</span>{/if}
              </div>
            </div>

          {:else if propNode === 'security'}
            <div class="sp-form">
              <h3>Security</h3>
              <div class="pm-tabs">
                <button class:active={securityTab === 'password'} onclick={() => securityTab = 'password'}>Password reset</button>
                <button class:active={securityTab === 'apikeys'} onclick={() => securityTab = 'apikeys'}>API Keys</button>
              </div>
              {#if securityTab === 'password'}
                <form onsubmit={changePassword}>
                  <div class="pm-field"><label for="pm-cpw">Current password</label><input id="pm-cpw" type="password" bind:value={currentPw} autocomplete="current-password" /></div>
                  <div class="pm-field"><label for="pm-npw">New password</label><input id="pm-npw" type="password" bind:value={newPw} autocomplete="new-password" /></div>
                  <div class="pm-field"><label for="pm-xpw">Confirm new password</label><input id="pm-xpw" type="password" bind:value={confirmPw} autocomplete="new-password" /></div>
                  <div class="pm-actions">
                    <button type="submit" class="pm-save" disabled={pwSaving}>{pwSaving ? 'Updating…' : 'Update password'}</button>
                    {#if pwMsg}<span class="pm-msg" class:ok={pwMsg === 'Password updated'}>{pwMsg}</span>{/if}
                  </div>
                </form>
              {:else}
                <div class="pm-keyadd">
                  <input bind:value={newKeyName} placeholder="Key name (e.g. ci-pipeline)" />
                  <button type="button" class="pm-save" onclick={createKey}>Create key</button>
                </div>
                {#if createdKey}
                  <div class="pm-newkey"><span>Copy this key now — it won't be shown again:</span><code>{createdKey}</code></div>
                {/if}
                {#if apiKeys.length === 0}
                  <p class="sf-placeholder">No API keys yet.</p>
                {:else}
                  <div class="sf-agents-list">
                    {#each apiKeys as k}
                      <div class="sf-agent-row">
                        <div class="sf-agent-info"><strong>{k.name}</strong><code class="sf-agent-mode">{k.prefix}…</code></div>
                        <span class="sf-agent-meta">{k.created}</span>
                        <button class="pm-revoke" onclick={() => revokeKey(k.id)}>Revoke</button>
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>

          {:else if propNode === 'account'}
            <div class="sp-form">
              <h3>Account</h3>
              <div class="pm-tabs">
                <button class:active={accountTab === 'payment'} onclick={() => accountTab = 'payment'}>Payment Methods</button>
                <button class:active={accountTab === 'bills'} onclick={() => accountTab = 'bills'}>Bills</button>
              </div>
              {#if accountTab === 'payment'}
                <p class="sf-placeholder">No payment methods on file.</p>
                <div class="pm-actions"><button type="button" class="pm-save" disabled>Add payment method</button></div>
              {:else}
                <p class="sf-placeholder">No invoices yet.</p>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* Slide-panel styles mirror templates/agent-engine (.slide-panel / .sp-*). */
  .slide-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.12); z-index: 100;
    animation: overlayIn 0.2s ease-out;
  }
  .slide-overlay.closing { animation: overlayOut 0.2s ease-in forwards; }
  @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes overlayOut { from { opacity: 1; } to { opacity: 0; } }

  .slide-panel {
    position: fixed; top: 42px; left: 110px; right: 6px; bottom: 6px;
    background: var(--bg-page); border: 1px solid var(--border); border-radius: 14px;
    z-index: 101; overflow: hidden;
    box-shadow: 0 8px 40px rgba(0,0,0,0.14), 0 2px 10px rgba(0,0,0,0.06), -4px 0 20px rgba(0,0,0,0.04);
    animation: panelIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .slide-panel.closing { animation: panelOut 0.2s ease-in forwards; }
  @keyframes panelIn { from { transform: translateX(100%); } to { transform: translateX(0); } }
  @keyframes panelOut { from { transform: translateX(0); } to { transform: translateX(100%); } }

  .sp-layout { display: grid; grid-template-columns: 200px 1fr; height: 100%; }

  .sp-tree-panel { display: flex; flex-direction: column; background: var(--bg-surface); border-right: 1px solid var(--border); overflow-y: auto; }
  .sp-tree-head { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--border); }
  .sp-tree-icon { width: 28px; height: 28px; border-radius: 6px; display: grid; place-items: center; font-family: var(--font-mono); font-size: 10px; font-weight: 700; color: white; flex-shrink: 0; }
  .sp-tree-info { min-width: 0; }
  .sp-tree-name { font-family: var(--font-serif); font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sp-tree-meta { font-size: 10px; color: var(--text-4); margin-top: 1px; text-transform: capitalize; }

  .sp-tree { display: flex; flex-direction: column; padding: 6px 0; flex: 1; }
  .sp-tree-node { display: flex; align-items: center; gap: 8px; padding: 7px 14px; border: none; background: none; cursor: pointer; font-size: 12px; color: var(--text-3); text-align: left; transition: all 0.1s; }
  .sp-tree-node:hover { color: var(--text-1); background: var(--bg-subtle); }
  .sp-tree-node.active { color: var(--text-1); font-weight: 550; background: var(--bg-subtle); box-shadow: inset 3px 0 0 var(--spec, #c47028); }
  .sp-tree-node svg { color: var(--text-4); flex-shrink: 0; }
  .sp-tree-node.active svg { color: var(--spec, #c47028); }

  .sp-main { overflow: hidden; position: relative; padding-top: 44px; }
  .sp-close-bar { position: absolute; top: 0; left: 0; right: 0; height: 44px; display: flex; align-items: center; justify-content: flex-end; gap: 6px; padding: 0 12px; z-index: 10; background: var(--bg-page); border-bottom: 1px solid var(--border); }
  .sp-btn-wrap { position: relative; }
  .sp-tooltip { position: absolute; bottom: -30px; left: 50%; transform: translateX(-50%); padding: 4px 10px; background: var(--text-1); color: var(--bg-page); font-size: 11px; font-weight: 500; white-space: nowrap; border-radius: 5px; pointer-events: none; opacity: 0; transition: opacity 0.15s 0.3s; z-index: 20; }
  .sp-tooltip::before { content: ''; position: absolute; bottom: 100%; left: 50%; transform: translateX(-50%); border: 4px solid transparent; border-bottom-color: var(--text-1); }
  .sp-btn-wrap:hover .sp-tooltip { opacity: 1; }
  .sp-close { width: 32px; height: 32px; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-surface); color: var(--text-3); cursor: pointer; display: grid; place-items: center; transition: all 0.12s; }
  .sp-close:hover { background: var(--text-1); color: var(--bg-page); border-color: var(--text-1); }

  .sp-content { height: 100%; overflow-y: auto; }
  .sp-form { padding: 24px 32px; max-width: 600px; }
  .sp-form h3 { font-family: var(--font-serif); font-size: 18px; font-weight: 500; margin-bottom: 16px; }
  .sf-placeholder { font-size: 12px; color: var(--text-4); padding: 20px; text-align: center; background: var(--bg-subtle); border-radius: var(--r-sm); }
  .sf-agents-list { display: flex; flex-direction: column; gap: 4px; }
  .sf-agent-row { display: flex; align-items: center; gap: 12px; padding: 10px 14px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--r-sm); }
  .sf-agent-info { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .sf-agent-info strong { font-size: 13px; }
  .sf-agent-mode { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); padding: 2px 6px; background: var(--bg-subtle); border-radius: 3px; align-self: flex-start; }
  .sf-agent-meta { font-size: 10px; color: var(--text-4); }

  /* Form controls, matched to the panel's visual language. */
  .pm-field { display: flex; flex-direction: column; gap: 5px; margin-bottom: 14px; }
  .pm-field label { font-size: 12px; color: var(--text-3); font-weight: 500; }
  .pm-field input, .pm-field textarea, .pm-field select {
    padding: 9px 12px; border: 1px solid var(--border); border-radius: var(--r-sm);
    font-size: 14px; font-family: var(--font-sans); background: var(--bg-surface); color: var(--text-1); transition: border-color 0.15s;
  }
  .pm-field textarea { resize: vertical; }
  .pm-field input:focus, .pm-field textarea:focus, .pm-field select:focus { outline: none; border-color: var(--spec); box-shadow: 0 0 0 2px var(--spec-bg); }
  .pm-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

  .pm-actions { display: flex; align-items: center; gap: 12px; margin-top: 6px; }
  .pm-save { padding: 9px 18px; border: none; border-radius: var(--r-sm); background: var(--spec, #c47028); color: #fff; font-size: 13px; font-weight: 550; cursor: pointer; transition: opacity 0.15s; }
  .pm-save:hover:not(:disabled) { opacity: 0.9; }
  .pm-save:disabled { opacity: 0.5; cursor: not-allowed; }
  .pm-msg { font-size: 12px; color: var(--err); }
  .pm-msg.ok { color: var(--ok, #23987f); }
  .pm-check { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-2); margin: 4px 0 16px; cursor: pointer; }

  .pm-tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); margin: 0 0 20px; }
  .pm-tabs button { padding: 8px 12px; border: none; background: none; cursor: pointer; font-size: 13px; color: var(--text-3); border-bottom: 2px solid transparent; margin-bottom: -1px; }
  .pm-tabs button.active { color: var(--spec, #c47028); border-bottom-color: var(--spec, #c47028); font-weight: 550; }

  .pm-keyadd { display: flex; gap: 8px; margin-bottom: 14px; }
  .pm-keyadd input { flex: 1; padding: 9px 12px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 14px; background: var(--bg-surface); color: var(--text-1); }
  .pm-newkey { background: var(--spec-bg); border: 1px solid var(--spec); border-radius: var(--r-sm); padding: 12px 14px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--text-2); }
  .pm-newkey code { font-family: var(--font-mono); font-size: 12px; word-break: break-all; color: var(--text-1); }
  .pm-revoke { border: none; background: none; color: var(--err); font-size: 12px; cursor: pointer; }
</style>
