<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
  import type { PanelState } from './gen/PanelState';
  import { api, inTauri, message, onState } from './lib/api';
  import Panel from './panel/Panel.svelte';
  import Settings from './settings/Settings.svelte';
  import Render from './render/Render.svelte';
  const params = new URLSearchParams(location.search);
  const view = params.get('view') ?? 'panel';
  const chrome = params.get('chrome') === 'window';
  let panel = $state<PanelState>();
  let now = $state(Date.now());
  let error = $state('');
  let surface: HTMLDivElement | undefined = $state();
  onMount(() => {
    if (view === 'render' || view === 'settings') return;
    let disposed = false,
      unlisten: undefined | (() => void),
      events = 0;
    const timer = setInterval(() => (now = Date.now()), 1000);
    void (async () => {
      try {
        const remove = await onState((s) => {
          events++;
          panel = s;
        });
        if (disposed) {
          remove();
          return;
        }
        unlisten = remove;
        const initial = await api.getState();
        if (!events && !disposed) panel = initial;
      } catch (e) {
        error = message(e);
      }
    })();
    let lastHeight = 0;
    const observer = new ResizeObserver((entries) => {
      const height = Math.ceil(
        entries[0].target.getBoundingClientRect().height,
      );
      if (height > 0 && height !== lastHeight && inTauri()) {
        lastHeight = height;
        void getCurrentWindow()
          .setSize(new LogicalSize(340, height))
          .catch((e) => (error = message(e)));
      }
    });
    if (surface) observer.observe(surface);
    return () => {
      disposed = true;
      unlisten?.();
      clearInterval(timer);
      observer.disconnect();
    };
  });
</script>

<div bind:this={surface}>
  {#if chrome}<div
      class="window-chrome"
      data-tauri-drag-region
    ></div>{/if}{#if view === 'render'}<Render
    />{:else if view === 'settings'}<Settings />{:else if panel}<Panel
      {panel}
      nowMs={now}
    />{:else}<p class="dim" style="padding:14px">
      scanning providers
    </p>{/if}{#if error}<p class="error" style="padding:14px">{error}</p>{/if}
</div>
