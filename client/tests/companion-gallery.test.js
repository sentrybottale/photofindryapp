'use strict';

// Exercise the actual browser entry point with delayed HTTP responses. No model,
// worker, user library or external service is involved.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const photo = (id, extra = {}) => ({ id, name: `${id}.jpg`, caption: `Saved ${id}`, status: 'tagged', hasPreview: true, previewRevision: `${id}-v1`, ...extra });
const page = (photos, extra = {}) => ({ photos, totalCount: photos.length, offset: 0, limit: 60, ...extra });
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

function harness({ storage = new Map(), origin = 'http://127.0.0.1:54321', fragment = '', initialHandlers = {}, storageDenied = false, historyDenied = false, confirm = () => false, shellMount, summaryMount } = {}) {
  let document;
  class Node {
    constructor(tag = 'div', value = '') {
      this.tagName = tag.toUpperCase(); this.children = []; this.parentNode = null;
      this._text = value; this.value = ''; this.dataset = {}; this.attributes = {}; this.listeners = {};
      this.hidden = false; this.disabled = false; this.open = false; this.className = '';
      this.classList = { toggle: (name, on) => { const names = new Set(this.className.split(' ').filter(Boolean)); on ? names.add(name) : names.delete(name); this.className = [...names].join(' '); } };
    }
    append(...nodes) { for (let node of nodes) { if (typeof node === 'string') node = new Node('#text', node); if (node.tagName === 'FRAGMENT') { this.append(...[...node.children]); continue; } node.remove(); node.parentNode = this; this.children.push(node); } }
    insertBefore(node, before) { node.remove(); const at = before ? this.children.indexOf(before) : this.children.length; assert(at >= 0); this.children.splice(at, 0, node); node.parentNode = this; }
    replaceChildren(...nodes) { for (const child of this.children) child.parentNode = null; this.children = []; this._text = ''; this.append(...nodes); }
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(child => child !== this); this.parentNode = null; }
    setAttribute(key, value) { this.attributes[key] = String(value); }
    removeAttribute(key) { delete this.attributes[key]; }
    addEventListener(name, listener) { (this.listeners[name] ||= []).push(listener); }
    async fire(name, extra = {}) { for (const listener of this.listeners[name] || []) await listener({ target: this, preventDefault() {}, ...extra }); }
    click(extra) { return this.disabled ? Promise.resolve() : this.fire('click', extra); }
    focus() { document.activeElement = this; }
    scrollIntoView() {}
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    showModal() { this.open = true; }
    close() { if (!this.open) return; this.open = false; queueMicrotask(() => this.fire('close')); }
    querySelector(selector) { return descendants(this).find(node => selector.startsWith('.') ? node.className.split(' ').includes(selector.slice(1)) : node.tagName === selector.toUpperCase()) || null; }
    querySelectorAll(selector) { return descendants(this).filter(node => selector === '[data-detail-action]' && node.dataset.detailAction); }
    get firstChild() { return this.children[0]; }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this.replaceChildren(); this._text = String(value); }
  }
  const descendants = node => node.children.flatMap(child => [child, ...descendants(child)]);
  const html = fs.readFileSync(path.join(__dirname, '../web-companion/index.html'), 'utf8');
  const ids = new Map([...html.matchAll(/\bid="([^"]+)"/g)].map(match => [match[1], new Node()]));
  ids.get('app').hidden = true;
  const body = new Node('body');
  for (const [id, node] of ids) { node.id = id; body.append(node); }
  document = { body, activeElement: null, getElementById: id => ids.get(id), createElement: tag => new Node(tag), createTextNode: value => new Node('#text', value), createDocumentFragment: () => new Node('fragment') };
  const events = new Map();
  const confirmations = [];
  class CustomEvent { constructor(type, { detail } = {}) { this.type = type; this.detail = detail; } }
  const window = { addEventListener(name, listener) { if (!events.has(name)) events.set(name, []); events.get(name).push(listener); }, dispatchEvent(event) { for (const listener of events.get(event.type) || []) listener(event); }, confirm(message) { confirmations.push(message); return confirm(message); } };
  const history = [];
  window.location = { origin, hash: fragment, pathname: '/', search: '' };
  window.history = { replaceState(state, title, url) {
    if (historyDenied) throw new DOMException('History disabled', 'SecurityError');
    history.push({ state, title, url }); window.location.hash = '';
  } };
  if (shellMount || summaryMount) window.PhotoFindryShell = { mount: target => shellMount?.({ target, fragment: window.location.hash, history }), mountPhotoSummary: summaryMount };
  Object.defineProperty(window, 'sessionStorage', { get() {
    if (storageDenied) throw new DOMException('Storage disabled', 'SecurityError');
    return { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key) };
  } });
  const state = { galleryRevision: 1, libraryGeneration: 0 };
  let elapsed = 0;
  class Clock extends Date { static now() { return Date.now() + elapsed; } }
  const calls = [], createdURLs = [], revokedURLs = [], handlers = { ...initialHandlers };
  let library = page([photo('a'), photo('b')]);
  const fetch = async (route, request) => {
    const input = JSON.parse(request.body); calls.push({ route, input, signal: request.signal, headers: request.headers, fragment: window.location.hash });
    if (handlers[route]) {
      const result = await handlers[route](input, request);
      if (result instanceof Response) return result;
      return Response.json(result);
    }
    if (route === '/api/pair') return Response.json({ token: 'a'.repeat(64), apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' });
    if (route === '/api/bootstrap') return Response.json({ token: 'a'.repeat(64), apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' });
    if (route === '/api/session') return Response.json({ apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' });
    if (route === '/api/state') return Response.json(state);
    if (route === '/api/library') return Response.json(library);
    if (route === '/api/photo') return Response.json(photo(input.photoId, { detailSections: [{ title: 'Accepted facts', items: ['A saved fact'] }] }));
    if (route === '/api/preview') return new Response(Buffer.from('fixture image'), { headers: { 'Content-Type': 'image/png' } });
    if (route === '/api/disconnect') return Response.json({});
    if (route === '/api/quit') return Response.json({});
    throw Error(`Unhandled fixture route ${route}`);
  };
  const urls = { createObjectURL() { const url = `blob:fixture-${createdURLs.length}`; createdURLs.push(url); return url; }, revokeObjectURL(url) { revokedURLs.push(url); } };
  const timers = new Map();
  const timeout = (fn, ms) => { const timer = setTimeout(() => { timers.delete(timer); fn(); }, ms); timers.set(timer, ms); return timer; };
  const clear = timer => { timers.delete(timer); clearTimeout(timer); };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../web-companion/app.js'), 'utf8'), {
    window, document, CustomEvent, fetch, URL: urls, AbortController, DOMException, Blob, Response,
    setTimeout: timeout, clearTimeout: clear, queueMicrotask, Intl, Date: Clock, getComputedStyle: () => ({ gridTemplateColumns: '1fr 1fr' }),
  }, { filename: 'app.js' });
  const flush = async () => { for (let i = 0; i < 12; i++) await new Promise(resolve => setImmediate(resolve)); };
  return { ids, window, document, calls, createdURLs, revokedURLs, handlers, state, flush, storage, timers, history, confirmations,
    emit(type, extra = {}) { window.dispatchEvent({ type, ...extra }); },
    retrySession() { return descendants(ids.get('pair-form')).find(node => node.textContent === 'Retry local connection'); },
    advance(ms) { elapsed += ms; },
    setLibrary(next) { library = next; },
    async pair() { ids.get('pair-code').value = 'test-code'; await ids.get('pair-form').fire('submit'); await flush(); },
    async close() { await ids.get('disconnect').click(); await flush(); },
  };
}

test('framework mounts only after the launch ticket is removed and before authenticated requests', async () => {
  let mounted = false;
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, shellMount({ target, fragment, history }) {
    assert.equal(target.id, 'photofindry-root'); assert.equal(fragment, ''); assert.equal(history.length, 1); mounted = true;
  }, initialHandlers: { '/api/bootstrap': () => {
    assert(mounted); return { token: 'a'.repeat(64), apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' };
  } } });
  try { await ui.flush(); assert(mounted); assert.equal(ui.ids.get('app').hidden, false); }
  finally { await ui.close(); }
});

test('framework startup failure clears the launch ticket and offers a usable local reopen action', async () => {
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, shellMount() { throw Error('Fixture failed mount'); } });
  try {
    await ui.flush(); assert.equal(ui.window.location.hash, ''); assert.equal(ui.calls.length, 0);
    assert.equal(ui.ids.get('pair-error').hidden, false);
    assert.match(ui.ids.get('pair-error').textContent, /Open PhotoFindry again using the button above/);
    assert.equal(ui.ids.get('app').hidden, true);
    assert.match(fs.readFileSync(path.join(__dirname, '../web-companion/index.html'), 'utf8'), /class="primary-button local-open-link" href="photofindry-local:\/\/open"/);
  } finally { await ui.close(); }
});

