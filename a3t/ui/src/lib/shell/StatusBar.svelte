<script lang="ts">
  /// The status bar at the top of the console: one word in its colour —
  /// Connecting…, Loading…, Saving…, Saved, Warning, Error, Offline — on a thin
  /// line across the top. A message has ▾: the rest of it opens underneath, with
  /// Copy and a way to all notifications (the bell). Quick reads (under DELAY)
  /// show nothing, so polling pages do not flicker.
  import { onDestroy } from 'svelte';
  import { fly } from 'svelte/transition';
  import { activity, type Activity } from '$lib/core/status/activity';
  import { messages, notifications, openNotifications, type Message } from '$lib/core/status/messages';

  const DELAY = 350;          // busy this long before the bar says so
  const DONE = 1600;          // "Saved" stays this long
  const LIFE: Record<Message['type'], number> = { success: 2500, info: 4000, warning: 7000, error: 9000 };

  let msg = $state<Message | null>(null);
  let act = $state<Activity>({ kind: null, since: 0, connection: 'connecting', done: null });
  let offline = $state<Message | null>(null);
  let now = $state(Date.now());
  let open = $state(false);
  let hover = $state(false);
  let copied = $state(false);

  const unsubs = [
    messages.subscribe((m) => { msg = m; if (!m) open = false; }),
    activity.subscribe((a) => (act = a)),
    notifications.subscribe((l) => (offline = l.find((m) => m.key === 'offline') ?? null)),
  ];
  onDestroy(() => unsubs.forEach((u) => u()));

  // A clock that ticks only when something is due: the busy delay, the end of "Saved".
  $effect(() => {
    const due: number[] = [];
    if (act.kind) due.push(act.since + DELAY);
    if (act.done) due.push(act.done.at + DONE);
    const wait = Math.min(...due.map((t) => t - Date.now()).filter((w) => w > 0));
    if (!Number.isFinite(wait)) { now = Date.now(); return; }
    const t = setTimeout(() => (now = Date.now()), wait + 10);
    return () => clearTimeout(t);
  });

  // A message leaves after its time — not while it is open or under the pointer.
  $effect(() => {
    const m = msg;
    if (!m || open || hover) return;
    const t = setTimeout(() => messages.remove(m.id), LIFE[m.type]);
    return () => clearTimeout(t);
  });

  type Tone = 'neutral' | 'load' | 'save' | 'ok' | 'warn' | 'err';
  type View = { tone: Tone; word: string; busy: boolean; m: Message | null };
  const BUSY: Record<NonNullable<Activity['kind']>, [Tone, string]> = {
    loading: ['load', 'Loading…'], saving: ['save', 'Saving…'], deleting: ['save', 'Deleting…'], running: ['save', 'Running…'],
  };
  const TONE: Record<Message['type'], Tone> = { success: 'ok', info: 'load', warning: 'warn', error: 'err' };
  const WORD: Record<Message['type'], string> = { success: 'Done', info: 'Note', warning: 'Warning', error: 'Error' };

  const view = $derived.by((): View | null => {
    const busy = !!act.kind && now - act.since >= DELAY;
    if (msg) return { tone: TONE[msg.type], word: msg.key === 'offline' ? 'Offline' : WORD[msg.type], busy, m: msg };
    if (act.connection === 'offline') return { tone: 'err', word: 'Offline', busy, m: offline };
    if (busy && act.kind) {
      if (act.connection === 'connecting') return { tone: 'neutral', word: 'Connecting…', busy, m: null };
      const [tone, word] = BUSY[act.kind];
      return { tone, word, busy, m: null };
    }
    if (act.done && now - act.done.at < DONE) return { tone: 'ok', word: act.done.word, busy: false, m: null };
    return null;
  });

  function dismiss() {
    open = false;
    if (msg) messages.remove(msg.id);
  }
  function all() {
    const id = view?.m?.id ?? null;
    open = false;
    if (msg) messages.remove(msg.id);
    openNotifications(id);
  }
  async function copy(m: Message) {
    try { await navigator.clipboard?.writeText(m.detail || m.title); copied = true; setTimeout(() => (copied = false), 1200); } catch { /* no clipboard */ }
  }
  const time = (t: number) => new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
</script>

