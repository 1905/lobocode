<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { limits } from '../lib/view';
  import BracketPicker from '../widgets/BracketPicker.svelte';
  import BracketButton from '../widgets/BracketButton.svelte';
  import LocalStart from './LocalStart.svelte';
  let {
    panel,
    action,
    pick,
  }: {
    panel: PanelState;
    action: (name: string) => void;
    pick: (kind: string, value: string) => void;
  } = $props();
</script>

<section class="stack start">
  {#if panel.readiness?.local_supported}<BracketPicker
      label="target"
      options={['local', 'cloud']}
      value={panel.target}
      onpick={(v) => pick('target', v)}
    />{/if}
  {#if panel.target === 'local'}<LocalStart
      {panel}
      pick={(v) => pick('model', v)}
    /><BracketButton label="START" wide onclick={() => action('start')} />
  {:else if !panel.readiness?.cloud_ready}<div class="row">
      <span class="dim target">&gt; cloud</span><span class="amber"
        >no keys</span
      >
    </div>
    <BracketButton
      label="SETUP"
      tone="amber"
      wide
      onclick={() => action('settings')}
    />
  {:else}{#if panel.readiness.providers.length > 1}<BracketPicker
        label="provider"
        options={panel.readiness.providers}
        value={panel.provider}
        onpick={(v) => pick('provider', v)}
      />{:else}<div class="row">
        <span class="dim target">&gt; provider</span><span
          >{panel.provider}</span
        >
      </div>{/if}<BracketPicker
      label="model"
      options={panel.catalog_ids}
      value={panel.model}
      onpick={(v) => pick('model', v)}
    />
    <div class="row small">
      <span class="dim target">&gt; limits</span><span class="dim"
        >{limits(panel.config?.values ?? {})}</span
      >
    </div>
    <BracketButton label="START" wide onclick={() => action('start')} />{/if}
</section>

<style>
  .start {
    gap: 10px;
  }
  .target {
    width: 84px;
    flex: none;
  }
  .row.small {
    font-size: 11px;
    gap: 0;
  }
</style>
