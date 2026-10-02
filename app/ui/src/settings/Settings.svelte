<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { PanelState } from '../gen/PanelState';
  import type { OpenCodeInfo } from '../gen/OpenCodeInfo';
  import type { ConfigShow } from '../proto/ConfigShow';
  import type { Readiness } from '../proto/Readiness';
  import { api, message, onState, onSettingsTab } from '../lib/api';
  import {
    loadFields,
    changes,
    secretHint,
    providerTargets,
    savedMessage,
    maskNew,
    pickerValue,
  } from '../lib/settings';
  import Logo from '../widgets/Logo.svelte';
  import RasterBar from '../widgets/RasterBar.svelte';
  import BracketButton from '../widgets/BracketButton.svelte';
  import LinkButton from '../widgets/LinkButton.svelte';
  import Clients from './Clients.svelte';
  import {
    createClientsState,
    clientsRuntimeKey,
    invalidateClients,
    refreshClients,
    enterClients,
    chooseClients,
    submitClients,
    settingsTab,
    type SettingsTab,
  } from '../lib/view';
  type Fixture = {
    config: ConfigShow;
    readiness: Readiness;
    clients?: OpenCodeInfo;
  };
  let {
    fixture,
    rendering = false,
    initialTab = 'cloud',
  }: {
    fixture?: Fixture;
    rendering?: boolean;
    initialTab?: SettingsTab;
  } = $props();
  let config = $state<ConfigShow>();
  let readiness = $state<Readiness>();
  let fields = $state(loadFields());
  let saving = $state(false);
  let status = $state('');
  let tone = $state('dim');
  let tab = $state<SettingsTab>(untrack(() => initialTab));
  const clients = $state(
    createClientsState(untrack(() => fixture?.clients ?? null)),
  );
  let runtimeKey: string | undefined;
  function updateRuntime(state: PanelState) {
    const next = clientsRuntimeKey(state);
    if (next === runtimeKey) return;
    runtimeKey = next;
    invalidateClients(clients);
    if (tab === 'clients' && !rendering)
      void refreshClients(clients, api.opencodeInfo);
  }
  function selectTab(value: unknown) {
    const next = settingsTab(value);
    if (next) tab = next;
  }
  $effect(() => {
    if (tab === 'clients' && !rendering)
      untrack(() => {
        void enterClients(clients, api.opencodeInfo);
      });
  });
  const targets = $derived(readiness ? providerTargets(readiness) : null);
  const numeric = [
    ['LOBO_CTX', 'context', '65536'],
    ['LOBO_IDLE_MIN', 'idle min', '30'],
    ['LOBO_MAX_HOURS', 'max hours', '12'],
    ['LOBO_VAST_MAX_DPH', 'vast max $/h', '1.20'],
  ];
  async function attempt(f: () => Promise<unknown>) {
    try {
      await f();
    } catch (e) {
      status = message(e);
      tone = 'red';
    }
  }
  async function reload() {
    config = await api.configShow();
    fields = loadFields(config);
    const state = await api.getState();
    readiness = state.readiness ?? undefined;
    updateRuntime(state);
  }
  onMount(() => {
    let remove: undefined | (() => void),
      disposed = false;
    if (fixture) {
      config = fixture.config;
      readiness = fixture.readiness;
      fields = loadFields(config);
    } else if (!rendering) {
      void attempt(reload);
      void onState((s) => {
        readiness = s.readiness ?? undefined;
        updateRuntime(s);
      })
        .then((f) => {
          if (disposed) f();
          else remove = f;
        })
        .catch((e) => {
          status = message(e);
          tone = 'red';
        });
    }
    return () => {
      disposed = true;
      remove?.();
    };
  });
  onMount(() => {
    if (rendering) return;
    let disposed = false,
      remove: undefined | (() => void);
    let consuming = false,
      again = false;
    async function consumeTab() {
      again = true;
      if (consuming) return;
      consuming = true;
      try {
        while (again && !disposed) {
          again = false;
          const requested = await api.consumeSettingsTab();
          if (!disposed) selectTab(requested);
        }
      } catch {
        if (!disposed) {
          status = 'Cannot open the requested settings tab. Choose it above.';
          tone = 'red';
        }
      } finally {
        consuming = false;
      }
    }
    void onSettingsTab(() => {
      void consumeTab();
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        remove = unlisten;
        // Subscribe first, then consume requests made while this webview loaded.
        void consumeTab();
      })
      .catch(() => {
        if (!disposed) {
          status = 'Cannot receive settings shortcuts. Choose a tab above.';
          tone = 'red';
        }
      });
    return () => {
      disposed = true;
      remove?.();
    };
  });
  async function save() {
    if (rendering || saving || tab === 'clients') return;
    saving = true;
    status = '';
    try {
      if (!config?.set.LOBO_API_KEY && !fields.newApiKey)
        fields.newApiKey = await api.genApiKey();
      const set = changes(fields, config?.values ?? {}),
        n = Object.keys(set).length;
      if (!n) {
        status = 'nothing changed';
        tone = 'dim';
        return;
      }
      await api.configSave(set);
      await reload();
      status = savedMessage(n);
      tone = 'green';
    } catch (e) {
      status =
        typeof e === 'object' &&
        e !== null &&
        'kind' in e &&
        e.kind === 'invalid'
          ? message(e)
          : `save failed: ${message(e)}`;
      tone = 'red';
    } finally {
      saving = false;
    }
  }
  function keydown(e: KeyboardEvent) {
    if (e.metaKey && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (tab !== 'clients') void save();
    } else if (e.metaKey && e.key === 'q') {
      e.preventDefault();
      void attempt(api.quit);
    }
  }
