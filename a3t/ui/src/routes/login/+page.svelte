<script lang="ts">
  import { goto } from '$app/navigation';
  import { login, isAuthenticated } from '$lib/stores/auth';
  import { mcpCall } from '$lib/services/api';
  import { onMount } from 'svelte';

  let email = $state('');
  let password = $state('');
  let error = $state('');
  let loading = $state(false);

  onMount(() => {
    // Already logged in? Redirect to app
    const unsub = isAuthenticated.subscribe(v => { if (v) goto('/'); });
    return unsub;
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    error = '';
    loading = true;
    const result = await login(email, password);
    loading = false;
    if (result.ok) {
      goto('/');
    } else {
      error = result.error || 'Login failed';
    }
  }

  // ── Request Early Access — persisted via execute_flow on the MCP endpoint
  // (same /message mechanism the console uses for templates/agents). ──
  let showForm = $state(false);
  let submitting = $state(false);
  let submitted = $state(false);
  let formError = $state('');
  let form = $state({ name: '', email: '', company: '', use_case: '' });

  function openForm() { showForm = true; submitted = false; formError = ''; }
  function closeForm() { showForm = false; }

  async function submitRequest(e: Event) {
    e.preventDefault();
    formError = '';
    submitting = true;
    try {
      const payload = {
        id: `ea-${crypto.randomUUID().slice(0, 8)}`,
        name: form.name.trim(),
        email: form.email.trim(),
        company: form.company.trim(),
        use_case: form.use_case.trim(),
        created_at: new Date().toISOString(),
      };
      // Same call path as the console's template/agent lists → POST /message (public, CORS-enabled).
      const res = await mcpCall('execute_flow', { name: 'a3t-early-access', namespace: 'a3t', input: payload });
      if (!res.ok) throw new Error(res.error || 'Request failed');
      submitted = true;
      form = { name: '', email: '', company: '', use_case: '' };
    } catch (err: any) {
      formError = err?.message || 'Something went wrong. Please try again.';
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:head>
  <title>Sign in — A3T</title>
</svelte:head>

<div class="auth-page">
  <div class="auth-card">
    <div class="auth-header">
      <span class="auth-logo">A3T</span>
      <p class="auth-tagline">the agent platform</p>
    </div>

    <h1>Sign in</h1>
    <p class="auth-sub">Enter your credentials to access the platform.</p>

    {#if error}
      <div class="auth-error">{error}</div>
    {/if}

    <form onsubmit={handleSubmit}>
      <div class="field">
        <label for="email">Email</label>
        <input id="email" type="email" bind:value={email} placeholder="you@company.com" required autocomplete="email" />
      </div>
      <div class="field">
        <label for="password">Password</label>
        <input id="password" type="password" bind:value={password} placeholder="Password" required autocomplete="current-password" />
      </div>
      <button type="submit" class="auth-submit" disabled={loading}>
        {#if loading}
          <span class="spinner"></span> Signing in...
        {:else}
          Sign in
        {/if}
      </button>
    </form>

    <div class="auth-footer">
      <span>Don't have access yet?</span>
      <button type="button" class="link-btn" onclick={openForm}>Request early access</button>
    </div>
  </div>

  {#if showForm}
    <div class="modal-overlay" onclick={closeForm} role="presentation">
      <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <button class="modal-close" onclick={closeForm} aria-label="Close">×</button>
        {#if submitted}
          <div class="modal-success">
            <div class="success-check">✓</div>
            <h2>Request received</h2>
            <p>Thanks — we've logged your request. We'll reach out at the email you provided when a spot opens up.</p>
            <button type="button" class="auth-submit" onclick={closeForm}>Done</button>
          </div>
        {:else}
          <h2>Request Early Access</h2>
          <p class="modal-sub">A3T is invite-only during private beta. Tell us a bit about you.</p>
          {#if formError}<div class="auth-error">{formError}</div>{/if}
          <form onsubmit={submitRequest}>
            <div class="field">
              <label for="ea-name">Name</label>
              <input id="ea-name" bind:value={form.name} placeholder="Jane Doe" required />
            </div>
            <div class="field">
              <label for="ea-email">Work email</label>
              <input id="ea-email" type="email" bind:value={form.email} placeholder="jane@company.com" required />
            </div>
            <div class="field">
              <label for="ea-company">Company <span class="opt">(optional)</span></label>
              <input id="ea-company" bind:value={form.company} placeholder="Acme Inc." />
            </div>
            <div class="field">
              <label for="ea-use">What do you want to build? <span class="opt">(optional)</span></label>
              <textarea id="ea-use" rows="3" bind:value={form.use_case} placeholder="Multi-agent workflows for…"></textarea>
            </div>
            <button type="submit" class="auth-submit" disabled={submitting}>
              {#if submitting}<span class="spinner"></span> Submitting…{:else}Submit request{/if}
            </button>
          </form>
        {/if}
      </div>
    </div>
  {/if}

  <div class="auth-aside">
    <div class="aside-content">
      <h2>Build agents that think, remember, and act.</h2>
      <p>A3T is the infrastructure layer for autonomous AI agents — templates, memory, tools, and orchestration in one platform.</p>
      <div class="aside-features">
        <div class="af"><span class="af-ico">T</span> Templates — CNCF Serverless Workflow specs</div>
        <div class="af"><span class="af-ico">A</span> Agents — containers with memory &amp; lifecycle</div>
        <div class="af"><span class="af-ico">C</span> Canvas — K8s CRDs for multi-agent orchestration</div>
      </div>
      <div class="aside-cta">
        <button type="button" class="cta-btn" onclick={openForm}>
          Request Early Access <span class="cta-arrow">→</span>
        </button>
        <span class="cta-note">Invite-only during private beta.</span>
      </div>
    </div>
  </div>
</div>

<style>
  .auth-page {
    display: flex;
    height: 100vh;
    background: var(--bg-page);
  }

  .auth-card {
    width: 440px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 48px 52px;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
  }

  .auth-header { margin-bottom: 36px; }
  .auth-logo {
    font-family: var(--font-serif);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .auth-logo::after {
    content: '';
    display: inline-block;
    width: 6px; height: 6px;
    background: var(--ephemeral);
    border-radius: 50%;
    margin-left: 6px;
    vertical-align: middle;
    margin-bottom: 3px;
  }
  .auth-tagline {
    font-family: var(--font-serif);
    font-size: 13px;
    font-style: italic;
    color: var(--text-4);
    margin-top: 2px;
  }

  h1 {
    font-family: var(--font-serif);
    font-size: 24px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin-bottom: 6px;
  }
  .auth-sub { font-size: 13px; color: var(--text-3); margin-bottom: 24px; }

  .auth-error {
    background: var(--err-bg);
    color: #952d2d;
    border: 1px solid var(--err);
    border-radius: var(--r-sm);
    padding: 8px 12px;
    font-size: 13px;
    margin-bottom: 16px;
  }

  form { display: flex; flex-direction: column; gap: 16px; }

  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label {
    font-size: 12px; font-weight: 500; color: var(--text-2);
  }
  .field input {
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    font-size: 14px;
    font-family: var(--font-sans);
    background: var(--bg-surface);
    transition: border-color 0.15s;
  }
  .field input:focus {
    outline: none;
    border-color: var(--spec);
    box-shadow: 0 0 0 2px var(--spec-bg);
  }
  .field input::placeholder { color: var(--text-5); }

  .auth-submit {
    padding: 11px 20px;
    background: var(--text-1);
    color: white;
    border: none;
    border-radius: var(--r-sm);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    transition: background 0.15s;
    margin-top: 4px;
  }
  .auth-submit:hover { background: #000; }
  .auth-submit:disabled { opacity: 0.6; cursor: not-allowed; }

  .spinner {
    width: 14px; height: 14px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .auth-footer {
    margin-top: 24px;
    font-size: 13px;
    color: var(--text-3);
    display: flex;
    gap: 6px;
  }
  .auth-footer a,
  .link-btn {
    color: var(--spec);
    font-weight: 500;
    text-decoration: none;
  }
  .auth-footer a:hover,
  .link-btn:hover { text-decoration: underline; }
  .link-btn {
    background: none;
    border: none;
    padding: 0;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  /* Request Early Access modal */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    z-index: 100;
    animation: fade 0.15s ease;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
  .modal {
    position: relative;
    width: 440px;
    max-width: 100%;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 32px 32px 28px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
    animation: pop 0.15s ease;
  }
  @keyframes pop { from { transform: translateY(8px) scale(0.98); opacity: 0; } to { transform: none; opacity: 1; } }
  .modal-close {
    position: absolute;
    top: 14px; right: 16px;
    width: 30px; height: 30px;
    display: grid; place-items: center;
    background: none;
    border: none;
    border-radius: 6px;
    font-size: 22px;
    line-height: 1;
    color: var(--text-4);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .modal-close:hover { background: var(--bg-subtle); color: var(--text-1); }
  .modal h2 {
    font-family: var(--font-serif);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    margin-bottom: 6px;
  }
  .modal-sub { font-size: 13px; color: var(--text-3); margin-bottom: 22px; }
  .modal form { margin-top: 4px; }
  .modal textarea {
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    font-size: 14px;
    font-family: var(--font-sans);
    background: var(--bg-surface);
    resize: vertical;
    transition: border-color 0.15s;
  }
  .modal textarea:focus {
    outline: none;
    border-color: var(--spec);
    box-shadow: 0 0 0 2px var(--spec-bg);
  }
  .opt { color: var(--text-5); font-weight: 400; }
  .modal-success { text-align: center; padding: 12px 0 4px; }
  .success-check {
    width: 52px; height: 52px;
    margin: 0 auto 16px;
    display: grid; place-items: center;
    background: var(--spec-bg, #eef4ff);
    color: var(--spec);
    border-radius: 50%;
    font-size: 26px;
    font-weight: 700;
  }
  .modal-success p { font-size: 14px; color: var(--text-2); line-height: 1.6; margin: 8px 0 22px; }

  /* Right aside */
  .auth-aside {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 48px;
    background: linear-gradient(135deg, var(--bg-page) 0%, var(--bg-subtle) 100%);
  }
  .aside-content { max-width: 480px; }
  .aside-content h2 {
    font-family: var(--font-serif);
    font-size: 32px;
    font-weight: 500;
    letter-spacing: -0.02em;
    line-height: 1.3;
    margin-bottom: 14px;
  }
  .aside-content p {
    font-size: 15px;
    color: var(--text-2);
    line-height: 1.6;
    margin-bottom: 28px;
  }

  .aside-cta {
    margin-top: 34px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }
  .cta-btn {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 15px 30px;
    background: var(--spec);
    color: #fff;
    font-family: inherit;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
    border: none;
    border-radius: 10px;
    text-decoration: none;
    cursor: pointer;
    box-shadow: 0 6px 20px var(--spec-bg), 0 1px 2px rgba(0,0,0,0.08);
    transition: transform 0.15s ease, box-shadow 0.15s ease, filter 0.15s ease;
  }
  .cta-btn:hover {
    transform: translateY(-2px);
    filter: brightness(1.05);
    box-shadow: 0 10px 28px var(--spec-bg), 0 2px 6px rgba(0,0,0,0.12);
  }
  .cta-arrow { transition: transform 0.15s ease; }
  .cta-btn:hover .cta-arrow { transform: translateX(3px); }
  .cta-note { font-size: 12px; color: var(--text-4); }

  .aside-features { display: flex; flex-direction: column; gap: 12px; }
  .af {
    display: flex; align-items: center; gap: 12px;
    font-size: 13px; color: var(--text-2);
  }
  .af-ico {
    width: 28px; height: 28px;
    border-radius: 6px;
    display: grid; place-items: center;
    font-family: var(--font-mono);
    font-size: 11px; font-weight: 700;
    color: white;
    flex-shrink: 0;
  }
  .af:nth-child(1) .af-ico { background: var(--spec); }
  .af:nth-child(2) .af-ico { background: var(--runtime); }
  .af:nth-child(3) .af-ico { background: var(--control); }

  /* Tablet */
  @media (max-width: 1024px) {
    .auth-card { width: 380px; padding: 36px 40px; }
    .aside-content h2 { font-size: 26px; }
    .auth-aside { padding: 36px; }
  }

  /* Phone */
  @media (max-width: 768px) {
    .auth-page { flex-direction: column; }
    .auth-card {
      width: 100%;
      border-right: none;
      border-bottom: 1px solid var(--border);
      padding: 32px 24px;
      flex: 1;
    }
    .auth-aside { display: none; }
    .auth-header { margin-bottom: 28px; }
    h1 { font-size: 22px; }
  }

  /* Small phone */
  @media (max-width: 400px) {
    .auth-card { padding: 24px 18px; }
  }
</style>
