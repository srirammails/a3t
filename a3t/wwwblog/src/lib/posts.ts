import { getCollection, type CollectionEntry } from 'astro:content';

type Post = CollectionEntry<'blog'>;

const isSample = (p: Post) => p.id.startsWith('sample-');

/** Published posts, newest first, with template `sample-*` posts after real ones. Drafts show only in `npm run dev`. */
export async function getPosts(): Promise<Post[]> {
	const posts = await getCollection('blog', ({ data }) => import.meta.env.DEV || !data.draft);
	return posts.sort((a, b) => Number(isSample(a)) - Number(isSample(b)) || b.data.date.valueOf() - a.data.date.valueOf());
}

/** The hero post (newest with `pinned: true`, else the newest) and the rest. */
export function splitFeatured(posts: Post[]): { featured?: Post; rest: Post[] } {
	const featured = posts.find((p) => p.data.pinned) ?? posts[0];
	return { featured, rest: posts.filter((p) => p !== featured) };
}

export function readingMinutes(post: Post): number {
	const words = (post.body ?? '').split(/\s+/).filter(Boolean).length;
	return Math.max(1, Math.round(words / 230));
}

/** Library entries, newest additions first. */
export async function getLibrary(): Promise<CollectionEntry<'library'>[]> {
	const items = await getCollection('library', ({ data }) => import.meta.env.DEV || !data.draft);
	return items.sort((a, b) => b.data.added.valueOf() - a.data.added.valueOf());
}
