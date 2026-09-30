<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { ready } from '../lib/view';
  import LinkButton from '../widgets/LinkButton.svelte';
  import BracketButton from '../widgets/BracketButton.svelte';
  let {
    panel,
    nowMs,
    copy,
    stop,
  }: {
    panel: PanelState;
    nowMs: number;
    copy: (kind: 'url' | 'key') => Promise<void>;
    stop: () => void;
  } = $props();
  const v = $derived(ready(panel, nowMs));
  let copied = $state<string | null>(null);
  let generation = 0;
  async function doCopy(kind: 'url' | 'key') {
    await copy(kind);
    copied = kind;
    const n = ++generation;
    setTimeout(() => {
      if (n === generation) copied = null;
    }, 1200);
  }
</script>

<section class="stack">
  {#each [{ kind: 'url' as const, label: 'endpoint', value: v.endpoint }, { kind: 'key' as const, label: 'api key', value: v.apiKey }] as row}<div
      class="row value"
    >
      <span class="dim label">{row.label}</span><span
        class="grow clip selectable"
        title={row.value}>{row.value}</span
      ><LinkButton
        label={copied === row.kind ? 'copied' : 'copy'}
        tone={copied === row.kind ? 'green' : 'cyan'}
        onclick={() => {
          void doCopy(row.kind).catch(() => {});
        }}
      />
    </div>{/each}
  <div class="row tiles">
    {#each [{ label: 'gen', value: v.gen }, { label: 'prompt', value: v.prompt }] as tile}<div
        class="box tile"
      >
        <span class="tiny dim">{tile.label}</span>
        <div class="row">
          <strong>{tile.value}</strong><span class="tiny dim">tok/s</span>
        </div>
      </div>{/each}
  </div>
  {#if v.mem}<div class="row small memory">
      <span class="dim mem-label">{v.mem.label}</span><span class="copper"
        >{v.mem.bar}</span
      ><span>{v.mem.text}</span><span class="grow"></span>{#if v.mem.gpu}<span
          class={v.mem.gpu.hot ? 'green' : 'dim'}>{v.mem.gpu.text}</span
        >{/if}
    </div>{/if}
  <div class="row small time">
    <span
      >{#if v.kill}<span class="dim">{v.kill.label}</span><span
          class:amber={v.kill.warn}>{v.kill.text}</span
        >{/if}<span class="dim"> · T+{v.uptime}</span></span
    ><span class="grow"></span><span class:dim={panel.is_local}>{v.cost}</span>
  </div>
  <BracketButton label="STOP" tone="red" wide onclick={stop} />
</section>

<style>
  .value {
    font-size: 11px;
  }
  .label {
    width: 64px;
    flex: none;
  }
  .tiles {
    gap: 8px;
  }
  .tile {
    flex: 1;
    min-width: 0;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tile .row {
    align-items: baseline;
    gap: 4px;
  }
  .tile strong {
    font-size: 22px;
    color: var(--green);
    text-shadow: 0 0 6px #39ff8859;
  }
  .mem-label {
    width: 52px;
    flex: none;
  }
  .memory {
    gap: 6px;
    letter-spacing: -0.15px;
  }
  .memory .copper {
    letter-spacing: -1px;
  }
  .time {
    white-space: pre;
    gap: 0;
  }
</style>
