'use strict';

// Exercise the real guide handlers without a browser server, worker, photos or network.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../web-companion/onboarding.js'), 'utf8');
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const clone = (value) => structuredClone(value);
const fresh = (libraryId) => ({ version: 1, libraryId, step: 0, automaticEnabled: true,
  completed: false, hasStarted: false, starterSourceId: '', starterRootId: '', taggingJobID: '' });

function harness(initial = {}) {
  let document;
  class Node {
    constructor(tag = 'div', value = '') {
      this.tagName = tag.toUpperCase(); this._text = value; this.children = []; this.parentNode = null;
      this.className = ''; this.attributes = {}; this.listeners = {}; this.value = ''; this.checked = false;
      this.hidden = false; this.disabled = false; this.open = false; this.isConnected = true;
    }
    append(...nodes) { for (const node of nodes) { node.remove(); this.children.push(node); node.parentNode = this; node.isConnected = true; } }
    replaceChildren(...nodes) { for (const child of this.children) child.parentNode = null; this.children = []; this._text = ''; this.append(...nodes); }
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((child) => child !== this); this.parentNode = null; this.isConnected = false; }
    setAttribute(key, value) { this.attributes[key] = String(value); }
    addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
    async fire(name, extra = {}) { for (const handler of this.listeners[name] || []) await handler({ target: this, preventDefault() {}, ...extra }); }
    click() { return this.disabled ? Promise.resolve() : this.fire('click'); }
    focus() { document.activeElement = this; }
    showModal() { this.open = true; }
    close() { this.open = false; }
    get textContent() { return this._text + this.children.map((node) => node.textContent).join(''); }
    set textContent(value) { this._text = String(value); this.children = []; }
  }
  const descendants = (node) => node.children.flatMap((child) => [child, ...descendants(child)]);
  const body = new Node('body');
  const banner = new Node('section'); banner.id = 'onboarding'; banner.hidden = true;
  const navigation = new Node('div'); navigation.id = 'workspace-navigation';
  body.append(navigation, banner);
  document = {
    body, activeElement: null, createElement: (tag) => new Node(tag),
    getElementById: (id) => id === 'onboarding' ? banner : id === 'workspace-navigation' ? navigation : null,
  };
  const state = { libraryId: 'library-a', library: { total: 12, tagged: 2 }, folders: [], sources: [],
    settings: { modelConfigured: true, projectorConfigured: true, modelLabel: 'vision.gguf', projectorLabel: 'projector.gguf' },
    jobs: { jobs: [] }, libraryError: null, ...initial.state };
  const saved = new Map();
  saved.set(state.libraryId, { ...fresh(state.libraryId), ...initial.guide });
  let connected = true;
  let guideFailure = false;
  let pendingPage = initial.pendingPage || { photos: [], totalCount: 0 };
  let actionResponse = initial.actionResponse || { id: 'starter-job' };
  const calls = [], errors = [], notices = [], events = new Map();
  const bridge = {
    isConnected: () => connected,
    getState: () => state,
    showError: (message) => errors.push(String(message)),
    showNotice: (message) => notices.push(String(message)),
    async api(route, payload = {}) {
      calls.push({ route, payload: clone(payload) });
      if (route === '/api/library-info') return { libraryId: state.libraryId, locationLabel: '/Users/example/Library/Application Support/PhotoRecall', ready: true, error: null };
      if (route === '/api/onboarding') {
        if (guideFailure) throw Error('Invalid saved guide.');
        return clone(saved.get(state.libraryId) || fresh(state.libraryId));
      }
      if (route === '/api/onboarding/update') {
        assert.equal(payload.libraryId, state.libraryId);
        const next = { ...(saved.get(state.libraryId) || fresh(state.libraryId)), ...payload.changes };
        saved.set(state.libraryId, next); return clone(next);
      }
      if (route === '/api/onboarding/reset') {
        guideFailure = false; saved.set(state.libraryId, fresh(state.libraryId));
        return clone(saved.get(state.libraryId));
      }
      if (route === '/api/library') return typeof pendingPage === 'function' ? pendingPage(payload) : clone(pendingPage);
      throw Error(`Unexpected API route: ${route}`);
    },
    async action(name, params = {}) {
      calls.push({ route: `action:${name}`, payload: clone(params) });
      if (name === 'enqueueJob') return typeof actionResponse === 'function' ? actionResponse(params) : clone(actionResponse);
      return { opened: true };
    },
    async pick(kind) { calls.push({ route: `pick:${kind}`, payload: {} }); return { grantId: 'a'.repeat(64), label: 'Chosen folder' }; },
    async refreshLibrary() { calls.push({ route: 'refreshLibrary', payload: {} }); },
  };
  class CustomEvent { constructor(type, { detail } = {}) { this.type = type; this.detail = detail; } }
  const window = {
    PhotoFindry: bridge,
    addEventListener(name, handler) { if (!events.has(name)) events.set(name, []); events.get(name).push(handler); },
    dispatchEvent(event) { for (const handler of events.get(event.type) || []) handler(event); },
  };
  vm.runInNewContext(source, { window, document, CustomEvent, AbortController, queueMicrotask,
    Intl, Number, String, Array, Set, Map, JSON, Promise, Error, Object }, { filename: 'onboarding.js' });
  const all = () => descendants(body);
  const findButton = (label) => { const item = all().find((node) => node.tagName === 'BUTTON' && node.textContent === label); assert(item, `Missing button: ${label}`); return item; };
  const dialog = (className) => all().find((node) => node.tagName === 'DIALOG' && node.className === className && node.open);
  const emit = (name, detail) => window.dispatchEvent(new CustomEvent(`photofindry:${name}`, { detail }));
  emit('state', state);
  return { state, saved, calls, errors, notices, bridge, window, all, findButton, dialog, banner, navigation, emit,
    setPendingPage(value) { pendingPage = value; }, setActionResponse(value) { actionResponse = value; },
    failGuide() { guideFailure = true; },
    async ready() { await tick(); await tick(); },
    async disconnect() { connected = false; emit('session', { connected: false }); await tick(); },
    async reconnect() { connected = true; emit('session', { connected: true }); emit('state', state); await tick(); await tick(); },
  };
}

