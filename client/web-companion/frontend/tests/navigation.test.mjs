import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Window } from 'happy-dom';

const bundle = await readFile(new URL('../../ui-framework.js', import.meta.url), 'utf8');
const flush = async () => { for (let index = 0; index < 4; index += 1) await new Promise(resolve => setImmediate(resolve)); };
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const source = { id: 'source-a', label: 'Travel', mode: 'connected', accessOriginals: true };
const folder = (index, extra = {}) => ({ scopeId: `opaque-${index}`, sourceId: source.id, name: `Folder ${index}`, count: index + 1, path: '/private/ignored/never-send', ...extra });
const snapshot = (extra = {}) => ({ libraryId: 'library-a', libraryGeneration: 3, library: { total: 1200, tagged: 450 },
  sources: [source], folders: [folder(0)], people: { people: [{ id: 'one' }, { id: 'two' }] },
  jobs: { counts: { running: 1, paused: 2, queued: 3, completed: 99 } }, settings: { modelConfigured: true, projectorConfigured: true }, ...extra });
function fixture(width = 1440) {
  const window = new Window({ url: 'http://127.0.0.1:50123/', width, height: 900 });
  const calls = [], scopes = [], destinations = [], errors = [], handlers = new Map();
  let scope = { sourceId: '', scopeId: '', status: 'all', metadataFilters: { camera: 'Saved Camera' }, includeRelated: false };
  window.document.body.innerHTML = '<div id="photofindry-root"></div>';
  const emit = (name, detail) => window.dispatchEvent(new window.CustomEvent(`photofindry:${name}`, { detail }));
  window.PhotoFindry = {
    getScope: () => scope,
    async setScope(next) { scopes.push(next); scope = { ...scope, ...next }; emit('scope', scope); },
    showError: message => errors.push(message),
    async api(route, input, options = {}) {
      const request = { route, input, signal: options.signal }; calls.push(request);
      if (handlers.has(route)) return handlers.get(route)(request);
      if (route === '/api/appearance') return { mode: 'system' };
      if (route === '/api/action' && input.action === 'folderChildren') return { children: [] };
      throw Error(`Unexpected fixture request ${route}`);
    },
  };
  window.PhotoFindryWorkspace = { async navigate(destination) { destinations.push(destination); } };
  window.PhotoFindryPanels = { close() {} };
  window.eval(bundle); const shell = window.PhotoFindryShell; shell.mount();
  window.document.getElementById('app').hidden = false; window.document.getElementById('pair-screen').hidden = true;
  const get = id => window.document.getElementById(id);
  const button = label => [...window.document.querySelectorAll('button')].find(node => node.getAttribute('aria-label') === label);
  const navigationButton = label => [...get('workspace-navigation').querySelectorAll('button')].find(node => node.querySelector('.pf-nav-label')?.textContent === label);
  return { window, shell, get, button, navigationButton, calls, scopes, destinations, errors, handlers, emit, scope: () => scope,
    async state(value = snapshot()) { emit('state', value); await flush(); }, async close() { await shell.destroy(); await window.happyDOM.abort(); } };
}

test('compiled navigation receives native state, renders truthful counts and sends only opaque folder scope identities', async () => {
  const ui = fixture();
  try {
    await ui.state();
    assert.match(ui.get('library-nav').textContent, /1,200/);
    assert.equal(ui.navigationButton('People').querySelector('.pf-nav-count').textContent, '2');
    assert.equal(ui.navigationButton('Files').querySelector('.pf-nav-count').textContent, '1');
    assert.equal(ui.navigationButton('Jobs & activity').querySelector('.pf-nav-count').textContent, '6');
    assert.match(ui.window.document.querySelector('.pf-library-summary').textContent, /1,200 photographs · 450 tagged/);
    assert.match(ui.get('open-local-models').textContent, /Load automatically when needed/);
    await ui.navigationButton('Folder 0').click(); await flush();
    assert.deepEqual(JSON.parse(JSON.stringify(ui.scopes[0])), { sourceId: 'source-a', scopeId: 'opaque-0', status: 'all' });
    assert.equal(ui.scope().metadataFilters.camera, 'Saved Camera'); assert.equal(ui.scope().includeRelated, false);
    assert(!JSON.stringify(ui.scopes).includes('/private/'));
    ui.navigationButton('Files').click(); await flush(); assert.deepEqual(ui.destinations, ['Files']);
  } finally { await ui.close(); }
});

