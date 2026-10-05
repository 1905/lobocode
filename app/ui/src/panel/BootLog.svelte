<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { stepRows, downloadLine, bootElapsed, bootHealth } from '../lib/view';
  import Cursor from '../widgets/Cursor.svelte';
  import BracketButton from '../widgets/BracketButton.svelte';
  let {
    panel,
    nowMs,
    stop,
  }: { panel: PanelState; nowMs: number; stop: () => void } = $props();
  const dl = $derived(downloadLine(panel));
  const health = $derived(bootHealth(panel, nowMs));
</script>

<section class="boot">
  {#each stepRows(panel) as row}<div class="step">
      <div class="row">
        <span
          class={row.mark === 'ok'
            ? 'green'
            : row.mark === 'cur'
              ? 'cyan'
              : 'faint'}
          >{row.mark === 'ok'
            ? '[ OK ]'
            : row.mark === 'cur'
              ? '[ >> ]'
              : '[ .. ]'}</span
        ><span class={row.mark === 'wait' ? 'faint' : 'text'}>{row.label}</span
        >{#if row.mark === 'cur'}<Cursor />{/if}<span class="grow"
        ></span>{#if row.at}<span class="dim">{row.at}</span>{/if}
      </div>
      {#if row.step === 'download' && row.mark === 'cur' && dl}<div
          class="download row small"
        >
          {#if dl.kind === 'verify'}<span>verify</span>{:else}<span
              class="copper">{dl.bar}</span
            >{#if dl.kind === 'sha'}<span>verify sha256</span>{:else}<span
                >{dl.gb}</span
              ><span class={dl.mbpsTone}>{dl.mbps}</span>{#if dl.eta}<span
                  class="dim">{dl.eta}</span
                >{/if}{/if}{/if}
        </div>{/if}
    </div>{/each}
  <p class="tiny faint two-lines detail">{panel.last_detail}</p>
  <p class:amber={health.stale} class:dim={!health.stale} class="tiny">
    {health.text}
  </p>
  <div class="row spread bottom">
    <strong class="cyan">{bootElapsed(panel, nowMs)}</strong><BracketButton
      label="ABORT"
      tone="red"
      onclick={stop}
    />
  </div>
</section>

<style>
  .boot {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 11px;
  }
  .step {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .step > .row {
    gap: 8px;
  }
  .download {
    margin-left: 56px;
    gap: 6px;
    white-space: nowrap;
    letter-spacing: -0.3px;
  }
  .detail {
    margin-top: 2px;
  }
  .bottom {
    margin-top: 4px;
  }
</style>
