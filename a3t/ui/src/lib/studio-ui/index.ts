/// The Studio's entry point: all a host imports from studio-ui (checked by
/// scripts/check-layers.mjs). Svelte hosts use `<Studio>`; any other host (the
/// VS Code webview) calls `mountStudio`. Design: .ai/A3T_STUDIO_ARCHITECTURE.md §12.2.
import { mount, unmount } from 'svelte';
import Studio from './Studio.svelte';
import type { Item, StudioHost } from '$lib/studio-core';
import type { StudioConfig } from './studios';

export { Studio };
export { STUDIOS, type StudioConfig } from './studios';

export type MountedStudio = { open(item: Item): void; dispose(): void };

export function mountStudio(target: Element, opts: { host: StudioHost; config: StudioConfig; open?: Item[] }): MountedStudio {
  const app = mount(Studio, { target, props: { host: opts.host, config: opts.config, initial: opts.open ?? [] } });
  return {
    open: (item) => app.open(item),
    dispose: () => void unmount(app),
  };
}
