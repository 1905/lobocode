<script lang="ts">
  import type { PanelState } from '../gen/PanelState';
  import type { Target } from '../gen/Target';
  import { api, message } from '../lib/api';
  import { stopping } from '../lib/view';
  import Header from './Header.svelte';
  import Footer from './Footer.svelte';
  import SetupCard from './SetupCard.svelte';
  import StartCard from './StartCard.svelte';
  import BootLog from './BootLog.svelte';
  import ReadyCard from './ReadyCard.svelte';
  import FailCard from './FailCard.svelte';
  import Cursor from '../widgets/Cursor.svelte';
  let {
    panel,
    nowMs,
    rendering = false,
  }: { panel: PanelState; nowMs: number; rendering?: boolean } = $props();
  let error = $state('');
  async function perform(f: () => Promise<void>) {
    if (rendering) return;
    try {
      await f();
      error = '';
    } catch (e) {
      error = message(e);
      throw e;
    }
  }
  function action(name: string) {
    const f = (
      {
        start: api.start,
        stop: api.stop,
        dismiss: api.dismiss,
        settings: api.openSettings,
        reveal: api.revealConfig,
        quit: api.quit,
      } as Record<string, () => Promise<void>>
    )[name];
    if (f) void perform(f).catch(() => {});
  }
  function pick(kind: string, value: string) {
    void perform(() =>
      kind === 'target'
        ? api.chooseTarget(value as Target)
        : kind === 'model'
          ? api.setModel(value)
          : api.setProvider(value),
    ).catch(() => {});
  }
  function keydown(e: KeyboardEvent) {
    if (
      rendering ||
      (e.target instanceof HTMLElement &&
        ['INPUT', 'TEXTAREA'].includes(e.target.tagName))
    )
      return;
    if (e.metaKey && e.key === ',') {
      e.preventDefault();
      action('settings');
    } else if (e.metaKey && e.key === 'q') {
      e.preventDefault();
      action('quit');
    } else if (
      e.key === 'Enter' &&
      panel.phase.kind === 'off' &&
      (panel.target === 'local' || panel.readiness?.cloud_ready)
    ) {
      e.preventDefault();
      action('start');
    }
  }
</script>

<svelte:window onkeydown={keydown} />
<main class="panel" data-phase={panel.phase.kind}>
  <Header {panel} {rendering} />
  <div class="body">
    {#if panel.phase.kind === 'loading'}<div class="row dim">
        scanning providers<Cursor />
      </div>{:else if panel.phase.kind === 'no_config'}<SetupCard
        {panel}
        setup={() => action('settings')}
      />{:else if panel.phase.kind === 'off'}<StartCard
        {panel}
        {action}
        {pick}
      />{:else if panel.phase.kind === 'booting'}<BootLog
        {panel}
        {nowMs}
        stop={() => action('stop')}
      />{:else if panel.phase.kind === 'ready'}<ReadyCard
        {panel}
        {nowMs}
        stop={() => action('stop')}
        copy={(kind) =>
          perform(() =>
            kind === 'key'
              ? api.copyApiKey()
              : api.copyText(panel.endpoint ?? ''),
          )}
      />{:else if panel.phase.kind === 'stopping'}<div class="row">
        <span class="cyan">[ .. ]</span><span>{stopping(panel)}</span><Cursor />
      </div>{:else}<FailCard {panel} {action} />{/if}{#if panel.warning}<p
        class="warning two-lines"
      >
        ! {panel.warning}
      </p>{/if}{#if error}<p class="error">{error}</p>{/if}
  </div>
  <Footer {action} />
</main>

<style>
  .panel {
    width: 340px;
    background: var(--bg);
  }
  .body {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