test('compiled folder navigation bounds roots, children and cache, excludes archived sources, and provides a route to remaining folders', async () => {
  const ui = fixture();
  try {
    const roots = Array.from({ length: 130 }, (_, index) => folder(index));
    await ui.state(snapshot({ sources: [source, { id: 'archive', mode: 'archived' }], folders: [folder(-1, { sourceId: 'archive', name: 'Archived root' }), ...roots] }));
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-0').length, 128);
    assert.equal(ui.navigationButton('Archived root'), undefined);
    assert([...ui.get('workspace-navigation').querySelectorAll('button')].some(node => node.textContent.includes('Browse all folders')));
    await ui.state(snapshot({ folders: roots.slice(0, 9) }));
    ui.handlers.set('/api/action', ({ input }) => ({ children: [{ scopeId: `child-${input.params.scopeId}`, name: `Child ${input.params.scopeId}`, count: 3 }] }));
    for (let index = 0; index < 9; index += 1) { ui.button(`Subfolders of Folder ${index}`).click(); await flush(); }
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-1').length, 8);
    assert.equal(ui.button('Subfolders of Folder 0').getAttribute('aria-expanded'), 'false');
    const before = ui.calls.length; ui.button('Subfolders of Folder 0').click(); await flush(); assert.equal(ui.calls.length, before + 1);
    ui.handlers.set('/api/action', () => ({ children: Array.from({ length: 90 }, (_, index) => ({ scopeId: `many-${index}`, name: `Nested ${index}`, count: 1 })) }));
    await ui.state(snapshot({ libraryGeneration: 4 })); ui.button('Subfolders of Folder 0').click(); await flush();
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-1').length, 80);
    assert.equal(ui.navigationButton('Nested 80'), undefined);
    const more = [...ui.get('workspace-navigation').querySelectorAll('button')].find(node => node.textContent.includes('Browse more folders'));
    more.click(); await flush(); assert.deepEqual(ui.destinations, ['Browse folders']);
  } finally { await ui.close(); }
});

test('folder requests are cancelled on replacement and restore, stale responses cannot cross libraries, and failures expose a working retry', async () => {
  const ui = fixture(); const first = deferred(), second = deferred();
  try {
    await ui.state(snapshot({ folders: [folder(0), folder(1)] }));
    ui.handlers.set('/api/action', ({ input }) => input.params.scopeId === 'opaque-0' ? first.promise : second.promise);
    ui.button('Subfolders of Folder 0').click(); await flush(); const request1 = ui.calls.at(-1);
    ui.button('Subfolders of Folder 1').click(); await flush(); const request2 = ui.calls.at(-1);
    assert.equal(request1.signal.aborted, true); assert.equal(request2.signal.aborted, false);
    first.resolve({ children: [{ scopeId: 'stale', name: 'Stale child' }] }); await flush(); assert.equal(ui.navigationButton('Stale child'), undefined);
    ui.emit('library-restored'); await flush(); assert.equal(request2.signal.aborted, true);
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row').length, 0);
    second.resolve({ children: [{ scopeId: 'wrong-library', name: 'Previous library child' }] }); await flush();
    await ui.state(snapshot({ libraryId: 'library-b', libraryGeneration: 10 })); assert.equal(ui.navigationButton('Previous library child'), undefined);
    let attempts = 0; ui.handlers.set('/api/action', () => { if (++attempts === 1) throw Error('Fixture directory unavailable.'); return { children: [{ scopeId: 'retry-child', name: 'Recovered child' }] }; });
    ui.button('Subfolders of Folder 0').click(); await flush();
    const retry = [...ui.get('workspace-navigation').querySelectorAll('button')].find(node => node.textContent === 'Retry subfolders');
    assert(retry); retry.click(); await flush(); assert(ui.navigationButton('Recovered child')); assert.equal(attempts, 2);
    ui.emit('session', { connected: false }); await flush();
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row').length, 0);
    assert.equal(ui.get('library-nav').querySelector('.pf-nav-count').textContent, '');
  } finally { await ui.close(); }
});

