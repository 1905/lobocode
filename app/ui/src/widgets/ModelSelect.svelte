<script lang="ts">
  import type { CatalogEntry } from '../gen/CatalogEntry';
  import { modelOption, modelCaption } from '../lib/view';
  let {
    models,
    value,
    label,
    disabled = false,
    onpick,
  }: {
    models: CatalogEntry[];
    value: string;
    label?: string;
    disabled?: boolean;
    onpick: (id: string) => void;
  } = $props();
  const selected = $derived(models.find((m) => m.id === value) ?? models[0]);
</script>

<div class="stack model">
  <div class="row">
    {#if label}<span class="dim target">&gt; {label}</span>{/if}
    <select
      aria-label="model"
      value={selected?.id ?? ''}
      {disabled}
      onchange={(e) => onpick(e.currentTarget.value)}
    >
      {#each models as m}<option value={m.id}>{modelOption(m)}</option>{/each}
    </select>
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
  select {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 12px;
    padding: 4px 8px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--text);
  }
  .tiny {
    margin: 0;
  }
</style>
