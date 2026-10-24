import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { Window } from 'happy-dom';

const bundle = await readFile(new URL('../../ui-framework.js', import.meta.url), 'utf8');
const contract = JSON.parse(await readFile(new URL('../legacy-dom-contract.json', import.meta.url), 'utf8'));
const flush = async () => { for (let index = 0; index < 3; index += 1) await new Promise(resolve => setImmediate(resolve)); };

function fixture(width = 390, platform = null) {
  const window = new Window({ url: 'http://127.0.0.1:50001/', width, height: 900 });
  if (platform) window.document.documentElement.dataset.platform = platform;
  let requests = 0;
  window.fetch = () => { requests += 1; throw new Error('The shell must not request library data or remote code.'); };
  window.document.body.innerHTML = '<p id="retained-document-sibling">Document resource sentinel</p><div id="photofindry-root"><p id="previous-static-frame">Static launch fallback</p></div>';
  const observed = new Map();
  const media = [];
  const matchMedia = window.matchMedia.bind(window);
  window.matchMedia = query => { const list = matchMedia(query); media.push(list); return list; };
  const add = window.addEventListener.bind(window), remove = window.removeEventListener.bind(window);
  window.addEventListener = (type, callback, options) => {
    if (type.startsWith('photofindry:') || type === 'resize') { if (!observed.has(type)) observed.set(type, new Set()); observed.get(type).add(callback); }
    add(type, callback, options);
  };
  window.removeEventListener = (type, callback, options) => { observed.get(type)?.delete(callback); remove(type, callback, options); };
  window.eval(bundle);
  const shell = window.PhotoFindryShell;
  const state = shell.mount();
  window.document.getElementById('app').hidden = false;
  window.document.getElementById('pair-screen').hidden = true;
  return { window, shell, state, observed, requests: () => requests, get: id => window.document.getElementById(id),
    resize(width) {
      const previous = media.map(list => list.matches);
      window.happyDOM.setViewport({ width, height: 900 });
      // Happy DOM 21 seeds its internal listener state with false, so an initial
      // true -> false transition misses the browser's required change event.
      media.forEach((list, index) => { if (previous[index] !== list.matches) list.dispatchEvent(new window.MediaQueryListEvent('change', { matches: list.matches, media: list.media })); });
    },
    async close() { await shell.destroy(); await window.happyDOM.abort(); } };
}

test('the compiled shell mounts synchronously, keeps every established controller ID and contains no inline handlers', async () => {
  const ui = fixture(1440);
  try {
    assert.equal(ui.state.framework, 'svelte'); assert.equal(ui.state.version, '5.57.2'); assert.equal(ui.state.mounted, true);
    assert.equal(ui.get('previous-static-frame'), null);
    assert.equal(ui.get('retained-document-sibling').textContent, 'Document resource sentinel');
    const all = [...ui.window.document.querySelectorAll('[id]')].map(node => node.id);
    assert.equal(new Set(all).size, all.length);
    for (const id of contract.ids) assert(ui.get(id), `Missing established controller island ${id}`);
    assert.equal(ui.get('gallery').getAttribute('role'), 'listbox');
    assert.equal(ui.get('gallery').getAttribute('aria-multiselectable'), 'true');
    assert.equal(ui.get('search-form').getAttribute('role'), 'search');
    assert.equal(ui.get('preview-dialog').tagName, 'DIALOG');
    assert.equal(ui.window.document.querySelector('[onclick]'), null);
    const gallery = ui.get('gallery'); ui.shell.mount(); assert.equal(ui.get('gallery'), gallery);
    assert.equal(ui.requests(), 0);
  } finally { await ui.close(); }
});

test('a failed framework mount restores the static fallback and permits a fresh local retry', async () => {
  const window = new Window({ url: 'http://127.0.0.1:50001/', width: 390 });
  try {
    window.document.body.innerHTML = '<div id="photofindry-root"><p id="static-fallback">Open PhotoFindry again.</p></div>';
    const fallback = window.document.getElementById('static-fallback');
    window.eval(bundle);
    const matchMedia = window.matchMedia.bind(window);
    window.matchMedia = () => { throw new Error('Fixture browser initialization failed'); };
    assert.throws(() => window.PhotoFindryShell.mount(), /Fixture browser initialization failed/);
    assert.equal(window.document.getElementById('static-fallback'), fallback); assert.equal(window.PhotoFindryShell.getState().mounted, false);
    window.matchMedia = matchMedia;
    window.PhotoFindryShell.mount(); assert(window.document.getElementById('gallery'));
    await window.PhotoFindryShell.destroy();
  } finally { await window.happyDOM.abort(); }
});

