<script lang="ts">
  import { execFlow, webhookChat, mcpCall, streamChat, fetchAgentHistory } from '$lib/services/api';
  import { onMount } from 'svelte';
  import { renderMarkdown } from '$lib/utils/markdown';

  type Msg = { role: string; content: string; notes?: string };
  type Conv = { id: string; title: string; time: string; active: boolean };
  const LS_CONV = 'a3t.chat.conversations';
  const LS_MSGS = 'a3t.chat.messages';
  const LS_AGENT = 'a3t.chat.agent';
  const LS_PENDING = 'a3t.chat.pending';
  const GREETING = "Hello! I'm the A3T platform assistant. I can help you manage templates, agents, canvas specs, resources, and more.\n\nWhat would you like to do?";

  function loadLS<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? (JSON.parse(v) as T) : fallback; } catch { return fallback; }
  }

  // Persisted chat state (survives reloads)
  let conversations = $state<Conv[]>(loadLS<Conv[]>(LS_CONV, []));
  let messagesById = $state<Record<string, Msg[]>>(loadLS<Record<string, Msg[]>>(LS_MSGS, {}));

  let inputValue = $state('');
  let sending = $state(false);
  // Text streamed for the in-flight reply, rendered as a live bubble.
  let streamingText = $state('');
  let streamingTool = $state('');
  // Full SSE transcript for the in-flight reply (intermediate reasoning across
  // iterations + tool markers). Kept after completion as the message's
  // collapsed execution log rather than discarded.
  let streamLog = '';
  let streamSeq = 0;
  let liveStream: { close: () => void } | null = null;

  // Active session id — restored from the persisted active conversation, else a fresh one.
  let sessionId = $state<string>('');
  {
    const active = conversations.find(c => c.active) ?? conversations[0];
    if (active) {
      sessionId = active.id;
      conversations = conversations.map(c => ({ ...c, active: c.id === active.id }));
    } else {
      const id = `session-${Date.now()}`;
      conversations = [{ id, title: 'New conversation', time: 'now', active: true }];
      messagesById = { [id]: [{ role: 'assistant', content: GREETING }] };
      sessionId = id;
    }
  }

  let messages = $derived(messagesById[sessionId] ?? []);

  function addMessage(m: Msg) {
    messagesById = { ...messagesById, [sessionId]: [...(messagesById[sessionId] ?? []), m] };
  }

  // Persist chat state whenever it changes.
  $effect(() => {
    try {
      localStorage.setItem(LS_CONV, JSON.stringify(conversations));
      localStorage.setItem(LS_MSGS, JSON.stringify(messagesById));
      localStorage.setItem(LS_AGENT, JSON.stringify(selectedAgent));
    } catch {}
  });

  // Auto-scroll the message list to the bottom whenever it changes.
  let messagesEl = $state<HTMLDivElement>();
  $effect(() => {
    messages.length;   // track new messages
    sending;           // track the typing indicator
    if (messagesEl) {
      requestAnimationFrame(() => { messagesEl!.scrollTop = messagesEl!.scrollHeight; });
    }
  });

  // The chat drives created Agents. Each agent is created from an Agent Canvas and
  // carries its own LLM binding (resource_bindings.llm); posting to the agent's
  // endpoint (EP) runs the shared a3t-chat-agent flow on that agent's bound LLM.
  // The EP is resolved from its trigger (agent-{id}-chat) → stored ep_path → /agent/{id}/chat.
  // Selection defaults to the system-default agent (variables.system_default, else agt-default).
  let agents = $state<{ id: string; name: string; llm: string; target: string; ep_path: string; system_default: boolean; enabled_tools: string[] }[]>([]);
  let selectedAgent = $state<string>(loadLS<string>(LS_AGENT, ''));
  let epByAgent = $state<Record<string, string>>({});   // agent id → endpoint path from its trigger

  function llmLabel(llm: string) { return llm ? llm.split('/').pop() : ''; }

  async function loadAgents() {
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' }, { silent: true });
    if (res.ok) {
      const rows = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
      agents = (rows as any[]).map(r => {
        let vars: any = {};
        try { vars = JSON.parse(r.variables || '{}'); } catch {}
        let rb: any = {};
        try { rb = JSON.parse(r.resource_bindings || '{}'); } catch {}
        return { id: r.id, name: r.name, llm: rb.llm || '', target: vars.target || '', ep_path: vars.ep_path || '', system_default: !!vars.system_default, enabled_tools: Array.isArray(vars.enabled_tools) ? vars.enabled_tools : [] };
      });
      // Default the selection to the system-default agent (else the stored one, else first).
      if (!selectedAgent || !agents.some(a => a.id === selectedAgent)) {
        const def = agents.find(a => a.system_default) ?? agents.find(a => a.id === 'agt-default') ?? agents[0];
        if (def) selectedAgent = def.id;
      }
    }
  }

  // Load triggers and map each agent's auto endpoint (trigger named agent-{id}-chat) → its path.
  // This honors path overrides done on the Triggers page.
  async function loadTriggers() {
    const res = await mcpCall('list_triggers', {});
    let d: any = res.data;
    if (typeof d === 'string') { try { d = JSON.parse(d); } catch { d = []; } }
    const list = Array.isArray(d) ? d : (d?.triggers ?? []);
    const map: Record<string, string> = {};
    for (const t of list) {
      const m = /^agent-(.+)-chat$/.exec(t?.name || '');
      const path = t?.config?.path;
      if (m && path) map[m[1]] = path;
    }
    epByAgent = map;
  }

  onMount(() => {
    loadAgents();
    loadTriggers();
    // Recover anything that completed while this component was unmounted.
    (async () => { await rehydrate(sessionId); await resumePending(); })();
  });

  // Start a fresh conversation (new session) with a greeting message.
  function startConversation(greeting: string) {
    const id = `session-${Date.now()}`;
    conversations = [{ id, title: 'New conversation', time: 'now', active: true }, ...conversations.map(c => ({ ...c, active: false }))];
    messagesById = { ...messagesById, [id]: [{ role: 'assistant', content: greeting }] };
    sessionId = id;
  }

  function newChat() { startConversation('How can I help you today?'); }

  function onAgentChange() {
    const a = agents.find(a => a.id === selectedAgent);
    const name = a?.name ?? 'Agent';
    const via = a?.llm ? ` (on ${llmLabel(a.llm)})` : '';
    startConversation(`Now chatting with ${name}${via}. How can I help?`);
  }

  function selectConversation(id: string) {
    conversations = conversations.map(c => ({ ...c, active: c.id === id }));
    sessionId = id;   // messages derives from the persisted history for this session
  }

  function deleteConversation(id: string) {
    conversations = conversations.filter(c => c.id !== id);
    const rest = { ...messagesById }; delete rest[id]; messagesById = rest;
    if (conversations.length === 0) { startConversation('How can I help you today?'); return; }
    if (!conversations.some(c => c.active)) {
      sessionId = conversations[0].id;
      conversations = conversations.map((c, i) => ({ ...c, active: i === 0 }));
    }
  }

  // The agent's full conversation lives server-side (`agent:session:<id>`, TTL
  // 1800s). Reconcile it into local state on mount: a reply that arrived while
  // this component was unmounted — PanelStack destroys it on navigation, taking
  // its persistence $effect with it — is otherwise lost from the client even
  // though the server has it.
  async function rehydrate(sid: string) {
    const server = await fetchAgentHistory(`${selectedAgent}:${sid}`);
    if (server.length === 0) return;
    const local = messagesById[sid] ?? [];
    // Append only assistant turns we are missing, matched by content, so the
    // greeting and optimistic user echoes are preserved and nothing duplicates.
    const seen = new Set(local.map(m => `${m.role}:${m.content}`));
    const missing = server.filter(m => !seen.has(`${m.role}:${m.content}`));
    if (missing.length > 0) {
      messagesById = { ...messagesById, [sid]: [...local, ...missing] };
    }
  }

  type Pending = { sessionId: string; agent: string; seq: number };
  function setPending(p: Pending | null) {
    try {
      if (p) localStorage.setItem(LS_PENDING, JSON.stringify(p));
      else localStorage.removeItem(LS_PENDING);
    } catch {}
  }
  function getPending(): Pending | null {
    try { const v = localStorage.getItem(LS_PENDING); return v ? JSON.parse(v) : null; } catch { return null; }
  }

  // Attach the live token stream for `sid`, resuming after `afterSeq`.
  function attachStream(sid: string, afterSeq = 0) {
    liveStream?.close();
    streamingText = '';
    streamingTool = '';
    streamLog = '';
    streamSeq = afterSeq;
    liveStream = streamChat(`${selectedAgent}:${sid}`, {
      onSeq: (n) => { streamSeq = n; setPending({ sessionId: sid, agent: selectedAgent, seq: n }); },
      onToken: (t) => { streamingText += t; streamLog += t; },
      onTool: (name) => { streamingTool = name; streamLog += `\n\n▸ calling ${name}\n`; },
      onDone: () => { streamingTool = ''; },
    }, afterSeq);
  }

  function detachStream() {
    liveStream?.close();
    liveStream = null;
    streamingText = '';
    streamingTool = '';
    // streamLog is intentionally retained — sendMessage attaches it to the
    // finished message before the next turn resets it.
  }

  /// The working notes for a finished reply: everything streamed except the
  /// final answer itself, which is rendered normally. Returns undefined when
  /// there was nothing beyond the answer, so no empty box is shown.
  function notesFor(finalContent: string): string | undefined {
    let log = streamLog;
    const tail = finalContent.trim();
    if (tail && log.trimEnd().endsWith(tail)) {
      log = log.trimEnd().slice(0, -tail.length);
    }
    log = log.trim();
    return log.length > 0 ? log : undefined;
  }

  // A run left in flight by a navigation or reload: re-attach its stream and
  // reconcile the final message from the server once it lands.
  async function resumePending() {
    const p = getPending();
    if (!p || p.sessionId !== sessionId || p.agent !== selectedAgent) return;
    sending = true;
    attachStream(p.sessionId, p.seq);
    for (let i = 0; i < 180; i++) {
      await new Promise(r => setTimeout(r, 1000));
      const before = (messagesById[p.sessionId] ?? []).length;
      await rehydrate(p.sessionId);
      if ((messagesById[p.sessionId] ?? []).length > before) break;
    }
    sending = false;
    detachStream();
    setPending(null);
  }

  async function sendMessage() {
    if (!inputValue.trim() || sending) return;
    const text = inputValue.trim();
    inputValue = '';
    addMessage({ role: 'user', content: text });
    sending = true;

    // Update conversation title from first user message
    const active = conversations.find(c => c.active);
    if (active && active.title === 'New conversation') {
      active.title = text.slice(0, 40) + (text.length > 40 ? '...' : '');
      conversations = [...conversations];
    }

    // Post the message to the selected target's endpoint:
    //  - claude-cli-max (platform): the built-in LLM via the wrapper (a3t-chat-agent flow)
    //  - an agent: POST to its endpoint (EP), resolved from its trigger → ep_path → default
    // Post to the selected agent's endpoint (EP). The agent's bound LLM (resource_bindings.llm)
    // is forwarded as llm_resource, which a3t-chat-agent applies to the reasoning loop.
    const a = agents.find(x => x.id === selectedAgent);
    const ep = epByAgent[selectedAgent] || a?.ep_path || `/agent/${selectedAgent}/chat`;
    const body: Record<string, any> = { message: text, session_id: `${selectedAgent}:${sessionId}` };
    if (a?.llm) body.llm_resource = a.llm;
    // Forward the agent's capability ceiling so the policy engine enforces it in chat too.
    if (a?.enabled_tools?.length) body.enabled_tools = a.enabled_tools;
    setPending({ sessionId, agent: selectedAgent, seq: 0 });
    attachStream(sessionId);
    const res = await webhookChat(ep, body, { maxPolls: 90, pollInterval: 1000 });
    setPending(null);

    if (res.ok) {
      const d = res.data;
      const raw = d?.content ?? d?.output?.content ?? d?.output ?? d?.agent_output ?? (typeof d === 'string' ? d : JSON.stringify(d));
      const output = typeof raw === 'string' ? raw : JSON.stringify(raw);
      // A run can succeed with no text — e.g. the agent exhausted its step limit
      // mid-task. `??` above does not catch that, since '' is neither null nor
      // undefined, so it used to render an empty bubble with no explanation.
      if (!output.trim()) {
        const reason = d?.stopped_reason === 'max_iterations'
          ? `stopped at its ${d?.iterations ?? 'step'} step limit after ${d?.tool_calls_made ?? 0} tool calls`
          : 'returned no content';
        addMessage({
          role: 'assistant',
          content: `_No response — the agent ${reason}. Try narrowing the request, or ask it to continue._`,
          notes: notesFor(''),
        });
      } else {
        addMessage({ role: 'assistant', content: output, notes: notesFor(output) });
      }
    } else {
      // A client-side timeout does not mean the run failed — the server may have
      // finished after we stopped polling. Prefer the persisted transcript.
      const before = (messagesById[sessionId] ?? []).length;
      await rehydrate(sessionId);
      if ((messagesById[sessionId] ?? []).length === before) {
        addMessage({ role: 'assistant', content: `Error: ${res.error}` });
      }
    }
    detachStream();
    streamLog = '';
    sending = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }
