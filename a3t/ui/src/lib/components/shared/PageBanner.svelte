<script lang="ts">
  /// The intro banner at the top of a section page: what this part of the platform
  /// is for, in the product's own voice.
  ///
  /// `highlightWord` is rendered in italic within the headline, so the headline is
  /// written as one sentence and the emphasis is data rather than markup. Matching
  /// is case-insensitive, and a word that does not occur simply leaves the headline
  /// unstyled instead of breaking it.
  let {
    headline = '',
    highlightWord = '',
    subtitle = '',
    description = '',
    accent = 'spec',
    docsPath = '',
  }: {
    headline?: string;
    highlightWord?: string;
    subtitle?: string;
    description?: string;
    /// One of the platform's four accent families.
    accent?: 'spec' | 'runtime' | 'control' | 'ephemeral';
    docsPath?: string;
  } = $props();

  let dismissed = $state(false);

  const parts = $derived.by(() => {
    if (!highlightWord) return { before: headline, highlight: '', after: '' };
    const i = headline.toLowerCase().indexOf(highlightWord.toLowerCase());
    if (i === -1) return { before: headline, highlight: '', after: '' };
    return {
      before: headline.slice(0, i),
      highlight: headline.slice(i, i + highlightWord.length),
      after: headline.slice(i + highlightWord.length),
    };
  });

  const ACCENTS: Record<string, { color: string; bg: string }> = {
    spec:      { color: 'var(--spec, #5530a8)',      bg: 'var(--spec-bg, #ede7ff)' },
    runtime:   { color: 'var(--runtime, #1a7158)',   bg: 'var(--runtime-bg, #e3f4ef)' },
    control:   { color: 'var(--control, #c97c2e)',   bg: 'var(--control-bg, #fbf0e0)' },
    ephemeral: { color: 'var(--ephemeral, #3a7fff)', bg: 'var(--ephemeral-bg, #eaf1ff)' },
  };
  const tone = $derived(ACCENTS[accent] ?? ACCENTS.spec);

  const docsUrl = $derived(
    docsPath ? `https://docs.a3t.com${docsPath.startsWith('/') ? '' : '/'}${docsPath}` : 'https://docs.a3t.com',
  );
</script>

{#if !dismissed}
  <div
    class="page-banner"
    style="background: linear-gradient(135deg, {tone.bg} 0%, var(--bg-subtle) 100%); border-color: {tone.bg};"
  >
    <button class="banner-close" title="Dismiss" aria-label="Dismiss" onclick={() => (dismissed = true)}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <path d="M4 4l8 8M12 4l-8 8" />
      </svg>
    </button>

    <h2 class="banner-headline">
      {parts.before}<em class="banner-highlight" style="color: {tone.color};">{parts.highlight}</em>{parts.after}
    </h2>

    {#if subtitle}<p class="banner-subtitle">{subtitle}</p>{/if}
    {#if description}<p class="banner-desc">{description}</p>{/if}

    <a class="banner-docs" href={docsUrl} target="_blank" rel="noopener noreferrer" style="color: {tone.color};">
      Learn more
      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <path d="M6 3h7v7M13 3L6 10" />
      </svg>
    </a>
  </div>
{/if}

<style>
  .page-banner {
    position: relative;
    border: 1px solid;
    border-radius: var(--r);
    padding: 28px 32px 24px;
    margin-bottom: 24px;
  }
  .banner-close {
    position: absolute; top: 12px; right: 12px;
    width: 28px; height: 28px;
    display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--border); border-radius: 50%;
    background: var(--bg-surface); color: var(--text-4);
    cursor: pointer; transition: all 0.12s;
  }
  .banner-close:hover {
    background: var(--bg-subtle); color: var(--text-1); border-color: var(--border-strong);
  }
  .banner-headline {
    font-family: var(--font-serif);
    font-size: 26px; font-weight: 500; line-height: 1.35;
    letter-spacing: -0.02em;
    color: var(--text-1);
    margin-bottom: 12px;
    padding-right: 32px;   /* clear of the dismiss button */
  }
  .banner-highlight { font-style: italic; }
  .banner-subtitle {
    font-size: 15px; font-weight: 500; line-height: 1.5;
    color: var(--text-2); margin-bottom: 8px;
  }
  .banner-desc {
    font-size: 13px; line-height: 1.6;
    color: var(--text-3); max-width: 640px;
  }
  .banner-docs {
    display: inline-flex; align-items: center; gap: 4px;
    margin-top: 12px;
    font-size: 12px; font-weight: 500;
    text-decoration: none;
    transition: opacity 0.12s;
  }
  .banner-docs:hover { opacity: 0.8; }
</style>