test('status, density and filter controls preserve gallery nodes, query and selection while forwarding status through the shared controller', async () => {
  const ui = fixture();
  try {
    await ui.state();
    const gallery = ui.get('gallery'), tile = ui.window.document.createElement('div'), image = ui.window.document.createElement('img');
    tile.dataset.photoId = 'saved'; tile.setAttribute('aria-selected', 'true'); tile.append(image); gallery.append(tile);
    ui.get('search').value = 'preserved query'; ui.get('selection-count').textContent = '1 selected';
    const tagged = [...ui.window.document.querySelectorAll('.pf-status-tabs button')].find(node => node.textContent === 'Tagged');
    tagged.click(); await flush(); assert.equal(ui.scope().status, 'tagged'); assert.equal(tagged.getAttribute('aria-pressed'), 'true');
    ui.button('Compact thumbnails').click(); await flush(); assert.equal(ui.shell.getState().density, 'compact'); assert.equal(ui.get('app').dataset.density, 'compact');
    ui.button('Large thumbnails').click(); await flush(); assert.equal(ui.get('app').dataset.density, 'large');
    ui.get('filter-toggle').click(); await flush(); assert.equal(ui.get('library-filters').hidden, false); assert.equal(ui.get('filter-toggle').getAttribute('aria-expanded'), 'true');
    assert.equal(ui.get('gallery'), gallery); assert.equal(gallery.firstElementChild, tile); assert.equal(tile.firstElementChild, image);
    assert.equal(tile.getAttribute('aria-selected'), 'true'); assert.equal(ui.get('search').value, 'preserved query'); assert.equal(ui.get('selection-count').textContent, '1 selected');
    assert.equal(ui.calls.length, 0); assert.deepEqual(JSON.parse(JSON.stringify(ui.scopes)), [{ status: 'tagged' }]);
  } finally { await ui.close(); }
});

test('the integrated photo component factory owns only its bounded metadata slots and releases them on explicit or shell teardown', async () => {
  const ui = fixture(); let previews = 0;
  const data = { name: 'saved.jpg', caption: 'Saved accepted caption.', sourceLabel: 'Travel', sourceStatus: '', statusLabel: 'Tagged · 9 observations', statusTone: 'success',
    reviewLabel: 'Reviewed', reviewTone: 'neutral', reviewHint: 'Accepted details unchanged.', matchLabel: '', matchText: '', locationEvidence: '', onPreview: () => { previews += 1; } };
  try {
    const tile = ui.window.document.createElement('div'), image = ui.window.document.createElement('img'), slot = ui.window.document.createElement('div');
    slot.className = 'photo-summary-slot'; tile.append(image, slot); ui.get('gallery').append(tile);
    const summary = ui.shell.mountPhotoSummary(slot, data); assert.match(slot.textContent, /Tagged · 9 observations/);
    const filename = slot.querySelector('.photo-name'); summary.update({ ...data, name: 'updated.jpg', reviewLabel: 'Review undone' });
    assert.equal(slot.querySelector('.photo-name'), filename); assert.equal(tile.firstElementChild, image); assert.equal(filename.textContent, 'updated.jpg');
    slot.querySelector('button').click(); await flush(); assert.equal(previews, 1);
    assert.throws(() => ui.shell.mountPhotoSummary(image, data), /target is unavailable/);
    await summary.destroy(); assert.equal(slot.childElementCount, 0); assert.equal(tile.firstElementChild, image);
    for (let index = 0; index < 60; index += 1) { const target = ui.window.document.createElement('div'); target.className = 'photo-summary-slot'; ui.get('gallery').append(target); ui.shell.mountPhotoSummary(target, data); }
    assert.equal(ui.window.document.querySelectorAll('.pf-photo-summary').length, 60);
    await ui.shell.destroy(); assert.equal(ui.window.document.querySelectorAll('.pf-photo-summary').length, 0);
    assert.equal(ui.calls.length, 0);
  } finally { await ui.close(); }
});

