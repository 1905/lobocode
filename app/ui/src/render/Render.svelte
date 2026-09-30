<script lang="ts">
  import Panel from '../panel/Panel.svelte';
  import Settings from '../settings/Settings.svelte';
  import type { PanelState } from '../gen/PanelState';
  import type { OpenCodeInfo } from '../gen/OpenCodeInfo';
  import settings from '../fixtures/settings.json';
  type Fixture = { name: string; now_ms: number; state: PanelState };
  const files = import.meta.glob<Fixture>('../fixtures/panel_*.json', {
    eager: true,
    import: 'default',
  });
  const fixtures = Object.values(files);
  const name = new URLSearchParams(location.search).get('state') ?? '';
  const menubar = name.startsWith('menubar_');
  const clientsFixture: OpenCodeInfo = {
    path: '/example/config/opencode/opencode.jsonc',
    endpoint: name === 'clients_off' ? null : 'http://127.0.0.1:8931/v1',
    provider: name === 'clients_off' ? null : 'lobo-local',
    model_alias: name === 'clients_off' ? null : 'qwen3-coder-next-q6',
    context: name === 'clients_off' ? null : 8192,
    can_configure: name !== 'clients_off',
    reason:
      name === 'clients_off'
        ? 'Start a runtime before configuring OpenCode.'
        : null,
    warnings:
      name === 'clients_restricted'
        ? ['The selected config disables this provider.']
        : [],
  };
  const fixture = fixtures.find(
    (f) => f.name === (menubar ? name.slice(8) : name),
  );
  function icon(s: PanelState) {
    return s.phase.kind === 'booting'
      ? 'boot_50'
      : s.phase.kind === 'no_config'
        ? 'setup'
        : s.phase.kind === 'stopping'
          ? 'stop'
          : s.phase.kind === 'failed'
            ? 'fail'
            : s.phase.kind;
  }
</script>

<div class="render" data-render-ready="true">
  {#if name === 'settings'}<Settings
      fixture={settings}
      rendering
    />{:else if ['clients', 'clients_off', 'clients_restricted'].includes(name)}<Settings
      fixture={{ ...settings, clients: clientsFixture }}
      initialTab="clients"
      rendering
    />{:else if fixture}{#if menubar}<div class="menubar">
        <img
          class:template={fixture.state.phase.kind === 'off'}
          alt=""
          src={`/tray/tray_${icon(fixture.state)}.png`}
        /><span>{fixture.state.menu_text}</span>
      </div>{:else}<Panel
        panel={fixture.state}
        nowMs={fixture.now_ms}
        rendering
      />{/if}{:else}<nav>
      {#each fixtures as f}<a href={`?view=render&state=${f.name}`}>{f.name}</a
        >{/each}<a href="?view=render&state=settings">settings</a>
      {#each ['clients', 'clients_off', 'clients_restricted'] as client}<a
          href={`?view=render&state=${client}`}>{client}</a
        >{/each}
    </nav>{/if}
</div>

<style>
  .menubar {
    background: #2a2a2e;
    padding: 3px 8px;
    display: flex;
    gap: 4px;
    align-items: center;
    color: white;
    font-size: 12px;
  }
  .menubar img {
    width: 16px;
    height: 16px;
  }
  .menubar img.template {
    filter: invert(0.6);
  }
  nav {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  a {
    color: var(--cyan);
  }
</style>
