<script lang="ts">
  import { onMount } from 'svelte';
  import type { ConfigShow } from '../proto/ConfigShow';
  import type { Readiness } from '../proto/Readiness';
  import type { Listing } from '../proto/Listing';
  import { api, message, onState } from '../lib/api';
  import { gb } from '../lib/fmt';
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
  type Fixture = { config: ConfigShow; readiness: Readiness; models: Listing };
  let {
    fixture,
    rendering = false,
  }: { fixture?: Fixture; rendering?: boolean } = $props();
  let config = $state<ConfigShow>();
  let readiness = $state<Readiness>();
  let models = $state<Listing>();
  let fields = $state(loadFields());
  let saving = $state(false);
  let status = $state('');
  let tone = $state('dim');
  let free = $state<number | null>(null);
  let tab = $state('local');
  const targets = $derived(readiness ? providerTargets(readiness) : null);
  const plain = [
    ['LOBO_DOMAIN', 'domain', 'lobo.example.com'],
    ['LOBO_BUCKET_URL', 'bucket url', 'https://pub-….r2.dev'],
  ];
  const numeric = [
    ['LOBO_MIN_MBPS', 'min MB/s', '100'],
    ['LOBO_CTX', 'context', '65536'],
    ['LOBO_IDLE_MIN', 'idle min', '30'],
    ['LOBO_MAX_HOURS', 'max hours', '12'],
    ['LOBO_VAST_MAX_DPH', 'vast max $/h', '1.20'],
    ['LOBO_POD_IMAGE', 'pod image', 'ghcr.io/1905/lobocode@sha256:…'],
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
    models = await api.localModels().catch(() => undefined);
  }
  onMount(() => {
    let remove: undefined | (() => void),
      disposed = false;
    if (fixture) {
      config = fixture.config;
      readiness = fixture.readiness;
      models = fixture.models;
      fields = loadFields(config);
      free = models.free_bytes;
    } else if (!rendering) {
      void attempt(reload);
      void onState((s) => {
        readiness = s.readiness ?? undefined;
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
  $effect(() => {
    const path = fields.plain.LOBO_WEIGHTS_DIR?.trim() ?? '';
    const listing = models;
    if (rendering) {
      free = listing?.free_bytes ?? null;
      return;
    }
    if (!path || path === listing?.weights) {
      free = listing?.free_bytes ?? null;
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      void api
        .freeBytes(path)
        .then((n) => {
          if (!cancelled) free = n;
        })
        .catch(() => {
          if (!cancelled) free = null;
        });
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });
  async function save() {
    if (rendering || saving) return;
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
  async function choose() {
    const path = await api.chooseWeights(
      fields.plain.LOBO_WEIGHTS_DIR.trim() || models?.weights || '',
    );
    if (path) fields.plain.LOBO_WEIGHTS_DIR = path;
  }
  function keydown(e: KeyboardEvent) {
    if (e.metaKey && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void save();
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
    <RasterBar active={saving} />
  </div>
  <div class="box file">
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
  </div>
  <div class="tabs" role="tablist" aria-label="Settings">
    {#each [['local', 'Local'], ['cloud', 'Cloud'], ['defaults', 'Defaults']] as [id, label]}
      <button
        role="tab"
        id={`tab-${id}`}
        aria-selected={tab === id}
        aria-controls={`section-${id}`}
        onclick={() => (tab = id)}>{label}</button
      >
    {/each}
  </div>
  {#if tab === 'cloud'}
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
        <label class="field"
          ><span>{plain[0][1]}</span><input
            aria-label={plain[0][1]}
            placeholder={plain[0][2]}
            bind:value={fields.plain.LOBO_DOMAIN}
            disabled={saving}
            spellcheck="false"
          /></label
        >
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
          ><span>tunnel token</span><input
            aria-label="tunnel token"
            type="password"
            placeholder={secretHint(config, 'CF_TUNNEL_TOKEN')}
            bind:value={fields.secrets.CF_TUNNEL_TOKEN}
            autocomplete="off"
            spellcheck="false"
            disabled={saving}
          /></label
        >
        <label class="field"
          ><span>{plain[1][1]}</span><input
            aria-label={plain[1][1]}
            placeholder={plain[1][2]}
            bind:value={fields.plain.LOBO_BUCKET_URL}
            disabled={saving}
            spellcheck="false"
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
        <h2 class="copper">// defaults for lobo up (empty = built-in)</h2>
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
            {#each ['q8', 'q6'] as option}<button
                class="pick"
                class:green={pickerValue(fields, 'LOBO_MODEL', 'q8') === option}
                aria-pressed={pickerValue(fields, 'LOBO_MODEL', 'q8') ===
                  option}
                onclick={() => (fields.plain.LOBO_MODEL = option)}
                disabled={saving}
                >{pickerValue(fields, 'LOBO_MODEL', 'q8') === option
                  ? `[${option}]`
                  : ` ${option} `}</button
              >{/each}
          </div>
        </div>
        {#each numeric.slice(0, 4) as [key, label, placeholder]}<label
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
        {#each numeric.slice(4) as [key, label, placeholder]}<label
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
  {:else}
    <div
      class="tab-content"
      role="tabpanel"
      id="section-local"
      aria-labelledby="tab-local"
    >
      {#if readiness?.local_supported}<section>
          <h2 class="copper">// local</h2>
          <div class="field">
            <label class="dim" for="weights">weights</label>
            <div class="row grow">
              <input
                id="weights"
                class="grow"
                aria-label="weights"
                placeholder={models?.weights ??
                  '~/Library/Application Support/lobo/weights'}
                bind:value={fields.plain.LOBO_WEIGHTS_DIR}
                disabled={saving}
                spellcheck="false"
              /><LinkButton
                label="[choose…]"
                tone="cyan"
                disabled={saving}
                onclick={() => {
                  if (!rendering) void attempt(choose);
                }}
              />
            </div>
          </div>
          {#if free !== null}<p class="small dim free">
              {gb(free)} GB free
            </p>{/if}<label class="field"
            ><span>port</span><input
              aria-label="port"
              placeholder="8931"
              bind:value={fields.plain.LOBO_LOCAL_PORT}
              disabled={saving}
            /></label
          >
        </section>{:else}<p class="dim">
          Local models require Apple Silicon.
        </p>{/if}
    </div>
  {/if}
  <div class="row spread actions">
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
  </div>
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
  .field > span:first-child,
  .field > label:first-child {
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
  .free {
    margin-left: 118px;
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