test('navigation and postponing preserve a host-owned chapter without starting work', async () => {
  const ui = harness(); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'existing configured library does not force the guide');
  await ui.findButton('Getting started').click();
  assert(ui.dialog('pf-guide-dialog'));
  await ui.findButton('4. Your first tags').click();
  assert.equal(ui.saved.get('library-a').step, 3);
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 0);
  await ui.findButton('Continue later').click();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined);
  await ui.findButton('Getting started').click();
  assert(ui.dialog('pf-guide-dialog').textContent.includes('CHAPTER 4 OF 10'));
  await ui.findButton('Start guide again').click();
  assert.equal(ui.saved.get('library-a').step, 0);
  assert(ui.dialog('pf-guide-dialog').textContent.includes('CHAPTER 1 OF 10'));
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 0);
});

test('live checklist and banner update in place without losing focus', async () => {
  const ui = harness({ guide: { step: 9, hasStarted: true } }); await ui.ready();
  const bannerTitle = ui.banner.children[0];
  await ui.findButton('Getting started').click();
  const checklist = ui.all().find((node) => node.className === 'pf-guide-checklist'); assert(checklist);
  const modelLine = checklist.children[0]; modelLine.focus();
  ui.state.library.tagged = 7; ui.state.settings.modelLabel = 'different.gguf';
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.banner.children[0], bannerTitle);
  assert.equal(checklist.children[0], modelLine);
  assert.equal(ui.dialog('pf-guide-dialog')?.open, true);
  assert(modelLine.textContent.includes('configured'));
  const automatic = ui.all().find((node) => node.className === 'pf-guide-toggle').children[0];
  automatic.checked = false; await automatic.fire('change'); await ui.ready();
  assert.equal(ui.saved.get('library-a').automaticEnabled, false);
});

