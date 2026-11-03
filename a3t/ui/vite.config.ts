import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';
import { checkLayers } from './scripts/check-layers.mjs';

/// Fails the build when the Studio's layer rules are broken (scripts/check-layers.mjs).
const studioLayers = (): Plugin => ({
	name: 'a3t-studio-layers',
	buildStart() {
		const problems = checkLayers(process.cwd());
		if (problems.length) this.error(`Studio layer rules:\n${problems.join('\n')}`);
	},
});

export default defineConfig({
	plugins: [studioLayers(), sveltekit()],
	ssr: {
		noExternal: ['@xyflow/svelte', '@xyflow/system']
	}
});