test('a browser that cannot clear its launch fragment stays on the static recovery screen', async () => {
  let mounted = false;
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, historyDenied: true, shellMount() { mounted = true; } });
  try {
    await ui.flush(); assert.equal(mounted, false); assert.equal(ui.calls.length, 0);
    assert.match(ui.ids.get('pair-error').textContent, /could not clear this local launch link/);
    assert.equal(ui.ids.get('app').hidden, true);
  } finally { await ui.close(); }
});

test('same-page refresh keeps decoded tiles, selection, inspector and focus through a delayed response', async () => {
  const ui = harness();
  try {
    await ui.pair();
    const gallery = ui.ids.get('gallery'), tile = gallery.children[0], image = tile.querySelector('img');
    await tile.click(); await ui.flush();
    const inspector = ui.ids.get('inspector-content'), detailNode = inspector.children[0];
    assert.match(inspector.textContent, /A saved fact/);
    const delayed = deferred(); ui.handlers['/api/library'] = () => delayed.promise;
    const refreshing = ui.window.PhotoFindry.refreshCurrentPage();
    assert.equal(gallery.children[0], tile); assert.equal(tile.querySelector('img'), image);
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['a']);
    assert.equal(inspector.children[0], detailNode); assert.equal(ui.document.activeElement, gallery);
    assert.equal(ui.ids.get('loading-state').hidden, true);
    delayed.resolve(page([photo('a'), photo('b')])); await refreshing; await ui.flush();
    assert.equal(gallery.children[0], tile); assert.equal(tile.querySelector('img'), image);
    assert.equal(inspector.children[0], detailNode); assert.equal(ui.document.activeElement, gallery);
    assert.equal(ui.calls.filter(call => call.route === '/api/preview').length, 2);
    assert.deepEqual(ui.revokedURLs, []);
  } finally { await ui.close(); assert.deepEqual([...ui.revokedURLs].sort(), [...ui.createdURLs].sort()); }
});

test('rank-only refresh preserves inspector focus; source revision refreshes omitted accepted details', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([photo('a', { score: 5 })])); await ui.pair();
    await ui.ids.get('gallery').children[0].click(); await ui.flush();
    const inspector = ui.ids.get('inspector-content');
    const button = inspector.querySelectorAll('[data-detail-action]')[0]; button.focus();
    const requests = ui.calls.filter(call => call.route === '/api/photo').length;
    ui.setLibrary(page([photo('a', { score: 2 })])); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.document.activeElement, button); assert.equal(inspector.querySelectorAll('[data-detail-action]')[0], button);
    assert.equal(ui.calls.filter(call => call.route === '/api/photo').length, requests);
    ui.handlers['/api/photo'] = () => photo('a', { detailSections: [{ title: 'Accepted facts', items: ['New saved fact'] }] });
    ui.state.galleryRevision = 2; await ui.window.PhotoFindry.refreshState();
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.match(inspector.textContent, /New saved fact/);
    assert.equal(ui.document.activeElement.dataset.detailAction, 'preview');
    assert(inspector.contains(ui.document.activeElement));
  } finally { await ui.close(); }
});

test('automatic refresh retries temporary preview failures with bounded backoff', async () => {
  const ui = harness(); let attempts = 0;
  try {
    ui.setLibrary(page([photo('a')]));
    ui.handlers['/api/preview'] = () => {
      attempts += 1;
      if (attempts === 1) return Response.json({ error: 'Temporary unavailable preview' }, { status: 503 });
      return new Response(Buffer.from('recovered'), { headers: { 'Content-Type': 'image/png' } });
    };
    await ui.pair(); assert.equal(attempts, 1);
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush(); assert.equal(attempts, 1);
    ui.advance(5001); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(attempts, 2); assert(ui.ids.get('gallery').children[0].querySelector('img'));
  } finally { await ui.close(); }
});

test('overlapping refreshes coalesce and preserve selection made while a page is in flight', async () => {
  const ui = harness();
  try {
    await ui.pair(); const delayed = deferred(); let requests = 0;
    ui.handlers['/api/library'] = () => ++requests === 1 ? delayed.promise : page([photo('a'), photo('b', { caption: 'Latest saved description' })]);
    const first = ui.window.PhotoFindry.refreshCurrentPage();
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.window.PhotoFindry.refreshCurrentPage();
    assert.equal(requests, 1);
    await ui.ids.get('gallery').children[1].click(); await ui.flush();
    delayed.resolve(page([photo('a'), photo('b')])); await first;
    await new Promise(resolve => setTimeout(resolve, 130)); await ui.flush();
    assert.equal(requests, 2); assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['b']);
    assert.match(ui.ids.get('gallery').children[1].textContent, /Latest saved description/);
    assert.equal(ui.calls.filter(call => call.route === '/api/preview').length, 2);
  } finally { await ui.close(); }
});

test('ordinary search replays a delayed content invalidation once without clearing its usable page or scope', async () => {
  const ui = harness(); let attempts = 0;
  try {
    await ui.pair(); ui.ids.get('search').value = 'boy';
    await ui.window.PhotoFindry.setScope({ sourceId: 'source-a', scopeId: 'folder-a', status: 'tagged', metadataFilters: { camera: 'saved-camera' } });
    const gallery = ui.ids.get('gallery'), tile = gallery.children[0];
    await tile.click(); await ui.flush();
    const delayed = deferred();
    ui.handlers['/api/library'] = input => {
      attempts += 1;
      assert.equal(input.query, 'boy'); assert.equal(input.sourceId, 'source-a'); assert.equal(input.scopeId, 'folder-a');
      assert.equal(input.status, 'tagged'); assert.equal(input.metadataFilters.camera, 'saved-camera');
      return attempts === 1 ? delayed.promise : page([photo('a', { caption: 'Current saved description' }), photo('b')]);
    };
    const search = ui.window.PhotoFindry.refreshCurrentPage();
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.window.PhotoFindry.refreshCurrentPage();
    delayed.resolve(Response.json({ error: 'The library changed during search. Run it again to use current details.' }, { status: 409 }));
    await ui.flush();
    assert.equal(attempts, 1); assert.equal(gallery.children[0], tile); assert.equal(ui.ids.get('library-error').hidden, true);
    await search; await new Promise(resolve => setTimeout(resolve, 150)); await ui.flush();
    assert.equal(attempts, 2); assert.equal(gallery.children[0], tile);
    assert.match(tile.textContent, /Current saved description/); assert.equal(ui.ids.get('search').value, 'boy');
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['a']); assert.equal(ui.ids.get('library-error').hidden, true);
    assert.equal(ui.calls.filter(call => call.route === '/api/action').length, 0);
  } finally { await ui.close(); }
});

test('a scope change cancels a queued search replay and fences its old query', async () => {
  const ui = harness(); const requests = [];
  try {
    await ui.pair(); ui.ids.get('search').value = 'old';
    ui.handlers['/api/library'] = input => {
      requests.push(input);
      return input.query === 'old'
        ? Response.json({ error: 'The operation was cancelled.' }, { status: 409 })
        : page([photo('new')]);
    };
    const old = ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(requests.length, 1); assert.equal(ui.ids.get('library-error').hidden, true);
    ui.ids.get('search').value = 'new';
    await ui.window.PhotoFindry.setScope({ sourceId: 'source-b' }); await old;
    await new Promise(resolve => setTimeout(resolve, 250)); await ui.flush();
    assert.deepEqual(requests.map(input => [input.query, input.sourceId]), [['old', undefined], ['new', 'source-b']]);
    assert.equal(ui.ids.get('gallery').children[0].dataset.photoId, 'new'); assert.equal(ui.ids.get('library-error').hidden, true);
  } finally { await ui.close(); }
});

