// /sitemap.xml: every docs page in one urlset. Starlight's own sitemap-index.xml / sitemap-0.xml
// stay as well; this is the conventional address search engines and tools look for first.
import type { APIRoute } from 'astro';
import { getCollection } from 'astro:content';

export const GET: APIRoute = async ({ site }) => {
	const docs = await getCollection('docs');
	const urls = docs
		.map((entry) => new URL(entry.id === 'index' ? '/' : `/${entry.id}/`, site).href)
		.sort();
	const body =
		'<?xml version="1.0" encoding="UTF-8"?>\n' +
		'<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
		urls.map((loc) => `  <url><loc>${loc}</loc></url>`).join('\n') +
		'\n</urlset>\n';
	return new Response(body, { headers: { 'Content-Type': 'application/xml; charset=utf-8' } });
};
