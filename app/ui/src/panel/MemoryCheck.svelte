<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { memoryView } from '../lib/view';
  let { panel }: { panel: PanelState } = $props();
  const memory = $derived(memoryView(panel));
  const summary = $derived(
    memory.status === 'unavailable'
      ? memory.message.replace(
          /\. (?:Close other apps and retry, or use Cloud|Retry or use Cloud)\.$/,
          '',
        )
      : memory.message,
  );
</script>

<div class="memory small" role="status" data-memory={memory.status}>
  {#if memory.values}<p class="dim">{memory.values}</p>{/if}
  <p
    class="memory-message"
    class:amber={['insufficient', 'unavailable'].includes(memory.status)}
    class:dim={memory.status === 'checking'}
    title={memory.message}
  >
    {summary}
  </p>
  {#if memory.status === 'unavailable'}<p class="amber">
      Checks repeat automatically. Close other apps or use Cloud.
    </p>{/if}
</div>

<style>
  .memory {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .memory-message {
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    overflow: hidden;
  }
</style>