test('guide state rebinds by library, auto-opens only a successful new empty library, and resets explicitly', async () => {
  const ui = harness({ guide: { step: 5, hasStarted: true, completed: true } }); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined);
  ui.state.libraryId = 'library-b'; ui.state.library = { total: 8, tagged: 0 };
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'different existing library is not forced to setup');
  assert.equal(ui.saved.get('library-a').step, 5);
  await ui.findButton('Getting started').click();
  assert.equal(ui.saved.get('library-b').step, 0);
  await ui.findButton('Continue later').click();
  ui.failGuide(); ui.emit('session', { connected: false });
  // Reconnect to the same library with a malformed host record; only explicit reset changes it.
  ui.bridge.isConnected = () => true;
  ui.emit('state', ui.state); await ui.ready();
  assert(ui.banner.textContent.includes('Invalid saved guide.'));
  await ui.findButton('Reset guide progress').click();
  assert(ui.dialog('pf-guide-dialog'));
  assert.equal(ui.saved.get('library-b').step, 0);
});

test('new empty library auto-opens but navigation never adds folders or starts tagging', async () => {
  const ui = harness({ state: { library: { total: 0, tagged: 0 }, settings: { modelConfigured: false, projectorConfigured: false } } });
  await ui.ready();
  assert(ui.dialog('pf-guide-dialog'));
  assert.equal(ui.saved.get('library-a').hasStarted, true);
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob' || call.route === 'pick:folder').length, 0);
  await ui.findButton('Continue later').click();
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'postpone remains effective in this session');
});

test('reviewed starter sample submits only explicit pending IDs and blocks duplicate active work', async () => {
  const ui = harness({ state: { library: { total: 20, tagged: 0 }, folders: [
    { name: 'Starter', count: 20, sourceId: 'source-a', sourceRootId: 'root-a', scopeId: 'scope-a' },
  ], sources: [{ id: 'source-a', label: 'Starter' }] }, pendingPage: {
    photos: Array.from({ length: 11 }, (_, index) => ({ id: `photo-${index}`, name: `Photo ${index}.jpg` })), totalCount: 20,
  } });
  await ui.ready(); await ui.findButton('Getting started').click(); await ui.findButton('4. Your first tags').click();
  const selector = ui.all().find((node) => node.tagName === 'SELECT' && node.attributes['aria-label'] === 'Starter photo folder');
  assert(selector);
  selector.value = '0'; await selector.fire('change'); await ui.ready();
  assert.deepEqual(ui.calls.find((call) => call.route === '/api/library').payload,
    { sourceId: 'source-a', scopeId: 'scope-a', status: 'pending', sort: 'library', offset: 0 });
  assert.equal(ui.saved.get('library-a').starterSourceId, 'source-a');
  assert.equal(ui.saved.get('library-a').starterRootId, 'root-a');
  assert.equal(Object.hasOwn(ui.saved.get('library-a'), 'scopeId'), false);
  assert.equal(ui.all().filter((node) => node.className === 'pf-guide-sample-row').length, 8);
  assert(ui.findButton('Tag 3 selected photos'));
  const first = ui.all().find((node) => node.className === 'pf-guide-sample-row').children[0];
  first.checked = false; await first.fire('change');
  ui.state.jobs.jobs.push({ id: 'another-tagging-job', kind: 'tagging', state: 'running' });
  ui.emit('state', ui.state); assert(ui.findButton('Tag 2 selected photos').disabled);
  ui.state.jobs.jobs.pop(); ui.emit('state', ui.state);
  await ui.findButton('Tag 2 selected photos').click();
  const submissions = ui.calls.filter((call) => call.route === 'action:enqueueJob');
  assert.equal(submissions.length, 1);
  assert.deepEqual(submissions[0].payload, { kind: 'tagging', params: {
    photoIds: ['photo-1', 'photo-2'], sourceId: 'source-a', scopeId: 'scope-a',
  } });
  assert.equal(ui.saved.get('library-a').taggingJobID, 'starter-job');
  ui.state.jobs.jobs.push({ id: 'starter-job', kind: 'tagging', state: 'running', completed: 0, total: 2 });
  ui.emit('state', ui.state); await ui.ready();
  assert(ui.findButton('Tag 0 selected photos').disabled);
  await ui.findButton('Tag 0 selected photos').click();
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 1);
});

