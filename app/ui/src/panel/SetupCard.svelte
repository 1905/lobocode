<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import BracketButton from '../widgets/BracketButton.svelte';
  let { panel, setup }: { panel: PanelState; setup: () => void } = $props();
</script>

<section class="stack">
  <strong class="amber">no usable config yet</strong>
  <p class="small dim selectable">
    {panel.config?.path ?? '~/.config/lobo/config.env'}
  </p>
  <p class="small dim">
    {panel.readiness?.local_supported
      ? 'needs an api key. cloud also needs a provider key.'
      : 'needs a provider key (RunPod or Vast).'}
  </p>
  <BracketButton label="SETUP" tone="amber" wide onclick={setup} />
</section>
