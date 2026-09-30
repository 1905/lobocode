import { expect, test } from 'vitest';
import {
  changes,
  loadFields,
  providerTargets,
  savedMessage,
  maskNew,
} from './settings';
import type { Readiness } from '../proto/Readiness';
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
