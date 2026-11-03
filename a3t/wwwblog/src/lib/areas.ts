import type { CollectionEntry } from 'astro:content';

// The five areas the home page filters by. A post's areas come from its tags, unless its
// frontmatter lists `areas:` itself. Rename a label here and every pill follows.
export const AREAS = [
	{ id: 'workflows', label: 'Workflows & Agents', tags: ['workflows', 'cncf', 'yaml', 'agents', 'use-cases'] },
	{ id: 'runtime', label: 'Runtime & Platform', tags: ['runtime', 'platform', 'kubernetes'] },
	{ id: 'ai', label: 'AI/LLM', tags: ['llm', 'kblam', 'kv-cache'] },
	{ id: 'data', label: 'Data & Knowledge', tags: ['memory', 'aql', 'knowledge-graph'] },
	{ id: 'security', label: 'Security & Governance', tags: ['security', 'policy'] },
] as const;

export const FORMATS = [
	{ id: 'text', label: 'Text' },
	{ id: 'audio', label: 'Audio' },
	{ id: 'video', label: 'Video' },
] as const;

export type AreaId = (typeof AREAS)[number]['id'];
export type FormatId = (typeof FORMATS)[number]['id'];
export const AREA_IDS = AREAS.map((a) => a.id) as [AreaId, ...AreaId[]];

type Post = CollectionEntry<'blog'>;

export function areasFor(post: Post): (typeof AREAS)[number][] {
	const own = post.data.areas;
	return AREAS.filter((a) => (own ? own.includes(a.id) : a.tags.some((t) => post.data.tags.includes(t))));
}

export function formatFor(post: Post): (typeof FORMATS)[number] {
	return FORMATS.find((f) => f.id === (post.data.media?.type ?? 'text'))!;
}
