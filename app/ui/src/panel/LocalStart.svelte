<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { modelRows, localFooter } from '../lib/view';
  import BracketPicker from '../widgets/BracketPicker.svelte';
  let { panel, pick }: { panel: PanelState; pick: (id: string) => void } =
    $props();
</script>

{#if panel.models}<div class="stack models">
    {#each modelRows(panel) as m}<button
        class="row model"
        type="button"
        aria-pressed={m.picked}
        onclick={() => pick(m.id)}
        ><span class:green={m.picked} class:dim={!m.picked} class="model-id"
          >{m.picked ? `[${m.id}]` : ` ${m.id} `}</span
        ><span class="size dim">{m.size}</span><span
          class={m.state === 'partial'
            ? 'cyan'
            : m.state === 'on'
              ? 'text'
              : 'dim'}
          >{m.state === 'on'
            ? '✓ on disk'
            : m.state === 'partial'
              ? `partial ${m.pct}%`
              : '↓ download'}</span
        ></button
      >{/each}
    <p class="small dim clip" title={localFooter(panel.models)}>
      {localFooter(panel.models)}
    </p>
  </div>{:else}<BracketPicker
    label="model"
    options={panel.catalog_ids}
    value={panel.model}
    onpick={pick}
  />{/if}

<style>
  .models {
    gap: 8px;
  }
  .model {
    border: 0;
    background: none;
    text-align: left;
    padding: 0;
    gap: 10px;
  }
  .model-id {
    width: 38px;
    white-space: pre;
  }
  .size {
    width: 66px;
    font-size: 11px;
  }
  .model:hover {
    background: var(--card);
  }
</style>
