import { defineCollection, z } from 'astro:content';
import { file, glob } from 'astro/loaders';

// Optional player for a post or library entry. `src` is a YouTube/Vimeo URL, an embed URL
// (Spotify, a podcast host, ...), or a media file (.mp4/.webm/.mp3/.m4a/...) under public/ or on a CDN.
const media = z.object({
	type: z.enum(['video', 'audio']),
	src: z.string(),
	poster: z.string().optional(), // file videos only
	duration: z.string().optional(), // shown on cards, e.g. "24 min"
});

const blog = defineCollection({
	loader: glob({ base: './src/content/blog', pattern: '**/*.{md,mdx}' }),
	schema: z.object({
		title: z.string(),
		// Also the LinkedIn / search snippet; keep it under ~160 characters.
		description: z.string().max(200),
		date: z.coerce.date(),
		updated: z.coerce.date().optional(),
		author: z.string().default('A3T Team'),
		tags: z.array(z.string()).default([]),
		// Home-page filter areas (see src/lib/areas.ts). Leave out to derive them from tags.
		areas: z.array(z.enum(['workflows', 'runtime', 'ai', 'data', 'security'])).optional(),
		// Path under public/ (e.g. /covers/post.png), ideally 1200×627 for LinkedIn.
		// Without one the post gets a generated cover on the site and the default OG image.
		image: z.string().optional(),
		imageAlt: z.string().optional(),
		// A video or audio post: the player replaces the cover at the top; the body is notes/transcript.
		media: media.optional(),
		// Pinned post becomes the hero on the home page (newest pinned wins, else newest post).
		pinned: z.boolean().default(false),
		draft: z.boolean().default(false),
	}),
});

// Papers, posts, talks, podcasts and repos worth reading. The Markdown body is your commentary.
const library = defineCollection({
	loader: glob({ base: './src/content/library', pattern: '**/*.md' }),
	schema: z.object({
		title: z.string(),
		url: z.string().url(),
		kind: z.enum(['paper', 'post', 'video', 'podcast', 'repo', 'book']),
		source: z.string(), // arXiv, OOPSLA 2024, a blog's name, a channel, ...
		authors: z.string().optional(),
		year: z.number().int().optional(),
		added: z.coerce.date(),
		tags: z.array(z.string()).default([]),
		media: media.optional(), // play a talk or episode in place
		related: z.string().optional(), // slug of the A3T post that builds on this
		draft: z.boolean().default(false),
	}),
});

// One-line links: a note and a URL. Kept in a single file, shown in file order.
const links = defineCollection({
	loader: file('src/content/links.yaml'),
	schema: z.object({
		title: z.string(),
		url: z.string().url(),
		note: z.string().max(140),
	}),
});

export const collections = { blog, library, links };
