<script lang="ts">
  import { messages } from '$lib/core/status/messages';

  let progress = $state(0);
  let loading = $state(false);

  // Current visible message (one at a time, slides down then up)
  let currentMsg = $state<{ id: number; type: string; text: string } | null>(null);
  let visible = $state(false);
  let barColor = $state('var(--ok)');
  let barVisible = $state(false);

  messages.subscribe(msgs => {
    if (msgs.length > 0) {
      const latest = msgs[msgs.length - 1];
      showMessage(latest);
    }
  });

  function showMessage(msg: { id: number; type: string; text: string }) {
    currentMsg = msg;
    barColor = typeColor(msg.type);
    barVisible = true;

    // Slide down
    requestAnimationFrame(() => { visible = true; });

    // Slide up after delay, then hide bar
    const duration = msg.type === 'error' ? 5000 : 3000;
    setTimeout(() => {
      visible = false;
      // After slide-up animation, fade out bar
      setTimeout(() => {
        barVisible = false;
        currentMsg = null;
        messages.remove(msg.id);
      }, 400);
    }, duration);
  }

  function typeColor(type: string) {
    switch (type) {
      case 'success': return 'var(--ok)';
      case 'error': return 'var(--err)';
      case 'warning': return 'var(--warn)';
      default: return 'var(--warn)';
    }
  }

  // Expose globally
  if (typeof window !== 'undefined') {
    (window as any).__a3t_progress = {
      start: () => { loading = true; progress = 15; animate(); },
      set: (v: number) => { progress = v; },
      done: () => { progress = 100; setTimeout(() => { loading = false; progress = 0; }, 400); },
    };
  }

  function animate() {
    if (!loading) return;
    const tick = () => {
      if (!loading) return;
      if (progress < 90) progress += (90 - progress) * 0.04;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
</script>

<div class="status-bar">
  <!-- Progress bar -->
  <div class="progress-track" class:bar-hidden={!barVisible && !loading} style="background: {barColor}; opacity: {loading ? 1 : 0.75};">
    {#if loading}
      <div class="progress-fill" style="width: {progress}%; background: white; opacity: 0.4;"></div>
    {/if}
  </div>

  <!-- Message slides down from bar -->
  {#if currentMsg}
    <div class="message-strip" class:visible style="background: {barColor};">
      <span class="msg-text">{currentMsg.text}</span>
    </div>
  {/if}
</div>

<style>
  .status-bar {
    grid-area: status;
    position: relative;
    height: 3px;
    border-radius: 16px 16px 0 0;
    overflow: visible;
    z-index: 100;
  }

  .progress-track {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 3px;
    border-radius: 16px 16px 0 0;
    overflow: hidden;
    transition: background 0.4s ease, opacity 0.5s ease;
  }
  .progress-track.bar-hidden {
    opacity: 0 !important;
  }
  .progress-fill {
    height: 100%;
    transition: width 0.3s ease;
    border-radius: 16px 0 0 0;
  }

  .message-strip {
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%) translateY(-100%);
    width: 420px;
    padding: 8px 20px;
    border-radius: 0 0 8px 8px;
    font-size: 12px;
    font-weight: 500;
    color: white;
    white-space: nowrap;
    text-align: center;
    opacity: 0;
    transition: transform 0.35s cubic-bezier(0.4, 0, 0.2, 1),
                opacity 0.35s ease;
    box-shadow: 0 2px 8px rgba(0,0,0,0.12);
  }
  .message-strip.visible {
    transform: translateX(-50%) translateY(0);
    opacity: 1;
  }

  .msg-text {
    font-family: var(--font-mono);
    letter-spacing: 0.01em;
  }
</style>