test('compact navigation is a controlled modal drawer with focus, inert background, Escape and backdrop recovery', async () => {
  const ui = fixture();
  try {
    const toggle = ui.get('sidebar-toggle'), sidebar = ui.get('sidebar-navigation');
    assert.equal(sidebar.inert, true); assert.equal(sidebar.getAttribute('aria-hidden'), 'true');
    assert.notEqual(ui.window.document.activeElement, toggle);
    toggle.click(); await flush();
    assert.equal(toggle.getAttribute('aria-expanded'), 'true'); assert.equal(sidebar.getAttribute('role'), 'dialog');
    assert.equal(sidebar.getAttribute('aria-modal'), 'true'); assert.equal(sidebar.inert, false);
    assert.equal(ui.window.document.querySelector('.library').inert, true);
    assert.equal(ui.window.document.activeElement, ui.get('sidebar-close'));
    const last = [...sidebar.querySelectorAll('button')].at(-1); last.focus();
    const tab = new ui.window.KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    last.dispatchEvent(tab); await flush(); assert.equal(tab.defaultPrevented, true); assert.equal(ui.window.document.activeElement, ui.get('sidebar-close'));
    ui.get('sidebar-close').dispatchEvent(new ui.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await flush(); assert.equal(ui.shell.getState().navigationOpen, false); assert.equal(ui.window.document.activeElement, toggle);
    assert.equal(ui.window.document.querySelector('.library').inert, false);
    toggle.click(); await flush(); ui.get('sidebar-backdrop').click(); await flush();
    assert.equal(ui.shell.getState().navigationOpen, false); assert.equal(ui.window.document.activeElement, toggle);
  } finally { await ui.close(); }
});

test('pointer-opened drawers restore their explicit toggle and API-opened details preserve a usable origin', async () => {
  const ui = fixture();
  try {
    const search = ui.get('search'), navigation = ui.get('sidebar-toggle'), details = ui.get('inspector-toggle');
    search.focus(); navigation.click(); await flush(); ui.get('sidebar-backdrop').click(); await flush();
    assert.equal(ui.window.document.activeElement, navigation);
    search.focus(); details.click(); await flush(); ui.get('inspector-close').click(); await flush();
    assert.equal(ui.window.document.activeElement, details);
    search.focus(); await ui.shell.showDetails(); ui.get('inspector-backdrop').click(); await flush();
    assert.equal(ui.window.document.activeElement, search);
    navigation.click(); await flush(); await ui.shell.showDetails(); ui.get('inspector-close').click(); await flush();
    assert.equal(ui.window.document.activeElement, details);
  } finally { await ui.close(); }
});

test('imperative gallery, inspector and live controls survive shell updates without re-creating photos or requests', async () => {
  const ui = fixture();
  try {
    const gallery = ui.get('gallery'), details = ui.get('inspector-content');
    const tile = ui.window.document.createElement('div'); tile.textContent = 'Saved photo'; tile.dataset.photoId = 'saved-id';
    gallery.append(tile); details.textContent = 'Complete accepted details';
    ui.get('search').value = 'preserved query'; ui.get('job-strip').hidden = false; ui.get('job-strip').textContent = 'A running local job';
    for (let index = 0; index < 12; index += 1) {
      ui.get('sidebar-toggle').click(); await flush(); ui.get('library-nav').click(); await flush();
      assert.equal(ui.shell.getState().navigationOpen, false);
      await ui.shell.showDetails(); await flush();
      assert.equal(ui.shell.getState().inspectorOpen, true); assert.equal(ui.get('inspector').inert, false);
      assert.equal(ui.window.document.activeElement, ui.get('inspector-close'));
      ui.get('inspector-backdrop').click(); await flush();
      assert.equal(ui.get('gallery'), gallery); assert.equal(gallery.firstElementChild, tile);
      assert.equal(ui.get('inspector-content'), details); assert.equal(details.textContent, 'Complete accepted details');
      assert.equal(ui.get('search').value, 'preserved query'); assert.equal(ui.get('job-strip').hidden, false);
    }
    assert.equal(ui.requests(), 0); assert.equal(gallery.children.length, 1);
  } finally { await ui.close(); }
});

test('desktop resize releases drawer focus restrictions and native dialogs retain Escape ownership', async () => {
  const ui = fixture();
  try {
    ui.get('sidebar-toggle').click(); await flush();
    ui.get('preview-dialog').setAttribute('open', '');
    const escape = new ui.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    ui.get('sidebar-close').dispatchEvent(escape); await flush();
    assert.equal(escape.defaultPrevented, false); assert.equal(ui.shell.getState().navigationOpen, true);
    ui.get('preview-dialog').removeAttribute('open');
    ui.resize(1440); await flush();
    assert.equal(ui.shell.getState().compactNavigation, false); assert.equal(ui.shell.getState().compactInspector, false);
    assert.equal(ui.shell.getState().navigationOpen, false); assert.equal(ui.get('sidebar-navigation').inert, false);
    assert.equal(ui.get('sidebar-navigation').hasAttribute('aria-hidden'), false); assert.equal(ui.get('inspector').inert, false);
    assert.equal(ui.window.document.querySelector('.library').inert, false);
    assert.equal(ui.window.document.activeElement, ui.get('library-nav'));
  } finally { await ui.close(); }
});

test('compact session actions use the established buttons after closing their drawer', async () => {
  const ui = fixture(); const actions = [];
  try {
    for (const id of ['disconnect', 'quit-companion']) ui.get(id).addEventListener('click', () => {
      assert.equal(ui.shell.getState().navigationOpen, false); assert.equal(ui.window.document.querySelector('.app-header').inert, false); actions.push(id);
    });
    ui.get('sidebar-toggle').click(); await flush(); ui.get('sidebar-disconnect').click(); await flush();
    ui.get('sidebar-toggle').click(); await flush(); ui.get('sidebar-quit').click(); await flush();
    assert.deepEqual(actions, ['disconnect', 'quit-companion']); assert.equal(ui.requests(), 0);
  } finally { await ui.close(); }
});

test('session invalidation closes drawers and unmount removes framework-owned subscriptions before remount', async () => {
  const ui = fixture();
  try {
    ui.get('sidebar-toggle').click(); await flush();
    ui.window.dispatchEvent(new ui.window.CustomEvent('photofindry:session', { detail: { connected: false } })); await flush();
    assert.equal(ui.shell.getState().navigationOpen, false);
    await ui.shell.showDetails(); await flush();
    ui.window.dispatchEvent(new ui.window.CustomEvent('photofindry:library-restored')); await flush();
    assert.equal(ui.shell.getState().inspectorOpen, false);
    assert.equal(ui.observed.get('photofindry:session').size, 2);
    assert.equal(ui.observed.get('resize').size, 3);
    await ui.shell.destroy(); assert.equal(ui.shell.getState().mounted, false); assert.equal(ui.get('gallery'), null);
    assert.equal(ui.observed.get('photofindry:session').size, 0); assert.equal(ui.observed.get('photofindry:library-restored').size, 0);
    assert.equal(ui.observed.get('resize').size, 0);
    ui.shell.mount(); assert.equal(ui.observed.get('photofindry:session').size, 2); assert(ui.get('gallery'));
  } finally { await ui.close(); }
});

test('the exact shipped bundle is small, self-contained, production-only and carries its runtime notices', async () => {
  const manifest = JSON.parse(await readFile(new URL('../../ui-framework.manifest.json', import.meta.url), 'utf8'));
  assert.equal(manifest.framework.name, 'svelte');
  assert(Buffer.byteLength(bundle) <= 96 * 1024); assert(gzipSync(bundle).length <= 36 * 1024);
  assert.doesNotMatch(bundle, /\b(?:eval|import)\s*\(|new\s+Function\s*\(|sourceMappingURL|localhost:5173|__SVELTE_HMR|fetch\s*\(/);
  for (const [asset, record] of Object.entries(manifest.assets)) {
    const bytes = await readFile(new URL(`../../${asset}`, import.meta.url));
    assert.equal(bytes.length, record.bytes); assert.equal(createHash('sha256').update(bytes).digest('hex'), record.sha256);
  }
  const notices = await readFile(new URL('../../ui-framework.NOTICES.txt', import.meta.url), 'utf8');
  for (const dependency of manifest.runtimeDependencies) assert(notices.includes(`${dependency.name} ${dependency.version}`));
  assert(notices.includes('Permission is hereby granted')); assert(notices.includes('MIT licensed')); assert(notices.includes('proprietary engines'));
});


for (const platform of ['linux', 'windows']) test(`${platform} frame uses its host identity and retains core controls without unavailable feature links`, async () => {
  const ui = fixture(1440, platform);
  try {
    assert.match(ui.get('pair-title').textContent, /your computer/);
    assert.match(ui.window.document.querySelector('.app-footer').textContent, platform === 'linux' ? /Linux preview/ : /Windows preview/);
    assert.doesNotMatch(ui.window.document.querySelector('.app-footer').textContent, /macOS/);
    assert.equal(ui.window.document.querySelector('.local-open-link').hidden, true);
    const button = label => [...ui.window.document.querySelectorAll('button')].find(node => node.textContent.trim() === label);
    assert.equal(button('Photo improvement').hidden, true);
    assert.equal(button('Backups & archives').hidden, true);
    assert.equal(button('People').hidden, false);
    assert(ui.get('tag-pending')); assert(ui.get('ai-search')); assert(button('Jobs & activity'));
    assert.equal(ui.requests(), 0);
  } finally { await ui.close(); }
});
