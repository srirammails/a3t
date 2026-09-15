<script lang="ts">
  import { setActiveSelection, getActivePanel, navigateToDetail } from '$lib/stores/panels.svelte.ts';
  // Same layout as the original chat (conversation sidebar + main pane); the
  // message engine underneath is @ai-sdk/svelte's `Chat` talking to POST /chat,
  // which serves the AI SDK UI Message Stream v1 protocol from Rust.
  //
  // Two deliberate deviations from SDK defaults:
  //  1. The *server* owns the transcript (`agent:session:<id>`, TTL 1800s), so
  //     prepareSendMessagesRequest sends only the new message + session id.
  //     Posting the whole array (the SDK default) would double-count turns.
  //  2. Conversations are kept client-side per session id, exactly as before, so
  //     switching chats restores history without a round-trip.
  import { onMount, untrack } from 'svelte';
  import { Chat } from '@ai-sdk/svelte';
  import { DefaultChatTransport } from 'ai';
  import type { UIMessage } from '@ai-sdk/svelte';
  import { API_BASE } from '$lib/config';
  import { execFlow, mcpCall, getAccessToken, fetchAgentHistory,
           fetchChatSessions, fetchChatSession, deleteChatSession } from '$lib/services/api';
  import { renderMarkdown } from '$lib/utils/markdown';
  import ArtifactWidget from '$lib/components/chat/ArtifactWidget.svelte';
  import ArtifactPane from '$lib/components/chat/ArtifactPane.svelte';
  import ApprovalCard from '$lib/components/chat/ApprovalCard.svelte';
  import PermissionsPanel from '$lib/components/chat/PermissionsPanel.svelte';
  import ListWidget from '$lib/components/chat/ListWidget.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import { ARTIFACTS, LIST_KIND_TO_ITEM, toEditorText, type ArtifactKind } from '$lib/components/chat/artifacts';
  import { bumpContext, setPendingMessage, takePendingMessage, trackOpen } from '$lib/stores/apps.svelte.ts';

  /// `id` is the client-side conversation id; `serverId` is the full server
  /// session key ("<agent>:<id>") that owns the transcript, and `agent` the agent
  /// it was held with — both needed so a conversation restored on another device
  /// reopens against the same agent and transcript.
  type Conv = { id: string; title: string; time: string; active: boolean;
                agent?: string; serverId?: string };
  const LS_CONV = 'a3t.chat.conversations';
  const LS_MSGS = 'a3t.chat.ai.messages';
  const LS_AGENT = 'a3t.chat.agent';
  const GREETING = "Hello! I'm the A3T platform assistant. I can help you manage templates, agents, canvas specs, resources, and more.\n\nWhat would you like to do?";

  /// Unique per conversation. Date.now() alone collides when two are created in
  /// the same millisecond, which would alias two chats onto one server session.
  function newSessionId() {
    return `session-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  /// Session id from /chat/<session>, so a URL is shareable and survives reload.
  function sessionFromUrl(): string | null {
    if (typeof window === 'undefined') return null;
    const seg = window.location.pathname.split('/').filter(Boolean);
    return seg[0] === 'chat' && seg[1] ? decodeURIComponent(seg[1]) : null;
  }

  /// Point the shell at this conversation.
  ///
  /// This used to write `history` directly, which left the panel store's
  /// `props.selectedId` stale: the shell no longer knew which session was open,
  /// so the context list could not highlight it and switching apps lost it.
  function setSessionUrl(id: string, _replace = false) {
    setActiveSelection(id);
  }

  function loadLS<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? (JSON.parse(v) as T) : fallback; } catch { return fallback; }
  }
  const greetingMsg = (): UIMessage => ({
    id: `greet-${Date.now()}`, role: 'assistant', parts: [{ type: 'text', text: GREETING }],
  } as UIMessage);

  // localStorage is a *cache* for instant paint and offline reads. The server's
  // owner-scoped session index is the record — see syncServerSessions().
  let conversations = $state<Conv[]>(loadLS<Conv[]>(LS_CONV, []));
  let messagesById = $state<Record<string, UIMessage[]>>(loadLS<Record<string, UIMessage[]>>(LS_MSGS, {}));
  let inputValue = $state('');

  // The conversation list, its resizing and its icon rail all moved to the
  // shell's context pane, so this page no longer owns a side column.

  // ── Artifact pane width ──
  // The pane had no handle after the conversation list moved to the shell, so it
  // was stuck at a fixed share of the width. Persisted per browser.
  const LS_PANE = 'a3t.chat.pane.w';
  const PANE_MIN = 320;
  let paneW = $state<number>(loadLS<number>(LS_PANE, 560));
  let paneDragging = $state(false);
  let chatLayoutEl = $state<HTMLDivElement>();

  function paneMax() {
    const w = chatLayoutEl?.getBoundingClientRect().width ?? 1200;
    return Math.max(PANE_MIN, w - 420);   // always leave the transcript readable
  }
  function setPaneW(w: number) {
    paneW = Math.min(paneMax(), Math.max(PANE_MIN, Math.round(w)));
    try { localStorage.setItem(LS_PANE, JSON.stringify(paneW)); } catch {}
  }
  function startPaneDrag(e: PointerEvent) {
    paneDragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function onPaneDrag(e: PointerEvent) {
    if (!paneDragging || !chatLayoutEl) return;
    // Measured from the right edge: the pane is the right-hand column.
    setPaneW(chatLayoutEl.getBoundingClientRect().right - e.clientX);
  }
  function endPaneDrag(e: PointerEvent) {
    paneDragging = false;
    try { (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId); } catch {}
  }

  // ── Right-hand artifact pane (flow viewer/editor) ──
  /// Ceiling on the unsaved-buffer text sent in `ui_context`, in characters.
  const UI_DRAFT_MAX = 12000;
  let openArtifact = $state<{ kind: ArtifactKind; namespace: string; name: string; text: string; id?: string } | null>(null);
  /// Which section of the open artifact is on screen — sent with the next turn
  /// so "tighten this" resolves to what the user is actually reading.
  let openSection = $state('graph');
  /// Newest streamed version of whatever the pane has open. A revised draft
  /// should refresh the open pane rather than appear only as another card the
  /// user has to click.
  let openRevision = $state<{ text: string; seq: number } | null>(null);
  /// The live editor buffer for the open artifact, including edits the user has
  /// not saved. Sent with the next turn so the agent reasons about what is on
  /// screen rather than the registry copy, which may now be stale.
  let openEdit = $state<{ text: string; dirty: boolean } | null>(null);
  let revisionSeq = 0;
  let lastRevisionKey = '';

  $effect(() => {
    const msgs = chat.messages as any[];
    const open = openArtifact;
    if (!open) { lastRevisionKey = ''; return; }
    for (let i = msgs.length - 1; i >= 0; i--) {
      const parts = msgs[i]?.parts ?? [];
      for (let j = parts.length - 1; j >= 0; j--) {
        const p = parts[j];
        const t = p?.type;
        if (typeof t !== 'string' || !t.startsWith('data-')) continue;
        const kind = t.slice(5);
        if (!(kind in ARTIFACTS) || kind !== open.kind) continue;
        const d = p.data ?? {};
        if (`${d.namespace}/${d.name}` !== `${open.namespace}/${open.name}`) continue;
        const key = `${msgs[i].id}:${j}`;
        if (key === lastRevisionKey) return;   // already seen this part
        lastRevisionKey = key;
        const next = toEditorText(d.raw ?? '', ARTIFACTS[kind as ArtifactKind].format);
        if (next && next !== open.text) openRevision = { text: next, seq: ++revisionSeq };
        return;
      }
    }
  });
  let messagesEl = $state<HTMLDivElement>();
  let agents = $state<{ id: string; name: string; llm: string; target: string; ep_path: string; system_default: boolean; enabled_tools?: string[] }[]>([]);
  let selectedAgent = $state<string>(loadLS<string>(LS_AGENT, ''));
  let sessionId = $state<string>('');

  {
    // A /chat/<session> URL is authoritative — it may name a conversation this
    // browser has not seen, which then rehydrates from the server transcript.
    // A bare `/chat` is the LANDING state: a centred composer and no session at
    // all. Minting one on arrival created an empty conversation every time the
    // app was opened; now the session is created by the first message.
    const fromUrl = sessionFromUrl();
    if (fromUrl) {
      sessionId = fromUrl;
      if (!conversations.some(c => c.id === fromUrl)) {
        conversations = [{ id: fromUrl, title: 'New conversation', time: 'now', active: true },
                         ...conversations.map(c => ({ ...c, active: false }))];
        messagesById = { ...messagesById, [fromUrl]: [greetingMsg()] };
      } else {
        conversations = conversations.map(c => ({ ...c, active: c.id === fromUrl }));
      }
    }
  }

  /// No session in the URL — show the centred composer, create nothing yet.
  const landing = $derived(!sessionId);

  function llmLabel(llm: string) { return llm ? llm.split('/').pop() : ''; }
  /// Server-side session key — agent-scoped, matching the webhook path so both
  /// front-ends observe the same transcript.
  ///
  /// A conversation restored from the server carries the exact key its transcript
  /// is stored under, so prefer that: recomputing it from `selectedAgent` would
  /// silently address a *different* transcript once the agent list finishes
  /// loading (or fails to), splitting one conversation across two server rows.
  const serverSession = () => {
    const conv = conversations.find(c => c.id === sessionId);
    return conv?.serverId ?? `${selectedAgent}:${sessionId}`;
  };

  const chat = new Chat({
    messages: sessionId ? (messagesById[sessionId] ?? [greetingMsg()]) : [],
    transport: new DefaultChatTransport({
      api: `${API_BASE}/chat`,
      headers: (): Record<string, string> => {
        const t = getAccessToken();
        return t ? { Authorization: `Bearer ${t}` } : {};
      },
      prepareSendMessagesRequest: ({ messages }) => {
        const last = messages[messages.length - 1];
        const text = (last?.parts ?? [])
          .filter((p: any) => p.type === 'text').map((p: any) => p.text).join('');
        const a = agents.find(x => x.id === selectedAgent);
        const ui = openArtifact
          ? {
              kind: openArtifact.kind,
              name: `${openArtifact.namespace}/${openArtifact.name}`,
              section: openSection,
              // Unsaved edits: the registry copy the agent would read with
              // get_flow is no longer what the user sees, so send the buffer.
              // Capped so a large flow cannot crowd out the transcript; the
              // server states plainly when it was truncated.
              ...(openEdit?.dirty
                ? {
                    dirty: true,
                    draft_text: openEdit.text.slice(0, UI_DRAFT_MAX),
                    draft_truncated: openEdit.text.length > UI_DRAFT_MAX,
                  }
                : {}),
            }
          : undefined;
        return {
          body: {
            message: text,
            session_id: serverSession(),
            ...(ui ? { ui_context: ui } : {}),
            ...(a?.llm ? { llm_resource: a.llm } : {}),
            // Send the tool restriction whenever the agent HAS one, including an
            // empty one. Gating this on `.length` meant that removing an agent's
            // last tool dropped the field entirely, so the server saw no
            // restriction at all and the agent came back UNRESTRICTED — the exact
            // opposite of what removing a tool should do. Absent and empty are
            // different: absent is "never configured", empty is "configured down
            // to nothing".
            ...(Array.isArray(a?.enabled_tools) ? { enabled_tools: a.enabled_tools } : {}),
          },
        };
      },
    }),
    onError: (e) => console.error('[chat]', e),
  });

  const busy = $derived(chat.status === 'submitted' || chat.status === 'streaming');

  // A finished turn is when the server first has a transcript (and a title) for a
  // brand-new conversation, so re-read the index then rather than polling.
  let lastBusy = false;
  $effect(() => {
    const nowBusy = busy;
    const was = lastBusy;
    lastBusy = nowBusy;
    // untrack: syncServerSessions writes `conversations`, which this effect must
    // not depend on — that self-retrigger is what froze reactivity before.
    if (was && !nowBusy) untrack(() => {
      syncServerSessions();
      // The turn just wrote the session row (and its title) — the shell's list
      // has to re-read or a brand-new conversation never shows up in it.
      bumpContext();
    });
  });

  // Persist the live conversation under its session id, so switching chats and
  // navigating away both restore.
  $effect(() => {
    // Track only the live messages and the active session. The write below must
    // be untracked: reading `messagesById` to spread it would make this effect
    // depend on what it writes, which self-retriggers and (in Svelte 5) trips
    // the update-depth guard — that froze all reactivity, so selecting a
    // conversation appeared to do nothing.
    const msgs = chat.messages as UIMessage[];
    const sid = sessionId;
    if (!sid) return;
    untrack(() => {
      messagesById = { ...messagesById, [sid]: msgs };
    });
  });

  /// Write the browser-side chat state immediately.
  ///
  /// The effect below normally handles this, but it runs AFTER the current tick —
  /// and anything that navigates (picking an agent, starting a conversation)
  /// remounts this page, because PanelStack keys on the selection. The remount
  /// then reads localStorage back before the effect ever ran, so the change was
  /// silently discarded: picking an agent always reverted to the default.
  function persistNow() {
    try {
      localStorage.setItem(LS_CONV, JSON.stringify(conversations));
      localStorage.setItem(LS_MSGS, JSON.stringify(messagesById));
      localStorage.setItem(LS_AGENT, JSON.stringify(selectedAgent));
    } catch { /* private mode — the shell still works, it just forgets */ }
  }

  $effect(() => {
    // Touch each one so the effect tracks them.
    conversations; messagesById; selectedAgent;
    persistNow();
  });

  $effect(() => {
    chat.messages.length; chat.status;
    if (messagesEl) requestAnimationFrame(() => { messagesEl!.scrollTop = messagesEl!.scrollHeight; });
  });

  async function loadAgents() {
    const res = await execFlow('a3t-agents-list', 'a3t', { org_id: 'org-default' }, { silent: true });
    if (!res.ok) return;
    const rows = Array.isArray(res.data) ? res.data : (res.data?.rows ?? res.data?.output ?? []);
    agents = (rows as any[]).map(r => ({
      id: r.id,
      name: r.name ?? r.id,
      llm: r.llm ?? r.resource_bindings?.llm ?? '',
      target: r.target ?? '',
      ep_path: r.ep_path ?? '',
      system_default: !!r.system_default,
      // Left UNDEFINED when the agent has no tool restriction, rather than
      // coerced to []. The two mean different things now: undefined is "never
      // configured" (not narrowed), [] is "configured down to nothing" (every
      // tool asks). Defaulting to [] here would silently restrict every agent
      // that had simply never been given a tool list.
      enabled_tools: Array.isArray(r.enabled_tools) ? r.enabled_tools : undefined,
    }));
    if (!selectedAgent && agents.length > 0) {
      selectedAgent = (agents.find(a => a.system_default) ?? agents[0]).id;
    }
  }

  async function loadTriggers() { /* endpoints resolved server-side for /chat */ }

  /// Split a server session id ("<agent>:<clientId>") back into its parts.
  ///
  /// The agent prefix is often EMPTY (":session-…"): the chat resolves its agent
  /// list from a flow, and when that lookup yields nothing `selectedAgent` stays
  /// "". Returning null for those dropped every real conversation from the
  /// sidebar, which looked exactly like "sessions are not syncing". Never drop an
  /// entry — an unknown agent is recoverable, a hidden conversation is not.
  function splitServerId(sid: string): { agent: string; id: string } {
    const i = sid.indexOf(':');
    if (i < 0) return { agent: '', id: sid };                 // no prefix at all
    const id = sid.slice(i + 1);
    if (!id) return { agent: '', id: sid };                   // trailing colon only
    return { agent: sid.slice(0, i), id };                    // agent may be ""
  }

  function relTime(iso: string): string {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return '';
    const mins = Math.floor((Date.now() - t) / 60000);
    if (mins < 1) return 'now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    const days = Math.floor(hrs / 24);
    return days < 30 ? `${days}d ago` : new Date(t).toLocaleDateString();
  }

  /// Merge the signed-in user's server-side conversations into the sidebar.
  ///
  /// This is what makes history follow the user: on a device that has never seen
  /// these chats, `conversations` starts empty and is filled from the server index
  /// rather than from localStorage. Local-only entries (a brand-new chat with no
  /// turn yet, so no server row) are kept, and the server's title wins for
  /// anything it knows about.
  async function syncServerSessions() {
    const remote = await fetchChatSessions();
    if (remote.length === 0) return;
    const byId = new Map<string, Conv>();
    for (const c of conversations) byId.set(c.id, c);
    for (const r of remote) {
      const parts = splitServerId(r.session_id);
      const existing = byId.get(parts.id);
      byId.set(parts.id, {
        id: parts.id,
        title: r.title || existing?.title || 'Conversation',
        time: relTime(r.updated_at),
        active: existing?.active ?? false,
        agent: parts.agent,
        serverId: r.session_id,
      });
    }
    // Server order (newest first) for known sessions; local-only ones lead, since
    // an unsent new chat is the one the user is looking at.
    const order = new Map(remote.map((r, i) => [splitServerId(r.session_id).id, i]));
    conversations = [...byId.values()].sort((a, b) => {
      const ai = order.has(a.id) ? order.get(a.id)! : -1;
      const bi = order.has(b.id) ? order.get(b.id)! : -1;
      return ai - bi;
    });
    if (!conversations.some(c => c.active) && conversations.length > 0) {
      conversations = conversations.map(c => ({ ...c, active: c.id === sessionId }));
    }
  }

  /// Load a conversation's transcript from the server into the view. Used when
  /// selecting a chat this browser has no cached messages for — the cross-device
  /// case.
  async function loadServerTranscript(conv: Conv) {
    const sid = conv.serverId ?? `${conv.agent ?? selectedAgent}:${conv.id}`;
    const msgs = await fetchChatSession(sid);
    if (msgs.length === 0) return;
    const restored = msgs.map((m, i) => ({
      id: `hist-${conv.id}-${i}`, role: m.role, parts: [{ type: 'text', text: m.content }],
    })) as any;
    // Only adopt it if the user is still on this conversation — an await means
    // they may have clicked away.
    if (sessionId !== conv.id) return;
    chat.messages = restored;
    messagesById = { ...messagesById, [conv.id]: restored };
  }

  /// Switch this page to another conversation.
  ///
  /// The list lives in the shell now, and selecting there swaps the panel's
  /// `selectedId` — but the panel stays the same *type*, so this component is
  /// never remounted and would otherwise keep showing the previous transcript.
  // Switching conversations is a REMOUNT, not an in-place swap: PanelStack keys
  // the page on `props.selectedId`, so selecting in the context list mounts this
  // component fresh and the code below loads that session from the URL. Doing it
  // in place needed the page to re-resolve keys, agents and caches by hand, which
  // is where the "wrong transcript"/"greeting instead of messages" bugs came from.

  /// Reconcile the server transcript into this conversation — a reply that
  /// arrived while the page was unmounted is otherwise lost from the client.
  async function rehydrate() {
    if (!selectedAgent) return;
    const server = await fetchAgentHistory(serverSession());
    if (server.length === 0) return;
    const have = new Set(
      (chat.messages as any[]).map(m =>
        `${m.role}:${(m.parts ?? []).filter((p: any) => p.type === 'text').map((p: any) => p.text).join('')}`),
    );
    const missing = server
      .filter(m => !have.has(`${m.role}:${m.content}`))
      .map((m, i) => ({ id: `hist-${Date.now()}-${i}`, role: m.role, parts: [{ type: 'text', text: m.content }] }));
    if (missing.length > 0) chat.messages = [...chat.messages, ...missing] as any;
  }

  onMount(() => {
    // Landing has no session, so nothing to pin to the URL.
    if (sessionId) setSessionUrl(sessionId, true);
    const handover = takePendingMessage();
    if (handover && sessionId) chat.sendMessage({ text: handover });
    (async () => {
      await loadAgents();
      // The server index before the transcript: on a fresh device the sidebar is
      // empty until this lands, and the active conversation may be one the server
      // knows about but this browser has never cached.
      await syncServerSessions();
      const active = conversations.find(c => c.id === sessionId);
      if (active && !(messagesById[sessionId]?.some(m => m.role === 'user'))) {
        await loadServerTranscript(active);
      }
      await rehydrate();
    })();

    const onPop = () => {
      const id = sessionFromUrl();
      if (id && id !== sessionId) {
        if (!conversations.some(c => c.id === id)) {
          conversations = [{ id, title: 'Conversation', time: 'now', active: true },
                           ...conversations.map(c => ({ ...c, active: false }))];
        } else {
          conversations = conversations.map(c => ({ ...c, active: c.id === id }));
        }
        sessionId = id;
        chat.messages = (messagesById[id] ?? []) as any;
        rehydrate();
      }
    };
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  });

  function startConversation(greeting: string) {
    const id = newSessionId();
    conversations = [{ id, title: 'New conversation', time: 'now', active: true,
                       agent: selectedAgent, serverId: `${selectedAgent}:${id}` },
                     ...conversations.map(c => ({ ...c, active: false }))];
    const seed = [{ id: `greet-${id}`, role: 'assistant', parts: [{ type: 'text', text: greeting }] }] as any;
    messagesById = { ...messagesById, [id]: seed };
    sessionId = id;
    chat.messages = seed;
    // Before navigating: setSessionUrl remounts this component.
    persistNow();
    setSessionUrl(id);
  }

  function onAgentChange() {
    const a = agents.find(a => a.id === selectedAgent);
    const via = a?.llm ? ` (on ${llmLabel(a.llm)})` : '';
    startConversation(`Now chatting with ${a?.name ?? 'Agent'}${via}. How can I help?`);
  }

  // selectConversation / deleteConversation now live in the shell context pane:
  // it navigates to /chat/<session>, which remounts this page onto that session.
  // ── Attachments ──
  //
  // The chat transport carries a single text field (`Message.content` is a string
  // server-side), so there is no binary attachment path yet. Text files are still
  // genuinely useful, so they are inlined into the outgoing message as fenced
  // blocks; anything that does not decode as text is refused rather than silently
  // sent as mojibake.
  const ATTACH_MAX_BYTES = 128 * 1024;
  const ATTACH_MAX_FILES = 5;

  let attachments = $state<{ name: string; text: string; size: number }[]>([]);
  let attachError = $state('');
  /// The permissions popover. `permRev` forces a reload each time it opens, so
  /// it never shows a stale answer from earlier in the conversation.
  let permOpen = $state(false);
  let permRev = $state(0);

  $effect(() => {
    if (!permOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!(e.target as HTMLElement)?.closest?.('.perm-pick')) permOpen = false;
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') permOpen = false; };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  });
  let fileInput = $state<HTMLInputElement>();

  /// Heuristic: a NUL byte or a run of replacement chars means it was not text.
  function looksBinary(t: string): boolean {
    if (t.includes('\u0000')) return true;
    const bad = (t.match(/\uFFFD/g) ?? []).length;
    return bad > 0 && bad / Math.max(1, t.length) > 0.01;
  }

  async function onFilesPicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const picked = Array.from(input.files ?? []);
    input.value = '';   // so picking the same file twice still fires
    attachError = '';
    for (const f of picked) {
      if (attachments.length >= ATTACH_MAX_FILES) {
        attachError = `At most ${ATTACH_MAX_FILES} files per message.`;
        break;
      }
      if (f.size > ATTACH_MAX_BYTES) {
        attachError = `${f.name} is larger than ${Math.round(ATTACH_MAX_BYTES / 1024)} KB.`;
        continue;
      }
      try {
        const text = await f.text();
        if (looksBinary(text)) {
          attachError = `${f.name} is not a text file — images and binaries are not supported yet.`;
          continue;
        }
        attachments = [...attachments, { name: f.name, text, size: f.size }];
      } catch {
        attachError = `Could not read ${f.name}.`;
      }
    }
  }

  function removeAttachment(name: string) {
    attachments = attachments.filter(a => a.name !== name);
    attachError = '';
  }

  /// Fenced per file so the model can tell the attachment from the prompt.
  function withAttachments(text: string): string {
    if (attachments.length === 0) return text;
    const blocks = attachments
      .map(a => `Attached file: ${a.name}\n\`\`\`\n${a.text}\n\`\`\``)
      .join('\n\n');
    return text ? `${blocks}\n\n${text}` : blocks;
  }

  // ── Agent picker ──
  let agentMenuOpen = $state(false);
  const activeAgent = $derived(agents.find(a => a.id === selectedAgent));
  const agentLabel = $derived(activeAgent?.name ?? (agents.length ? 'Select agent' : 'No agents'));

  function pickAgent(id: string) {
    agentMenuOpen = false;
    if (id === selectedAgent) return;
    selectedAgent = id;
    persistNow();          // survives the remount that onAgentChange triggers
    onAgentChange();
  }

  $effect(() => {
    if (!agentMenuOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!(e.target as HTMLElement)?.closest?.('.agent-pick')) agentMenuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') agentMenuOpen = false; };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  });

  function sendMessage() {
    const text = inputValue.trim();
    // An attachment on its own is a legitimate message.
    if ((!text && attachments.length === 0) || busy) return;
    inputValue = '';
    if (!sessionId) {
      // Landing state: mint the conversation now. Navigating remounts this page
      // (PanelStack keys on the selection), so hand the message over rather than
      // sending into a component that is about to be torn down.
      const id = newSessionId();
      setPendingMessage(withAttachments(text));
      attachments = []; attachError = '';
      trackOpen('chat', 'AI Chat');
      navigateToDetail('chat', 'AI Chat', id);
      return;
    }
    const active = conversations.find(c => c.active);
    if (active && active.title === 'New conversation') {
      active.title = text.slice(0, 40) + (text.length > 40 ? '...' : '');
      conversations = [...conversations];
    }
    chat.sendMessage({ text: withAttachments(text) });
    attachments = [];
    attachError = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  }

  const textOf = (m: any) => (m.parts ?? []).filter((p: any) => p.type === 'text').map((p: any) => p.text).join('');
  /// Typed data parts the server streams (`data-flow`, …) — rendered as widgets
  /// rather than prose. Unknown kinds are ignored so a new server-side widget
  /// type cannot break an older client.
  /// Permission questions asked during this message, each paired with its answer
  /// if one has arrived.
  ///
  /// Kept out of `widgetsOf` on purpose: that function collapses parts by
  /// `namespace/name`, which approvals do not have — every question would fold
  /// into a single card and only the last would survive. These are keyed by
  /// `request_id`, which is what actually distinguishes them.
  function approvalsOf(m: any) {
    const parts = m.parts ?? [];
    const answers = new Map<string, any>();
    for (const p of parts) {
      if (p.type === 'data-authz-resolved' && p.data?.request_id) {
        answers.set(p.data.request_id, p.data);
      }
    }
    const seen = new Set<string>();
    const out: any[] = [];
    for (const p of parts) {
      if (p.type !== 'data-authz-request' || !p.data?.request_id) continue;
      if (seen.has(p.data.request_id)) continue;
      seen.add(p.data.request_id);
      out.push({ request: p.data, resolved: answers.get(p.data.request_id) ?? null });
    }
    return out;
  }

  function widgetsOf(m: any) {
    const all = (m.parts ?? [])
      .filter((p: any) => typeof p.type === 'string' && p.type.startsWith('data-'))
      // Approvals have their own card and their own identity — see approvalsOf.
      .filter((p: any) => !p.type.startsWith('data-authz-'))
      .map((p: any) => ({ kind: p.type.slice(5), data: p.data }));
    // One turn can produce several parts for the same artifact — a first
    // proposal then a correction, or a list fetched twice. Render only the
    // newest per identity, or the user sees duplicate cards and cannot tell
    // which one is current.
    const latest = new Map<string, { kind: string; data: any }>();
    for (const w of all) {
      const d = w.data ?? {};
      latest.set(`${w.kind}:${d.namespace ?? ''}/${d.name ?? ''}`, w);
    }
    return [...latest.values()];
  }

  /// A widget applied a change: tell the agent in a short turn. The server owns
  /// the transcript, so this is all the awareness it needs — no push into a run
  /// that has already returned.
  function noteChange(summary: string) {
    chat.sendMessage({ text: summary });
  }

  function toolsOf(m: any) {
    return (m.parts ?? [])
      .filter((p: any) => typeof p.type === 'string' && p.type.startsWith('tool-'))
      .map((p: any) => ({
        name: p.toolName ?? p.type.replace(/^tool-/, ''),
        state: p.state ?? '',
        input: p.input,
        output: p.output,
      }));
  }
</script>

<div class="chat-layout" bind:this={chatLayoutEl}
     style="grid-template-columns: 1fr{openArtifact ? ` 6px ${paneW}px` : ''};">
  <!-- Left: conversation history -->
  <!-- Chat body. The conversation list is the shell's context pane now, so this
       owns only the agent selection and the transcript. -->
  <div class="chat-main" class:landing={landing}>
    {#if landing}
      <!-- Chat's title bar: the section pages lead with a PageBanner, this leads
           with its own line of text. No session yet — the composer sits in the
           middle and sending is what creates the conversation. -->
      <div class="landing-head">
        <h1 class="landing-title">What would you like to do?</h1>
        <p class="landing-sub">Ask about templates, agents, flows, resources — or start building one.</p>
      </div>
    {/if}
    <div class="chat-messages" class:hidden={landing} bind:this={messagesEl}>
      {#each chat.messages as msg (msg.id)}
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
            {#if textOf(msg)}
              <div class="msg-content md">{@html renderMarkdown(textOf(msg))}</div>
            {/if}
            {#each approvalsOf(msg) as a (a.request.request_id)}
              <ApprovalCard request={a.request} resolved={a.resolved} />
            {/each}
            {#each widgetsOf(msg) as w}
              {#if w.kind in ARTIFACTS}
                <ArtifactWidget kind={w.kind as ArtifactKind} data={w.data} onopen={(a) => (openArtifact = a)} />
              {:else if w.kind in LIST_KIND_TO_ITEM}
                <ListWidget kind={w.kind} data={w.data} onopen={(a) => (openArtifact = a)} />
              {/if}
            {/each}
            {#if toolsOf(msg).length > 0}
              <details class="exec-log">
                <summary>Execution log · {toolsOf(msg).length} tool call{toolsOf(msg).length === 1 ? '' : 's'}</summary>
                <div class="exec-log-body">{#each toolsOf(msg) as t}▸ {t.name}{t.state ? ` (${t.state})` : ''}
{t.input !== undefined ? `  input:  ${JSON.stringify(t.input)}` : ''}
{t.output !== undefined ? `  output: ${(typeof t.output === 'string' ? t.output : JSON.stringify(t.output)).slice(0, 800)}` : ''}
{/each}</div>
              </details>
            {/if}
          </div>
        </div>
      {/each}

      {#if chat.status === 'submitted'}
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
      {#if chat.error}
        <div class="message assistant">
          <div class="msg-avatar"><span class="agent-mark">A3</span></div>
          <div class="msg-body">
            <div class="msg-role">A3T</div>
            <div class="msg-content">Error: {chat.error.message}</div>
          </div>
        </div>
      {/if}
    </div>

    <div class="chat-input-area">
      <div class="chat-input-wrap">
        {#if attachments.length > 0 || attachError}
          <div class="attach-row">
            {#each attachments as a}
              <span class="attach-chip" title={`${a.name} · ${Math.max(1, Math.round(a.size / 1024))} KB`}>
                <span class="attach-name">{a.name}</span>
                <button class="attach-x" onclick={() => removeAttachment(a.name)} aria-label={`Remove ${a.name}`}>
                  <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 4L4 12M4 4l8 8" /></svg>
                </button>
              </span>
            {/each}
            {#if attachError}<span class="attach-err">{attachError}</span>{/if}
          </div>
        {/if}
        <textarea
          class="chat-input"
          placeholder="Ask anything about the platform..."
          bind:value={inputValue}
          onkeydown={handleKeydown}
          rows="1"
        ></textarea>

        <!-- Controls live under the textarea, inside the same box: which agent
             answers is a setting you glance at, not a banner above the chat. -->
        <div class="chat-input-bar">
          <!-- Attach: text files are inlined into the message (see withAttachments). -->
          <input
            class="sr-only" type="file" multiple bind:this={fileInput}
            onchange={onFilesPicked}
            accept=".txt,.md,.yaml,.yml,.json,.csv,.log,.ts,.js,.rs,.py,.sh,.toml,.xml,.html,.css,text/*"
          />
          <button class="bar-icon" onclick={() => fileInput?.click()} title="Attach a text file" aria-label="Attach a text file">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10" /></svg>
          </button>

          <!-- Permissions: what this conversation has allowed or declined, and the
               only place a refusal can be reopened. Hidden until a session exists,
               since there is nothing to show before the first turn. -->
          {#if sessionId}
            <div class="perm-pick">
              <button
                class="bar-icon"
                class:on={permOpen}
                onclick={() => { permOpen = !permOpen; if (permOpen) permRev += 1; }}
                title="Permissions in this conversation"
                aria-haspopup="dialog"
                aria-expanded={permOpen}
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M8 1l5.5 2.2v4c0 3.5-2.3 6.4-5.5 7.6C4.8 13.6 2.5 10.7 2.5 7.2v-4L8 1z" />
                </svg>
              </button>
              {#if permOpen}
                <div class="perm-pop">
                  <PermissionsPanel sessionId={sessionId} rev={permRev} />
                </div>
              {/if}
            </div>
          {/if}

          <!-- Agent: a pill sized to its own label, opening an in-place picker. -->
          <div class="agent-pick">
            <button
              class="agent-chip"
              class:open={agentMenuOpen}
              onclick={() => (agentMenuOpen = !agentMenuOpen)}
              title={activeAgent?.llm ? `${agentLabel} · ${llmLabel(activeAgent.llm)}` : agentLabel}
              aria-haspopup="menu"
              aria-expanded={agentMenuOpen}
              disabled={agents.length === 0}
            >
              <span class="agent-chip-name">{agentLabel}</span>
              <svg width="9" height="9" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6l4 4 4-4" /></svg>
            </button>

            {#if agentMenuOpen}
              <div class="agent-menu" role="menu">
                {#each agents as a}
                  <button class="agent-opt" class:on={a.id === selectedAgent} onclick={() => pickAgent(a.id)} role="menuitem">
                    <span class="agent-opt-tick">{a.id === selectedAgent ? '✓' : ''}</span>
                    <span class="agent-opt-text">
                      <span class="agent-opt-name">{a.name}{a.system_default ? ' · default' : ''}</span>
                      {#if a.llm}<span class="agent-opt-llm">{llmLabel(a.llm)}</span>{/if}
                    </span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>

          <span class="bar-spacer"></span>
          <span class="chat-input-hint">Enter to send · Shift+Enter for new line</span>

          {#if busy}
            <button class="send-btn" onclick={() => chat.stop()} title="Stop generating" aria-label="Stop generating">
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="4" y="4" width="8" height="8" rx="1"/></svg>
            </button>
          {:else}
            <button class="send-btn" onclick={sendMessage} disabled={!inputValue.trim()} aria-label="Send">
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14 2L7 9M14 2l-5 12-2-5-5-2z"/></svg>
            </button>
          {/if}
        </div>
      </div>
    </div>
    {#if landing}
      <!-- Same Pinned / Latest lists as every other app home, below the composer. -->
      <div class="landing-lists">
        <AppHome appType="chat" embedded />
      </div>
    {/if}
  </div>

  {#if openArtifact}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="pane-resizer"
      class:dragging={paneDragging}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the editor pane"
      onpointerdown={startPaneDrag}
      onpointermove={onPaneDrag}
      onpointerup={endPaneDrag}
      onpointercancel={endPaneDrag}
      ondblclick={() => setPaneW(560)}
    ></div>
    <ArtifactPane
      artifact={openArtifact}
      revision={openRevision}
      onclose={() => { openArtifact = null; openEdit = null; }}
      onapplied={noteChange}
      onsection={(sec) => (openSection = sec)}
      onedit={(st) => (openEdit = st)}
      onask={(prefix) => { inputValue = prefix; }}
    />
  {/if}
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
  .pane-resizer {
    cursor: col-resize;
    background: var(--border);
    opacity: 0.35;
    transition: opacity 0.12s;
    touch-action: none;
  }
  .pane-resizer:hover, .pane-resizer.dragging { opacity: 1; background: var(--border-strong); }

  .chat-layout {
    display: grid;
    /* columns are set inline: sidebar | resizer | chat | optional artifact pane */
    height: 100%;
    /* The row must be capped at the container. Without this it is an implicit
       `auto` row: tall content in the artifact pane grew the row past the
       viewport, so the pane never scrolled AND the transcript lost its own
       scrollbar, because .chat-main's height comes from this row. */
    grid-template-rows: minmax(0, 1fr);
    overflow: hidden;
  }
  /* Grid items default to min-height/min-width auto and refuse to shrink below
     their content — the other half of the same failure. */
  .chat-layout > * { min-width: 0; min-height: 0; }

  /* Sidebar resizer */

  /* Icon-rail mode */

  /* Sidebar */



  /* Main chat */

  .chat-main {
    display: flex;
    flex-direction: column;
    background: var(--bg-page);
    min-height: 0;      /* allow the flex column to shrink so children can scroll */
    overflow: hidden;
  }

  /* Landing: centre the composer instead of pinning it to the bottom. */
  .chat-main.landing { justify-content: center; }
  .chat-messages.hidden { display: none; }
  .chat-main.landing .chat-input-area { border-top: none; background: transparent; }
  .landing-head { flex: none; padding: 0 32px 18px; text-align: center; }
  .landing-title {
    font-family: var(--font-serif); font-size: 26px; font-weight: 500;
    color: var(--text-1); margin: 0 0 6px;
  }
  .landing-sub { font-size: 12.5px; color: var(--text-4); margin: 0; }

  .landing-lists { max-width: 860px; margin: 0 auto; width: 100%; padding: 0 32px 24px; box-sizing: border-box; }


  .chat-messages {
    flex: 1;
    min-height: 0;      /* required for overflow scrolling inside a flex column */
    overflow-y: auto;
    padding: 24px 28px 16px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .message {
    display: flex;
    gap: 12px;
    /* Fill the column. Assistant turns carry tables, YAML and widgets, so a
       fixed 720px wasted the width gained by collapsing the chats rail or
       closing the artifact pane — the column itself is the constraint now. */
    max-width: 100%;
  }
  /* Own messages stay compact and right-aligned; they are short prose and read
     badly stretched across a wide column. */
  .message.user {
    margin-left: auto;
    flex-direction: row-reverse;
    max-width: min(720px, 100%);
  }

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
    /* Capped so the box does not stretch the full width of a wide monitor; the
       transcript above it still uses the full width. */
    --composer-max: 860px;
    padding: 16px 32px 20px;
    border-top: 1px solid var(--border);
    background: var(--bg-page);
    flex-shrink: 0;     /* keep the input anchored at the bottom */
  }
  .perm-pick { position: relative; display: inline-flex; }
  .perm-pop {
    position: absolute; bottom: calc(100% + 8px); left: 0;
    width: 330px; max-width: min(330px, calc(100vw - 32px));
    max-height: 60vh; overflow-y: auto;
    padding: 10px 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.16);
    z-index: 200;
  }

  .chat-input-wrap {
    max-width: var(--composer-max);
    margin: 0 auto;
    width: 100%;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;   /* textarea above, controls beneath, one box */
    gap: 6px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 10px 10px 8px 14px;
    transition: border-color 0.15s;
  }
  .chat-input-wrap:focus-within {
    border-color: var(--ephemeral);
    box-shadow: 0 0 0 2px var(--ephemeral-bg);
  }
  .chat-input {
    width: 100%;
    box-sizing: border-box;
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

  .chat-input-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .bar-spacer { flex: 1; }

  .bar-icon {
    width: 24px; height: 24px; flex: 0 0 auto;
    display: grid; place-items: center;
    border: 1px solid transparent; border-radius: 6px;
    background: transparent; color: var(--text-4); cursor: pointer;
  }
  .bar-icon:hover { color: var(--text-1); border-color: var(--border); }

  /* Sized to its own label — the previous native <select> stretched to a
     percentage width and read as a field rather than a chip. */
  .agent-pick { position: relative; flex: 0 1 auto; min-width: 0; }
  .agent-chip {
    max-width: 220px;
    display: inline-flex; align-items: center; gap: 5px;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-subtle);
    color: var(--text-3);
    font-family: var(--font-sans);
    font-size: 11px; line-height: 1.6;
    cursor: pointer;
  }
  .agent-chip:hover:not(:disabled), .agent-chip.open { color: var(--text-1); border-color: var(--border-strong); }
  .agent-chip:disabled { opacity: 0.5; cursor: default; }
  .agent-chip-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* Opens upward: the composer sits at the bottom of the view. */
  .agent-menu {
    position: absolute; bottom: calc(100% + 6px); left: 0;
    min-width: 220px; max-height: 46vh; overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.16);
    padding: 4px; z-index: 120;
  }
  .agent-opt {
    width: 100%;
    display: flex; align-items: center; gap: 7px;
    padding: 6px 8px;
    border: none; border-radius: var(--r-sm);
    background: transparent; cursor: pointer; text-align: left;
  }
  .agent-opt:hover { background: var(--bg-subtle); }
  .agent-opt.on { background: var(--bg-subtle); }
  .agent-opt-tick { width: 10px; flex: 0 0 auto; font-size: 10px; color: var(--text-2); }
  .agent-opt-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .agent-opt-name {
    font-size: 12px; color: var(--text-2);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .agent-opt.on .agent-opt-name { color: var(--text-1); font-weight: 500; }
  .agent-opt-llm { font-size: 10px; color: var(--text-4); }

  .attach-row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .attach-chip {
    display: inline-flex; align-items: center; gap: 5px;
    max-width: 220px;
    padding: 2px 4px 2px 8px;
    border: 1px solid var(--border); border-radius: 999px;
    background: var(--bg-subtle); color: var(--text-2); font-size: 11px;
  }
  .attach-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .attach-x {
    width: 16px; height: 16px; flex: 0 0 auto;
    display: grid; place-items: center;
    border: none; border-radius: 50%;
    background: transparent; color: var(--text-4); cursor: pointer;
  }
  .attach-x:hover { color: var(--text-1); background: var(--bg-surface); }
  .attach-err { font-size: 10.5px; color: #dc2626; }

  .sr-only {
    position: absolute; width: 1px; height: 1px;
    padding: 0; margin: -1px; overflow: hidden;
    clip: rect(0 0 0 0); white-space: nowrap; border: 0;
  }

  .send-btn {
    width: 28px; height: 28px;
    border-radius: 7px;
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
    font-size: 10px;
    color: var(--text-5);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* The hint is the first thing to go when the composer is narrow — the agent
     chip and send button must never be pushed out. */
  @media (max-width: 700px) {
    .chat-input-hint { display: none; }
  }
</style>
