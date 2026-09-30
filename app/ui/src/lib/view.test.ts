import { expect, test } from 'vitest';
import type { PanelState } from '../gen/PanelState';
import type { LocalMemory } from '../gen/LocalMemory';
import type { OpenCodeInfo } from '../gen/OpenCodeInfo';
import type { OpenCodeResult } from '../gen/OpenCodeResult';
import * as view from './view';
import cases from '../fixtures/time_cases.json';
const fixtures = import.meta.glob<{
  name: string;
  now_ms: number;
  state: PanelState;
}>('../fixtures/panel_*.json', { eager: true, import: 'default' });
const f = (name: string) =>
  Object.values(fixtures).find((f) => f.name === name)!;
test('header, limits and model rows', () => {
  expect(view.headerDetail(f('boot').state)).toBe(
    'vast · offer 51401937, 18877 Mbps down, California, US · $0.73/h',
  );
  expect(view.headerDetail(f('off').state)).toBe('no pod · $0.00/h');
  expect(view.headerDetail(f('off_local').state)).toBe('');
  expect(view.limits({})).toBe('idle 30m max 12h');
  expect(view.limits({ LOBO_IDLE_MIN: '0' })).toContain('idle 30m');
  const rows = view.modelRows(f('off_local').state);
  expect(rows[0]).toMatchObject({
    id: 'q6',
    picked: true,
    state: 'on',
    size: '22.1 GB',
  });
  expect(rows[1]).toMatchObject({ id: 'q8', state: 'partial', pct: 43 });
});
test('boot steps and each download state', () => {
  expect(view.stepRows(f('boot').state).map((s) => s.mark)).toEqual([
    'ok',
    'ok',
    'ok',
    'ok',
    'cur',
    'wait',
    'wait',
  ]);
  expect(view.downloadLine(f('boot').state)).toMatchObject({
    kind: 'sha',
  });
  expect(
    view.stepRows(f('boot').state).find((s) => s.step === 'download')?.label,
  ).toBe('verify model');
  expect(view.downloadLine(f('boot_local').state)).toMatchObject({
    kind: 'bytes',
    mbps: '88MB/s',
    mbpsTone: 'text',
  });
  expect(view.downloadLine(f('verify_local').state)).toEqual({
    kind: 'verify',
  });
  expect(view.downloadLine(f('boot_verify_sha').state)).toMatchObject({
    kind: 'sha',
  });
  expect(view.bootElapsed(f('boot').state, f('boot').now_ms)).toBe('T+1:12');
});
test('ready metrics, local differences and cost', () => {
  const v = view.ready(f('ready').state, f('ready').now_ms);
  expect(v).toMatchObject({
    gen: '45.1',
    prompt: '504',
    cost: '$1.60',
    uptime: '2:19:02',
  });
  expect(v.mem).toMatchObject({
    label: 'vram',
    text: '28.6/31.8 GB',
    gpu: { text: 'gpu 87%', hot: true },
  });
  const local = view.ready(f('ready_local').state, f('ready_local').now_ms);
  expect(local.mem?.label).toBe('memory');
  expect(local.mem?.gpu).toBeUndefined();
  expect(local.cost).toBe('local · $0');
  expect(local.kill?.label).toBe('idle-stop ');
  expect(
    view.ready(f('ready_kill_soon').state, f('ready_kill_soon').now_ms).kill
      ?.warn,
  ).toBe(true);
});
test('shared countdown cases and failure action', () => {
  for (const c of cases.kill_left) {
    const fixture = f('ready');
    const s = structuredClone(fixture.state);
    s.snap!.status!.kill_in_s = c.kill_in_s;
    s.snap!.status!.llama!.requests_processing = c.processing;
    expect(
      view.ready(s, fixture.now_ms + c.since_snap_s * 1000).kill?.text,
    ).toBe(c.left_s === 1600 ? '26:40' : c.left_s === 1634 ? '27:14' : '0:00');
  }
  expect(view.fail(f('fail_pod').state).primary).toEqual({
    label: 'STOP runpod',
    action: 'stop',
    tone: 'red',
  });
  expect(view.fail(f('fail').state).primary.label).toBe('RETRY');
});