<div class="status-bar">
  <div class="line {view?.tone ?? ''}" class:on={!!view} class:busy={view?.busy || (!!act.kind && now - act.since >= DELAY)}></div>

  {#if view}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="wrap" onmouseenter={() => (hover = true)} onmouseleave={() => (hover = false)} in:fly={{ y: -16, duration: 180 }} out:fly={{ y: -16, duration: 160 }}>
      <div class="pill {view.tone}" role="status" aria-live="polite">
        {#if view.busy && !view.m}
          <span class="spin" aria-hidden="true"></span>
        {:else if view.tone === 'ok'}
          <svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
        {:else if view.tone === 'warn'}
          <svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2l6.5 11.5h-13z M8 6.5v3.5 M8 12v.01" /></svg>
        {:else if view.tone === 'err'}
          <svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6" /><path d="M8 4.8v4 M8 11.2v.01" /></svg>
        {:else}
          <span class="dot" aria-hidden="true"></span>
        {/if}
        <span class="word">{view.word}</span>
        {#if view.m && view.m.count > 1}<span class="count" title="{view.m.count} times">×{view.m.count}</span>{/if}
        {#if view.m}
          <button class="caret" class:open onclick={() => (open = !open)} aria-expanded={open} aria-label={open ? 'Hide the message' : 'Show the message'} title={open ? 'Hide' : view.m.title}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg>
          </button>
        {/if}
      </div>

      {#if open && view.m}
        {@const m = view.m}
        <div class="card {view.tone}" transition:fly={{ y: -6, duration: 140 }}>
          <div class="title">{m.title}</div>
          {#if m.detail}<pre class="detail">{m.detail}</pre>{/if}
          <div class="meta">
            {#if m.source}<span class="src">{m.source}</span>{/if}
            <span>{time(m.at)}</span>
            {#if m.count > 1}<span>{m.count} times since {time(m.first)}</span>{/if}
          </div>
          <div class="acts">
            <button onclick={() => copy(m)}>{copied ? 'Copied' : 'Copy'}</button>
            {#if m.type === 'error' || m.type === 'warning'}<button onclick={all}>All notifications</button>{/if}
            <span class="grow"></span>
            {#if msg}<button onclick={dismiss}>Dismiss</button>{/if}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .status-bar { grid-area: status; position: relative; height: 3px; z-index: 120; }

  /* The thin line across the top: solid while there is something to say,
     a moving stripe while busy. */
  .line { position: absolute; inset: 0; border-radius: 16px 16px 0 0; overflow: hidden; opacity: 0; transition: opacity 0.3s, background 0.3s; --c: var(--text-4); }
  .line.on, .line.busy { opacity: 1; }
  .line.on { background: var(--c); }
  .line.busy { background: color-mix(in srgb, var(--c) 25%, transparent); }
  .line.busy::after { content: ''; position: absolute; top: 0; bottom: 0; width: 30%; background: var(--c); animation: slide 1.1s ease-in-out infinite; }
  @keyframes slide { from { left: -30%; } to { left: 100%; } }

  .neutral { --c: var(--text-3); --bg: var(--bg-subtle); --ink: var(--text-2); }
  .load { --c: var(--ephemeral); --bg: var(--ephemeral-bg); --ink: var(--ephemeral-ink); }
  .save { --c: var(--spec); --bg: var(--spec-bg); --ink: var(--spec-ink); }
  .ok { --c: var(--ok); --bg: var(--ok-bg); --ink: var(--runtime-ink, var(--ok)); }
  .warn { --c: var(--warn); --bg: var(--warn-bg); --ink: color-mix(in srgb, var(--warn) 70%, var(--text-1)); }
  .err { --c: var(--err); --bg: var(--err-bg); --ink: color-mix(in srgb, var(--err) 75%, var(--text-1)); }

  .wrap { position: absolute; top: 0; left: 50%; transform: translateX(-50%); display: flex; flex-direction: column; align-items: center; }

  .pill {
    display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 10px 0 9px;
    background: var(--bg); color: var(--ink); border: 1px solid color-mix(in srgb, var(--c) 35%, transparent); border-top: 0;
    border-radius: 0 0 9px 9px; font-size: 12px; font-weight: 600; letter-spacing: 0.01em; white-space: nowrap;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
  }
  .word { line-height: 1; }
  .count { font: 500 10.5px var(--font-mono); opacity: 0.75; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .ic { width: 13px; height: 13px; fill: none; stroke: var(--c); stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .spin { width: 11px; height: 11px; border-radius: 50%; border: 2px solid color-mix(in srgb, var(--c) 30%, transparent); border-top-color: var(--c); animation: rot 0.8s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }

  .caret { display: grid; place-items: center; width: 18px; height: 18px; margin-right: -4px; border: 0; border-radius: 4px; background: transparent; color: inherit; cursor: pointer; padding: 0; }
  .caret:hover { background: color-mix(in srgb, var(--c) 15%, transparent); }
  .caret svg { width: 11px; height: 11px; fill: none; stroke: currentColor; stroke-width: 2; transition: transform 0.15s; }
  .caret.open svg { transform: rotate(180deg); }

  .card {
    margin-top: 6px; width: min(560px, calc(100vw - 32px)); background: var(--bg-surface); color: var(--text-1);
    border: 1px solid var(--border); border-left: 3px solid var(--c); border-radius: 8px; box-shadow: var(--shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.14));
    padding: 10px 12px; display: flex; flex-direction: column; gap: 8px;
  }
  .title { font-size: 13px; font-weight: 600; line-height: 1.35; overflow-wrap: anywhere; }
  .detail { margin: 0; max-height: 260px; overflow: auto; padding: 8px 10px; background: var(--bg-subtle); border-radius: 5px;
            font: 11.5px/1.5 var(--font-mono); color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
  .meta { display: flex; flex-wrap: wrap; gap: 10px; font-size: 11px; color: var(--text-4); }
  .src { font-family: var(--font-mono); color: var(--text-3); }
  .acts { display: flex; gap: 6px; }
  .acts .grow { flex: 1; }
  .acts button { border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-2); font-size: 11.5px; padding: 3px 10px; border-radius: 5px; cursor: pointer; }
  .acts button:hover { background: var(--bg-subtle); color: var(--text-1); }
</style>
