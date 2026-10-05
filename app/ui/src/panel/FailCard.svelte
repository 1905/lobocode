<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { fail } from '../lib/view';
  import BracketButton from '../widgets/BracketButton.svelte';
  import StartCard from './StartCard.svelte';
  let {
    panel,
    action,
    pick,
  }: {
    panel: PanelState;
    action: (name: string) => void;
    pick: (kind: string, value: string) => void;
  } = $props();
  const v = $derived(fail(panel));
</script>

<section class="stack failure">
  <div class="row message">
    <strong class="red">[FAIL]</strong><span title={v.message}>{v.message}</span
    >
  </div>
  {#if v.tail.length}<div class="tail">
      {#each v.tail as line}<p class="tiny faint clip" title={line}>
          {line}
        </p>{/each}
    </div>{/if}
  {#if !panel.snap?.pod && panel.start_allowed}<StartCard
      {panel}
      {action}
      {pick}
      startLabel="RETRY"
    />{/if}
  <div class="row">
    {#if !panel.start_allowed}
      <BracketButton
        label="RETRY STOP"
        tone="red"
        onclick={() => action('stop')}
      />
    {:else if panel.snap?.pod}
      <BracketButton
        label={v.primary.label}
        tone={v.primary.tone}
        onclick={() => action(v.primary.action)}
      />{/if}<BracketButton
      label="DISMISS"
      tone="dim"
      disabled={!panel.start_allowed}
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
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
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
