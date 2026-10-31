<script lang="ts">
  import type { CatalogEntry } from '../gen/CatalogEntry';
  import { modelOption, modelCaption } from '../lib/view';
  let {
    models,
    value,
    label,
  }: {
    models: CatalogEntry[];
    value: string;
    label?: string;
  } = $props();
  const selected = $derived(models.find((m) => m.id === value) ?? models[0]);
</script>

<div class="stack model">
  <div class="row">
    {#if label}<span class="dim target">&gt; {label}</span>{/if}
    {#if selected}<span class="fixed" aria-label="model"
        >{modelOption(selected)}</span
      >{/if}
  </div>
  {#if selected}<p class="tiny dim">{modelCaption(selected)}</p>{/if}
</div>

<style>
  .model {
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  .target {
    width: 84px;
    flex: none;
  }
  .fixed {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    padding: 5px 0;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tiny {
    margin: 0;
  }
</style>
