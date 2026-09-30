import { expect, test } from 'vitest';
import {
  changes,
  loadFields,
  providerTargets,
  savedMessage,
  maskNew,
} from './settings';
import type { Readiness } from '../proto/Readiness';
import type { ConfigShow } from '../proto/ConfigShow';
test('new setup uses SSH and preserves a saved public connection', () => {
  const fresh = loadFields();
  expect(fresh.plain.LOBO_CONNECTION).toBe('ssh');
  expect(fresh.plain).not.toHaveProperty('LOBO_DOMAIN');
  expect(fresh.secrets).not.toHaveProperty('CF_TUNNEL_TOKEN');
  const legacy = {
    values: {},
    set: { LOBO_DOMAIN: true, CF_TUNNEL_TOKEN: true },
  } as unknown as ConfigShow;
  expect(loadFields(legacy).plain.LOBO_CONNECTION).toBe('cloudflare');
  legacy.values.LOBO_CONNECTION = 'ssh';
  expect(loadFields(legacy).plain.LOBO_CONNECTION).toBe('ssh');
});
test('only changed values, keep or remove secrets', () => {
  const f = loadFields();
  f.plain = { LOBO_DOMAIN: 'lobo.x.cc', LOBO_MIN_MBPS: '150', LOBO_CTX: '' };
  f.secrets = {
    RUNPOD_API_KEY: '',
    VASTAI_API_KEY: '-',
    CF_TUNNEL_TOKEN: ' new-token ',
  };
  expect(
    changes(f, {
      LOBO_DOMAIN: 'lobo.x.cc',
      LOBO_MIN_MBPS: '100',
      LOBO_CTX: '',
      VASTAI_API_KEY: '3f9c…c0de',
    }),
  ).toEqual({
    LOBO_MIN_MBPS: '150',
    VASTAI_API_KEY: '',
    CF_TUNNEL_TOKEN: 'new-token',
  });
  f.plain = {
    LOBO_WEIGHTS_DIR: ' /Volumes/Extreme/_lobocode ',
    LOBO_LOCAL_PORT: '8931',
  };
  f.secrets = {};
  expect(changes(f, { LOBO_LOCAL_PORT: '8931' })).toEqual({
    LOBO_WEIGHTS_DIR: '/Volumes/Extreme/_lobocode',
  });
});
test('provider targets and saved text', () => {
  const r = {
    local_supported: true,
    providers: ['runpod', 'vast'],
  } as Readiness;
  expect(providerTargets(r)).toEqual({
    options: ['local', 'runpod', 'vast'],
    def: 'runpod',
  });
  expect(providerTargets({ ...r, providers: [] })).toBeNull();
  expect(savedMessage(1)).toBe('saved 1 key');
  expect(savedMessage(3)).toBe('saved 3 keys');
  expect(maskNew('sk-0123456789abcdef')).toBe('sk-0…cdef  (new, unsaved)');
});
