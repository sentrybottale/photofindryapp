import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { compile } from 'svelte/compiler';
import { Window } from 'happy-dom';

const directory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const built = await build({
  absWorkingDir: directory,
  stdin: { contents: `import {mount,unmount,flushSync} from 'svelte';import Appearance from './src/Appearance.svelte';let component;window.appearanceTest={mount(){flushSync(()=>component=mount(Appearance,{target:document.getElementById('root')}));},destroy(){return unmount(component);}};`, resolveDir: directory },
  bundle: true, write: false, format: 'iife', platform: 'browser', conditions: ['browser', 'production'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'appearance-test-component', setup(builder) {
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path: filename }) => {
      const result = compile(await readFile(filename, 'utf8'), { filename: path.relative(directory, filename), generate: 'client', css: 'external', runes: true, dev: false, hmr: false, discloseVersion: false });
      assert.deepEqual(result.warnings, []);
      return { contents: result.js.code, loader: 'js', resolveDir: path.dirname(filename) };
    });
  } }],
});
const bundle = built.outputFiles[0].text;
const flush = async () => { for (let index = 0; index < 3; index += 1) await new Promise(resolve => setImmediate(resolve)); };

function fixture(mode = 'system', dark = false) {
  const window = new Window({ url: 'http://127.0.0.1:50001/' });
  window.document.documentElement.dataset.appearance = mode;
  window.document.body.innerHTML = '<div id="root"></div><button id="outside">Outside</button>';
  const calls = [];
  const listeners = new Set();
  const media = { matches: dark, addEventListener(type, callback) { listeners.add(callback); }, removeEventListener(type, callback) { listeners.delete(callback); } };
  window.matchMedia = () => media;
  window.PhotoFindry = { isConnected: () => false, api(route, body, options) { return new Promise((resolve, reject) => calls.push({ route, body: JSON.parse(JSON.stringify(body)), options, resolve, reject })); } };
  window.eval(bundle); window.appearanceTest.mount();
  const query = selector => window.document.querySelector(selector);
  return { window, calls, query, listeners,
    choose(value) { query(`[data-appearance-choice="${value}"]`).click(); },
    session(connected) { window.dispatchEvent(new window.CustomEvent('photofindry:session', { detail: { connected } })); },
    system(dark) { media.matches = dark; for (const callback of listeners) callback({ matches: dark }); },
    mode() { return window.document.documentElement.dataset.appearance; },
    theme() { return window.document.documentElement.dataset.theme; },
    async close() { await window.appearanceTest.destroy(); await window.happyDOM.abort(); },
  };
}

test('server-selected mode is immediate and only Follow system tracks OS changes', async () => {
  const ui = fixture('dark', false);
  try {
    assert.equal(ui.mode(), 'dark'); assert.equal(ui.theme(), 'dark'); assert.equal(ui.calls.length, 0);
    ui.system(false); assert.equal(ui.theme(), 'dark');
    ui.choose('light'); await flush(); assert.equal(ui.mode(), 'light'); assert.equal(ui.theme(), 'light');
    ui.system(true); assert.equal(ui.theme(), 'light');
    ui.choose('system'); await flush(); assert.equal(ui.theme(), 'dark');
    ui.system(false); await flush(); assert.equal(ui.theme(), 'light');
    assert.equal(ui.query('[data-appearance-choice="system"]').getAttribute('aria-pressed'), 'true');
    assert.match(ui.query('.pf-theme-status').textContent, /Open your local library/);
    assert.equal(ui.calls.length, 0, 'Disconnected choices do not access the local API');
    ui.session(true); await flush();
    assert.equal(ui.calls.length, 1); assert.equal(ui.calls[0].route, '/api/appearance/update');
    assert.deepEqual(ui.calls[0].body, { mode: 'system' });
    ui.calls[0].resolve({ mode: 'system' }); await flush();
    assert.match(ui.query('.pf-theme-status').textContent, /Saved on this Mac/);
  } finally { await ui.close(); }
});

test('rapid selections serialize writes and persist only the newest queued choice', async () => {
  const ui = fixture();
  try {
    ui.session(true); ui.calls[0].resolve({ mode: 'system' }); await flush();
    ui.choose('dark'); await flush();
    ui.choose('light'); ui.choose('system'); await flush();
    assert.equal(ui.calls.length, 2, 'The accepted dark write owns the sole request lane');
    assert.equal(ui.mode(), 'system');
    ui.calls[1].resolve({ mode: 'dark' }); await flush();
    assert.equal(ui.calls.length, 3); assert.deepEqual(ui.calls[2].body, { mode: 'system' });
    assert.equal(ui.mode(), 'system', 'Older success cannot undo the newest visible choice');
    ui.calls[2].resolve({ mode: 'system' }); await flush();
    assert.match(ui.query('.pf-theme-status').textContent, /Saved on this Mac/);
    assert.equal(ui.query('.pf-theme-error'), null);
  } finally { await ui.close(); }
});

