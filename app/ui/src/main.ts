import { mount } from 'svelte';
import App from './App.svelte';

if (import.meta.env.VITE_LOBO_E2E === '1') {
  await import('@wdio/tauri-plugin');
}
import './lib/theme.css';
mount(App, { target: document.getElementById('app')! });
