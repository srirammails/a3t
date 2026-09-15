// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	site: 'https://docs.a3t.io',
	integrations: [
		starlight({
			title: 'A3T Docs',
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/openagen/flowr' },
			],
			customCss: ['./src/styles/custom.css'],
			editLink: { baseUrl: 'https://github.com/openagen/flowr/edit/main/a3t/docs/' },
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Introduction', slug: 'getting-started/introduction' },
						{ label: 'Quick Start', slug: 'getting-started/quickstart' },
						{ label: 'Architecture', slug: 'getting-started/architecture' },
					],
				},
				{
					label: 'Concepts',
					items: [
						{ label: 'Workflows', slug: 'concepts/workflows' },
						{ label: 'Task Types', slug: 'concepts/task-types' },
						{ label: 'Resources', slug: 'concepts/resources' },
						{ label: 'Expressions', slug: 'concepts/expressions' },
						{ label: 'Agents', slug: 'concepts/agents' },
						{ label: 'Memory', slug: 'concepts/memory' },
						{ label: 'Triggers', slug: 'concepts/triggers' },
					],
				},
				{
					label: 'Task Reference',
					autogenerate: { directory: 'tasks' },
				},
				{
					label: 'API Reference',
					autogenerate: { directory: 'api' },
				},
				{
					label: 'Guides',
					autogenerate: { directory: 'guides' },
				},
			],
		}),
	],
});
