// Picking an agent must survive the remount that starting a conversation causes.
//
// PanelStack keys the page on the selection, so navigating tears the chat down and
// builds it again; the new instance reads `selectedAgent` back from localStorage.
// If the write only happens in a reactive effect (which runs after the tick), the
// remount reads the OLD value and loadAgents() falls back to the system default —
// the "always goes back to Qwen" bug.

const LS = { store: {}, getItem(k){ return k in this.store ? this.store[k] : null; },
             setItem(k,v){ this.store[k]=String(v); } };

function mountChat({ agents, pendingEffect }) {
  // A mount reads persisted state, then loadAgents() fills in a default if empty.
  let selectedAgent = JSON.parse(LS.getItem('a3t.chat.agent') ?? '""');
  if (!selectedAgent && agents.length) {
    selectedAgent = (agents.find(a => a.system_default) ?? agents[0]).id;
  }
  const inst = {
    get agent(){ return selectedAgent; },
    pick(id, { persistNow }) {
      selectedAgent = id;
      if (persistNow) LS.setItem('a3t.chat.agent', JSON.stringify(id));
      // startConversation -> setSessionUrl -> panel selection changes -> REMOUNT.
      // The persistence effect is still queued and never runs.
      pendingEffect.queued = true;
      return mountChat({ agents, pendingEffect });
    },
  };
  return inst;
}

const AGENTS = [
  { id: 'agt-default', name: 'System Default (Qwen)', system_default: true },
  { id: 'agt-c78ff75a', name: 'Claude Assistant' },
];

let pass = 0, fail = 0;
const check = (n, c, extra='') => c ? (pass++, console.log(`  PASS  ${n}`))
                                    : (fail++, console.log(`  FAIL  ${n}  ${extra}`));

// 1. The bug: without a synchronous write the pick is lost on remount.
LS.store = {};
{
  const a = mountChat({ agents: AGENTS, pendingEffect: {} });
  const after = a.pick('agt-c78ff75a', { persistNow: false });
  check('reproduces the bug (effect-only write reverts to the default)',
        after.agent === 'agt-default', `got ${after.agent}`);
}
// 2. The fix.
LS.store = {};
{
  const a = mountChat({ agents: AGENTS, pendingEffect: {} });
  const after = a.pick('agt-c78ff75a', { persistNow: true });
  check('picked agent survives the remount', after.agent === 'agt-c78ff75a', `got ${after.agent}`);
  const again = mountChat({ agents: AGENTS, pendingEffect: {} });
  check('  and a later reload too', again.agent === 'agt-c78ff75a', `got ${again.agent}`);
}
// 3. First ever visit still defaults sensibly.
LS.store = {};
{
  const a = mountChat({ agents: AGENTS, pendingEffect: {} });
  check('first visit uses the system default', a.agent === 'agt-default', `got ${a.agent}`);
}
// 4. No agents at all — no crash, no phantom selection.
LS.store = {};
{
  const a = mountChat({ agents: [], pendingEffect: {} });
  check('no agents leaves the selection empty', a.agent === '', `got "${a.agent}"`);
}
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
