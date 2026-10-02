import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PanelState } from '../gen/PanelState';
import type { OpenCodeInfo } from '../gen/OpenCodeInfo';
import type { OpenCodeResult } from '../gen/OpenCodeResult';
import type { ConfigShow } from '../proto/ConfigShow';
import type { Model } from '../proto/Model';
import type { SettingsTab } from './view';
export const inTauri = () => '__TAURI_INTERNALS__' in window;
function call<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) return Promise.reject(new Error('not in tauri'));
  return invoke<T>(name, args);
}
export const api = {
  getState: () => call<PanelState>('get_state'),
  start: () => call<void>('start'),
  stop: () => call<void>('stop'),
  dismiss: () => call<void>('dismiss'),
  setProvider: (v: string) => call<void>('set_provider', { v }),
  setModel: (v: string) => call<void>('set_model', { v }),
  refresh: () => call<void>('refresh'),
  copyApiKey: () => call<void>('copy_api_key'),
  copyText: (s: string) => call<void>('copy_text', { s }),
  configShow: () => call<ConfigShow>('config_show'),
  configSave: (set: Record<string, string>) =>
    call<void>('config_save', { set }),
  catalog: () => call<Model[]>('catalog'),
  genApiKey: () => call<string>('gen_api_key'),
  opencodeInfo: (path?: string) =>
    call<OpenCodeInfo>('opencode_info', { path }),
  chooseOpencodeConfig: () => call<string | null>('choose_opencode_config'),
  configureOpencode: (path: string, makeDefault: boolean) =>
    call<OpenCodeResult>('configure_opencode', { path, makeDefault }),
  openSettings: (tab?: SettingsTab) => call<void>('open_settings', { tab }),
  consumeSettingsTab: () => call<string | null>('consume_settings_tab'),
  revealConfig: () => call<void>('reveal_config'),
  openConfig: () => call<void>('open_config'),
  quit: () => call<void>('quit'),
};
export function onState(cb: (s: PanelState) => void) {
  if (!inTauri()) return Promise.reject(new Error('not in tauri'));
  return listen<PanelState>('lobo://state', (e) => cb(e.payload));
}
export function onSettingsTab(cb: () => void) {
  if (!inTauri()) return Promise.reject(new Error('not in tauri'));
  return listen<void>('lobo://settings-tab', cb, { target: 'settings' });
}
export function message(e: unknown): string {
  return typeof e === 'object' && e !== null && 'message' in e
    ? String(e.message)
    : String(e);
}
