<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import { active, headerDetail, phaseTone, phaseWord } from '../lib/view';
  import Logo from '../widgets/Logo.svelte';
  import RasterBar from '../widgets/RasterBar.svelte';
  import Scanlines from '../widgets/Scanlines.svelte';
  import GlitchText from '../widgets/GlitchText.svelte';
  let { panel, rendering = false }: { panel: PanelState; rendering?: boolean } =
    $props();
  const detail = $derived(headerDetail(panel));
</script>

<header>
  <Scanlines /><Logo /><RasterBar active={active(panel)} />
  <div class="row">
    <span class="dim sys">sys:</span><GlitchText
      text={phaseWord(panel)}
      color={phaseTone(panel)}
      settled={rendering}
    /><span class="grow"></span><span class="tiny faint clip"
      >{panel.snap?.version?.version ?? ''}</span
    >
  </div>
  {#if detail}<p class="small dim two-lines">{detail}</p>{/if}
</header>

<style>
  header {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 14px 10px;
    background: var(--card);
    border-bottom: 1px solid var(--line);
  }
  .sys {
    font-size: 11px;
  }
</style>
