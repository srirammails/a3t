// Covers the bare-/chat landing state and the first-message handover.
//
// Mirrors ChatPageAI's init block and sendMessage branch. The risk being guarded
// is that `/chat` silently mints an empty conversation again, or that the typed
// first message is lost across the remount that navigation causes.

function initSession({ fromUrl, conversations = [], messagesById = {} }) {
  let sessionId = '';
  if (fromUrl) {
    sessionId = fromUrl;
    if (!conversations.some(c => c.id === fromUrl)) {
      conversations = [{ id: fromUrl, title: 'New conversation', time: 'now', active: true },
                       ...conversations.map(c => ({ ...c, active: false }))];
      messagesById = { ...messagesById, [fromUrl]: [{ role: 'assistant', text: 'GREETING' }] };
    } else {
      conversations = conversations.map(c => ({ ...c, active: c.id === fromUrl }));
    }
  }
  return { sessionId, conversations, messagesById, landing: !sessionId };
}

// The handover, exactly as the store implements it.
let pending = null;
const setPendingMessage = t => { pending = t; };
const takePendingMessage = () => { const t = pending; pending = null; return t; };

let pass = 0, fail = 0;
const check = (n, c, extra = '') => c ? (pass++, console.log(`  PASS  ${n}`))
                                      : (fail++, console.log(`  FAIL  ${n}  ${extra}`));

// 1. Bare /chat creates nothing.
{
  const st = initSession({ fromUrl: null });
  check('bare /chat is the landing state', st.landing === true);
  check('  no session id', st.sessionId === '');
  check('  no conversation invented', st.conversations.length === 0, JSON.stringify(st.conversations));
  check('  no messages seeded', Object.keys(st.messagesById).length === 0);
}
// 2. Bare /chat with existing history still lands (does not silently reopen one).
{
  const st = initSession({ fromUrl: null, conversations: [{ id: 'old', title: 'Old', active: true }] });
  check('existing history does not auto-open', st.landing === true && st.sessionId === '');
}
// 3. A session URL opens that session.
{
  const st = initSession({ fromUrl: 'sX', conversations: [{ id: 'sX', title: 'X', active: false }] });
  check('/chat/<id> opens that session', st.sessionId === 'sX' && !st.landing);
  check('  marked active', st.conversations.find(c => c.id === 'sX').active === true);
}
// 4. A session URL this browser has never seen.
{
  const st = initSession({ fromUrl: 'sNew' });
  check('unknown /chat/<id> is adopted', st.sessionId === 'sNew' && !st.landing);
  check('  gets a placeholder conversation', st.conversations[0].id === 'sNew');
  check('  seeded with a greeting', !!st.messagesById.sNew);
}
// 5. The handover survives exactly one read.
{
  setPendingMessage('hello there');
  const first = takePendingMessage();
  const second = takePendingMessage();
  check('first message is handed over', first === 'hello there');
  check('  and cleared, so it cannot resend', second === null);
}
// 6. Nothing pending on a normal mount.
{
  check('no handover on a normal mount', takePendingMessage() === null);
}
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