test('a delayed read cannot overwrite a choice and failed saves expose a working retry', async () => {
  const ui = fixture();
  try {
    ui.session(true); await flush(); const read = ui.calls[0];
    ui.choose('dark'); await flush();
    assert.equal(read.options.signal.aborted, true);
    read.resolve({ mode: 'light' }); await flush(); assert.equal(ui.mode(), 'dark');
    ui.calls[1].reject(new Error('The disk is temporarily unavailable.')); await flush();
    assert.equal(ui.theme(), 'dark'); assert.match(ui.query('.pf-theme-error').textContent, /disk/);
    assert.equal(ui.query('.pf-theme-retry').textContent, 'Retry saving');
    ui.query('.pf-theme-retry').click(); await flush();
    assert.equal(ui.calls.length, 3); assert.deepEqual(ui.calls[2].body, { mode: 'dark' });
    ui.calls[2].resolve({ mode: 'dark' }); await flush();
    assert.equal(ui.query('.pf-theme-error'), null); assert.equal(ui.query('.pf-theme-retry'), null);
    assert.match(ui.query('.pf-theme-status').textContent, /Saved on this Mac/);
  } finally { await ui.close(); }
});

test('revoked-session responses are fenced and a pending choice saves after reconnect', async () => {
  const ui = fixture();
  try {
    ui.session(true); const oldRead = ui.calls[0];
    ui.session(false); ui.session(true); await flush();
    ui.calls[1].resolve({ mode: 'light' }); await flush();
    oldRead.resolve({ mode: 'dark' }); await flush(); assert.equal(ui.mode(), 'light');
    ui.choose('dark'); await flush(); const oldWrite = ui.calls[2];
    ui.session(false); ui.choose('system'); ui.session(true); await flush();
    assert.equal(ui.calls.length, 3, 'Reconnect cannot overlap an accepted old write');
    oldWrite.reject(new Error('Old session was revoked')); await flush();
    assert.equal(ui.query('.pf-theme-error'), null); assert.equal(ui.mode(), 'system');
    assert.equal(ui.calls.length, 4); assert.deepEqual(ui.calls[3].body, { mode: 'system' });
    ui.calls[3].resolve({ mode: 'system' }); await flush();
    assert.match(ui.query('.pf-theme-status').textContent, /Saved on this Mac/);
    ui.window.dispatchEvent(new ui.window.CustomEvent('photofindry:library-restored')); await flush();
    assert.equal(ui.mode(), 'system'); assert.equal(ui.calls.length, 4, 'A library restore does not reload the host theme');
  } finally { await ui.close(); }
});

test('read failure has recovery and dismissal restores focus without retaining subscriptions', async () => {
  const ui = fixture();
  try {
    ui.session(true); ui.calls[0].reject(new Error('Saved appearance could not be read.')); await flush();
    assert.equal(ui.query('.pf-theme-retry').textContent, 'Check again');
    ui.query('.pf-theme-retry').click(); await flush();
    ui.calls[1].resolve({ mode: 'dark' }); await flush(); assert.equal(ui.mode(), 'dark');
    const menu = ui.query('details'), trigger = ui.query('summary');
    menu.open = true; ui.query('[data-appearance-choice="dark"]').focus();
    const escape = new ui.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    ui.window.document.activeElement.dispatchEvent(escape); await flush();
    assert.equal(menu.open, false); assert.equal(escape.defaultPrevented, true); assert.equal(ui.window.document.activeElement, trigger);
    menu.open = true;
    ui.query('#outside').dispatchEvent(new ui.window.PointerEvent('pointerdown', { bubbles: true })); await flush();
    assert.equal(menu.open, false); assert.equal(ui.window.document.activeElement, trigger);
    assert.equal(ui.listeners.size, 1);
    await ui.window.appearanceTest.destroy(); assert.equal(ui.listeners.size, 0);
    const count = ui.calls.length;
    ui.session(true); ui.system(false); await flush();
    assert.equal(ui.calls.length, count); assert.equal(ui.theme(), 'dark');
  } finally { await ui.window.happyDOM.abort(); }
});

test('an older write failure cannot strand a newer choice and unmount never sends queued work', async () => {
  const ui = fixture();
  try {
    ui.session(true); ui.calls[0].resolve({ mode: 'system' }); await flush();
    ui.choose('dark'); ui.choose('light'); await flush();
    ui.calls[1].reject(new Error('Older dark write failed')); await flush();
    assert.equal(ui.query('.pf-theme-error'), null); assert.equal(ui.mode(), 'light');
    assert.equal(ui.calls.length, 3); assert.deepEqual(ui.calls[2].body, { mode: 'light' });
    ui.choose('system'); await flush();
    await ui.window.appearanceTest.destroy();
    ui.calls[2].resolve({ mode: 'light' }); await flush();
    assert.equal(ui.calls.length, 3, 'No queued write may begin after the component is gone');
    assert.equal(ui.listeners.size, 0); assert.equal(ui.mode(), 'system');
  } finally { await ui.window.happyDOM.abort(); }
});
