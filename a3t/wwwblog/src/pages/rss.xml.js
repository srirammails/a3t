import rss from '@astrojs/rss';
import { BLOG_TITLE, BLOG_DESCRIPTION } from '../consts';
import { getPosts } from '../lib/posts';
import { url } from '../lib/url';

export async function GET(context) {
	const posts = await getPosts();
	return rss({
		title: BLOG_TITLE,
		description: BLOG_DESCRIPTION,
		site: context.site,
		items: posts.map((p) => ({
			title: p.data.title,
			description: p.data.description,
			pubDate: p.data.date,
			link: url(`${p.id}/`),
			categories: p.data.tags,
		})),
	});
}
