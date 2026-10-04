<script lang="ts">
  /// The AI tab of every designer — template, agent, flow, canvas, resource.
  ///
  /// It talks to a real chat agent (the default chat agent, or any agent picked
  /// here) through the same `/chat` stream as the main chat. Each turn carries the
  /// document currently in the editor, so the model works on what is on screen —
  /// unsaved edits included. Changes come back as `propose_edit` and are applied
  /// straight to the editor buffer: nothing is stored until the user presses the
  /// designer's own Save. Undo restores the text from before the proposal.
  import { Chat } from '@ai-sdk/svelte';
  import { DefaultChatTransport } from 'ai';
  import { API_BASE } from '$lib/config';
  import { execFlow, getAccessToken } from '$lib/services/api';
  import { renderMarkdown } from '$lib/utils/markdown';

  let {
    kind, name, gettext, onapply, readonly = false, placeholder = '',
  }: {
    /// What is being edited: template | agent | flow | canvas | resource.
    kind: string;
    /// `namespace/name` (or a label) of the item.
    name: string;
    /// The editor's current text, read at send time.
    gettext: () => string;
    /// Put proposed text into the editor (unsaved).
    onapply: (text: string) => void;
    readonly?: boolean;
    placeholder?: string;
  } = $props();

  const DRAFT_MAX = 24000;
  /// Lookups the designer AI may use to answer questions about related items.
  const DESIGNER_READS = [
    'get_flow', 'list_flows', 'list_resources', 'get_resource', 'list_task_types',
    'list_agent_templates', 'get_agent_template', 'list_agents', 'get_agent',
    'get_canvas', 'list_runtimes', 'list_canvas_presets', 'preview_placement',
  ];
  const LS_AGENT = 'a3t.designer.agent';

  type Agent = { id: string; name: string; llm: string; enabled_tools?: string[]; system_default: boolean };
  let agents = $state<Agent[]>([]);
  let agentId = $state('');
  try { agentId = JSON.parse(localStorage.getItem(LS_AGENT) ?? '""'); } catch {}

  const bindingsOf = (r: any) => {
    const b = r.resource_bindings;
    if (b && typeof b === 'object') return b;
    try { return JSON.parse(b ?? '{}'); } catch { return {}; }
  };
  (async () => {
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' }, { silent: true });
    const rows = res.ok ? (Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? [])) : [];
    agents = (rows as any[]).map((r) => ({
      id: r.id, name: r.name ?? r.id, llm: r.llm ?? bindingsOf(r).llm ?? '',
      enabled_tools: Array.isArray(r.enabled_tools) ? r.enabled_tools : undefined,
      system_default: !!r.system_default,
    }));
    if (!agents.some((a) => a.id === agentId)) {
      agentId = (agents.find((a) => a.system_default) ?? agents[0])?.id ?? '';
    }
  })();
  $effect(() => { try { localStorage.setItem(LS_AGENT, JSON.stringify(agentId)); } catch {} });

  // One conversation per designer session, under the chosen agent.
  const convId = `designer-${kind}-${Math.random().toString(36).slice(2, 10)}`;
  const agent = $derived(agents.find((a) => a.id === agentId));

  const chat = new Chat({
    messages: [],
    transport: new DefaultChatTransport({
      api: `${API_BASE}/chat`,
      headers: (): Record<string, string> => {
        const t = getAccessToken();
        return t ? { Authorization: `Bearer ${t}` } : {};
      },
      prepareSendMessagesRequest: ({ messages }) => {
        const last = messages[messages.length - 1];
        const text = (last?.parts ?? []).filter((p: any) => p.type === 'text').map((p: any) => p.text).join('');
        const doc = gettext() ?? '';
        const a = agent;
        // The designer's tool set is ALWAYS explicit: propose_edit (writes
        // nothing) plus read-only lookups — narrowed further to the agent's own
        // list when it has one. So "the AI here cannot save, delete or deploy"
        // is enforced by policy, not just asked of the model; and an agent with
        // no list does not fall into "every tool needs an approval" (which this
        // pane has no card for).
        const reads = Array.isArray(a?.enabled_tools)
          ? DESIGNER_READS.filter((t) => a!.enabled_tools!.includes(t))
          : DESIGNER_READS;
        const tools = ['propose_edit', ...reads];
        return {
          body: {
            message: text,
            session_id: `${agentId}:${convId}`,
            ui_context: {
              kind, name, mode: 'designer', section: 'editor',
              draft_text: doc.slice(0, DRAFT_MAX),
              draft_truncated: doc.length > DRAFT_MAX,
            },
            ...(a?.llm ? { llm_resource: a.llm } : {}),
            enabled_tools: tools,
          },
        };
      },
    }),
    onError: (e) => console.error('[designer-ai]', e),
  });

  const busy = $derived(chat.status === 'submitted' || chat.status === 'streaming');

  // ── Apply proposals to the editor, once each, with undo ──
  let applied = new Set<string>();
  let undoText = $state<string | null>(null);
  let lastSummary = $state('');
  $effect(() => {
    for (const m of chat.messages as any[]) {
      if (m.role !== 'assistant') continue;
      (m.parts ?? []).forEach((p: any, i: number) => {
        if (p.type !== 'data-edit_draft' || !p.data?.raw) return;
        const key = `${m.id}:${i}`;
        if (applied.has(key)) return;
        applied.add(key);
        if (readonly) return;
        undoText = gettext();
        lastSummary = p.data.summary || 'Edit applied';
        onapply(p.data.raw);
      });
    }
  });
  function undo() {
    if (undoText == null) return;
    onapply(undoText);
    undoText = null;
    lastSummary = '';
  }

  let input = $state('');
  let listEl: HTMLDivElement | undefined = $state();
  function send() {
    const t = input.trim();
    if (!t || busy || !agentId) return;
    input = '';
    chat.sendMessage({ text: t });
  }
  $effect(() => {
    chat.messages.length; chat.status;
    if (listEl) requestAnimationFrame(() => { listEl!.scrollTop = listEl!.scrollHeight; });
  });

  const textOf = (m: any) => (m.parts ?? []).filter((p: any) => p.type === 'text').map((p: any) => p.text).join('');
  const editsIn = (m: any) => (m.parts ?? []).filter((p: any) => p.type === 'data-edit_draft').length;