test('a current ordinary search cancelled by the worker recovers without claiming the library failed to open', async () => {
  const ui = harness(); let attempts = 0;
  try {
    await ui.pair(); ui.ids.get('search').value = 'b';
    ui.handlers['/api/library'] = () => {
      attempts += 1;
      return attempts === 1 ? Response.json({ error: 'The operation was cancelled.' }, { status: 409 }) : page([photo('b')]);
    };
    const search = ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.ids.get('gallery').children.length, 0); assert.equal(ui.ids.get('library-error').hidden, true);
    assert.equal(ui.ids.get('result-count').textContent, 'Updating saved results…');
    await search; await ui.flush();
    assert.equal(attempts, 2); assert.equal(ui.ids.get('gallery').children[0].dataset.photoId, 'b');
    assert.equal(ui.ids.get('search').value, 'b'); assert.match(ui.ids.get('result-count').textContent, /1 match/);
    assert.equal(ui.ids.get('library-error').hidden, true);
  } finally { await ui.close(); }
});

test('persistent search churn stops after bounded recovery and state ticks cannot restart it until explicit retry', async () => {
  const ui = harness(); let attempts = 0; const destinations = [];
  try {
    ui.window.addEventListener('photofindry:guide-destination', event => destinations.push(event.detail.panel));
    await ui.pair(); ui.ids.get('search').value = 'boy'; await ui.window.PhotoFindry.refreshCurrentPage();
    const tile = ui.ids.get('gallery').children[0];
    ui.handlers['/api/library'] = () => {
      attempts += 1;
      return Response.json({ error: 'The library changed during search. Run it again to use current details.' }, { status: 409 });
    };
    await ui.window.PhotoFindry.refreshCurrentPage(); assert.equal(attempts, 4);
    assert.equal(ui.ids.get('gallery').children[0], tile); assert.equal(ui.ids.get('library-error').hidden, false);
    assert.match(ui.ids.get('library-error-text').textContent, /query and filters are kept.*Retry search.*Open Jobs/);
    assert.equal(ui.ids.get('result-count').textContent, 'Previous results · waiting for saved details');
    const jobs = ui.ids.get('library-error').children.find(node => node.textContent === 'Open Jobs');
    assert.equal(jobs.hidden, false); await jobs.click(); assert.deepEqual(destinations, ['jobs']);
    assert.doesNotMatch(ui.ids.get('result-count').textContent, /Library could not be loaded/);
    ui.state.galleryRevision += 1; await ui.window.PhotoFindry.refreshState();
    await new Promise(resolve => setTimeout(resolve, 750)); await ui.flush(); assert.equal(attempts, 4);
    ui.handlers['/api/library'] = () => { attempts += 1; return page([photo('recovered')]); };
    await ui.ids.get('retry-library').click(); await ui.flush();
    assert.equal(attempts, 5); assert.equal(ui.ids.get('gallery').children[0].dataset.photoId, 'recovered');
    assert.equal(ui.ids.get('search').value, 'boy'); assert.equal(ui.ids.get('library-error').hidden, true);
    assert.equal(jobs.hidden, true);
  } finally { await ui.close(); }
});

test('disconnect aborts a pending search replay and no cancelled read can resurrect the gallery', async () => {
  const ui = harness(); let attempts = 0;
  try {
    await ui.pair();
    ui.handlers['/api/library'] = () => {
      attempts += 1; return Response.json({ error: 'The operation was cancelled.' }, { status: 409 });
    };
    const search = ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    await ui.close(); await search;
    await new Promise(resolve => setTimeout(resolve, 250)); await ui.flush();
    assert.equal(attempts, 1); assert.equal(ui.ids.get('gallery').children.length, 0); assert.equal(ui.window.PhotoFindry.isConnected(), false);
    assert.equal(ui.ids.get('library-error').hidden, true);
  } finally { await ui.close(); }
});

test('real library-open failures and AI-result errors are actionable and never retried as ordinary search churn', async () => {
  const ui = harness(); let attempts = 0;
  try {
    ui.handlers['/api/library'] = () => {
      attempts += 1; return Response.json({ error: 'The local library is unavailable.' }, { status: 503 });
    };
    await ui.pair(); assert.equal(attempts, 1);
    assert.match(ui.ids.get('result-count').textContent, /Library could not be loaded/);
    assert.match(ui.ids.get('library-error-text').textContent, /local library is unavailable.*Try again.*reopen PhotoFindry/);
    let actions = 0;
    ui.handlers['/api/action'] = () => { actions += 1; return Response.json({ error: 'The library changed during search.' }, { status: 409 }); };
    await ui.window.PhotoFindry.openAIResults('saved-ai');
    await new Promise(resolve => setTimeout(resolve, 250)); await ui.flush();
    assert.equal(actions, 1); assert.equal(attempts, 1);
    assert.match(ui.ids.get('result-count').textContent, /AI search results could not be loaded/);
  } finally { await ui.close(); }
});

test('reordering reuses tile identities, while preview revision and permission changes retire only affected images', async () => {
  const ui = harness();
  try {
    await ui.pair(); const gallery = ui.ids.get('gallery'), [a, b] = gallery.children;
    const oldA = a.querySelector('img').src, oldB = b.querySelector('img').src;
    ui.setLibrary(page([photo('b'), photo('a')])); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(gallery.children[0], b); assert.equal(gallery.children[1], a); assert.deepEqual(ui.revokedURLs, []);
    ui.setLibrary(page([photo('b'), photo('a', { previewRevision: 'a-v2' })])); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(gallery.children[1], a); assert.notEqual(a.querySelector('img').src, oldA);
    assert.equal(b.querySelector('img').src, oldB); assert.deepEqual(ui.revokedURLs, [oldA]);
    ui.setLibrary(page([photo('b', { hasPreview: false, source: { id: 'source-b', mode: 'metadata' } }), photo('a', { previewRevision: 'a-v2' })]));
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(b.querySelector('img'), null); assert(ui.revokedURLs.includes(oldB));
  } finally { await ui.close(); }
});

test('new AI search shows its own pending state without old results, direct counts or scope loss', async () => {
  const ui = harness();
  try {
    await ui.pair(); await ui.window.PhotoFindry.setScope({ sourceId: 'source-a', metadataFilters: { camera: 'saved-camera' } });
    ui.handlers['/api/action'] = ({ params }) => page([photo('a', { matchKind: 'ai-ranked' })], { aiJobId: params.id, aiResultsReady: true, aiQuery: 'boy', aiSourceId: 'source-a', aiMetadataFilters: { camera: 'saved-camera' }, aiCandidateCount: 16, aiRefinedCount: 16, directCount: 0 });
    await ui.window.PhotoFindry.openAIResults('old'); await ui.flush();
    assert.match(ui.ids.get('result-count').textContent, /candidate/); assert.doesNotMatch(ui.ids.get('result-count').textContent, /direct|match/);
    const pending = deferred(); ui.handlers['/api/action'] = () => pending.promise;
    const next = ui.window.PhotoFindry.openAIResults('new');
    assert.equal(ui.window.PhotoFindry.getPage(), null); assert.equal(ui.ids.get('gallery').children.length, 0);
    pending.resolve(page([], { aiJobId: 'new', aiResultsPending: true, aiResultsReady: false, aiQuery: 'boy' })); await next;
    assert.equal(ui.window.PhotoFindry.getPage().aiJobId, 'new');
    assert.equal(ui.ids.get('library-error').hidden, true); assert.match(ui.ids.get('empty-title').textContent, /Preparing/);
    assert.equal(ui.window.PhotoFindry.getScope().sourceId, 'source-a');
    assert.equal(ui.window.PhotoFindry.getScope().metadataFilters.camera, 'saved-camera');
    ui.handlers['/api/action'] = () => page([], { aiJobId: 'new', aiResultsPending: false, aiResultsReady: false, aiStopped: true,
      aiQuery: 'boy', aiSourceId: 'source-b', aiScopeId: 'folder-b', aiMetadataFilters: { camera: 'other-camera' }, aiLocationOptions: {} });
    await ui.window.PhotoFindry.refreshCurrentPage();
    assert.equal(ui.window.PhotoFindry.getScope().sourceId, 'source-b');
    assert.equal(ui.window.PhotoFindry.getScope().scopeId, 'folder-b');
    assert.equal(ui.window.PhotoFindry.getScope().metadataFilters.camera, 'other-camera');
  } finally { await ui.close(); }
});

