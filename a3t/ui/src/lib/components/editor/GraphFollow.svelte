<script lang="ts">
  /// Inside a SvelteFlow: keep a node in view — the run's position on the
  /// graph — centring on it whenever it changes to a node off screen.
  import { useSvelteFlow } from '@xyflow/svelte';

  /// `delay`: time for a freshly shown graph to lay out and fit first.
  let { nodeId = null, delay = 50 }: { nodeId?: string | null; delay?: number } = $props();
  const flow = useSvelteFlow();

  $effect(() => {
    const id = nodeId;
    if (!id) return;
    const t = setTimeout(() => {
      const n = flow.getInternalNode(id);
      if (!n) return;
      const { x, y } = n.internals.positionAbsolute;
      const w = n.measured.width ?? 180, h = n.measured.height ?? 60;
      const vp = flow.getViewport();
      const el = document.querySelector(`.svelte-flow__node[data-id="${CSS.escape(id)}"]`)?.closest('.svelte-flow') as HTMLElement | null;
      const sx = x * vp.zoom + vp.x, sy = y * vp.zoom + vp.y, m = 60;
      const inView = el && sx >= m && sy >= m && sx + w * vp.zoom <= el.clientWidth - m && sy + h * vp.zoom <= el.clientHeight - m;
      if (!inView) void flow.setCenter(x + w / 2, y + h / 2, { zoom: vp.zoom, duration: 300 });
    }, delay);
    return () => clearTimeout(t);
  });
</script>
