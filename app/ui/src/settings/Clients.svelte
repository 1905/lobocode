<script lang="ts">
  import { clientsView, type ClientsState } from '../lib/view';
  import LinkButton from '../widgets/LinkButton.svelte';
  import BracketButton from '../widgets/BracketButton.svelte';
  let {
    state,
    choose,
    configure,
    refresh,
  }: {
    state: ClientsState;
    choose: () => void;
    configure: () => void;
    refresh: () => void;
  } = $props();
  const v = $derived(clientsView(state));
</script>

<section class="clients">
  <div class="row spread">
    <h2 class="copper">// OpenCode</h2>
    <LinkButton
      label="refresh"
      tone="cyan"
      disabled={state.busy || state.refreshing}
      onclick={refresh}
    />
  </div>
  <div class="box target">
    <span class="small dim">config file</span>
    <p class="small selectable wrapped path" title={v.path}>
      {v.path || 'Finding OpenCode config…'}
    </p>
    <LinkButton
      label="choose existing config…"
      tone="cyan"
      disabled={state.busy}
      onclick={choose}
    />
  </div>
  <div class="metadata small">
    <span class="dim">endpoint</span><span
      class="wrapped selectable"
      title={v.endpoint}>{v.endpoint}</span
    >
    <span class="dim">model</span><span
      class="wrapped selectable"
      title={v.model}>{v.model}</span
    >
    {#if v.context}<span class="dim">context</span><span>{v.context}</span>{/if}
  </div>
  <label class="row small default">
    <input
      type="checkbox"
      bind:checked={state.makeDefault}
      disabled={state.busy}
    />
    <span>Use Lobocode by default</span>
  </label>
  <BracketButton
    label={state.busy ? 'WORKING…' : 'Configure/Repair'}
    disabled={v.disabled}
    onclick={configure}
  />
  <div class="messages small" aria-live="polite">
    {#if state.refreshing}<p class="dim">Checking OpenCode setup…</p>{/if}
    {#if v.reason}<p class="dim">{v.reason}</p>{/if}
    {#each v.warnings as warning}<p class="amber">{warning}</p>{/each}
    {#if state.error}<p class="red">{state.error}</p>{/if}
    {#if state.result}<p class="green">{v.result}</p>
      <p class="dim wrapped" title={state.result.path}>
        {state.result.path}
      </p>{/if}
  </div>
</section>

<style>
  .clients {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 11px;
  }
  .target {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px;
    min-width: 0;
  }
  .wrapped {
    min-width: 0;
    max-width: 100%;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
  }
  .path {
    width: 100%;
  }
  .metadata {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr);
    gap: 6px 8px;
  }
  .default {
    align-items: center;
  }
  input[type='checkbox'] {
    flex: none;
    width: 14px;
    height: 14px;
    margin: 0;
    accent-color: var(--green);
  }
  .messages {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-wrap: anywhere;
  }
</style>
