<script lang="ts">
  import { currentUser, signOut } from '$lib/stores/auth';
  import { goto } from '$app/navigation';
  import ProfileModal from './ProfileModal.svelte';
  import NotificationsModal from './NotificationsModal.svelte';
  import { notifications, openNotifications } from '$lib/core/status/messages';

  // The bell: unread warnings and errors (red when any is an error).
  let unread = $state(0);
  let unreadError = $state(false);
  $effect(() => notifications.subscribe((l) => {
    const u = l.filter((m) => !m.read);
    unread = u.length;
    unreadError = u.some((m) => m.type === 'error');
  }));

  let menuOpen = $state(false);
  let profileOpen = $state(false);

  function openProfile() {
    menuOpen = false;
    profileOpen = true;
  }

  let userInitials = $derived((() => {
    const u = $currentUser;
    if (!u?.name) return '??';
    return u.name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
  })());

  function handleSignOut() {
    menuOpen = false;
    signOut();
    goto('/login');
  }
  import AppPalette from './AppPalette.svelte';
  import ActiveApps from './ActiveApps.svelte';
  import ProjectSwitcher from './ProjectSwitcher.svelte';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';

  let activeType = $derived(getActivePanel()?.type ?? '');
</script>

<header class="header">
  <div class="brand">
    <span class="logo">A3T</span>
    <span class="tagline">the agent platform</span>
  </div>

  <!-- Horizontal app menus: the Squares palette launches any app, Active Apps
       lists what is open and reveals an unpinned context list. Both stay reachable
       when their side surfaces are unpinned. -->
  <nav class="hdr-menus">
    <AppPalette {activeType} />
    <ActiveApps {activeType} />
    <ProjectSwitcher />
  </nav>

  <span style="flex: 1;"></span>

  <button class="bell" class:has={unread > 0} onclick={() => openNotifications()} aria-label={unread ? `${unread} unread notification${unread === 1 ? '' : 's'}` : 'Notifications'} title="Notifications — warnings and errors">
    <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M4 11V7a4 4 0 018 0v4l1.5 1.5h-11zM6.5 14a1.5 1.5 0 003 0" /></svg>
    {#if unread > 0}<span class="badge" class:err={unreadError}>{unread > 99 ? '99+' : unread}</span>{/if}
  </button>

  <!-- User menu -->
  <div class="user-area">
    <button class="avatar-btn" onclick={() => menuOpen = !menuOpen}>
      <div class="avatar">{userInitials}</div>
      {#if $currentUser}
        <span class="user-name">{$currentUser.name}</span>
      {/if}
      <svg class="chevron" class:open={menuOpen} width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6l4 4 4-4"/></svg>
    </button>

    {#if menuOpen}
      <div class="user-dropdown">
        {#if $currentUser}
          <div class="ud-header">
            <div class="ud-name">{$currentUser.name}</div>
            <div class="ud-email">{$currentUser.email}</div>
            <div class="ud-role">{$currentUser.role}</div>
          </div>
        {/if}
        <button class="ud-item ud-nav" onclick={openProfile}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 8a3 3 0 100-6 3 3 0 000 6zM2.5 14a5.5 5.5 0 0111 0"/></svg>
          Profile
        </button>
        <button class="ud-item" onclick={handleSignOut}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M6 2H3a1 1 0 00-1 1v10a1 1 0 001 1h3M11 11l3-3-3-3M14 8H6"/></svg>
          Sign out
        </button>
      </div>
    {/if}
  </div>
</header>

{#if menuOpen}
  <button class="backdrop" onclick={() => menuOpen = false} aria-label="Close menu"></button>
{/if}

<ProfileModal bind:open={profileOpen} />
<NotificationsModal />

<style>
  .header {
    grid-area: header;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    border-radius: 16px 16px 0 0;
    display: flex;
    align-items: center;
    padding: 0 24px;
    gap: 20px;
  }
  /* Stacked: the wordmark on top, the tagline on its own line beneath. Tight
     line-heights keep both inside the 58px header row. */
  .brand {
    display: flex; flex-direction: column; align-items: flex-start;
    gap: 0; line-height: 1.05;
  }
  .brand .logo {
    font-family: var(--font-serif); font-weight: 600; font-size: 20px; letter-spacing: -0.02em;
  }
  .brand .logo::after {
    content: ''; display: inline-block; width: 6px; height: 6px;
    background: var(--ephemeral); border-radius: 50%;
    margin-left: 6px; vertical-align: middle; margin-bottom: 3px;
  }
  .brand .tagline {
    font-family: var(--font-serif); font-size: 11px; font-weight: 400;
    font-style: italic; color: var(--text-4); letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .hdr-menus { display: flex; align-items: center; gap: 8px; margin-left: 6px; }

  .bell { position: relative; display: grid; place-items: center; width: 34px; height: 34px; border: 0; border-radius: 50%; background: none; color: var(--text-3); cursor: pointer; transition: background 0.12s, color 0.12s; }
  .bell:hover { background: var(--bg-subtle); color: var(--text-1); }
  .bell .badge { position: absolute; top: 3px; right: 2px; min-width: 15px; height: 15px; padding: 0 4px; border-radius: 8px; background: var(--warn); color: #fff;
                 font: 600 9.5px/15px var(--font-sans); text-align: center; box-shadow: 0 0 0 2px var(--bg-surface); }
  .bell .badge.err { background: var(--err); }

  .user-area { position: relative; }
  .avatar-btn {
    display: flex; align-items: center; gap: 8px;
    border: none; background: none; cursor: pointer;
    padding: 4px 8px 4px 4px; border-radius: var(--r-sm);
    transition: background 0.12s;
  }
  .avatar-btn:hover { background: var(--bg-subtle); }
  .avatar {
    width: 30px; height: 30px; border-radius: 50%;
    background: linear-gradient(135deg, #e8c9a3, #d19863);
    color: white; font-weight: 600; font-size: 11px;
    display: grid; place-items: center;
    border: 1px solid rgba(0,0,0,0.05);
  }
  .user-name { font-size: 13px; color: var(--text-2); font-weight: 500; }
  .chevron { color: var(--text-4); transition: transform 0.2s; }
  .chevron.open { transform: rotate(180deg); }

  .user-dropdown {
    position: absolute; top: 100%; right: 0; margin-top: 6px;
    width: 220px; background: var(--bg-surface);
    border: 1px solid var(--border); border-radius: var(--r);
    box-shadow: var(--shadow-lg); z-index: 100;
    overflow: hidden;
  }
  .ud-header {
    padding: 12px 14px; border-bottom: 1px solid var(--border);
  }
  .ud-name { font-size: 13px; font-weight: 550; color: var(--text-1); }
  .ud-email { font-size: 11px; color: var(--text-3); margin-top: 2px; }
  .ud-role {
    font-family: var(--font-mono); font-size: 10px; color: var(--text-4);
    background: var(--bg-subtle); padding: 1px 5px; border-radius: 3px;
    display: inline-block; margin-top: 6px;
  }
  .ud-item {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 10px 14px; border: none; background: none;
    font-size: 13px; color: var(--text-2); cursor: pointer; text-align: left;
  }
  .ud-item:hover { background: var(--bg-subtle); color: var(--err); }
  .ud-nav { border-bottom: 1px solid var(--border); }
  .ud-nav:hover { color: var(--spec); }

  .backdrop {
    position: fixed; inset: 0; background: transparent;
    border: none; z-index: 50; cursor: default;
  }
</style>
