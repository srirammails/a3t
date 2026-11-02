/// A Workspace whose state is Svelte state, so views follow it.
import { Workspace, emptyWorkspace, type StudioHost } from '$lib/studio-core';
import { webHost } from '$lib/host/web';

export function useWorkspace(app: string, host: StudioHost = webHost): Workspace {
  const state = $state(emptyWorkspace());
  return new Workspace(app, state, host);
}
