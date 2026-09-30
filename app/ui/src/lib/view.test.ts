import { expect, test } from 'vitest';
import type { PanelState } from '../gen/PanelState';
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
  expect(view.limits({})).toBe('≥100MB/s idle 30m max 12h');
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
    kind: 'bytes',
    gb: '12.4/28.6G',
    mbps: '713MB/s',
    mbpsTone: 'green',
  });
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
