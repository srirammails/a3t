<script lang="ts">
  /// The permission question, asked inline in the conversation.
  ///
  /// The agent is SUSPENDED while this is on screen — it stopped mid-turn and is
  /// waiting on the answer. So the card shows everything needed to judge the
  /// request (which tool, on which server, with what arguments) rather than
  /// asking people to approve a name they cannot evaluate.
  ///
  /// Once answered it collapses to a one-line record instead of disappearing, so
  /// the transcript still shows what was decided and when.
  import { answerApproval } from '$lib/services/api';

  let { request, resolved = null }: {
    request: {
      request_id: string;
      tool: string;
      server?: string | null;
      arguments?: any;
      reason?: string;
      scopes?: string[];
    };
    /// The matching `authz-resolved` part, once it arrives.
    resolved?: { allowed: boolean; scope?: string } | null;
  } = $props();

  let busy = $state(false);
  let error = $state('');
  /// Set locally the moment we answer, so the card settles immediately rather
  /// than waiting for the server's resolved part to round-trip.
  let answered = $state<{ allowed: boolean; scope?: string } | null>(null);
  let showArgs = $state(false);

  const outcome = $derived(resolved ?? answered);
  const scopes = $derived(request.scopes?.length ? request.scopes : ['once', 'session']);

  const SCOPE_LABEL: Record<string, string> = {
    once: 'Just this once',
    session: 'For this conversation',
    agent: 'Always for this agent',
  };

  async function decide(allow: boolean, scope: string) {
    if (busy) return;
    busy = true;
    error = '';
    const ok = await answerApproval(request.request_id, allow, scope as 'once' | 'session');
    busy = false;
    if (ok) answered = { allowed: allow, scope };
    // A failed answer must not look like a decision — the agent is still waiting.
    else error = 'Could not send that answer. The agent is still waiting.';
  }

  const argsText = $derived(() => {
    try {
      return JSON.stringify(request.arguments ?? {}, null, 2);
    } catch {
      return String(request.arguments ?? '');
    }
  });
  const hasArgs = $derived(
    request.arguments && Object.keys(request.arguments).length > 0,
  );
</script>

{#if outcome}
  <div class="ac settled" class:denied={!outcome.allowed}>
    <span class="ac-dot" aria-hidden="true"></span>
    <span class="ac-settled-text">
      {outcome.allowed ? 'Allowed' : 'Declined'}
      <code>{request.tool}</code>
      {#if outcome.allowed && outcome.scope}
        <span class="ac-scope">· {SCOPE_LABEL[outcome.scope] ?? outcome.scope}</span>
      {/if}
      {#if !outcome.allowed}
        <span class="ac-scope">· won’t be asked again in this conversation</span>
      {/if}
    </span>
  </div>
{:else}
  <div class="ac">
    <div class="ac-head">
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M8 1l5.5 2.2v4c0 3.5-2.3 6.4-5.5 7.6C4.8 13.6 2.5 10.7 2.5 7.2v-4L8 1z" />
      </svg>
      <span class="ac-title">Permission needed</span>
    </div>

    <p class="ac-body">
      The agent wants to use <code>{request.tool}</code>{#if request.server}
        <span class="ac-server"> on <code>{request.server}</code></span>{/if}.
    </p>
    {#if request.reason}<p class="ac-reason">{request.reason}</p>{/if}

    {#if hasArgs}
      <button class="ac-args-toggle" onclick={() => (showArgs = !showArgs)}>
        {showArgs ? 'Hide' : 'Show'} what it will send
      </button>
      {#if showArgs}<pre class="ac-args">{argsText()}</pre>{/if}
    {/if}

    <div class="ac-actions">
      {#each scopes as scope}
        <button class="ac-btn allow" disabled={busy} onclick={() => decide(true, scope)}>
          {SCOPE_LABEL[scope] ?? scope}
        </button>
      {/each}
      <button class="ac-btn deny" disabled={busy} onclick={() => decide(false, 'session')}>
        Decline
      </button>
    </div>

    <p class="ac-note">
      Declining stops it being asked again in this conversation. You can change that
      from the permissions panel.
    </p>
    {#if error}<p class="ac-error">{error}</p>{/if}
  </div>
{/if}

<style>
  .ac {
    border: 1px solid var(--control, #c97c2e);
    border-radius: var(--r, 10px);
    background: var(--control-bg, #fbf0e0);
    padding: 14px 16px;
    margin: 10px 0;
    max-width: 640px;
  }
  .ac-head { display: flex; align-items: center; gap: 7px; color: var(--control, #c97c2e); margin-bottom: 8px; }
  .ac-title { font-size: 12px; font-weight: 600; letter-spacing: 0.02em; text-transform: uppercase; }
  .ac-body { margin: 0 0 4px; font-size: 13.5px; line-height: 1.5; color: var(--text-1); }
  .ac-body code, .ac-settled-text code {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 12.5px; padding: 1px 5px; border-radius: 4px;
    background: var(--bg-surface); border: 1px solid var(--border);
  }
  .ac-server { color: var(--text-2); }
  .ac-reason { margin: 0 0 8px; font-size: 12px; color: var(--text-3); line-height: 1.5; }

  .ac-args-toggle {
    border: none; background: transparent; padding: 0; margin-bottom: 6px;
    color: var(--text-3); font-size: 11.5px; text-decoration: underline; cursor: pointer;
  }
  .ac-args {
    margin: 0 0 10px; padding: 9px 11px; max-height: 190px; overflow: auto;
    background: var(--bg-surface); border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11.5px; line-height: 1.5; color: var(--text-2);
    white-space: pre-wrap; word-break: break-word;
  }

  .ac-actions { display: flex; flex-wrap: wrap; gap: 7px; margin: 10px 0 8px; }
  .ac-btn {
    padding: 7px 12px; font-size: 12px; font-weight: 500;
    border-radius: var(--r-sm, 6px); border: 1px solid var(--border-strong);
    background: var(--bg-surface); color: var(--text-1); cursor: pointer;
  }
  .ac-btn:hover:not(:disabled) { border-color: var(--control, #c97c2e); }
  .ac-btn:disabled { opacity: 0.5; cursor: default; }
  .ac-btn.deny { color: #b91c1c; }
  .ac-btn.deny:hover:not(:disabled) { border-color: #b91c1c; }

  .ac-note { margin: 0; font-size: 11px; color: var(--text-4); line-height: 1.45; }
  .ac-error { margin: 8px 0 0; font-size: 11.5px; color: #b91c1c; }

  /* Answered: a quiet one-liner that keeps the record in the transcript. */
  .ac.settled {
    display: flex; align-items: center; gap: 8px;
    border-color: var(--border); background: var(--bg-subtle);
    padding: 8px 12px;
  }
  .ac-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--runtime, #1a7158); flex: 0 0 auto;
  }
  .ac.settled.denied .ac-dot { background: var(--text-4); }
  .ac-settled-text { font-size: 12px; color: var(--text-2); }
  .ac-scope { color: var(--text-4); }

  @media (max-width: 520px) {
    .ac-actions { flex-direction: column; align-items: stretch; }
  }
</style>
