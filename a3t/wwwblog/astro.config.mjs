// @ts-check
import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import sitemap from '@astrojs/sitemap';
import { SITE_URL } from './src/consts.ts';

// Astro 5 (not 6 like wwwdocs): this pod has Node 20, Astro 6 needs Node 22.12+.
export default defineConfig({
	site: SITE_URL,
	// BLOG_BASE serves the site under a path: /blog for a3t.io/blog, /absproxy/4330 for the
	// code-server preview (see README). Defaults to the root of blog.a3t.io.
	base: process.env.BLOG_BASE || '/',
	trailingSlash: 'always',
	integrations: [mdx(), sitemap()],
	// Dev and preview servers are reached through the code-server proxy host.
	vite: { server: { allowedHosts: ['coder.dev.pubcontext.com', 'localhost', '127.0.0.1'] }, preview: { allowedHosts: ['coder.dev.pubcontext.com', 'localhost', '127.0.0.1'] } },
	markdown: { shikiConfig: { theme: 'github-light' } },
});