test('a superseded page or mismatched AI job cannot overwrite the selected view', async () => {
  const ui = harness();
  try {
    await ui.pair(); const delayed = deferred(); ui.handlers['/api/library'] = () => delayed.promise;
    const old = ui.window.PhotoFindry.refreshCurrentPage();
    ui.handlers['/api/action'] = () => page([photo('c')], { aiJobId: 'new', aiResultsReady: true });
    await ui.window.PhotoFindry.openAIResults('new'); delayed.resolve(page([photo('stale')])); await old; await ui.flush();
    assert.equal(ui.ids.get('gallery').children[0].dataset.photoId, 'c');
    ui.handlers['/api/action'] = () => page([photo('wrong')], { aiJobId: 'other', aiResultsReady: true });
    await ui.window.PhotoFindry.refreshCurrentPage();
    assert.equal(ui.ids.get('gallery').children[0].dataset.photoId, 'c');
    assert.match(ui.ids.get('library-error-text').textContent, /different AI search/);
  } finally { await ui.close(); }
});

test('disconnect and explicit AI invalidation clear images immediately and fence late preview replies', async () => {
  const ui = harness();
  try {
    await ui.pair();
    ui.handlers['/api/action'] = () => page([photo('c')], { aiJobId: 'new', aiResultsReady: true });
    const image = deferred(); ui.handlers['/api/preview'] = () => image.promise;
    await ui.window.PhotoFindry.openAIResults('new');
    ui.state.aiResults = { aiJobId: 'new', invalidated: true, requestId: 'request' };
    await ui.window.PhotoFindry.refreshState();
    assert.equal(ui.ids.get('gallery').children.length, 0);
    await ui.close();
    image.resolve(new Response(Buffer.from('late'), { headers: { 'Content-Type': 'image/png' } })); await ui.flush();
    assert.equal(ui.ids.get('gallery').children.length, 0); assert.equal(ui.createdURLs.length, 2);
    assert.deepEqual([...ui.revokedURLs].sort(), [...ui.createdURLs].sort());
  } finally { await ui.close(); }
});

test('a genuinely new library offers add-folder and restore actions instead of another companion', async () => {
  const ui = harness(); let picks = 0; const destinations = [];
  try {
    ui.setLibrary(page([]));
    ui.ids.get('add-folder').addEventListener('click', () => { picks += 1; });
    ui.window.addEventListener('photofindry:guide-destination', event => destinations.push(event.detail.panel));
    await ui.pair();
    assert.equal(ui.ids.get('empty-title').textContent, 'Start your local library');
    assert.equal(ui.ids.get('empty-actions').hidden, false);
    assert.doesNotMatch(ui.ids.get('empty-description').textContent, /existing PhotoFindry library/);
    await ui.ids.get('empty-add-folder').click(); assert.equal(picks, 1);
    await ui.ids.get('empty-restore').click(); assert.deepEqual(destinations, ['portability']);
    assert(!ui.calls.some(call => call.input.action === 'enqueueJob'));
  } finally { await ui.close(); }
});

test('empty filtered searches keep their scope until the explicit Clear filters action', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([])); await ui.pair();
    await ui.window.PhotoFindry.setScope({ sourceId: 'source-a', status: 'tagged', metadataFilters: { camera: 'Fuji' } });
    assert.equal(ui.ids.get('empty-title').textContent, 'No photographs in this view');
    assert.equal(ui.ids.get('clear-filters').hidden, false); assert.equal(ui.ids.get('empty-actions').hidden, true);
    ui.ids.get('search').value = 'bicycle'; await ui.window.PhotoFindry.refreshLibrary();
    await ui.ids.get('clear-search').click(); await ui.flush();
    assert.equal(ui.window.PhotoFindry.getScope().sourceId, 'source-a');
    assert.equal(ui.window.PhotoFindry.getScope().status, 'tagged');
    ui.ids.get('search').value = 'bicycle';
    await ui.ids.get('clear-filters').click(); await ui.flush();
    assert.equal(ui.window.PhotoFindry.getScope().sourceId, '');
    assert.equal(ui.window.PhotoFindry.getScope().status, 'all');
    assert.equal(ui.window.PhotoFindry.getScope().query, 'bicycle');
  } finally { await ui.close(); }
});