test('compact folder expansion stays in the drawer, retries in place, and closes only after selecting a destination', async () => {
  const ui = fixture(390);
  try {
    await ui.state();
    const pending = deferred(); ui.handlers.set('/api/action', () => pending.promise);
    ui.get('sidebar-toggle').click(); await flush(); assert.equal(ui.shell.getState().navigationOpen, true);
    ui.button('Subfolders of Folder 0').click(); await flush();
    assert.equal(ui.shell.getState().navigationOpen, true, 'Expanding a folder must keep its navigation context visible');
    pending.reject(Error('Fixture read interrupted.')); await flush();
    assert.equal(ui.shell.getState().navigationOpen, true);
    ui.handlers.set('/api/action', () => ({ children: [{ scopeId: 'child-0', name: 'A nested folder', count: 2 }] }));
    const retry = [...ui.get('workspace-navigation').querySelectorAll('button')].find(node => node.textContent === 'Retry subfolders');
    assert(retry); retry.click(); await flush(); assert.equal(ui.shell.getState().navigationOpen, true);
    assert(ui.navigationButton('A nested folder'));
    ui.navigationButton('A nested folder').click(); await flush();
    assert.equal(ui.shell.getState().navigationOpen, false);
    assert.equal(ui.scope().scopeId, 'child-0'); assert.equal(ui.get('gallery').closest('[inert]'), null);
    assert.equal(ui.window.document.activeElement, ui.get('gallery'), 'Folder selection returns keyboard focus to the visible gallery');
  } finally { await ui.close(); }
});

test('teardown aborts outstanding folder requests and the integrated summary factory enforces its hard page bound', async () => {
  const ui = fixture(); const pending = deferred();
  try {
    await ui.state(); ui.handlers.set('/api/action', () => pending.promise);
    ui.button('Subfolders of Folder 0').click(); await flush(); const request = ui.calls.at(-1);
    const data = { name: 'bounded.jpg', caption: '', sourceLabel: '', statusLabel: 'Tagged', statusTone: 'success' };
    for (let index = 0; index < 240; index += 1) {
      const slot = ui.window.document.createElement('div'); slot.className = 'photo-summary-slot'; ui.get('gallery').append(slot); ui.shell.mountPhotoSummary(slot, data);
    }
    const extra = ui.window.document.createElement('div'); extra.className = 'photo-summary-slot';
    assert.throws(() => ui.shell.mountPhotoSummary(extra, data), /display bound/);
    await ui.shell.destroy(); assert.equal(request.signal.aborted, true);
    pending.resolve({ children: [{ scopeId: 'late', name: 'Late child' }] }); await flush();
    assert.equal(ui.window.document.querySelectorAll('.pf-photo-summary, .pf-folder-row').length, 0);
    ui.shell.mount(); ui.get('app').hidden = false; await ui.state();
    const slot = ui.window.document.createElement('div'); slot.className = 'photo-summary-slot'; ui.get('gallery').append(slot);
    const summary = ui.shell.mountPhotoSummary(slot, data); assert.match(slot.textContent, /bounded.jpg/); await summary.destroy();
  } finally { await ui.close(); }
});

test('folder scope summary identifies the exact selected root before falling back to the source', async () => {
  const ui = fixture();
  try {
    await ui.state(snapshot({ folders: [folder(0), folder(1)] }));
    ui.navigationButton('Folder 1').click(); await flush();
    assert.equal(ui.window.document.querySelector('.pf-scope-label').textContent.trim(), 'Folder 1');
  } finally { await ui.close(); }
});

