import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { initializeAppearance } from './lib/appearance';

const disposeAppearance = initializeAppearance();
if (import.meta.hot) import.meta.hot.dispose(disposeAppearance);

const target = document.getElementById('app');

if (!target) {
  throw new Error('The application mount element is missing.');
}

const app = mount(App, { target });

export default app;