test('uncertain starter submission cannot be repeated from the guide', async () => {
  const ui = harness({ state: { folders: [{ name: 'Starter', count: 3, sourceId: 'source-a', sourceRootId: 'root-a', scopeId: 'scope-a' }] },
    pendingPage: { photos: [{ id: 'one', name: 'One.jpg' }], totalCount: 1 },
    actionResponse: () => { throw Error('The request timed out.'); } });
  await ui.ready(); await ui.findButton('Getting started').click(); await ui.findButton('4. Your first tags').click();
  const selector = ui.all().find((node) => node.tagName === 'SELECT'); selector.value = '0'; await selector.fire('change'); await ui.ready();
  await ui.findButton('Tag 1 selected photo').click();
  assert(ui.errors.at(-1).includes('Check Jobs before trying again'));
  assert(ui.findButton('Tag 1 selected photo').disabled);
  await ui.findButton('Tag 1 selected photo').click();
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 1);
});

test('adding a source sends only a native picker grant', async () => {
  const ui = harness(); await ui.ready(); await ui.findButton('Getting started').click();
  await ui.findButton('3. Photo folders').click();
  await ui.findButton('Add photo folder').click();
  assert.deepEqual(ui.calls.find((call) => call.route === 'action:addFolders').payload,
    { folderGrants: ['a'.repeat(64)] });
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 0);
});

test('late sample responses are discarded after a library switch', async () => {
  let release;
  const waiting = new Promise((resolve) => { release = resolve; });
  const ui = harness({ state: { folders: [{ name: 'Starter', count: 1, sourceId: 'source-a', sourceRootId: 'root-a', scopeId: 'scope-a' }] },
    pendingPage: () => waiting });
  await ui.ready(); await ui.findButton('Getting started').click(); await ui.findButton('4. Your first tags').click();
  const selector = ui.all().find((node) => node.tagName === 'SELECT'); selector.value = '0'; await selector.fire('change');
  ui.state.libraryId = 'library-b'; ui.emit('state', ui.state); await ui.ready();
  release({ photos: [{ id: 'old-photo', name: 'Old photo.jpg' }], totalCount: 1 }); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined);
  assert.equal(ui.calls.filter((call) => call.route === 'action:enqueueJob').length, 0);
});

test('This library reveals only the host-selected data folder and explains portable limits', async () => {
  const ui = harness(); await ui.ready();
  await ui.findButton('This library').click();
  const dialog = ui.dialog('pf-library-dialog'); assert(dialog);
  assert(dialog.textContent.includes('/Users/example/Library/Application Support/PhotoRecall'));
  assert(dialog.textContent.includes('Original photographs remain'));
  assert(dialog.textContent.includes('omit original photographs'));
  await ui.findButton('Show data folder in Finder').click();
  assert.deepEqual(ui.calls.find((call) => call.route === 'action:revealLibrary').payload, {});
  assert(!source.includes('localStorage'));
  assert.deepEqual(ui.errors, []);
});

test('This library opens the reviewed source-removal flow and Files keeps its internal route', async () => {
  const ui=harness({state:{sources:[{id:'source-a',label:'My photographs',mode:'connected'}]}});await ui.ready();
  let opened=0;ui.window.PhotoFindryPanels={async removeSource(){opened++;}};
  await ui.findButton('This library').click();
  const dialog=ui.dialog('pf-library-dialog');assert.match(dialog.textContent,/verified, restorable archive/);
  await ui.findButton('Remove source from gallery…').click();assert.equal(opened,1);assert.equal(ui.dialog('pf-library-dialog'),undefined);
  assert(!ui.calls.some(call=>call.route.startsWith('action:')));
  const destinations=[];ui.window.addEventListener('photofindry:guide-destination',event=>destinations.push(event.detail.panel));
  await ui.findButton('Getting started').click();await ui.findButton('3. Photo folders').click();await ui.findButton('Open Files').click();
  assert.deepEqual(destinations,['sources']);assert.deepEqual(ui.errors,[]);
});

test('This library cannot remove an absent or already archived source', async () => {
  for(const sources of [[],[{id:'archived-source',mode:'archived'}]]) {
    const ui=harness({state:{sources}});await ui.ready();
    ui.window.PhotoFindryPanels={async removeSource(){assert.fail('No active source should be removable.');}};
    await ui.findButton('This library').click();assert.equal(ui.findButton('Remove source from gallery…').disabled,true);
    await ui.findButton('Remove source from gallery…').click();assert.deepEqual(ui.errors,[]);
  }
});

