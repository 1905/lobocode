import type { PanelState } from '../gen/PanelState';
import type { OpenCodeInfo } from '../gen/OpenCodeInfo';
import type { OpenCodeResult } from '../gen/OpenCodeResult';
import { bar, duration, gb, tps, usd } from './fmt';

export const phaseWord = (s: PanelState) =>
  ({
    loading: 'SCAN',
    no_config: 'SETUP',
    off: 'OFF',
    booting: 'BOOT',
    ready: 'RUN',
    stopping: 'STOP',
    failed: 'FAIL',
  })[s.phase.kind];
export const phaseTone = (s: PanelState) =>
  ({
    loading: 'cyan',
    no_config: 'amber',
    off: 'dim',
    booting: 'cyan',
    ready: 'green',
    stopping: 'cyan',
    failed: 'red',
  })[s.phase.kind];
export const active = (s: PanelState) =>
  ['loading', 'booting', 'stopping'].includes(s.phase.kind);
export function canStart(s: PanelState): boolean {
  return s.start_allowed === true && s.readiness?.cloud_ready === true;
}
export function headerDetail(s: PanelState): string {
  const p = s.snap?.pod;
  if (p)
    return [
      p.provider,
      p.detail,
      ...(p.cost_per_hr > 0 ? [`${usd(p.cost_per_hr)}/h`] : []),
    ]
      .filter(Boolean)
      .join(' · ');
  if (s.phase.kind === 'booting') return `renting ${s.provider}`;
  return s.phase.kind === 'off' ? 'no pod · $0.00/h' : '';
}
export function limits(values: Record<string, string>): string {
  const n = (key: string, fallback: number) =>
    Number(values[key]) > 0 ? Number(values[key]) : fallback;
  return `idle ${n('LOBO_IDLE_MIN', 30)}m max ${n('LOBO_MAX_HOURS', 12)}h`;
}
export function stepRows(s: PanelState) {
  return s.boot_steps.map((step) => {
    const mark = s.steps.find((m) => m.step === step);
    const label = step === 'download' ? 'verify model' : step;
    return {
      step,
      label,
      mark:
        step === s.current_step
          ? ('cur' as const)
          : mark
            ? ('ok' as const)
            : ('wait' as const),
      at: mark ? duration(mark.at_s) : undefined,
    };
  });
}
export function downloadLine(s: PanelState) {
  const d = s.download;
  if (!d || d.total <= 0)
    return s.up_phase === 'verify' || s.snap?.status?.stage === 'verify'
      ? { kind: 'verify' as const }
      : null;
  const raster = bar(d.bytes / d.total, 14);
  if (d.verifying) return { kind: 'sha' as const, bar: raster };
  const eta = d.mbps > 0 ? (d.total - d.bytes) / (d.mbps * 1e6) : 0;
  return {
    kind: 'bytes' as const,
    bar: raster,
    gb: `${gb(d.bytes)}/${gb(d.total)}G`,
    mbps: `${Math.trunc(d.mbps)}MB/s`,
    mbpsTone: d.mbps >= 100 ? 'green' : 'amber',
    eta: eta > 0 ? duration(eta) : undefined,
  };
}
export const bootElapsed = (s: PanelState, nowMs: number) =>
  `T+${duration(s.boot_start_ms === null ? 0 : (nowMs - s.boot_start_ms) / 1000)}`;
export function bootHealth(s: PanelState, nowMs: number) {
  if (s.last_update_ms == null || !Number.isFinite(s.last_update_ms)) {
    return { text: 'Waiting for a startup update', stale: true };
  }
  const age = Math.max(0, (nowMs - s.last_update_ms) / 1000);
  return age > 45
    ? { text: `No update for ${duration(age)} · status unknown`, stale: true }
    : { text: `Last update ${duration(age)} ago`, stale: false };
}
function elapsed(at: string | undefined, nowMs: number) {
  const t = Date.parse(at ?? '');
  return Number.isFinite(t) && t > 0 ? Math.max(0, (nowMs - t) / 1000) : 0;
}
export function ready(s: PanelState, nowMs: number) {
  const st = s.snap?.status,
    gpu = st?.gpu,
    processing = (st?.llama?.requests_processing ?? 0) > 0;
  const left = st
    ? Math.max(0, st.kill_in_s - (processing ? 0 : elapsed(s.snap?.at, nowMs)))
    : 0;
  const uptime = elapsed(s.snap?.pod?.started_at, nowMs);
  return {
    endpoint: s.endpoint ?? '?',
    apiKey: s.config?.values.LOBO_API_KEY ?? '?',
    gen: tps(st?.llama?.gen_tps),
    prompt: tps(st?.llama?.prompt_tps),
    mem: gpu
      ? {
          label: 'vram',
          bar: bar(gpu.vram_used_mb / Math.max(1, gpu.vram_total_mb), 12),
          text: `${(gpu.vram_used_mb / 1024).toFixed(1)}/${(gpu.vram_total_mb / 1024).toFixed(1)} GB`,
          gpu: { text: `gpu ${gpu.util_pct}%`, hot: gpu.util_pct > 0 },
        }
      : undefined,
    kill: st
      ? {
          label: 'idle-kill ',
          text: duration(left),
          warn: left < 300,
        }
      : undefined,
    uptime: duration(uptime),
    cost: s.snap?.pod ? usd((s.snap.pod.cost_per_hr * uptime) / 3600) : '',
  };
}
export function fail(s: PanelState) {
  return {
    message: s.phase.kind === 'failed' ? s.phase.message : '',
    tail: s.log_tail.slice(-5),
    primary: s.snap?.pod
      ? {
          label: `STOP ${s.snap.pod.provider}`,
          action: 'stop' as const,
          tone: 'red',
        }
      : { label: 'RETRY', action: 'start' as const, tone: 'green' },
  };
}
export const stopping = (s: PanelState) =>
  `deleting pod on ${s.snap?.pod?.provider ?? s.provider}`;

