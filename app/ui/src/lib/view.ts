import type { PanelState } from '../gen/PanelState';
import type { Step } from '../gen/Step';
import type { Listing } from '../proto/Listing';
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
  if (s.target === 'cloud') return s.readiness?.cloud_ready === true;
  const m = s.local_memory;
  return (
    !!m &&
    m.model === s.model &&
    m.status === 'ready' &&
    m.required_bytes !== null &&
    m.budget_bytes !== null &&
    m.required_bytes <= m.budget_bytes
  );
}
export function memoryView(s: PanelState) {
  const m = s.local_memory?.model === s.model ? s.local_memory : null;
  return {
    status: m?.status ?? 'checking',
    message:
      m?.status === 'insufficient'
        ? `Not enough Mac memory for ${m.model} with ${m.ctx} context tokens. Close other apps or use Cloud.`
        : (m?.message ?? 'Checking Mac memory…'),
    values:
      m && m.required_bytes !== null && m.budget_bytes !== null
        ? `${(m.required_bytes / 2 ** 30).toFixed(1)} GiB required · ${(m.budget_bytes / 2 ** 30).toFixed(1)} GiB budget`
        : null,
  };
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
  if (s.phase.kind === 'booting')
    return s.is_local ? 'starting local' : `renting ${s.provider}`;
  return s.phase.kind === 'off' && !s.is_local ? 'no pod · $0.00/h' : '';
}
export function limits(values: Record<string, string>): string {
  const n = (key: string, fallback: number) =>
    Number(values[key]) > 0 ? Number(values[key]) : fallback;
  return `idle ${n('LOBO_IDLE_MIN', 30)}m max ${n('LOBO_MAX_HOURS', 12)}h`;
}
export function modelRows(s: PanelState) {
  return (s.models?.models ?? []).map((m) => ({
    id: m.id,
    picked: s.model === m.id,
    size: `${gb(m.size)} GB`,
    state:
      m.size > 0 && m.on_disk >= m.size
        ? ('on' as const)
        : m.on_disk > 0
          ? ('partial' as const)
          : ('missing' as const),
    pct:
      m.size > 0
        ? Math.min(99, Math.floor((m.on_disk / m.size) * 100))
        : undefined,
  }));
}
export const localFooter = (l: Listing) =>
  `${l.weights} · ${gb(l.free_bytes)} GB free`;
export function stepRows(s: PanelState) {
  return s.boot_steps.map((step) => {
    const mark = s.steps.find((m) => m.step === step);
    const label = s.is_local
      ? ((
          { rent: 'start', gpu: 'metal', download: 'model' } as Partial<
            Record<Step, string>
          >
        )[step] ?? step)
      : step === 'download'
        ? 'verify model'
        : step;
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
    mbpsTone: s.is_local ? 'text' : d.mbps >= 100 ? 'green' : 'amber',
    eta: eta > 0 ? duration(eta) : undefined,
  };
}
export const bootElapsed = (s: PanelState, nowMs: number) =>
  `T+${duration(s.boot_start_ms === null ? 0 : (nowMs - s.boot_start_ms) / 1000)}`;
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
          label: s.is_local ? 'memory' : 'vram',
          bar: bar(gpu.vram_used_mb / Math.max(1, gpu.vram_total_mb), 12),
          text: `${(gpu.vram_used_mb / 1024).toFixed(1)}/${(gpu.vram_total_mb / 1024).toFixed(1)} GB`,
          gpu: s.is_local
            ? undefined
            : { text: `gpu ${gpu.util_pct}%`, hot: gpu.util_pct > 0 },
        }
      : undefined,
    kill: st
      ? {
          label: s.is_local ? 'idle-stop ' : 'idle-kill ',
          text: duration(left),
          warn: !s.is_local && left < 300,
        }
      : undefined,
    uptime: duration(uptime),
    cost: s.is_local
      ? 'local · $0'
      : s.snap?.pod
        ? usd((s.snap.pod.cost_per_hr * uptime) / 3600)
        : '',
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
  s.is_local
    ? 'stopping llama.cpp'
    : `deleting pod on ${s.snap?.pod?.provider ?? s.provider}`;