</script>

<svelte:window onkeydown={keydown} />
<main class="settings">
  <div class="stack title" data-tauri-drag-region>
    <div class="row spread title-row">
      <Logo /><span class="small dim">config</span>
    </div>
    <RasterBar active={saving || clients.busy || clients.refreshing} />
  </div>
  {#if tab !== 'clients'}<div class="box file">
      <p class="selectable clip" title={config?.path}>
        {config?.path ?? '~/.config/lobo/config.env'}
      </p>
      <div class="row">
        <LinkButton
          label="reveal in finder"
          tone="cyan"
          onclick={() => {
            if (!rendering) void attempt(api.revealConfig);
          }}
        /><LinkButton
          label="open in editor"
          tone="cyan"
          disabled={!config?.exists}
          onclick={() => {
            if (!rendering) void attempt(api.openConfig);
          }}
        />
      </div>
    </div>{/if}
  <div class="tabs" role="tablist" aria-label="Settings">
    {#each [['cloud', 'Cloud'], ['defaults', 'Defaults'], ['clients', 'Clients']] as [id, label]}
      <button
        role="tab"
        id={`tab-${id}`}
        aria-selected={tab === id}
        aria-controls={`section-${id}`}
        onclick={() => selectTab(id)}>{label}</button
      >
    {/each}
  </div>
  {#if tab === 'clients'}
    <div
      class="tab-content"
      role="tabpanel"
      id="section-clients"
      aria-labelledby="tab-clients"
    >
      <Clients
        state={clients}
        refresh={() => {
          if (!rendering && !clients.busy)
            void refreshClients(clients, api.opencodeInfo);
        }}
        choose={() => {
          if (!rendering)
            void chooseClients(
              clients,
              api.chooseOpencodeConfig,
              api.opencodeInfo,
            );
        }}
        configure={() => {
          if (!rendering) void submitClients(clients, api.configureOpencode);
        }}
      />
    </div>
  {:else if tab === 'cloud'}
    <div
      class="tab-content"
      role="tabpanel"
      id="section-cloud"
      aria-labelledby="tab-cloud"
    >
      <section>
        <h2 class="copper">// providers (one is enough)</h2>
        {#each [['RUNPOD_API_KEY', 'runpod key'], ['VASTAI_API_KEY', 'vast key']] as [key, label]}<label
            class="field"
            ><span>{label}</span><input
              aria-label={label}
              type="password"
              placeholder={secretHint(config, key)}
              bind:value={fields.secrets[key]}
              autocomplete="off"
              spellcheck="false"
              disabled={saving}
            /></label
          >{/each}
      </section>
      <section>
        <h2 class="copper">// access</h2>
        <div class="field">
          <span>connection</span><span class="small"
            >{fields.plain.LOBO_CONNECTION === 'cloudflare'
              ? 'Existing public tunnel'
              : 'Private · encrypted SSH'}</span
          >
        </div>
        {#if fields.plain.LOBO_CONNECTION === 'cloudflare'}
          <LinkButton
            label="use private connection"
            tone="cyan"
            disabled={saving}
            onclick={() => (fields.plain.LOBO_CONNECTION = 'ssh')}
          />
        {:else}
          <p class="small dim">No domain or tunnel account required.</p>
        {/if}
        <div class="field">
          <span class="dim">api key</span>
          <div class="row grow">
            <span
              class:amber={Boolean(fields.newApiKey)}
              class="small clip grow"
              >{fields.newApiKey
                ? maskNew(fields.newApiKey)
                : (config?.values.LOBO_API_KEY ?? 'not set')}</span
            ><LinkButton
              label="generate"
              tone="cyan"
              disabled={saving}
              onclick={() => {
                if (!rendering)
                  void attempt(async () => {
                    fields.newApiKey = await api.genApiKey();
                  });
              }}
            />
          </div>
        </div>
        <label class="field"
          ><span>cloud port</span><input
            aria-label="cloud port"
            placeholder="8933"
            bind:value={fields.plain.LOBO_CLOUD_PORT}
            disabled={saving}
          /></label
        >
      </section>
    </div>
  {:else if tab === 'defaults'}
    <div
      class="tab-content"
      role="tabpanel"
      id="section-defaults"
      aria-labelledby="tab-defaults"
    >
      <section>
        <h2 class="copper">// cloud defaults (empty = built-in)</h2>
        {#if targets}<div class="field">
            <span class="dim">provider</span>
            <div class="row">
              {#each targets.options as option}<button
                  class="pick"
                  class:green={pickerValue(
                    fields,
                    'LOBO_PROVIDER',
                    targets.def,
                  ) === option}
                  aria-pressed={pickerValue(
                    fields,
                    'LOBO_PROVIDER',
                    targets.def,
                  ) === option}
                  onclick={() => (fields.plain.LOBO_PROVIDER = option)}
                  disabled={saving}
                  >{pickerValue(fields, 'LOBO_PROVIDER', targets.def) === option
                    ? `[${option}]`
                    : ` ${option} `}</button
                >{/each}
            </div>
          </div>{/if}
        <div class="field">
          <span class="dim">model</span>
          <div class="row">
            {#each ['q6', 'q8'] as option}<button
                class="pick"
                class:green={pickerValue(fields, 'LOBO_MODEL', 'q6') === option}
                aria-pressed={pickerValue(fields, 'LOBO_MODEL', 'q6') ===
                  option}
                onclick={() => (fields.plain.LOBO_MODEL = option)}
                disabled={saving}
                >{pickerValue(fields, 'LOBO_MODEL', 'q6') === option
                  ? `[${option}]`
                  : ` ${option} `}</button
              >{/each}
          </div>
        </div>
        {#each numeric.slice(0, 3) as [key, label, placeholder]}<label
            class="field"
            ><span>{label}</span><input
              aria-label={label}
              {placeholder}
              bind:value={fields.plain[key]}
              disabled={saving}
            /></label
          >{/each}
        <div class="field">
          <span class="dim">runpod cloud</span>
          <div class="row">
            {#each ['community', 'secure'] as option}<button
                class="pick"
                class:green={pickerValue(fields, 'LOBO_CLOUD', 'community') ===
                  option}
                aria-pressed={pickerValue(fields, 'LOBO_CLOUD', 'community') ===
                  option}
                onclick={() => (fields.plain.LOBO_CLOUD = option)}
                disabled={saving}
                >{pickerValue(fields, 'LOBO_CLOUD', 'community') === option
                  ? `[${option}]`
                  : ` ${option} `}</button
              >{/each}
          </div>
        </div>
        {#each numeric.slice(3) as [key, label, placeholder]}<label
            class="field"
            ><span>{label}</span><input
              aria-label={label}
              {placeholder}
              bind:value={fields.plain[key]}
              disabled={saving}
              spellcheck="false"
            /></label
          >{/each}
      </section>
    </div>
  {/if}
  {#if tab !== 'clients'}<div class="row spread actions">
      <p class={`small ${tone}`} role="status">{status}</p>
      <div class="row">
        <BracketButton
          label="REVERT"
          tone="dim"
          disabled={saving}
          onclick={() => {
            fields = loadFields(config);
            status = '';
          }}
        /><BracketButton
          label="SAVE"
          disabled={saving}
          onclick={() => {
            void save();
          }}
        />
      </div>
    </div>{/if}
</main>

<style>
  .settings {
    width: 520px;
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .title {
    gap: 18px;
  }
  .title :global(*) {
    pointer-events: none;
  }
  .title-row {
    align-items: flex-end;
  }
  .file {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .file .row {
    gap: 14px;
  }
  .tabs {
    display: flex;
    padding: 3px;
    gap: 3px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--card);
  }
  .tabs button {
    flex: 1;
    padding: 7px 12px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--dim);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 12px;
  }
  .tabs button[aria-selected='true'] {
    background: var(--line);
    color: var(--text);
  }
  .tab-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 280px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h2 {
    width: fit-content;
    margin: 0;
    font-size: 11px;
    white-space: pre;
    font-weight: 700;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .field > span:first-child {
    width: 110px;
    flex: none;
    color: var(--dim);
    font-size: 11px;
  }
  input {
    min-width: 0;
    flex: 1;
    width: 0;
    padding: 4px 8px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--text);
    font-size: 12px;
  }
  input::placeholder {
    color: var(--faint);
  }
  .pick {
    background: none;
    border: 0;
    padding: 0;
    color: var(--dim);
    white-space: pre;
  }
  .pick.green {
    color: var(--green);
  }
  .pick:hover {
    color: var(--text);
  }
  .actions {
    align-items: flex-start;
    gap: 8px;
  }
  .actions > p {
    flex: 1;
    overflow-wrap: anywhere;
  }
</style>
