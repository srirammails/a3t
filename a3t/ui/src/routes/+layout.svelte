<script lang="ts">
  import '../app.css';
  import type { Snippet } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import Sidebar from '$lib/shell/Sidebar.svelte';
  import Topbar from '$lib/shell/Topbar.svelte';
  import StatusBar from '$lib/shell/StatusBar.svelte';
  import PanelStack from '$lib/shell/PanelStack.svelte';
  import ContextList from '$lib/shell/ContextList.svelte';
  import { isContextDismissed,
    isRailPinned, isContextPinned, isContextOpen, hasContext,
    getContextW, setContextW, trackOpen, setPins, markPinsLoaded, RAIL_W,
  } from '$lib/stores/apps.svelte.ts';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { untrack } from 'svelte';
  import { navigateTo, initFromUrl, setupPopState } from '$lib/stores/panels.svelte.ts';
  import { initContext } from '$lib/stores/context.svelte.ts';
  import PublishDialog from '$lib/components/shared/PublishDialog.svelte';
  import { messages } from '$lib/core/status/messages';
  import { initAuth, isAuthenticated } from '$lib/stores/auth';
  import { initEditorContext } from '$lib/editor/context';
  import { fetchPins } from '$lib/services/api';
  import { onMount } from 'svelte';

  let { children }: { children: Snippet } = $props();

  // ── Shell geometry ──
  // The app rail and the context list share the side region and are independently
  // pinnable, so each column is sized to 0 when its surface is unpinned rather
  // than the elements being conditionally reordered.
  const activeType = $derived(getActivePanel()?.type ?? '');
  const railW = $derived(isRailPinned() ? RAIL_W : 0);
  const selectedId = $derived(getActivePanel()?.props?.selectedId ?? '');
  // Follows the selection: no item open → the app shows its home and the bar is
  // hidden. Pinning keeps it on the home page too.
  // Docked when pinned, or beside a selected item unless the user closed it (×).
  const ctxDocked = $derived(hasContext(activeType) && (isContextPinned() || (!!selectedId && !isContextDismissed())));
  const ctxW = $derived(ctxDocked ? getContextW() : 0);
  /// Unpinned but revealed from the Active Apps menu: drawn over the main view so
  /// it does not resize the app behind it.
  const ctxFloating = $derived(hasContext(activeType) && !ctxDocked && isContextOpen());

  // ── Resizing the context list ──
  let shellEl = $state<HTMLDivElement>();
  let ctxDragging = $state(false);

  function startCtxDrag(e: PointerEvent) {
    ctxDragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function onCtxDrag(e: PointerEvent) {
    if (!ctxDragging || !shellEl) return;
    // Width measured from the context column's own left edge, which sits after
    // the rail — so the handle tracks the pointer whether the rail is pinned.
    const left = shellEl.getBoundingClientRect().left + railW;
    setContextW(e.clientX - left);
  }
  function endCtxDrag(e: PointerEvent) {
    ctxDragging = false;
    try { (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId); } catch {}
  }

  // Pins are server-owned and per user; load once so every surface reads one copy.
  onMount(() => {
    if (!authed) return;
    fetchPins().then((p) => { setPins(p); markPinsLoaded(); }).catch(() => {});
  });

  // Register the current app in the taskbar. Done here rather than at each call
  // site so the rail, the palette, a deep link and a back/forward all count.
  $effect(() => {
    const p = getActivePanel();
    if (!p) return;
    // untrack: trackOpen reads the open-app list it also writes, which would make
    // this effect retrigger itself and trip Svelte's update-depth guard.
    untrack(() => trackOpen(p.type, p.title));
  });

  let authChecked = $state(false);
  let authed = $state(false);

  const publicPaths = ['/login', '/signup'];

  onMount(() => {
    const hasSession = initAuth();
    authChecked = true;
    authed = hasSession;

    if (!hasSession && !publicPaths.includes(window.location.pathname)) {
      goto('/login');
      return;
    }

    if (hasSession) {
      // Restore from URL, else land on Chat — Dashboard is hidden from the menu,
      // so defaulting there would open a page the rail cannot get back to.
      // The project comes first: the panels read the rest of the same URL.
      initContext();
      const restored = initFromUrl();
      if (!restored) {
        navigateTo('chat', 'AI Chat');
      }
      setupPopState();
      // Initialize editor context (registry + ajv) in background
      initEditorContext().then(ctx => {
        if (ctx.error) {
          console.warn('[layout] Editor context init warning:', ctx.error);
        } else {
          console.log(`[layout] Editor context ready: ${ctx.functions.size} functions loaded`);
        }
      });
      setTimeout(() => messages.success('Connected to k3s-eu-west-1'), 1500);
    }

    const unsub = isAuthenticated.subscribe(v => {
      authed = v;
      if (v && authChecked && publicPaths.includes(window.location.pathname)) {
        navigateTo('chat', 'AI Chat');
      } else if (!v && authChecked && !publicPaths.includes(window.location.pathname)) {
        goto('/login');
      }
    });
    return unsub;
  });

  // Derive if current route is public
  let isPublicRoute = $derived(publicPaths.some(p => $page.url.pathname.startsWith(p)));
</script>

<svelte:head>
  <title>A3T — Agent Platform</title>
</svelte:head>

{#if isPublicRoute}
  <!-- Public pages (login/signup) render without the shell -->
  {@render children()}
{:else if authed}
  <!-- Authenticated shell -->
  <div class="shell" bind:this={shellEl}
       style="grid-template-columns: {railW}px {ctxW}px {ctxDocked ? 6 : 0}px 1fr;">
    <StatusBar />
    <Topbar />
    {#if isRailPinned()}<Sidebar />{/if}
    {#if ctxDocked}<ContextList appType={activeType} />{/if}
    {#if ctxDocked}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="ctx-resizer"
        class:dragging={ctxDragging}
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the list pane"
        onpointerdown={startCtxDrag}
        onpointermove={onCtxDrag}
        onpointerup={endCtxDrag}
        onpointercancel={endCtxDrag}
        ondblclick={() => setContextW(260)}
      ></div>
    {/if}
    <main class="main">
      {#if ctxFloating}<ContextList appType={activeType} />{/if}
      <PanelStack />
    </main>
    <PublishDialog />
  </div>
  {@render children()}
{:else if authChecked}
  <!-- Redirect in progress -->
  <div class="loading-screen">
    <span class="loading-logo">A3T</span>
    <span class="loading-text">Redirecting...</span>
  </div>
{:else}
  <!-- Checking auth -->
  <div class="loading-screen">
    <span class="loading-logo">A3T</span>
    <span class="loading-text">Loading...</span>
  </div>
{/if}

<style>
  .shell {
    display: grid;
    /* columns are set inline from the pin state: rail | context | main */
    /* minmax(0,1fr), not 1fr: a plain `1fr` is minmax(AUTO,1fr), so a tall page
       can push the body row past the viewport and every scroll container inside
       it stops working. Same defect as the chat grid. */
    grid-template-rows: 3px 58px minmax(0, 1fr);
    grid-template-areas:
      "status  status  status   status"
      "header  header  header   header"
      "sidebar context resizer  main";
    height: calc(100vh - 12px);
    margin: 6px;
    background: var(--bg-sunken);
    border: 1px solid var(--border);
    border-radius: 16px 16px 0 0;
    overflow: hidden;
  }
  /* Drag handle between the context list and the main view. Its own grid column
     so it never overlaps either side. */
  .ctx-resizer {
    grid-area: resizer;
    cursor: col-resize;
    background: var(--border);
    opacity: 0.35;
    transition: opacity 0.12s;
    touch-action: none;   /* pointer events, not scrolling */
  }
  .ctx-resizer:hover, .ctx-resizer.dragging { opacity: 1; background: var(--border-strong); }

  .main {
    grid-area: main;
    background: var(--bg-page);
    /* Grid items default to min-height/min-width auto and will not shrink below
       their content. */
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  .loading-screen {
    height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--bg-page);
  }
  .loading-logo {
    font-family: var(--font-serif);
    font-size: 32px;
    font-weight: 600;
    color: var(--text-1);
  }
  .loading-text {
    font-size: 13px;
    color: var(--text-4);
  }
</style>