test('imported folder aliases with repeated opaque IDs form one destination and never stop later state or child updates', async () => {
  const ui = fixture(); const runtimeErrors = [];
  const onError = event => { runtimeErrors.push(event.error || event.message); event.preventDefault(); };
  ui.window.addEventListener('error', onError);
  try {
    // The older imported alias and its current location identify the same root.
    // Deduplication must precede the 128 root and 80 child presentation bounds.
    const imported = Array.from({ length: 140 }, (_, index) => folder(0, { name: `Old alias ${index}`, count: 999 }));
    const sources = [{ ...source, photoCount: 330, roots: [{ scopeId: 'opaque-0' }] }];
    const folders = [...imported, folder(0, { name: 'Current root', count: 888 }), folder(1, { name: 'Another saved folder', count: 4 })];
    await ui.state(snapshot({ sources, folders }));
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-0').length, 2);
    assert.equal(ui.navigationButton('Current root').querySelector('.pf-nav-count').textContent, '330');
    assert(ui.navigationButton('Another saved folder'));
    assert(![...ui.get('workspace-navigation').querySelectorAll('button')].some(node => node.textContent.includes('Browse all folders')));
    ui.handlers.set('/api/action', () => ({ children: [
      ...Array.from({ length: 90 }, (_, index) => ({ scopeId: 'shared-child', name: `Old child alias ${index}`, count: 11 })),
      { scopeId: 'shared-child', name: 'Current child', count: 12 }, { scopeId: 'other-child', name: 'Other child', count: 5 },
    ] }));
    ui.button('Subfolders of Current root').click(); await flush();
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-1').length, 2);
    assert(ui.navigationButton('Current child')); assert(ui.navigationButton('Other child'));
    assert(![...ui.get('workspace-navigation').querySelectorAll('button')].some(node => node.textContent.includes('Browse more folders')));
    ui.navigationButton('Current child').click(); await flush();
    assert.equal(ui.scope().scopeId, 'shared-child'); assert.equal(ui.scope().sourceId, source.id);
    await ui.state(snapshot({ sources, folders, library: { total: 1234, tagged: 456 } }));
    assert.match(ui.get('library-nav').textContent, /1,234/);
    assert.match(ui.window.document.querySelector('.pf-library-summary').textContent, /1,234 photographs · 456 tagged/);
    assert.equal(ui.window.document.querySelectorAll('.pf-folder-row.depth-0').length, 2);
    assert.deepEqual(runtimeErrors, []);
  } finally { ui.window.removeEventListener('error', onError); await ui.close(); }
});

test('only an exact single source root uses source photoCount and changed authoritative counts invalidate outstanding folder reads', async () => {
  const ui = fixture(); const pending = deferred();
  try {
    const sources = [
      { ...source, photoCount: 330, roots: [{ scopeId: 'opaque-0' }] },
      { id: 'multi', mode: 'connected', photoCount: 1000, roots: [{ scopeId: 'multi-0' }, { scopeId: 'multi-1' }] },
    ];
    const folders = [folder(0, { count: 999 }), folder(1, { count: 17 }),
      folder(2, { scopeId: 'multi-0', sourceId: 'multi', name: 'Multiple root one', count: 400 }),
      folder(3, { scopeId: 'multi-1', sourceId: 'multi', name: 'Multiple root two', count: 600 })];
    await ui.state(snapshot({ sources, folders }));
    const count = label => ui.navigationButton(label).querySelector('.pf-nav-count').textContent;
    assert.equal(count('Folder 0'), '330'); assert.equal(count('Folder 1'), '17');
    assert.equal(count('Multiple root one'), '400'); assert.equal(count('Multiple root two'), '600');
    ui.handlers.set('/api/action', () => pending.promise);
    ui.button('Subfolders of Folder 0').click(); await flush(); const request = ui.calls.at(-1);
    await ui.state(snapshot({ sources: [{ ...sources[0], photoCount: 331 }, sources[1]], folders }));
    assert.equal(count('Folder 0'), '331'); assert.equal(request.signal.aborted, true);
    pending.resolve({ children: [{ scopeId: 'outdated', name: 'Old count child' }] }); await flush();
    assert.equal(ui.navigationButton('Old count child'), undefined);
    assert.equal(count('Folder 1'), '17'); assert.equal(count('Multiple root one'), '400');
  } finally { await ui.close(); }
});
