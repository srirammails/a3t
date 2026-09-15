<script lang="ts">
  import { goto } from '$app/navigation';
  import { signup, isAuthenticated } from '$lib/stores/auth';
  import { onMount } from 'svelte';

  let name = $state('');
  let email = $state('');
  let password = $state('');
  let confirmPassword = $state('');
  let error = $state('');
  let loading = $state(false);

  onMount(() => {
    const unsub = isAuthenticated.subscribe(v => { if (v) goto('/'); });
    return unsub;
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    error = '';

    if (password !== confirmPassword) {
      error = 'Passwords do not match';
      return;
    }

    loading = true;
    const result = await signup(name, email, password);
    loading = false;
    if (result.ok) {
      goto('/');
    } else {
      error = result.error || 'Signup failed';
    }
  }
</script>

<svelte:head>
  <title>Create account — A3T</title>
</svelte:head>

<div class="auth-page">
  <div class="auth-card">
    <div class="auth-header">
      <span class="auth-logo">A3T</span>
      <p class="auth-tagline">the agent platform</p>
    </div>

    <h1>Create account</h1>
    <p class="auth-sub">Get started with A3T — build, deploy, and orchestrate agents.</p>

    {#if error}
      <div class="auth-error">{error}</div>
    {/if}

    <form onsubmit={handleSubmit}>
      <div class="field">
        <label for="name">Full name</label>
        <input id="name" type="text" bind:value={name} placeholder="John Doe" required autocomplete="name" />
      </div>
      <div class="field">
        <label for="email">Email</label>
        <input id="email" type="email" bind:value={email} placeholder="you@company.com" required autocomplete="email" />
      </div>
      <div class="field">
        <label for="password">Password</label>
        <input id="password" type="password" bind:value={password} placeholder="Min 8 characters" required autocomplete="new-password" />
      </div>
      <div class="field">
        <label for="confirm">Confirm password</label>
        <input id="confirm" type="password" bind:value={confirmPassword} placeholder="Repeat password" required autocomplete="new-password" />
      </div>
      <button type="submit" class="auth-submit" disabled={loading}>
        {#if loading}
          <span class="spinner"></span> Creating account...
        {:else}
          Create account
        {/if}
      </button>
    </form>

    <div class="auth-footer">
      <span>Already have an account?</span>
      <a href="/login">Sign in</a>
    </div>
  </div>

  <div class="auth-aside">
    <div class="aside-content">
      <h2>Build agents that think, remember, and act.</h2>
      <p>A3T is the infrastructure layer for autonomous AI agents — templates, memory, tools, and orchestration in one platform.</p>
      <div class="aside-features">
        <div class="af"><span class="af-ico">T</span> Templates — CNCF Serverless Workflow specs</div>
        <div class="af"><span class="af-ico">A</span> Agents — containers with memory &amp; lifecycle</div>
        <div class="af"><span class="af-ico">C</span> Canvas — K8s CRDs for multi-agent orchestration</div>
      </div>
    </div>
  </div>
</div>

<style>
  .auth-page { display: flex; height: 100vh; background: var(--bg-page); }

  .auth-card {
    width: 440px; display: flex; flex-direction: column; justify-content: center;
    padding: 48px 52px; background: var(--bg-surface); border-right: 1px solid var(--border);
  }

  .auth-header { margin-bottom: 36px; }
  .auth-logo { font-family: var(--font-serif); font-size: 28px; font-weight: 600; letter-spacing: -0.02em; }
  .auth-logo::after { content: ''; display: inline-block; width: 6px; height: 6px; background: var(--ephemeral); border-radius: 50%; margin-left: 6px; vertical-align: middle; margin-bottom: 3px; }
  .auth-tagline { font-family: var(--font-serif); font-size: 13px; font-style: italic; color: var(--text-4); margin-top: 2px; }

  h1 { font-family: var(--font-serif); font-size: 24px; font-weight: 500; letter-spacing: -0.02em; margin-bottom: 6px; }
  .auth-sub { font-size: 13px; color: var(--text-3); margin-bottom: 24px; }

  .auth-error { background: var(--err-bg); color: #952d2d; border: 1px solid var(--err); border-radius: var(--r-sm); padding: 8px 12px; font-size: 13px; margin-bottom: 16px; }

  form { display: flex; flex-direction: column; gap: 14px; }

  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: 12px; font-weight: 500; color: var(--text-2); }
  .field input {
    padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r-sm);
    font-size: 14px; font-family: var(--font-sans); background: var(--bg-surface); transition: border-color 0.15s;
  }
  .field input:focus { outline: none; border-color: var(--spec); box-shadow: 0 0 0 2px var(--spec-bg); }
  .field input::placeholder { color: var(--text-5); }

  .auth-submit {
    padding: 11px 20px; background: var(--text-1); color: white; border: none; border-radius: var(--r-sm);
    font-size: 14px; font-weight: 500; cursor: pointer; display: flex; align-items: center; justify-content: center; gap: 8px; margin-top: 4px;
  }
  .auth-submit:hover { background: #000; }
  .auth-submit:disabled { opacity: 0.6; cursor: not-allowed; }

  .spinner { width: 14px; height: 14px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.6s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .auth-footer { margin-top: 24px; font-size: 13px; color: var(--text-3); display: flex; gap: 6px; }
  .auth-footer a { color: var(--spec); font-weight: 500; text-decoration: none; }
  .auth-footer a:hover { text-decoration: underline; }

  .auth-aside { flex: 1; display: flex; align-items: center; justify-content: center; padding: 48px; background: linear-gradient(135deg, var(--bg-page) 0%, var(--bg-subtle) 100%); }
  .aside-content { max-width: 480px; }
  .aside-content h2 { font-family: var(--font-serif); font-size: 32px; font-weight: 500; letter-spacing: -0.02em; line-height: 1.3; margin-bottom: 14px; }
  .aside-content p { font-size: 15px; color: var(--text-2); line-height: 1.6; margin-bottom: 28px; }
  .aside-features { display: flex; flex-direction: column; gap: 12px; }
  .af { display: flex; align-items: center; gap: 12px; font-size: 13px; color: var(--text-2); }
  .af-ico { width: 28px; height: 28px; border-radius: 6px; display: grid; place-items: center; font-family: var(--font-mono); font-size: 11px; font-weight: 700; color: white; flex-shrink: 0; }
  .af:nth-child(1) .af-ico { background: var(--spec); }
  .af:nth-child(2) .af-ico { background: var(--runtime); }
  .af:nth-child(3) .af-ico { background: var(--control); }

  @media (max-width: 1024px) {
    .auth-card { width: 380px; padding: 36px 40px; }
    .aside-content h2 { font-size: 26px; }
    .auth-aside { padding: 36px; }
  }

  @media (max-width: 768px) {
    .auth-page { flex-direction: column; }
    .auth-card {
      width: 100%; border-right: none; border-bottom: 1px solid var(--border);
      padding: 32px 24px; flex: 1;
    }
    .auth-aside { display: none; }
    .auth-header { margin-bottom: 28px; }
    h1 { font-size: 22px; }
  }

  @media (max-width: 400px) {
    .auth-card { padding: 24px 18px; }
  }
</style>
