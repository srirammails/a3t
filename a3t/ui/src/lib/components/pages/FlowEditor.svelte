<script lang="ts">
  /// Flow Studio: the Studio Workbench with flows as the home kind. What it can
  /// open and do comes from the kinds (studio-core); this page only says which
  /// app it is and what shows with nothing open.
  import PageBanner from '$lib/components/shared/PageBanner.svelte';
  import AppHome from '$lib/shell/AppHome.svelte';
  import Workbench from '$lib/studio-ui/Workbench.svelte';
  import { Studio } from '$lib/studio-ui/layout.svelte.ts';
  import { useWorkspace } from '$lib/studio-ui/workspace.svelte.ts';
  import { getActivePanel } from '$lib/stores/panels.svelte.ts';
  import { getProject } from '$lib/stores/context.svelte.ts';

  const studio = new Studio('flows', {
    left: { size: 280, minimized: false, active: 'explorer' },
    right: { size: 380, minimized: false, active: 'form' },
    bottom: { size: 320, minimized: true, active: 'runs' },
  });
  const workspace = useWorkspace('flows');

  // A flow named in the address (/flow-editor/<ns>~<name>) opens on arrival.
  const sel = getActivePanel()?.props?.selectedId as string | undefined;
  if (sel && sel.includes('~')) {
    const [project, name] = sel.split('~');
    workspace.open({ project, kind: 'flow', name });
  }
</script>

<Workbench {studio} {workspace} appType="flow-editor" homeKind="flow" create={['flow']}>
  {#snippet home()}
    <!-- Nothing open: lead like the other section pages, with what the editor
         is for, then the flows you return to. -->
    <div class="welcome">
      <PageBanner
        headline="Author a flow."
        highlightWord="flow"
        subtitle="CNCF Serverless Workflow definitions, edited as a graph, as YAML, or as a form."
        description="Open a flow from the Explorer or start a new one. Check it under Problems, deploy it, then run or debug it from Runs — all without leaving the editor."
        accent="spec"
        docsPath="/concepts/workflows/"
      />
      <div class="welcome-actions">
        <button class="btn spec sm" onclick={() => workspace.create('flow', getProject() || 'a3t')}>+ New flow</button>
      </div>
      <AppHome appType="flow-editor" embedded />
    </div>
  {/snippet}
</Workbench>

<style>
  .welcome { overflow-y: auto; padding: 20px 32px 40px; }
  .welcome-actions { margin: -8px 0 16px; }
</style>
