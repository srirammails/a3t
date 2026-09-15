// Mirrors switchToSession's logic with the network stubbed, to cover the case I
// just broke: a session that is on the SERVER but not in the local cache.
function splitServerId(sid) {
  const i = sid.indexOf(':');
  if (i < 0) return { agent: '', id: sid };
  const id = sid.slice(i + 1);
  if (!id) return { agent: '', id: sid };
  return { agent: sid.slice(0, i), id };
}

function makeCtx({ conversations = [], messagesById = {}, remote = [], transcripts = {}, selectedAgent = 'agt-default' }) {
  const ctx = {
    sessionId: 'old', conversations, messagesById, selectedAgent,
    messages: [{ role: 'assistant', parts: [{ type: 'text', text: 'OLD CONVO' }] }],
    calls: [],
    greeting: () => ({ role: 'assistant', parts: [{ type: 'text', text: 'GREETING' }] }),
    fetchChatSessions: async () => { ctx.calls.push('list'); return remote; },
    fetchChatSession: async (sid) => { ctx.calls.push(`get:${sid}`); return transcripts[sid] ?? []; },
  };
  return ctx;
}

async function switchToSession(ctx, id) {
  if (!id || id === ctx.sessionId) return;
  ctx.sessionId = id;
  const conv = ctx.conversations.find(c => c.id === id);
  if (conv?.agent && conv.agent !== ctx.selectedAgent) ctx.selectedAgent = conv.agent;

  const cached = ctx.messagesById[id] ?? [];
  ctx.messages = cached;
  if (cached.some(m => m.role === 'user')) return;

  let sid = conv?.serverId ?? null;
  if (!sid) {
    const remote = await ctx.fetchChatSessions();
    sid = remote.find(r => splitServerId(r.session_id).id === id)?.session_id ?? null;
    if (sid) {
      const parts = splitServerId(sid);
      const entry = { id, title: conv?.title ?? 'Conversation', time: '', active: true, agent: parts.agent, serverId: sid };
      ctx.conversations = ctx.conversations.some(c => c.id === id)
        ? ctx.conversations.map(c => (c.id === id ? { ...c, ...entry } : c))
        : [entry, ...ctx.conversations];
      if (parts.agent && parts.agent !== ctx.selectedAgent) ctx.selectedAgent = parts.agent;
    }
  }
  if (ctx.sessionId !== id) return;
  if (!sid) { ctx.messages = [ctx.greeting()]; return; }

  const msgs = await ctx.fetchChatSession(sid);
  if (ctx.sessionId !== id) return;
  const restored = msgs.map((m, i) => ({ id: `hist-${id}-${i}`, role: m.role, parts: [{ type: 'text', text: m.content }] }));
  ctx.messages = restored.length ? restored : [ctx.greeting()];
  ctx.messagesById = { ...ctx.messagesById, [id]: ctx.messages };
}

const text = c => c.messages.map(m => m.parts.map(p => p.text).join('')).join(' | ');
let pass = 0, fail = 0;
const check = (name, cond, extra = '') => {
  if (cond) { pass++; console.log(`  PASS  ${name}`); }
  else { fail++; console.log(`  FAIL  ${name}  ${extra}`); }
};

// 1. On the server but NOT in the local cache — the case I broke.
{
  const ctx = makeCtx({
    remote: [{ session_id: 'agt-c78ff75a:sX', title: 'T', updated_at: '', message_count: 2 }],
    transcripts: { 'agt-c78ff75a:sX': [{ role: 'user', content: 'hello' }, { role: 'assistant', content: 'hi' }] },
  });
  await switchToSession(ctx, 'sX');
  check('uncached session loads its transcript', text(ctx) === 'hello | hi', `got "${text(ctx)}"`);
  check('  resolves the real server key', ctx.calls.includes('get:agt-c78ff75a:sX'), ctx.calls.join(','));
  check('  adopts the conversation agent', ctx.selectedAgent === 'agt-c78ff75a', ctx.selectedAgent);
  check('  records serverId for replies', ctx.conversations.find(c => c.id === 'sX')?.serverId === 'agt-c78ff75a:sX');
}
// 2. Brand new, not on the server.
{
  const ctx = makeCtx({ remote: [] });
  await switchToSession(ctx, 'brandNew');
  check('new conversation shows the greeting', text(ctx) === 'GREETING', `got "${text(ctx)}"`);
  check('  old messages are gone', !text(ctx).includes('OLD CONVO'));
  check('  no pointless transcript fetch', !ctx.calls.some(c => c.startsWith('get:')), ctx.calls.join(','));
}
// 3. Already cached locally.
{
  const ctx = makeCtx({ messagesById: { sC: [{ role: 'user', parts: [{ type: 'text', text: 'cached' }] }] } });
  await switchToSession(ctx, 'sC');
  check('cached conversation renders from cache', text(ctx) === 'cached', `got "${text(ctx)}"`);
  check('  no network needed', ctx.calls.length === 0, ctx.calls.join(','));
}
// 4. In the local cache with a serverId.
{
  const ctx = makeCtx({
    conversations: [{ id: 'sD', serverId: 'agt-x:sD', agent: 'agt-x', title: 'D' }],
    transcripts: { 'agt-x:sD': [{ role: 'user', content: 'q' }] },
  });
  await switchToSession(ctx, 'sD');
  check('known session skips the index lookup', !ctx.calls.includes('list'), ctx.calls.join(','));
  check('  loads via the recorded key', text(ctx) === 'q', `got "${text(ctx)}"`);
}
// 5. Empty id / same id are no-ops.
{
  const ctx = makeCtx({}); const before = text(ctx);
  await switchToSession(ctx, ''); await switchToSession(ctx, 'old');
  check('empty and unchanged ids are no-ops', text(ctx) === before && ctx.calls.length === 0);
}
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