test('local Start requires a current complete memory pass; Cloud is independent', () => {
  const s = structuredClone(f('off_local').state);
  s.local_memory = null;
  expect(view.canStart(s)).toBe(false);
  expect(view.memoryView(s).status).toBe('checking');
  const memory: LocalMemory = {
    model: s.model,
    ctx: 8192,
    status: 'ready',
    message: 'q6 · 8192 context tokens',
    required_bytes: 26 * 2 ** 30,
    budget_bytes: 28 * 2 ** 30,
    total_bytes: 64 * 2 ** 30,
    available_bytes: 32 * 2 ** 30,
    metal_limit_bytes: 48 * 2 ** 30,
  };
  s.local_memory = memory;
  expect(view.canStart(s)).toBe(true);
  expect(view.memoryView(s).values).toBe('26.0 GiB required · 28.0 GiB budget');
  s.local_memory = { ...memory, status: 'insufficient' };
  expect(view.canStart(s)).toBe(false);
  s.local_memory = { ...memory, model: 'other' };
  expect(view.canStart(s)).toBe(false);
  expect(view.memoryView(s).status).toBe('checking');
  s.local_memory = { ...memory, required_bytes: null };
  expect(view.canStart(s)).toBe(false);
  s.target = 'cloud';
  expect(view.canStart(s)).toBe(true);
});

test('unavailable memory preserves the error and never invents byte values', () => {
  const s = structuredClone(f('fail_local').state);
  const message = 'Cannot assess Mac memory: '.repeat(40);
  s.local_memory = {
    model: s.model,
    ctx: 8192,
    status: 'unavailable',
    message,
    total_bytes: null,
    available_bytes: null,
    metal_limit_bytes: null,
    required_bytes: null,
    budget_bytes: null,
  };
  expect(view.memoryView(s)).toMatchObject({
    status: 'unavailable',
    message,
    values: null,
  });
  expect(view.canStart(s)).toBe(false);
});

