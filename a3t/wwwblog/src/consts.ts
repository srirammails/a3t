// Where the blog lives and where readers go next. Moving the blog to a3t.io/blog:
// change SITE_URL and set `base: '/blog'` in astro.config.mjs.
export const SITE_URL = 'https://blog.a3t.io';
export const MAIN_URL = 'https://a3t.io';
export const DOCS_URL = 'https://docs.a3t.io';
export const QUICKSTART_URL = 'https://docs.a3t.io/getting-started/quickstart/';
export const CONSOLE_URL = 'https://dash.a3t.io';
export const DEMO_URL = 'https://calendly.com/srirammails';

export const BLOG_TITLE = 'A3T Blog';
export const BLOG_DESCRIPTION =
	'Notes on building governed, mission-critical AI agents: runtime, policy, memory and knowledge graphs.';
export const DEFAULT_OG_IMAGE = 'https://a3t.io/assets/screens/16-chat-home.png';

// Tag → colour family from the a3t.io design tokens. Unknown tags fall back to "signal".
export const TAG_TONES: Record<string, string> = {
	runtime: 'ephemeral',
	policy: 'checkpoint',
	security: 'checkpoint',
	memory: 'signal',
	aql: 'signal',
	'knowledge-graph': 'signal',
	agents: 'ok',
	'use-cases': 'ok',
	kubernetes: 'k8s',
	platform: 'k8s',
	workflows: 'ephemeral',
	cncf: 'ephemeral',
	yaml: 'ephemeral',
};
export const toneFor = (tag?: string) => (tag && TAG_TONES[tag]) || 'signal';
