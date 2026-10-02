<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
  import type { PanelState } from './gen/PanelState';
  import { api, inTauri, message, onState } from './lib/api';
  import Panel from './panel/Panel.svelte';
  import Settings from './settings/Settings.svelte';
  import Render from './render/Render.svelte';
  import { settingsTab } from './lib/view';
  const params = new URLSearchParams(location.search);
  const view = params.get('view') ?? 'panel';
  let panel = $state<PanelState>();
  let now = $state(Date.now());
  let error = $state('');
  let surface: HTMLDivElement | undefined = $state();
  onMount(() => {
    if (view === 'render' || !surface || !inTauri()) return;
    document.documentElement.classList.add('native');
    let previous = '';
    let disposed = false;
    let inset = 0;
    const nativeWindow = getCurrentWindow();
    const observer = new ResizeObserver(() => {
      if (!surface) return;
      const bounds = surface.getBoundingClientRect();
      const width = view === 'settings' ? 520 : 340;
      const height = Math.ceil(bounds.height + inset);
      const size = `${width}:${height}`;
      if (height <= 0 || previous === size) return;
      previous = size;
      void nativeWindow
        .setSize(new LogicalSize(width, height))
        .catch((e) => (error = message(e)));
    });
    // macOS reports a content size that includes the native title-bar inset.
    // Measure it once before resizing; the webview needs its full content height.
    void Promise.all([nativeWindow.innerSize(), nativeWindow.scaleFactor()])
      .then(([size, scale]) => {
        inset = Math.max(0, size.height / scale - window.innerHeight);
        if (!disposed && surface) observer.observe(surface);
      })
      .catch((e) => (error = message(e)));
    return () => {
      disposed = true;
      observer.disconnect();
    };
  });
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
    return () => {
      disposed = true;
      unlisten?.();
      clearInterval(timer);
    };
  });
</script>

<div bind:this={surface} class="surface">
  {#if view === 'render'}<Render />{:else if view === 'settings'}<Settings
      initialTab={settingsTab(params.get('tab')) ?? 'cloud'}
    />{:else if panel}<Panel {panel} nowMs={now} />{:else}<p
      class="dim"
      style="padding:14px"
    >
      scanning providers
    </p>{/if}{#if error}<p class="error" style="padding:14px">{error}</p>{/if}
</div>

<style>
  .surface {
    width: max-content;
    min-width: 340px;
    border-radius: 12px;
    overflow: clip;
    background: var(--bg);
  }
</style>
