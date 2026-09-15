<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
</script>

<div class="join-node">
  <Handle type="target" position={Position.Top} id="in" style="background: var(--text-4, #999);" />
  <div class="join-diamond"></div>
  <Handle type="source" position={Position.Bottom} id="out" style="background: var(--text-4, #999);" />
</div>

<style>
  .join-node {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
  }
  .join-diamond {
    width: 18px;
    height: 18px;
    background: var(--bg-subtle, #f4f2ed);
    border: 2px solid var(--border-strong, #d9d4c6);
    border-radius: 3px;
    transform: rotate(45deg);
    transition: border-color 0.15s;
  }
  .join-node:hover .join-diamond {
    border-color: var(--spec, #5530a8);
    background: var(--spec-bg, #f3eefb);
  }
</style>
