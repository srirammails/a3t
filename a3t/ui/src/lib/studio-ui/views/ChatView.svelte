<script lang="ts">
  /// An LLM's Chat view: talk to the model behind the resource. The
  /// conversation lives on the document (not saved); Clear starts over.
  import EditButton from '../parts/EditButton.svelte';
  import type { ChatMessage, Doc, Workspace } from '$lib/studio-core';

  let { doc, workspace }: { doc: Doc; workspace: Workspace } = $props();

  const kind = $derived(workspace.kindOf(doc));
  const messages = $derived((doc.ui.messages ?? []) as ChatMessage[]);
  let list = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!doc.loaded) void workspace.load(doc);
  });
  // Keep the newest message in view.
  $effect(() => {
    messages.length;
    queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));
  });

  async function send() {
    const text = (doc.ui.draft ?? '').trim();
    if (!text || doc.ui.sending || !kind.chat) return;
    const next: ChatMessage[] = [...messages, { role: 'user', content: text }];
    doc.ui.messages = next;
    doc.ui.draft = '';
    doc.ui.sending = true;
    doc.ui.error = '';
    const r = await kind.chat(doc, next);
    doc.ui.sending = false;
    if (r.ok) doc.ui.messages = [...next, { role: 'assistant', content: r.data }];
    else doc.ui.error = r.error;
  }
  const model = $derived(doc.data?.config?.model ?? '');
</script>

<div class="chv">
  <div class="head">
    <span class="kind">{kind.label}{doc.type ? ` · ${doc.type}` : ''}</span>
    <span class="mono">{doc.ref.project}/{doc.ref.name}</span>
    {#if model}<span class="model" title="Model">{model}</span>{/if}
    <span class="grow"></span>
    <EditButton {doc} {workspace} />
    <button class="btn ghost sm" disabled={!messages.length || doc.ui.sending} onclick={() => { doc.ui.messages = []; doc.ui.error = ''; }}>Clear</button>
  </div>

  <div class="list" bind:this={list}>
    {#if !messages.length}
      <div class="empty">Say something to {doc.ref.name}. Answers come from the model this resource points at, through the platform.</div>
    {/if}
    {#each messages as m, i (i)}
      <div class="msg {m.role}"><span class="who">{m.role === 'user' ? 'You' : doc.ref.name}</span><div class="text">{m.content}</div></div>
    {/each}
    {#if doc.ui.sending}<div class="msg assistant"><span class="who">{doc.ref.name}</span><div class="text dim">Thinking…</div></div>{/if}
    {#if doc.ui.error}<div class="err">{doc.ui.error}</div>{/if}
  </div>

  <div class="compose">
    <textarea rows="2" placeholder="Message {doc.ref.name} — Enter sends, Shift+Enter for a new line" bind:value={doc.ui.draft}
      onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void send(); } }}></textarea>
    <button class="btn spec sm" disabled={doc.ui.sending || !(doc.ui.draft ?? '').trim()} onclick={send}>Send</button>
  </div>
</div>

<style>
  .chv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-3); }
  .mono { font-family: var(--font-mono); font-size: 12.5px; }
  .model { font-family: var(--font-mono); font-size: 11px; color: var(--spec); background: var(--spec-bg); padding: 1px 6px; border-radius: 3px; }
  .grow { flex: 1; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 12px; }
  .empty { color: var(--text-4); font-size: 13px; max-width: 520px; }
  .msg { max-width: 760px; display: flex; flex-direction: column; gap: 3px; }
  .msg.user { align-self: flex-end; align-items: flex-end; }
  .who { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-4); }
  .text { padding: 8px 12px; border-radius: 10px; font-size: 13.5px; line-height: 1.55; white-space: pre-wrap; word-break: break-word; background: var(--bg-surface); border: 1px solid var(--border); }
  .msg.user .text { background: var(--spec-bg); border-color: var(--spec-bg); }
  .text.dim { color: var(--text-4); }
  .err { padding: 8px 10px; border-radius: var(--r-sm); background: var(--err-bg, #fde8e8); color: var(--err, #b42318); font-size: 12px; white-space: pre-wrap; }
  .compose { display: flex; gap: 8px; align-items: flex-end; padding: 10px 12px; border-top: 1px solid var(--border); background: var(--bg-surface); }
  .compose textarea { flex: 1; resize: vertical; min-height: 40px; max-height: 200px; padding: 8px 10px; font-size: 13px; font-family: inherit; }
</style>