test('reload restores the process credential only after server validation', async () => {
  const first = harness(); let reloaded;
  try {
    await first.pair();
    const key = 'photofindry.local-session.v2';
    const original = JSON.parse(first.storage.get(key));
    assert.deepEqual(Object.keys(original).sort(), ['instanceId', 'origin', 'token', 'version']);
    first.emit('pagehide', { persisted: false }); await first.flush();
    assert.equal(first.window.PhotoFindry.isConnected(), false);
    assert.equal(first.ids.get('app').hidden, true);
    assert.equal(first.ids.get('gallery').children.length, 0);
    assert(!first.calls.some(call => call.route === '/api/disconnect'));
    assert(first.storage.has(key));

    const validation = deferred();
    reloaded = harness({ storage: first.storage, initialHandlers: { '/api/session': () => validation.promise } });
    await reloaded.flush();
    assert.deepEqual(reloaded.calls.map(call => call.route), ['/api/session']);
    assert.equal(reloaded.window.PhotoFindry.isConnected(), false);
    assert.equal(reloaded.ids.get('app').hidden, true);
    validation.resolve({ apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' }); await reloaded.flush();
    assert.equal(reloaded.window.PhotoFindry.isConnected(), true);
    assert.equal(reloaded.ids.get('gallery').children.length, 2);
    assert(!reloaded.calls.some(call => call.route === '/api/pair'));
    assert.equal(reloaded.calls[0].headers.Authorization, `Bearer ${original.token}`);
    assert.deepEqual(JSON.parse(first.storage.get(key)), original);
  } finally { if (reloaded) await reloaded.close(); await first.close(); }
});

test('Back/Forward revalidates once and resumes through a fresh session event after scrubbing private state', async () => {
  const ui = harness(); const sessions = [];
  ui.window.addEventListener('photofindry:session', event => sessions.push(event.detail.connected));
  try {
    await ui.pair();
    await ui.ids.get('gallery').children[0].click(); await ui.flush();
    assert.match(ui.ids.get('inspector-content').textContent, /A saved fact/);
    ui.emit('pagehide', { persisted: true }); await ui.flush();
    assert.doesNotMatch(ui.ids.get('inspector-content').textContent, /A saved fact/);
    assert.match(ui.ids.get('inspector-content').textContent, /Select a photo/);
    assert.deepEqual(sessions, [false, true, false]);
    const validation = deferred(); ui.handlers['/api/session'] = () => validation.promise;
    ui.emit('pageshow', { persisted: true }); ui.emit('pageshow', { persisted: true }); await ui.flush();
    assert.equal(ui.calls.filter(call => call.route === '/api/session').length, 1);
    assert.equal(ui.ids.get('app').hidden, true);
    validation.resolve({ apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' }); await ui.flush();
    assert.deepEqual(sessions, [false, true, false, true]);
    assert.equal(ui.window.PhotoFindry.isConnected(), true);
    assert(!ui.calls.some(call => call.route === '/api/disconnect'));
  } finally { await ui.close(); }
});

test('navigation fences delayed replies and a late old 401 cannot log out the restored session', async () => {
  const ui = harness();
  try {
    await ui.pair();
    const late = deferred(); ui.handlers['/api/late'] = () => late.promise;
    const pending = ui.window.PhotoFindry.api('/api/late', {}).catch(error => error);
    const signal = ui.calls.at(-1).signal;
    ui.emit('pagehide', { persisted: true });
    assert(signal.aborted);
    ui.emit('pageshow', { persisted: true }); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected());
    late.resolve(Response.json({ error: 'Old revoked request' }, { status: 401 }));
    assert.equal((await pending).name, 'AbortError'); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected());
    assert(ui.storage.has('photofindry.local-session.v2'));
    assert.equal(ui.ids.get('app').hidden, false);
  } finally { await ui.close(); }
});

test('an old poll cannot duplicate the restored poller or publish a deferred action continuation', async () => {
  const oldState = deferred();
  const ui = harness({ initialHandlers: { '/api/action': () => ({ restarted: true }) } });
  let restored = 0;
  ui.window.addEventListener('photofindry:library-restored', () => { restored += 1; });
  try {
    await ui.pair();
    ui.handlers['/api/state'] = () => oldState.promise;
    void ui.window.PhotoFindry.refreshState().catch(() => {});
    const action = ui.window.PhotoFindry.action('restoreLibrary', {}).catch(error => error);
    await ui.flush();
    ui.emit('pagehide'); ui.handlers['/api/state'] = () => ({ galleryRevision: 2, libraryGeneration: 0 });
    ui.emit('pageshow'); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected());
    oldState.resolve({ galleryRevision: 1, libraryGeneration: 0 });
    assert.equal((await action).name, 'AbortError'); await ui.flush();
    assert.equal(restored, 0);
    assert.equal([...ui.timers.values()].filter(delay => delay === 1200).length, 1);
    assert.equal(ui.window.PhotoFindry.getState().galleryRevision, 2);
  } finally { oldState.resolve({ galleryRevision: 1, libraryGeneration: 0 }); await ui.close(); }
});

test('temporary restoration failure retains access for retry; true revocation clears it', async () => {
  const ui = harness();
  try {
    await ui.pair();
    ui.emit('pagehide');
    ui.handlers['/api/session'] = () => { throw new TypeError('Disconnected network'); };
    ui.emit('pageshow'); await ui.flush();
    assert(!ui.window.PhotoFindry.isConnected());
    assert(ui.storage.has('photofindry.local-session.v2'));
    assert.equal(ui.retrySession().hidden, false);
    assert.match(ui.ids.get('pair-error').textContent, /retry the local connection/);
    ui.handlers['/api/session'] = () => ({ apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' });
    await ui.retrySession().click(); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected());
    ui.emit('pagehide');
    ui.handlers['/api/session'] = () => Response.json({ error: 'Revoked or different companion process' }, { status: 401 });
    ui.emit('pageshow'); await ui.flush();
    assert(!ui.window.PhotoFindry.isConnected());
    assert.equal(ui.storage.size, 0);
    assert.equal(ui.retrySession().hidden, true);
    assert.match(ui.ids.get('pair-error').textContent, /access ended/);
  } finally { await ui.close(); }
});

test('cross-origin, malformed, incompatible and changed-instance restored credentials fail closed', async () => {
  const key = 'photofindry.local-session.v2';
  const good = { version: 2, origin: 'http://127.0.0.1:54321', token: 'a'.repeat(64), instanceId: 'b'.repeat(64) };
  for (const raw of [JSON.stringify({ ...good, instanceId: 'short' }), JSON.stringify({ ...good, origin: 'http://127.0.0.1:54322' }), JSON.stringify({ ...good, version: 1 }), JSON.stringify({ ...good, token: 'short' }), JSON.stringify({ ...good, path: '/private/photo.jpg' }), 'x'.repeat(1025), '{']) {
    const ui = harness({ storage: new Map([[key, raw]]) });
    try { await ui.flush(); assert.equal(ui.calls.length, 0); assert.equal(ui.storage.size, 0); assert(!ui.window.PhotoFindry.isConnected()); }
    finally { await ui.close(); }
  }
  const ui = harness({ storage: new Map([[key, JSON.stringify(good)]]), initialHandlers: { '/api/session': () => ({ apiVersion: 3, instanceId: 'b'.repeat(64), sessionLifetime: 'process' }) } });
  try {
    await ui.flush(); assert.equal(ui.storage.size, 0); assert(!ui.window.PhotoFindry.isConnected());
    assert.equal(ui.calls.length, 1); assert.match(ui.ids.get('pair-error').textContent, /incompatible/);
  } finally { await ui.close(); }
  const changed = harness({ storage: new Map([[key, JSON.stringify(good)]]), initialHandlers: { '/api/session': () => ({ apiVersion: 4, instanceId: 'c'.repeat(64), sessionLifetime: 'process' }) } });
  try {
    await changed.flush(); assert.equal(changed.storage.size, 0); assert(!changed.window.PhotoFindry.isConnected());
    assert.equal(changed.calls.length, 1); assert.match(changed.ids.get('pair-error').textContent, /companion changed/);
  } finally { await changed.close(); }
});

test('time spent away does not terminate a process session, but return still requires server validation', async () => {
  const ui = harness();
  try {
    await ui.pair(); ui.emit('pagehide'); ui.advance(24 * 60 * 60 * 1000); ui.emit('pageshow'); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected()); assert.equal(ui.storage.size, 1);
    assert.equal(ui.calls.filter(call => call.route === '/api/session').length, 1);
    assert(![...ui.timers.values()].some(ms => ms === 1800000));
  } finally { await ui.close(); }
});

test('blocked browser storage keeps the current page usable and warns that reload may need pairing', async () => {
  const ui = harness({ storageDenied: true });
  try {
    await ui.pair(); assert(ui.window.PhotoFindry.isConnected());
    assert.match(ui.ids.get('global-notice').textContent, /cannot save access for a reload/);
    assert.equal(ui.ids.get('gallery').children.length, 2);
    ui.emit('pagehide', { persisted: true }); ui.emit('pageshow', { persisted: true }); await ui.flush();
    assert(ui.window.PhotoFindry.isConnected(), 'An in-memory BFCache copy can still be revalidated.');
  } finally { await ui.close(); }
});

test('explicit Disconnect still revokes the server token and removes the saved tab credential', async () => {
  const ui = harness();
  await ui.pair(); assert(ui.storage.size > 0);
  await ui.close();
  assert.equal(ui.storage.size, 0); assert(!ui.window.PhotoFindry.isConnected());
  assert.equal(ui.calls.filter(call => call.route === '/api/disconnect').length, 1);
  ui.emit('pageshow'); await ui.flush();
  assert(!ui.calls.some(call => call.route === '/api/session'));
});

test('native launch ticket is removed synchronously, exchanged once in a body, and never stored as the bearer', async () => {
  const ticket = 'c'.repeat(64), exchange = deferred();
  const ui = harness({ fragment: `#bootstrap=${ticket}`, initialHandlers: { '/api/bootstrap': () => exchange.promise } });
  try {
    assert.deepEqual(ui.history, [{ state: null, title: '', url: '/' }]);
    assert.equal(ui.window.location.hash, '');
    ui.emit('pageshow'); await ui.flush();
    assert.deepEqual(ui.calls.map(call => call.route), ['/api/bootstrap']);
    assert.equal(ui.calls[0].fragment, '');
    assert.deepEqual(ui.calls[0].input, { ticket });
    assert.equal(ui.calls[0].headers.Authorization, undefined);
    assert.equal(ui.ids.get('app').hidden, true); assert.equal(ui.storage.size, 0);
    exchange.resolve({ apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process', token: 'a'.repeat(64) });
    await ui.flush();
    assert(ui.window.PhotoFindry.isConnected());
    const record = JSON.parse(ui.storage.get('photofindry.local-session.v2'));
    assert.equal(record.token, 'a'.repeat(64)); assert.equal(record.instanceId, 'b'.repeat(64));
    assert(!JSON.stringify([...ui.storage]).includes(ticket));
    ui.emit('pagehide'); ui.emit('pageshow'); await ui.flush();
    assert.equal(ui.calls.filter(call => call.route === '/api/bootstrap').length, 1);
    assert.equal(ui.calls.filter(call => call.route === '/api/session').length, 1);
    assert(!ui.calls.some(call => call.route.includes(ticket) || call.route.includes('a'.repeat(64))));
  } finally { exchange.resolve({}); await ui.close(); }
});

test('malformed launch fragments and history removal failures never send a ticket or load a library', async () => {
  for (const fragment of ['#bootstrap=', `#bootstrap=${'c'.repeat(64)}&next=evil`, `#bootstrap=${'C'.repeat(64)}`, '#anything']) {
    const ui = harness({ fragment });
    try {
      await ui.flush(); assert.equal(ui.history.length, 1); assert.equal(ui.window.location.hash, '');
      assert.equal(ui.calls.length, 0); assert(!ui.window.PhotoFindry.isConnected());
      assert.match(ui.ids.get('pair-error').textContent, /launch link is invalid/);
    } finally { await ui.close(); }
  }
  const denied = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, historyDenied: true });
  try {
    await denied.flush(); assert.equal(denied.calls.length, 0); assert.equal(denied.storage.size, 0);
    assert.match(denied.ids.get('pair-error').textContent, /could not clear/);
  } finally { await denied.close(); }
});

test('expired or replayed bootstrap offers another local launch without retrying a spent ticket', async () => {
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, initialHandlers: {
    '/api/bootstrap': () => Response.json({ error: 'Invalid or expired ticket' }, { status: 401 }),
  } });
  try {
    await ui.flush(); assert(!ui.window.PhotoFindry.isConnected()); assert.equal(ui.storage.size, 0);
    assert.match(ui.ids.get('pair-error').textContent, /expired or was already used/);
    ui.emit('pageshow'); await ui.flush();
    assert.equal(ui.calls.length, 1);
  } finally { await ui.close(); }
});