const openCodeInfo: OpenCodeInfo = {
  path: '/example/config/opencode.jsonc',
  endpoint: 'http://127.0.0.1:8931/v1',
  provider: 'lobo-local',
  model_alias: 'running-q6',
  context: 8192,
  can_configure: true,
  reason: null,
  warnings: [],
};
const openCodeResult: OpenCodeResult = {
  path: openCodeInfo.path,
  provider: 'lobo-local',
  model_alias: 'running-q6',
  changed: true,
  message: 'Saved.',
  warnings: [],
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
test('Clients starts checked and preserves explicit uncheck and chosen path across refresh', async () => {
  const state = view.createClientsState(openCodeInfo);
  expect(state.makeDefault).toBe(true);
  state.makeDefault = false;
  const path = '/example/custom chosen config.jsonc';
  await view.chooseClients(
    state,
    async () => path,
    async (selected) => {
      expect(selected).toBe(path);
      return { ...openCodeInfo, path: selected! };
    },
  );
  await view.refreshClients(state, async (selected) => {
    expect(selected).toBe(path);
    return {
      ...openCodeInfo,
      path: selected!,
      model_alias: 'actual-running-q8',
    };
  });
  expect(state.makeDefault).toBe(false);
  expect(view.clientsView(state)).toMatchObject({
    path,
    model: 'lobo-local/actual-running-q8',
    endpoint: 'http://127.0.0.1:8931/v1',
    context: '8,192 context tokens',
    disabled: false,
  });
  await view.chooseClients(
    state,
    async () => null,
    async () => {
      throw new Error('cancel must not load');
    },
  );
  expect(view.clientsView(state).path).toBe(path);
  expect(state.makeDefault).toBe(false);
});
test('Clients captures submission values and blocks immediate duplicate submissions', async () => {
  const state = view.createClientsState(openCodeInfo);
  state.makeDefault = false;
  const pending = deferred<OpenCodeResult>();
  const calls: [string, boolean][] = [];
  const configure = (path: string, makeDefault: boolean) => {
    calls.push([path, makeDefault]);
    return pending.promise;
  };
  const first = view.submitClients(state, configure);
  state.makeDefault = true;
  const second = view.submitClients(state, configure);
  expect(state.busy).toBe(true);
  expect(view.clientsView(state).disabled).toBe(true);
  expect(calls).toEqual([[openCodeInfo.path, false]]);
  pending.resolve(openCodeResult);
  await Promise.all([first, second]);
  expect(state.busy).toBe(false);
  expect(view.clientsView(state).result).toBe(
    'Saved. Restart OpenCode to reload. Project settings can override this file.',
  );
});
test('Clients rejects unsafe error text, clears busy and retains restrictions after success', async () => {
  const state = view.createClientsState({
    ...openCodeInfo,
    warnings: ['This provider is disabled.'],
  });
  await view.submitClients(state, async () => {
    throw { message: 'secret-key ' + 'x'.repeat(5000) };
  });
  expect(state.busy).toBe(false);
  expect(state.error).toBe(view.clientsFailure('configure'));
  expect(state.error.length).toBeLessThan(160);
  expect(state.error).not.toContain('secret-key');
  expect(
    view.clientsFailure('configure', {
      kind: 'opencode',
      message: 'x'.repeat(5000),
    }),
  ).toBe(view.clientsFailure('configure'));
  await view.submitClients(state, async () => {
    throw {
      kind: 'opencode',
      message: 'Runtime authentication failed. Check the API key.',
    };
  });
  expect(state.error).toBe('Runtime authentication failed. Check the API key.');
  await view.submitClients(state, async () => ({
    ...openCodeResult,
    changed: false,
    warnings: ['This provider is disabled.'],
  }));
  expect(view.clientsView(state).warnings).toEqual([
    'This provider is disabled.',
  ]);
  expect(view.clientsView(state).result).toContain(
    'Project settings can override this file.',
  );
});
test('re-entering Clients during Configure keeps the operation and success visible', async () => {
  const state = view.createClientsState(openCodeInfo);
  state.makeDefault = false;
  const pending = deferred<OpenCodeResult>();
  const submit = view.submitClients(state, () => pending.promise);
  let loads = 0;
  await view.enterClients(state, async () => {
    loads++;
    return openCodeInfo;
  });
  expect(loads).toBe(0);
  expect(state.busy).toBe(true);
  expect(state.makeDefault).toBe(false);
  pending.resolve(openCodeResult);
  await submit;
  expect(state.result).toEqual(openCodeResult);
  expect(view.clientsView(state).result).toContain('Saved. Restart OpenCode');
});
test('Clients disables setup without Ready metadata and ignores stale refresh results', async () => {
  const state = view.createClientsState({
    ...openCodeInfo,
    can_configure: false,
    reason: 'Start a runtime first.',
  });
  let calls = 0;
  await view.submitClients(state, async () => {
    calls++;
    return openCodeResult;
  });
  expect(calls).toBe(0);
  expect(view.clientsView(state).path).toBe(openCodeInfo.path);
  expect(view.clientsView(state).reason).toBe('Start a runtime first.');
  const old = deferred<OpenCodeInfo>();
  const first = view.refreshClients(state, () => old.promise);
  expect(view.clientsView(state).disabled).toBe(true);
  await view.refreshClients(state, async () => ({
    ...openCodeInfo,
    model_alias: 'new-runtime',
  }));
  old.resolve(openCodeInfo);
  await first;
  expect(view.clientsView(state).model).toBe('lobo-local/new-runtime');
  expect(state.refreshing).toBe(false);
});
test('a valid chosen file works after invalid default discovery', async () => {
  const state = view.createClientsState();
  await view.refreshClients(state, async () => {
    throw new Error('invalid default');
  });
  expect(view.clientsView(state).disabled).toBe(true);
  const path = '/example/valid/opencode.json';
  await view.chooseClients(
    state,
    async () => path,
    async (selected) => {
      expect(selected).toBe(path);
      return { ...openCodeInfo, path };
    },
  );
  expect(view.clientsView(state)).toMatchObject({ path, disabled: false });
  expect(state.error).toBe('');
});
test('runtime changes invalidate in-flight success; telemetry ticks do not change Clients runtime key', async () => {
  const state = view.createClientsState(openCodeInfo);
  const pending = deferred<OpenCodeResult>();
  const submit = view.submitClients(state, () => pending.promise);
  view.invalidateClients(state);
  pending.resolve(openCodeResult);
  await submit;
  expect(state.result).toBeNull();
  expect(view.clientsView(state).disabled).toBe(true);
  const runtime = structuredClone(f('ready_local').state);
  const key = view.clientsRuntimeKey(runtime);
  runtime.snap!.at = '2030-01-01T00:00:00Z';
  runtime.snap!.status!.idle_s++;
  runtime.snap!.status!.llama!.gen_tps++;
  expect(view.clientsRuntimeKey(runtime)).toBe(key);
  runtime.snap!.status!.boot_id = 'replacement-boot';
  expect(view.clientsRuntimeKey(runtime)).not.toBe(key);
});
test('Clients bounds reason and warnings without truncating the submitted file path', () => {
  const path = `/example/${'long-folder/'.repeat(60)}opencode.jsonc`;
  const state = view.createClientsState({
    ...openCodeInfo,
    path,
    reason: 'x'.repeat(1000),
    warnings: Array.from({ length: 8 }, (_, i) => `${i}${'x'.repeat(1000)}`),
  });
  expect(view.clientsView(state).path).toBe(path);
  expect(view.clientsView(state).reason.length).toBe(240);
  expect(view.clientsView(state).warnings).toHaveLength(4);
  expect(view.clientsView(state).warnings.every((w) => w.length <= 240)).toBe(
    true,
  );
  expect(view.settingsTab('clients')).toBe('clients');
  expect(view.settingsTab('unexpected')).toBeNull();
});