</script>

<div class="dc">
  <div class="dc-bar">
    <label class="dc-agent" title="The chat agent that answers here">
      <span>Agent</span>
      <select bind:value={agentId} disabled={busy}>
        {#each agents as a}<option value={a.id}>{a.name}{a.system_default ? ' (default)' : ''}</option>{/each}
      </select>
    </label>
    <span class="dc-ctx" title="Sent with every message — the editor's current text, unsaved edits included">
      context: {kind} <b>{name || 'new'}</b>
    </span>
  </div>

  {#if undoText != null}
    <div class="dc-applied">
      <span>✎ {lastSummary} — applied to the editor, <b>not saved</b>.</span>
      <button onclick={undo}>Undo</button>
    </div>
  {/if}

  <div class="dc-list" bind:this={listEl}>
    {#if chat.messages.length === 0}
      <div class="dc-msg assistant">
        <span class="dc-mark">A3</span>
        <p>I can see this {kind} as it is in the editor. Tell me what to change — I’ll edit it here, and you press <b>Save</b> when you’re happy.{readonly ? ' (This one is read-only, so I can only explain it.)' : ''}</p>
      </div>
    {/if}
    {#each chat.messages as m (m.id)}
      {#if m.role === 'user'}
        <div class="dc-msg user"><p>{textOf(m)}</p></div>
      {:else}
        <div class="dc-msg assistant">
          <span class="dc-mark">A3</span>
          <div>
            {#if textOf(m)}<div class="md">{@html renderMarkdown(textOf(m))}</div>{/if}
            {#if editsIn(m)}<div class="dc-edit">✎ edited the {kind} in the editor</div>{/if}
          </div>
        </div>
      {/if}
    {/each}
    {#if busy}<div class="dc-typing">thinking…</div>{/if}
  </div>

  <div class="dc-input">
    <textarea rows="2" bind:value={input} disabled={!agentId}
      placeholder={placeholder || `Describe a change to this ${kind}…`}
      onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}></textarea>
    {#if busy}
      <button class="btn sm" onclick={() => chat.stop()}>Stop</button>
    {:else}
      <button class="btn sm primary" onclick={send} disabled={!input.trim() || !agentId}>Send</button>
    {/if}
  </div>
</div>

<style>
  .dc { display: flex; flex-direction: column; height: 100%; min-height: 0; font-size: 13px; }
  .dc-bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .dc-agent { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); }
  .dc-agent select { font-size: 12px; padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg-surface); max-width: 220px; }
  .dc-ctx { font-size: 11.5px; color: var(--text-4); font-family: var(--font-mono); }
  .dc-ctx b { color: var(--text-2); font-weight: 500; }
  .dc-applied { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin: 8px 12px 0; padding: 7px 10px; border-radius: var(--r-sm); background: var(--runtime-bg, #e6f4f1); color: var(--runtime, #0f766e); font-size: 12.5px; }
  .dc-applied button { background: var(--bg-surface); border: 1px solid currentColor; color: inherit; border-radius: var(--r-sm); padding: 2px 10px; cursor: pointer; font-size: 12px; }
  .dc-list { flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 10px; min-height: 0; }
  .dc-msg { display: flex; gap: 8px; align-items: flex-start; }
  .dc-msg p { margin: 0; line-height: 1.5; }
  .dc-msg.user { justify-content: flex-end; }
  .dc-msg.user p { background: var(--bg-subtle); padding: 7px 10px; border-radius: 10px; max-width: 85%; white-space: pre-wrap; }
  .dc-mark { flex: none; width: 22px; height: 22px; border-radius: 6px; background: var(--spec-bg); color: var(--spec); font-size: 10px; font-weight: 600; display: grid; place-items: center; }
  .md :global(p) { margin: 0 0 6px; }
  .md :global(pre) { background: var(--bg-subtle); padding: 8px; border-radius: var(--r-sm); overflow-x: auto; font-size: 12px; }
  .dc-edit { margin-top: 4px; font-size: 12px; color: var(--runtime, #0f766e); }
  .dc-typing { font-size: 12px; color: var(--text-4); font-style: italic; }
  .dc-input { display: flex; gap: 8px; align-items: flex-end; padding: 10px 12px; border-top: 1px solid var(--border); }
  .dc-input textarea { flex: 1; resize: none; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-sm); font: inherit; font-size: 13px; background: var(--bg-surface); }
</style>