test('a bad bootstrap cannot discard an existing independent process credential', async () => {
  const key = 'photofindry.local-session.v2';
  const record = { version: 2, origin: 'http://127.0.0.1:54321', token: 'a'.repeat(64), instanceId: 'b'.repeat(64) };
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}`, storage: new Map([[key, JSON.stringify(record)]]), initialHandlers: {
    '/api/bootstrap': () => Response.json({ error: 'Expired ticket' }, { status: 401 }),
  } });
  try {
    await ui.flush(); assert(ui.window.PhotoFindry.isConnected());
    assert.deepEqual(ui.calls.slice(0, 2).map(call => call.route), ['/api/bootstrap', '/api/session']);
    assert.deepEqual(JSON.parse(ui.storage.get(key)), record);
  } finally { await ui.close(); }
});

test('native archive links wait for an authenticated loaded library and navigate once without persisting the ID', async () => {
  const archive = 'ABCDEF01-2345-6789-ABCD-EF0123456789', ready = deferred(), opened = [];
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}&archive=${archive}`, initialHandlers: { '/api/state': () => ready.promise } });
  ui.window.PhotoFindryPanels = { async openArchiveCode(id) {
    assert(ui.window.PhotoFindry.isConnected());
    assert.equal(ui.window.PhotoFindry.getState().libraryId, 'fixture-library');
    assert.equal(ui.window.PhotoFindry.getState().libraryGeneration, 0);
    opened.push(id);
  } };
  try {
    assert.equal(ui.window.location.hash, ''); assert.equal(ui.history.length, 1);
    await ui.flush(); assert.deepEqual(opened, []);
    assert.deepEqual(ui.calls.find(call => call.route === '/api/bootstrap').input, { ticket: 'c'.repeat(64) });
    ready.resolve({ galleryRevision: 1, libraryGeneration: 0, workerGeneration: 0, libraryId: 'fixture-library', sources: [{ id: archive.toLowerCase() }] });
    await ui.flush(); assert.deepEqual(opened, [archive.toLowerCase()]);
    assert(!JSON.stringify([...ui.storage]).includes(archive.toLowerCase()));
    ui.emit('pagehide'); ui.emit('pageshow'); await ui.flush();
    assert.deepEqual(opened, [archive.toLowerCase()], 'Back/Forward does not replay the navigation.');
    assert(!ui.calls.some(call => call.route === '/api/pick' || call.route === '/api/action'));
  } finally { ready.resolve({}); await ui.close(); }
});

test('unknown or malformed archive links never attach a source, choose files or run jobs', async () => {
  const archive = 'abcdef01-2345-6789-abcd-ef0123456789';
  const ui = harness({ fragment: `#bootstrap=${'c'.repeat(64)}&archive=${archive}`, initialHandlers: {
    '/api/state': () => ({ galleryRevision: 1, libraryGeneration: 0, libraryId: 'fixture-library', sources: [] }),
  } });
  ui.window.PhotoFindryPanels = { async openArchiveCode() { throw Error('This library does not contain that source. Attach its archive first.'); } };
  try {
    await ui.flush(); assert.match(ui.ids.get('global-error').textContent, /does not contain that source/);
    assert(!ui.calls.some(call => call.route === '/api/pick' || call.route === '/api/action'));
  } finally { await ui.close(); }
  for (const suffix of [`${archive}&attach=true`, `${archive}/extra`, `${archive}?callback=evil`, encodeURIComponent('/private/archive'), `${archive}&archive=${archive}`]) {
    const bad = harness({ fragment: `#bootstrap=${'c'.repeat(64)}&archive=${suffix}` });
    try { await bad.flush(); assert.equal(bad.calls.length, 0); assert.equal(bad.window.location.hash, ''); assert.match(bad.ids.get('pair-error').textContent, /launch link is invalid/); }
    finally { await bad.close(); }
  }
});

test('untagged photographs have stable decorative variety across refresh and reload without fetching photo data', async () => {
  const photos = Array.from({ length: 9 }, (_, index) => photo(`pending-${index}`, { status: 'pending', caption: '', hasPreview: false }));
  const ui = harness(); let reloaded;
  const artwork = current => current.ids.get('gallery').children.map(tile => {
    assert.equal(tile.querySelector('.preview-placeholder').textContent, 'Photo not tagged yet');
    const image = tile.querySelector('img'); assert(image);
    assert.equal(image.alt, ''); assert.equal(image.attributes['aria-hidden'], 'true');
    assert.match(image.src, /^\/assets\/untagged-(landscape|spaces|shapes)\.jpg$/);
    return image.src;
  });
  try {
    ui.setLibrary(page(photos)); await ui.pair();
    const original = artwork(ui);
    assert(new Set(original).size > 1, 'Different stable photo IDs provide decorative variety.');
    const firstTile = ui.ids.get('gallery').children[0], firstImage = firstTile.querySelector('img');
    ui.state.monitor = { sessionOutputTokens: 200 };
    await ui.window.PhotoFindry.refreshState();
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.ids.get('gallery').children[0], firstTile);
    assert.equal(firstTile.querySelector('img'), firstImage, 'Refresh does not replace the decorative image.');
    assert.deepEqual(artwork(ui), original);
    ui.emit('pagehide'); await ui.flush();
    reloaded = harness({ storage: ui.storage, initialHandlers: { '/api/library': () => page(photos) } });
    await reloaded.flush();
    assert.deepEqual(artwork(reloaded), original, 'Reload retains the same artwork for every photo.');
    for (const current of [ui, reloaded]) {
      assert(!current.calls.some(call => ['/api/photo', '/api/preview', '/api/action'].includes(call.route)), 'Decoration must not hydrate or process originals.');
      assert.deepEqual(current.createdURLs, []);
    }
  } finally { if (reloaded) await reloaded.close(); await ui.close(); }
});

test('missing-preview labels distinguish accepted work, metadata-only photos and tagging failures', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([
      photo('accepted', { hasPreview: false }),
      photo('metadata', { status: 'pending', hasPreview: false, source: { mode: 'metadata' } }),
      photo('archived', { status: 'pending', hasPreview: false, source: { mode: 'archived' } }),
      photo('failed', { status: 'needsAttention', hasPreview: false, caption: 'Previously accepted details remain saved.' }),
    ]));
    await ui.pair();
    const tiles = ui.ids.get('gallery').children;
    assert.deepEqual(tiles.map(tile => tile.querySelector('.preview-placeholder').textContent), [
      'Preview unavailable', 'Metadata-only photo', 'Metadata-only photo', 'Tagging needs attention',
    ]);
    assert(tiles.every(tile => tile.querySelector('img') === null), 'Only genuinely pending photos get decorative artwork.');
    assert.match(tiles[3].textContent, /Previously accepted details remain saved/);
    assert(!ui.calls.some(call => ['/api/photo', '/api/preview', '/api/action'].includes(call.route)));
  } finally { await ui.close(); }
});

