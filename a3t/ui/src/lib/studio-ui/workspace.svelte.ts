/// A Workspace whose state is Svelte state, so views follow it.
import { Workspace, emptyWorkspace, type StudioHost } from '$lib/studio-core';

export function useWorkspace(app: string, host: StudioHost): Workspace {
  const state = $state(emptyWorkspace());
  return new Workspace(app, state, host);
}