export type SettingsTab = 'cloud' | 'defaults' | 'clients';
export function settingsTab(value: unknown): SettingsTab | null {
  return ['cloud', 'defaults', 'clients'].includes(value as string)
    ? (value as SettingsTab)
    : null;
}
export type ClientsState = {
  makeDefault: boolean;
  busy: boolean;
  refreshing: boolean;
  chosenPath: string | undefined;
  info: OpenCodeInfo | null;
  result: OpenCodeResult | null;
  error: string;
  generation: number;
};
export function createClientsState(
  info: OpenCodeInfo | null = null,
): ClientsState {
  return {
    makeDefault: true,
    busy: false,
    refreshing: false,
    chosenPath: undefined,
    info,
    result: null,
    error: '',
    generation: 0,
  };
}
// Only the backend's static OpenCode errors are safe for display.
export function clientsFailure(
  action: 'load' | 'choose' | 'configure',
  error?: unknown,
): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'kind' in error &&
    error.kind === 'opencode' &&
    'message' in error &&
    typeof error.message === 'string' &&
    error.message.length > 0 &&
    error.message.length <= 240 &&
    !/[\x00-\x1f\x7f]/.test(error.message)
  )
    return error.message;
  return {
    load: 'Cannot read OpenCode setup. Refresh and retry.',
    choose:
      'Cannot choose that OpenCode config. Choose an existing JSON or JSONC file.',
    configure:
      'OpenCode setup failed. Check the config and running model, then retry.',
  }[action];
}
const boundedSetupText = (text: string) => text.slice(0, 240);
export function clientsView(s: ClientsState) {
  const i = s.info;
  return {
    path: s.chosenPath ?? i?.path ?? '',
    endpoint: i?.endpoint ?? 'No active endpoint',
    model:
      i?.provider && i.model_alias
        ? `${i.provider}/${i.model_alias}`
        : 'No running model',
    context:
      i?.context === null || i?.context === undefined
        ? null
        : `${i.context.toLocaleString('en-US')} context tokens`,
    disabled:
      s.busy || s.refreshing || !i?.can_configure || !(s.chosenPath ?? i?.path),
    reason: i?.reason ? boundedSetupText(i.reason) : '',
    warnings: [
      ...new Set([...(i?.warnings ?? []), ...(s.result?.warnings ?? [])]),
    ]
      .slice(0, 4)
      .map(boundedSetupText),
    result: s.result
      ? 'Saved. Restart OpenCode to reload. Project settings can override this file.'
      : '',
  };
}
export function clientsRuntimeKey(s: PanelState): string {
  const st = s.snap?.status;
  return JSON.stringify([
    s.phase.kind,
    s.provider,
    s.model,
    s.endpoint,
    s.snap?.pod?.provider,
    s.snap?.pod?.id,
    st?.boot_id,
    st?.model,
    st?.ctx,
    s.config?.path,
    s.config?.set.LOBO_API_KEY,
  ]);
}
export function invalidateClients(s: ClientsState) {
  s.generation++;
  s.refreshing = false;
  s.result = null;
  // Keep the target visible while making stale metadata unusable.
  if (s.info) s.info = { ...s.info, can_configure: false };
}
export async function refreshClients(
  s: ClientsState,
  load: (path?: string) => Promise<OpenCodeInfo>,
) {
  const generation = ++s.generation;
  s.refreshing = true;
  s.error = '';
  try {
    const info = await load(s.chosenPath);
    if (generation === s.generation) s.info = info;
  } catch (error) {
    if (generation === s.generation) {
      if (s.info) s.info = { ...s.info, can_configure: false };
      s.error = clientsFailure('load', error);
    }
  } finally {
    if (generation === s.generation) s.refreshing = false;
  }
}
export async function enterClients(
  s: ClientsState,
  load: (path?: string) => Promise<OpenCodeInfo>,
) {
  if (!s.busy && !s.refreshing) await refreshClients(s, load);
}
export async function chooseClients(
  s: ClientsState,
  choose: () => Promise<string | null>,
  load: (path?: string) => Promise<OpenCodeInfo>,
) {
  if (s.busy) return;
  s.busy = true;
  s.error = '';
  try {
    const path = await choose();
    if (path !== null) {
      s.chosenPath = path;
      s.result = null;
      await refreshClients(s, load);
    }
  } catch (error) {
    s.error = clientsFailure('choose', error);
  } finally {
    s.busy = false;
  }
}
export async function submitClients(
  s: ClientsState,
  configure: (path: string, makeDefault: boolean) => Promise<OpenCodeResult>,
) {
  if (clientsView(s).disabled) return;
  const path = clientsView(s).path,
    makeDefault = s.makeDefault,
    generation = s.generation;
  s.busy = true;
  s.error = '';
  s.result = null;
  try {
    const result = await configure(path, makeDefault);
    if (generation === s.generation) s.result = result;
  } catch (error) {
    if (generation === s.generation)
      s.error = clientsFailure('configure', error);
  } finally {
    s.busy = false;
  }
}