test('a pending photo becoming tagged without a preview retires its untagged artwork immediately', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([photo('progress', { status: 'pending', hasPreview: false })])); await ui.pair();
    const tile = ui.ids.get('gallery').children[0];
    assert(tile.querySelector('img')); assert.match(tile.textContent, /Photo not tagged yet/);
    ui.setLibrary(page([photo('progress', { status: 'tagged', hasPreview: false })]));
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.ids.get('gallery').children[0], tile);
    assert.equal(tile.querySelector('img'), null);
    assert.equal(tile.querySelector('.preview-placeholder').textContent, 'Preview unavailable');
    assert.doesNotMatch(tile.textContent, /Photo not tagged yet/);
    assert(!ui.calls.some(call => call.route === '/api/preview'));
  } finally { await ui.close(); }
});

test('cancelling Quit keeps access, displayed work and server state unchanged', async () => {
  const ui = harness({ confirm: () => false });
  try {
    await ui.pair();
    const credentials = [...ui.storage], tiles = [...ui.ids.get('gallery').children];
    const calls = ui.calls.length;
    await ui.ids.get('quit-companion').click(); await ui.flush();
    assert.equal(ui.confirmations.length, 1); assert.match(ui.confirmations[0], /Current processing will stop/);
    assert.match(ui.confirmations[0], /saved work is kept/);
    assert.equal(ui.calls.length, calls); assert.deepEqual([...ui.storage], credentials);
    assert(ui.window.PhotoFindry.isConnected()); assert.equal(ui.ids.get('app').hidden, false);
    assert.deepEqual(ui.ids.get('gallery').children, tiles);
  } finally { await ui.close(); }
});

test('confirmed Quit sends the authenticated stop request and immediately clears browser access', async () => {
  const stopped = deferred();
  const ui = harness({ confirm: () => true, initialHandlers: { '/api/quit': () => stopped.promise } });
  try {
    await ui.pair();
    const token = JSON.parse(ui.storage.get('photofindry.local-session.v2')).token;
    const quitting = ui.ids.get('quit-companion').click(); await ui.flush();
    assert.equal(ui.confirmations.length, 1);
    assert.equal(ui.storage.size, 0); assert(!ui.window.PhotoFindry.isConnected());
    assert.equal(ui.ids.get('app').hidden, true); assert.equal(ui.ids.get('gallery').children.length, 0);
    const requests = ui.calls.filter(call => call.route === '/api/quit');
    assert.equal(requests.length, 1); assert.deepEqual(requests[0].input, {});
    assert.equal(requests[0].headers.Authorization, `Bearer ${token}`);
    assert(!ui.calls.some(call => call.route === '/api/disconnect'));
    stopped.resolve({}); await quitting; await ui.flush();
    ui.emit('pageshow'); await ui.flush();
    assert(!ui.calls.some(call => call.route === '/api/session'));
    await ui.ids.get('quit-companion').click();
    assert.equal(ui.confirmations.length, 1, 'A disconnected page cannot request another Quit.');
  } finally { stopped.resolve({}); await ui.close(); }
});

test('ordinary search exposes GPS disambiguation and radius without inventing visual location proof', async () => {
  const ui = harness();
  const descendants = node => node.children.flatMap(child => [child, ...descendants(child)]);
  const find = label => descendants(ui.ids.get('query-notice')).find(node => node.tagName === 'BUTTON' && node.textContent === label);
  const location = { state: 'ambiguous', alternatives: [{ id: 'city-a', name: 'Paris', country: 'France' }, { id: 'city-b', name: 'Paris', country: 'United States' }], alternativeCount: 2, radiusKm: 30, matchedGPSCount: 2, gpsPhotoCount: 4, scopePhotoCount: 8 };
  try {
    ui.setLibrary(page([photo('a')], { locationSearch: location })); await ui.pair();
    assert.match(ui.ids.get('query-notice').textContent, /Choose the recorded location.*2 within the GPS\/time area.*4 of 8/s);
    await find('Paris, France').click(); await ui.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(ui.window.PhotoFindry.getScope().locationOptions)), { enabled: true, applied: true, placeId: 'city-a', radiusKm: 30 });
    ui.setLibrary(page([photo('a')], { locationSearch: { ...location, state: 'applied', place: location.alternatives[0], daypart: 'evening' } }));
    await ui.window.PhotoFindry.refreshCurrentPage();
    const radius = descendants(ui.ids.get('query-notice')).find(node => node.tagName === 'INPUT');
    radius.value = '4.5'; await find('Apply GPS radius').click(); await ui.flush();
    assert.equal(ui.window.PhotoFindry.getScope().locationOptions.radiusKm, 4.5);
    assert.match(ui.ids.get('query-notice').textContent, /Missing GPS does not match.*evening uses recorded camera time/s);
    await find('Search without GPS filter').click(); await ui.flush();
    assert.equal(ui.window.PhotoFindry.getScope().locationOptions.enabled, false);
    assert.equal(ui.window.PhotoFindry.getScope().locationOptions.placeId, null);
  } finally { await ui.close(); }
});

test('filename status recovery preserves query, folder and metadata; incomplete search coverage stays explicit', async () => {
  const ui = harness();
  const descendants = node => node.children.flatMap(child => [child, ...descendants(child)]);
  try {
    ui.setLibrary(page([], { filenameOutsideStatusCount: 3, semanticLimited: true, semanticAvailable: true, semanticIndexedPhotoCount: 4, semanticEligiblePhotoCount: 10 }));
    await ui.pair(); ui.ids.get('search').value = 'DSC1234';
    await ui.window.PhotoFindry.setScope({ sourceId: 'source-one', scopeId: 'folder-one', status: 'pending', metadataFilters: { camera: 'Fuji' } });
    const notice = ui.ids.get('query-notice'); assert.match(notice.textContent, /covers 4 of 10.*3 filename matches/s);
    const button = descendants(notice).find(node => node.tagName === 'BUTTON' && node.textContent === 'Show filename matches in all statuses');
    await button.click(); await ui.flush();
    const scope = ui.window.PhotoFindry.getScope(); assert.equal(scope.status, 'all'); assert.equal(scope.query, 'DSC1234'); assert.equal(scope.sourceId, 'source-one'); assert.equal(scope.scopeId, 'folder-one'); assert.equal(scope.metadataFilters.camera, 'Fuji');
    assert(!ui.calls.some(call => call.input.action === 'enqueueJob'));
  } finally { await ui.close(); }
});

test('a restore in another tab retires old contexts and delayed actions while retaining process authorization', async () => {
  const ui = harness(); const delayed = deferred(); let restores = 0;
  Object.assign(ui.state, { libraryId: 'same-id', libraryGeneration: 1, workerGeneration: 1 });
  ui.window.addEventListener('photofindry:library-restored', () => restores++);
  try {
    await ui.pair(); await ui.window.PhotoFindry.setScope({ sourceId: 'old-source', scopeId: 'old-folder', status: 'pending' });
    await ui.ids.get('gallery').children[0].click(); await ui.flush();
    ui.handlers['/api/action'] = () => delayed.promise;
    const staleAction = ui.window.PhotoFindry.action('ensurePreview', { photoId: 'a' }).then(() => 'published', error => error.name);
    ui.state.libraryGeneration = 2; ui.state.workerGeneration = 2;
    await ui.window.PhotoFindry.refreshState(); await ui.flush();
    delayed.resolve({}); assert.equal(await staleAction, 'AbortError');
    assert.equal(restores, 1); assert.equal(ui.window.PhotoFindry.isConnected(), true);
    assert.match(ui.ids.get('global-notice').textContent, /Open Files to choose and confirm the originals folders on this Mac/);
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], []);
    assert.equal(ui.window.PhotoFindry.getScope().sourceId, ''); assert.equal(ui.window.PhotoFindry.getScope().scopeId, '');
    assert.equal(ui.window.PhotoFindry.getScope().status, 'all');
    assert(!ui.calls.some(call => call.route === '/api/disconnect'));
    await ui.window.PhotoFindry.refreshState(); assert.equal(restores, 1);
  } finally { delayed.resolve({}); await ui.close(); }
});

