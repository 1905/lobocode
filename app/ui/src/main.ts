import { mount } from 'svelte';
import App from './App.svelte';
import './lib/theme.css';
mount(App, { target: document.getElementById('app')! });
