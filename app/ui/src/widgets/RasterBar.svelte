<script lang="ts">
  let { active = false }: { active?: boolean } = $props();
</script>

<div class="raster" aria-hidden="true">
  {#if active}<span></span>{/if}
</div>

<style>
  .raster {
    height: 2px;
    background: var(--copper);
    position: relative;
    isolation: isolate;
    overflow: hidden;
  }
  .raster::before {
    content: '';
    position: absolute;
    inset: 0;
    background: var(--card);
    opacity: 0.65;
  }
  .raster span {
    position: absolute;
    height: 2px;
    width: 25%;
    background: linear-gradient(90deg, transparent, #ffffffe6, transparent);
    animation: sweep 1.6s linear infinite;
  }
  @keyframes sweep {
    from {
      left: -25%;
    }
    to {
      left: 100%;
    }
  }
</style>