test('state-dependent requests require an applied library epoch and never adopt arbitrary response headers', async () => {
  const ui = harness(); delete ui.state.libraryGeneration;
  ui.handlers['/api/action'] = () => Response.json({}, { headers: { 'X-PhotoFindry-Library-Generation': '99' } });
  ui.handlers['/api/model-files/check'] = () => ({});
  ui.handlers['/api/review-files/check'] = () => ({});
  try {
    await ui.pair();
    for (const route of ['/api/action', '/api/model-files/check', '/api/review-files/check']) {
      await assert.rejects(ui.window.PhotoFindry.api(route, route === '/api/action' ? { action: 'assignPerson', params: { photoId: 'a', name: 'Test' } } : {}), /library state.*opening/);
      assert(!ui.calls.some(call => call.route === route), 'No action is sent without an applied state.');
    }
    ui.state.libraryGeneration = 4; await ui.window.PhotoFindry.refreshState();
    await ui.window.PhotoFindry.api('/api/action', { action: 'assignPerson', params: { photoId: 'a', name: 'Test' } });
    assert.equal(ui.calls.filter(call => call.route === '/api/action').at(-1).headers['X-PhotoFindry-Library-Generation'], '4');
    // Another tab has restored the library, but this tab has not applied its state.
    ui.state.libraryGeneration = 5;
    ui.handlers['/api/action'] = () => Response.json({ error: 'The library changed. Refresh first.' }, { status: 409, headers: { 'X-PhotoFindry-Library-Generation': '5' } });
    await assert.rejects(ui.window.PhotoFindry.api('/api/action', { action: 'assignPerson', params: { photoId: 'a', name: 'Old draft' } }), /library changed/);
    await ui.window.PhotoFindry.api('/api/review-files/check', {});
    assert.equal(ui.calls.filter(call => call.route === '/api/review-files/check').at(-1).headers['X-PhotoFindry-Library-Generation'], '4');
    assert.equal(ui.calls.filter(call => call.route === '/api/action').length, 2, 'A stale action is never retried automatically.');
    await ui.window.PhotoFindry.refreshState(); await ui.flush();
    await ui.window.PhotoFindry.api('/api/model-files/check', {});
    assert.equal(ui.calls.filter(call => call.route === '/api/model-files/check').at(-1).headers['X-PhotoFindry-Library-Generation'], '5');
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], []);
  } finally { await ui.close(); }
});

test('rich photo cards show real source, observation and review facts without turning candidate ranking into confirmation', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([
      photo('reviewed', { observationCount: 41, source: { id: 'work', label: 'Work', status: 'needsReview', accessOriginals: false },
        matchKind: 'ai-ranked', matchText: 'A person standing by a tree.', improvement: { reviewedAt: '2026-10-17T00:00:00Z', decisionProvider: 'intern-decision-4b', summaryState: 'suggestions', changeCount: 1 } }),
      photo('unknown', { tags: ['one', 'two'], source: { label: 'Offline album', status: 'offline' } }),
      photo('pending', { status: 'pending', hasPreview: false, observationCount: 17 }),
    ])); await ui.pair();
    const [reviewed, unknown, pending] = ui.ids.get('gallery').children;
    assert.match(reviewed.textContent, /Work · Confirm original location/);
    assert.match(reviewed.textContent, /Tagged · 41 observations/);
    assert.match(reviewed.textContent, /Review suggestions/);
    assert.match(reviewed.textContent, /Refined candidate/); assert.match(reviewed.textContent, /A person standing by a tree/);
    assert.doesNotMatch(reviewed.textContent, /Experimental|Direct match|confirmed match/i);
    assert.match(unknown.textContent, /Offline album · Drive offline/); assert.doesNotMatch(unknown.textContent, /2 observations/);
    assert.match(pending.textContent, /Waiting to be tagged/); assert.doesNotMatch(pending.textContent, /17 observations/);
  } finally { await ui.close(); }
});

test('card components update only on display changes and release when the page or session leaves', async () => {
  const mounted = [], updated = [], destroyed = [];
  const ui = harness({ summaryMount(target, data) {
    const record = { target, data }; mounted.push(record); target.textContent = data.name;
    return { update(next) { updated.push(next); record.data = next; target.textContent = next.name; }, destroy() { destroyed.push(record); target.replaceChildren(); } };
  } });
  try {
    await ui.pair(); assert.equal(mounted.length, 2); const tile = ui.ids.get('gallery').children[0], image = tile.querySelector('img');
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush(); assert.equal(updated.length, 0); assert.equal(mounted.length, 2);
    ui.setLibrary(page([photo('a', { observationCount: 21 }), photo('b')])); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(updated.length, 1); assert.equal(updated[0].statusLabel, 'Tagged · 21 observations'); assert.equal(destroyed.length, 0);
    assert.equal(tile.querySelector('img'), image);
    ui.setLibrary(page([photo('c')])); await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(mounted.length, 3); assert.equal(destroyed.length, 2);
  } finally { await ui.close(); assert.equal(destroyed.length, mounted.length); }
});

test('card presentation failure recovers to usable text and retains gallery selection and preview', async () => {
  const ui = harness({ summaryMount() { throw Error('Fixture presentation unavailable'); } });
  try {
    ui.setLibrary(page([photo('safe', { observationCount: 7, source: { label: 'Travel', status: 'available' } })])); await ui.pair();
    const tile = ui.ids.get('gallery').children[0]; assert.match(tile.textContent, /safe.jpgTravelTagged · 7 observations/);
    assert.equal(ui.ids.get('library-error').hidden, true); await tile.click(); await ui.flush();
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['safe']); assert.match(ui.ids.get('inspector-content').textContent, /A saved fact/);
  } finally { await ui.close(); }
});

test('richer inspector keeps all accepted details, uncertainty and source recovery actions', async () => {
  const ui = harness();
  const accepted = Array.from({ length: 80 }, (_, index) => `Accepted observation ${index + 1}`);
  try {
    ui.handlers['/api/photo'] = () => photo('a', { observationCount: 80, source: { id: 'work', label: 'Work', status: 'needsReview', accessOriginals: false, relativePath: 'trip/a.jpg' },
      detailSections: [{ title: 'People and appearance', items: accepted }], metadataFields: [{ label: 'Camera', value: 'Saved camera model' }],
      matchKind: 'review-suggestion', matchText: 'A possible saved activity.' });
    await ui.pair(); await ui.ids.get('gallery').children[0].click(); await ui.flush();
    const inspector = ui.ids.get('inspector-content');
    assert.match(inspector.textContent, /Tagged · 80 observations/); assert.match(inspector.textContent, /Confirm original location/);
    for (const detail of accepted) assert(inspector.textContent.includes(detail), detail);
    assert.match(inspector.textContent, /not a confirmed fact/); assert.match(inspector.textContent, /Appearance estimates remain estimates/);
    assert.match(inspector.textContent, /Saved camera model/); assert.match(inspector.textContent, /Connect originals in Files/);
    assert(inspector.querySelectorAll('[data-detail-action]').some(node => node.dataset.detailAction === 'source:work'));
  } finally { await ui.close(); }
});

test('text fallback review shortcuts open complete saved preview without changing selection', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([photo('a', { improvement: { reviewedAt: '2026-10-17T00:00:00Z', summaryState: 'unchanged' } }), photo('b')]));
    await ui.pair(); await ui.ids.get('gallery').children[1].click(); await ui.flush();
    const review = ui.ids.get('gallery').children[0].querySelector('button');
    assert.equal(review.textContent, 'Reviewed'); assert.match(review.attributes['aria-label'], /a.jpg/);
    await review.click(); await ui.flush(); assert.equal(ui.ids.get('preview-dialog').open, true);
    assert.equal(ui.ids.get('preview-title').textContent, 'a.jpg');
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['b']);
    assert.match(ui.ids.get('preview-details').textContent, /A saved fact/);
  } finally { await ui.close(); }
});
