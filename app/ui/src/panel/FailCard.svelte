<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { fail } from '../lib/view';
  import BracketButton from '../widgets/BracketButton.svelte';
  let { panel, action }: { panel: PanelState; action: (name: string) => void } =
    $props();
  const v = $derived(fail(panel));
</script>

<section class="stack failure">
  <div class="row message">
    <strong class="red">[FAIL]</strong><span>{v.message}</span>
  </div>
  {#if v.tail.length}<div class="tail">
      {#each v.tail as line}<p class="tiny faint clip" title={line}>
          {line}
        </p>{/each}
    </div>{/if}
  <div class="row">
    <BracketButton
      label={v.primary.label}
      tone={v.primary.tone}
      onclick={() => action(v.primary.action)}
    /><BracketButton
      label="DISMISS"
      tone="dim"
      onclick={() => action('dismiss')}
    />
  </div>
</section>

<style>
  .failure {
    gap: 8px;
  }
  .message {
    align-items: flex-start;
    font-size: 11px;
    gap: 8px;
  }
  .message strong {
    flex: none;
  }
  .message span {
    overflow-wrap: anywhere;
  }
  .tail {
    background: var(--card);
    border-radius: 4px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
</style>