test('Continue later survives transient loading and same-library attachment restart without hiding its result', async () => {
  const ui = harness({ guide: { step: 1, hasStarted: true } }); await ui.ready();
  assert(ui.dialog('pf-guide-dialog'));
  await ui.findButton('Continue later').click();
  const reads = ui.calls.filter(call => call.route === '/api/onboarding').length;
  ui.emit('state', { libraryId: null, loading: true });
  ui.emit('library-restored', { libraryGeneration: 2 });
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'The source-attachment result must remain visible after its worker restart.');
  assert.equal(ui.calls.filter(call => call.route === '/api/onboarding').length, reads + 1, 'Host-owned guide state is still reloaded after replacement.');
  assert.equal(ui.saved.get('library-a').step, 1); assert.equal(ui.saved.get('library-a').automaticEnabled, true);
  ui.emit('state', { ...ui.state, loading: true });
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'A loading snapshot retaining the library ID also preserves postponement.');
  await ui.findButton('Getting started').click();
  assert.match(ui.dialog('pf-guide-dialog').textContent, /CHAPTER 2 OF 10/);
  await ui.findButton('Start guide again').click();
  assert.match(ui.dialog('pf-guide-dialog').textContent, /CHAPTER 1 OF 10/);
  assert(!ui.calls.some(call => call.route === 'action:enqueueJob'));
});

test('explicit guide opening counts for the session, while another new library and a fresh login may offer their guide', async () => {
  const ui = harness(); await ui.ready();
  await ui.findButton('Getting started').click(); await ui.findButton('Continue later').click();
  ui.emit('library-restored', {}); ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'A manually opened and dismissed guide must not become an automatic offer after restart.');
  const original = clone(ui.state);
  Object.assign(ui.state, { libraryId: 'library-b', library: { total: 0, tagged: 0 }, settings: { modelConfigured: false, projectorConfigured: false } });
  ui.emit('state', ui.state); await ui.ready();
  assert(ui.dialog('pf-guide-dialog'), 'A genuinely different empty library receives its first offer.');
  await ui.findButton('Continue later').click();
  Object.assign(ui.state, original); ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined, 'Returning to an already offered library preserves its session dismissal.');
  await ui.disconnect(); await ui.reconnect();
  assert(ui.dialog('pf-guide-dialog'), 'A new authenticated session follows the saved show-at-launch preference.');
  assert.equal(ui.saved.get('library-a').automaticEnabled, true);
  assert(!ui.calls.some(call => call.route === 'action:enqueueJob'));
});

test('same-library worker replacement discards old sample data while keeping automatic guide suppression', async () => {
  let release;
  const waiting = new Promise(resolve => { release = resolve; });
  const ui = harness({ state: { folders: [{ name: 'Starter', count: 1, sourceId: 'source-a', sourceRootId: 'root-a', scopeId: 'scope-a' }] }, pendingPage: () => waiting });
  await ui.ready(); await ui.findButton('Getting started').click(); await ui.findButton('4. Your first tags').click();
  const selector = ui.all().find(node => node.tagName === 'SELECT'); selector.value = '0'; await selector.fire('change'); await ui.ready();
  ui.emit('state', { ...ui.state, loading: true });
  ui.emit('library-restored', { libraryGeneration: 2 });
  release({ photos: [{ id: 'old-photo', name: 'Old sample.jpg' }], totalCount: 1 });
  ui.emit('state', ui.state); await ui.ready();
  assert.equal(ui.dialog('pf-guide-dialog'), undefined);
  assert(!ui.all().some(node => node.textContent.includes('Old sample.jpg')));
  ui.setPendingPage({ photos: [{ id: 'current-photo', name: 'Current sample.jpg' }], totalCount: 1 });
  await ui.findButton('Getting started').click(); await ui.ready();
  assert.match(ui.dialog('pf-guide-dialog').textContent, /Current sample.jpg/);
  assert.doesNotMatch(ui.dialog('pf-guide-dialog').textContent, /Old sample.jpg/);
  await ui.findButton('Tag 1 selected photo').click();
  assert.deepEqual(ui.calls.filter(call => call.route === 'action:enqueueJob').map(call => call.payload.params.photoIds), [['current-photo']]);
});
