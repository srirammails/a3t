/**
 * Site-internal URL that respects Astro's `base` (e.g. /blog/ on a3t.io, or /absproxy/4330/ when
 * previewing through code-server). Absolute URLs pass through unchanged.
 *   url()             → base
 *   url('library/')   → base + 'library/'
 *   url('/covers/x')  → base + 'covers/x'
 */
export function url(path = ''): string {
	if (/^[a-z]+:\/\//i.test(path)) return path;
	const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');
	return base + path.replace(/^\//, '');
}
