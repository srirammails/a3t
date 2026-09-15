// Backend URL resolution.
//
// The API host is derived at runtime from where the app is served (one build,
// all environments), with two overrides:
//   1. VITE_API_URL  — a build-time pin (used for local dev).
//   2. localStorage 'a3t.apiBase' — a runtime switch set from Settings, so an
//      operator can flip the console between the CE and EE runtimes to test.
//
//   dash.dev.a3t.io  ->  https://console.dev.a3t.io
//   dash.a3t.io      ->  https://console.a3t.io

const KNOWN_DOMAINS = ['a3t.io', 'a3tplatform.com'];
const OVERRIDE_KEY = 'a3t.apiBase';

// UAT pin: the prod console (dash.a3t.io) currently has no updated backend
// (console.a3t.io runs an old image without the Security/IdP endpoints), so pin
// it to the vc-dev EE runtime which has the full API. Remove this entry once a
// prod backend runs the current image. The Settings runtime switch still wins.
const HOST_BACKEND_PIN: Record<string, string> = {
	'dash.a3t.io': 'https://console.dev.a3t.io',
};

/** Hostname-derived backend, ignoring any override. */
function hostnameApiBase(): string {
	if (typeof window === 'undefined') return '';
	const host = window.location.hostname;
	if (HOST_BACKEND_PIN[host]) return HOST_BACKEND_PIN[host];
	const domain = KNOWN_DOMAINS.find((d) => host === d || host.endsWith('.' + d));
	if (!domain) return ''; // unknown host -> same-origin
	const isDev = host === 'dev.' + domain || host.endsWith('.dev.' + domain);
	return `https://console.${isDev ? 'dev.' : ''}${domain}`;
}

/** The default backend (build pin or hostname), ignoring the runtime override. */
export function getDefaultApiBase(): string {
	return (import.meta.env.VITE_API_URL as string) || hostnameApiBase();
}

export function getApiOverride(): string | null {
	if (typeof window === 'undefined') return null;
	try {
		return localStorage.getItem(OVERRIDE_KEY);
	} catch {
		return null;
	}
}

export function setApiOverride(url: string) {
	try {
		localStorage.setItem(OVERRIDE_KEY, url);
	} catch {}
}

export function clearApiOverride() {
	try {
		localStorage.removeItem(OVERRIDE_KEY);
	} catch {}
}

function resolveApiBase(): string {
	const buildPin = import.meta.env.VITE_API_URL as string | undefined;
	if (buildPin) return buildPin;
	if (typeof window === 'undefined') return '';
	const runtime = getApiOverride(); // Settings runtime switch wins over hostname
	if (runtime) return runtime;
	return hostnameApiBase();
}

export const API_BASE = resolveApiBase();
