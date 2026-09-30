import type { ConfigShow } from '../proto/ConfigShow';
import type { Readiness } from '../proto/Readiness';
export const PLAIN_KEYS = [
  'LOBO_CONNECTION',
  'LOBO_CLOUD_PORT',
  'LOBO_BUCKET_URL',
  'LOBO_MIN_MBPS',
  'LOBO_CTX',
  'LOBO_IDLE_MIN',
  'LOBO_MAX_HOURS',
  'LOBO_VAST_MAX_DPH',
  'LOBO_POD_IMAGE',
  'LOBO_WEIGHTS_DIR',
  'LOBO_LOCAL_PORT',
  'LOBO_PROVIDER',
  'LOBO_MODEL',
  'LOBO_CLOUD',
];
export const SECRET_KEYS = ['RUNPOD_API_KEY', 'VASTAI_API_KEY'];
export type Fields = {
  secrets: Record<string, string>;
  plain: Record<string, string>;
  newApiKey?: string;
};
export function loadFields(c?: ConfigShow): Fields {
  return {
    secrets: Object.fromEntries(SECRET_KEYS.map((k) => [k, ''])),
    plain: Object.fromEntries(
      PLAIN_KEYS.map((k) => [
        k,
        c?.values[k] ||
          (k === 'LOBO_CONNECTION'
            ? c?.set.LOBO_DOMAIN && c?.set.CF_TUNNEL_TOKEN
              ? 'cloudflare'
              : 'ssh'
            : ''),
      ]),
    ),
  };
}
export function changes(f: Fields, current: Record<string, string>) {
  const set: Record<string, string> = {};
  for (const [k, v] of Object.entries(f.plain)) {
    const trimmed = v.trim();
    if (trimmed !== (current[k] ?? '')) set[k] = trimmed;
  }
  for (const [k, v] of Object.entries(f.secrets)) {
    const trimmed = v.trim();
    if (trimmed === '-') set[k] = '';
    else if (trimmed) set[k] = trimmed;
  }
  if (f.newApiKey) set.LOBO_API_KEY = f.newApiKey;
  return set;
}
export const secretHint = (c: ConfigShow | undefined, key: string) =>
  c?.values[key] ? `${c.values[key]}  (empty = keep, - = remove)` : 'not set';
export function providerTargets(r: Readiness) {
  const options = [...(r.local_supported ? ['local'] : []), ...r.providers];
  return options.length > 1
    ? { options, def: options.includes('runpod') ? 'runpod' : options[0] }
    : null;
}
export const pickerValue = (f: Fields, key: string, def: string) =>
  f.plain[key] || def;
export const savedMessage = (n: number) =>
  `saved ${n} key${n === 1 ? '' : 's'}`;
export const maskNew = (key: string) =>
  `${key.length < 12 ? '••••' : `${key.slice(0, 4)}…${key.slice(-4)}`}  (new, unsaved)`;