</script>

<div class="chat-layout">
  <!-- Left: conversation history -->
  <div class="chat-sidebar">
    <div class="chat-sidebar-header">
      <span class="chat-sidebar-title">Chats</span>
      <button class="new-chat-btn" onclick={newChat}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10"/></svg>
      </button>
    </div>
    <div class="chat-list">
      {#each conversations as conv}
        <div class="chat-item" class:active={conv.active}>
          <button class="chat-item-body" onclick={() => selectConversation(conv.id)}>
            <div class="chat-item-title">{conv.title}</div>
            <div class="chat-item-time">{conv.time}</div>
          </button>
          <button class="chat-item-delete" onclick={() => deleteConversation(conv.id)} title="Delete">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 4L4 12M4 4l8 8"/></svg>
          </button>
        </div>
      {/each}
    </div>
    <div class="chat-sidebar-footer">
      <label class="agent-select-label" for="agent-select">Agent</label>
      <select id="agent-select" class="agent-select" bind:value={selectedAgent} onchange={onAgentChange}>
        {#if agents.length === 0}
          <option value="" disabled>No agents — create one in Agents</option>
        {/if}
        {#each agents as a}
          <option value={a.id}>{a.name}{a.llm ? ` · ${llmLabel(a.llm)}` : ''}{a.system_default ? ' · default' : ''}</option>
        {/each}
      </select>
    </div>
  </div>

  <!-- Right: main chat -->
  <div class="chat-main">
    <div class="chat-messages" bind:this={messagesEl}>
      {#each messages as msg}
        <div class="message" class:user={msg.role === 'user'} class:assistant={msg.role === 'assistant'}>
          <div class="msg-avatar">
            {#if msg.role === 'user'}
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="5" r="3"/><path d="M2 14a6 6 0 0112 0"/></svg>
            {:else}
              <span class="agent-mark">A3</span>
            {/if}
          </div>
          <div class="msg-body">
            <div class="msg-role">{msg.role === 'user' ? 'You' : 'A3T'}</div>
            <div class="msg-content md">{@html renderMarkdown(msg.content)}</div>
            {#if msg.notes}
              <details class="exec-log">
                <summary>Execution log</summary>
                <div class="exec-log-body">{msg.notes}</div>
              </details>
            {/if}
          </div>
        </div>
      {/each}
      {#if sending && (streamingText || streamingTool)}
        <div class="message assistant">
          <div class="msg-avatar"><span class="agent-mark">A3</span></div>
          <div class="msg-body">
            <div class="msg-role">A3T</div>
            {#if streamingText}
              <div class="msg-content">{streamingText}</div>
            {/if}
            {#if streamingTool}
              <div class="msg-content tool-note">▸ calling {streamingTool}…</div>
            {/if}
          </div>
        </div>
      {/if}
      {#if sending && !streamingText}
        <div class="message assistant">
          <div class="msg-avatar"><span class="agent-mark">A3</span></div>
          <div class="msg-body">
            <div class="msg-role">A3T</div>
            <div class="msg-content typing">
              <span class="dot-pulse"></span>
            </div>
          </div>
        </div>
      {/if}
    </div>

    <div class="chat-input-area">
      <div class="chat-input-wrap">
        <textarea
          class="chat-input"
          placeholder="Ask anything about the platform..."
          bind:value={inputValue}
          onkeydown={handleKeydown}
          rows="1"
        ></textarea>
        <button class="send-btn" onclick={sendMessage} disabled={!inputValue.trim() || sending}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14 2L7 9M14 2l-5 12-2-5-5-2z"/></svg>
        </button>
      </div>
      <div class="chat-input-hint">
        <span>Enter to send</span>
        <span>Shift+Enter for new line</span>
      </div>
    </div>
  </div>
</div>

<style>
  .tool-note { opacity: 0.75; font-style: italic; }

  /* Collapsed execution log — the streamed working notes, kept after the run. */
  .exec-log {
    margin-top: 8px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 12px;
  }
  .exec-log summary {
    cursor: pointer;
    padding: 6px 12px;
    color: var(--text-2, #6b7280);
    user-select: none;
    list-style: none;
  }
  .exec-log summary::-webkit-details-marker { display: none; }
  .exec-log summary::before { content: "▸ "; display: inline-block; transition: transform 0.15s; }
  .exec-log[open] summary::before { content: "▾ "; }
  .exec-log summary:hover { color: var(--text-1); }
  .exec-log-body {
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--text-2, #6b7280);
    max-height: 340px;
    overflow-y: auto;
  }
  .chat-layout {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100%;
    overflow: hidden;
  }

  /* Sidebar */
  .chat-sidebar {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg-surface);
    /* Same scroll chain as ChatPageAI: without these the list grows past the
       row instead of scrolling, hiding older conversations. */
    min-height: 0;
    overflow: hidden;
  }
  .chat-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .chat-sidebar-title {
    font-family: var(--font-serif);
    font-size: 15px;
    font-weight: 500;
  }
  .new-chat-btn {
    width: 28px; height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-surface);
    color: var(--text-3);
    display: grid; place-items: center;
    cursor: pointer;
  }
  .new-chat-btn:hover { background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong); }

  .chat-list {
    flex: 1;
    min-height: 0;      /* required for overflow scrolling inside a flex column */
    overflow-y: auto;
    padding: 8px;
  }
  .chat-item {
    display: flex;
    align-items: center;
    gap: 0;
    border-radius: var(--r-sm);
    margin-bottom: 2px;
  }
  .chat-item:hover { background: var(--bg-subtle); }
  .chat-item.active { background: var(--bg-subtle); }
  .chat-item-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border: none;
    background: none;
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .chat-item-title {
    font-size: 13px;
    font-weight: 450;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chat-item-time {
    font-size: 11px;
    color: var(--text-4);
    font-family: var(--font-mono);
  }
  .chat-item-delete {
    width: 24px; height: 24px;
    border: none;
    background: none;
    color: var(--text-5);
    cursor: pointer;
    display: grid; place-items: center;
    border-radius: 4px;
    flex-shrink: 0;
    margin-right: 6px;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .chat-item:hover .chat-item-delete { opacity: 1; }
  .chat-item-delete:hover { background: var(--err-bg, #fde8e8); color: var(--err, #d44); }

  .chat-sidebar-footer {
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  /* Main chat */
  .chat-main {
    display: flex;
    flex-direction: column;
    background: var(--bg-page);
    min-height: 0;      /* allow the flex column to shrink so children can scroll */
    overflow: hidden;
  }
  .agent-select-label {
    font-size: 10px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 0.04em; color: var(--text-4);
  }
  .agent-select {
    width: 100%;
    font-size: 12px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-page);
    color: var(--text-1);
    cursor: pointer;
  }

  .chat-messages {
    flex: 1;
    min-height: 0;      /* required for overflow scrolling inside a flex column */
    overflow-y: auto;
    padding: 24px 32px 16px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .message {
    display: flex;
    gap: 12px;
    max-width: 720px;
  }
  .message.user { margin-left: auto; flex-direction: row-reverse; }

  .msg-avatar {
    width: 32px; height: 32px;
    border-radius: 8px;
    display: grid; place-items: center;
    flex-shrink: 0;
    font-size: 11px;
  }
  .message.assistant .msg-avatar {
    background: var(--bg-subtle);
    color: var(--text-3);
  }
  .message.user .msg-avatar {
    background: var(--ephemeral-bg);
    color: var(--ephemeral-ink);
  }
  .agent-mark {
    font-family: var(--font-serif);
    font-weight: 600;
    font-size: 11px;
  }

  .msg-body { min-width: 0; }
  .msg-role {
    font-size: 11px;
    font-weight: 550;
    color: var(--text-3);
    margin-bottom: 4px;
  }
  .message.user .msg-role { text-align: right; }

  .msg-content {
    font-size: 14px;
    line-height: 1.6;
    color: var(--text-1);
    padding: 12px 16px;
    border-radius: 12px;
  }
  .message.assistant .msg-content {
    background: var(--bg-surface);
    border: 1px solid var(--border);
  }
  .message.user .msg-content {
    background: var(--text-1);
    color: var(--bg-page);
    border: none;
  }

  .msg-content :global(strong) { font-weight: 600; }

  /* Rendered markdown in a reply bubble */
  .md :global(> *:first-child) { margin-top: 0; }
  .md :global(> *:last-child) { margin-bottom: 0; }
  .md :global(h1), .md :global(h2), .md :global(h3),
  .md :global(h4), .md :global(h5), .md :global(h6) {
    margin: 14px 0 6px; font-weight: 650; line-height: 1.3;
  }
  .md :global(h1) { font-size: 18px; }
  .md :global(h2) { font-size: 16px; }
  .md :global(h3) { font-size: 14.5px; }
  .md :global(h4), .md :global(h5), .md :global(h6) { font-size: 13.5px; }
  .md :global(p) { margin: 6px 0; }
  .md :global(ul), .md :global(ol) { margin: 6px 0; padding-left: 20px; }
  .md :global(li) { margin: 2px 0; }
  .md :global(li > p) { margin: 0; }
  .md :global(a) { color: var(--ephemeral, #2563eb); text-decoration: underline; }
  .md :global(hr) { border: 0; border-top: 1px solid var(--border); margin: 12px 0; }
  .md :global(blockquote) {
    margin: 8px 0; padding: 2px 12px;
    border-left: 3px solid var(--border-strong); color: var(--text-2);
  }
  .md :global(code) {
    font-family: var(--font-mono); font-size: 12.5px;
    background: var(--bg-subtle); border: 1px solid var(--border);
    border-radius: 4px; padding: 1px 4px;
  }
  .md :global(pre) {
    margin: 8px 0; padding: 10px 12px; overflow-x: auto;
    background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 8px;
  }
  .md :global(pre code) { background: none; border: 0; padding: 0; font-size: 12px; line-height: 1.5; }
  /* Tables scroll inside the bubble rather than widening it */
  .md :global(table) {
    display: block; width: max-content; max-width: 100%; overflow-x: auto;
    border-collapse: collapse; margin: 8px 0; font-size: 13px;
  }
  .md :global(th), .md :global(td) {
    border: 1px solid var(--border); padding: 5px 10px; text-align: left; vertical-align: top;
  }
  .md :global(th) { background: var(--bg-subtle); font-weight: 600; }
  .md :global(img) { max-width: 100%; }

  /* Typing indicator */
  .typing { padding: 16px 20px; }
  .dot-pulse {
    display: inline-flex;
    gap: 4px;
  }
  .dot-pulse::before, .dot-pulse::after, .dot-pulse {
    content: '';
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--text-4);
    animation: pulse-dot 1.2s infinite;
  }
  .dot-pulse::before { animation-delay: 0s; }
  .dot-pulse { animation-delay: 0.2s; }
  .dot-pulse::after { animation-delay: 0.4s; }
  @keyframes pulse-dot {
    0%, 100% { opacity: 0.3; }
    50% { opacity: 1; }
  }

  /* Input */
  .chat-input-area {
    padding: 16px 32px 20px;
    border-top: 1px solid var(--border);
    background: var(--bg-page);
    flex-shrink: 0;     /* keep the input anchored at the bottom */
  }
  .chat-input-wrap {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 10px 12px 10px 16px;
    transition: border-color 0.15s;
  }
  .chat-input-wrap:focus-within {
    border-color: var(--ephemeral);
    box-shadow: 0 0 0 2px var(--ephemeral-bg);
  }
  .chat-input {
    flex: 1;
    border: none;
    outline: none;
    background: none;
    font-size: 14px;
    font-family: var(--font-sans);
    color: var(--text-1);
    resize: none;
    line-height: 1.5;
    max-height: 120px;
  }
  .chat-input::placeholder { color: var(--text-4); }

  .send-btn {
    width: 36px; height: 36px;
    border-radius: 8px;
    border: none;
    background: var(--text-1);
    color: white;
    display: grid; place-items: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: opacity 0.12s;
  }
  .send-btn:hover { opacity: 0.85; }
  .send-btn:disabled { opacity: 0.3; cursor: default; }

  .chat-input-hint {
    display: flex;
    gap: 12px;
    margin-top: 6px;
    padding: 0 4px;
    font-size: 10px;
    color: var(--text-5);
    font-family: var(--font-mono);
  }
</style>
