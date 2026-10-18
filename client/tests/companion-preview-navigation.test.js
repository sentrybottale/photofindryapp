'use strict';

// The bundled browser entry point, with synthetic local responses only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const photo = (id, extra = {}) => ({ id, name: `${id}.jpg`, caption: `Saved ${id}`,
  status: 'tagged', hasPreview: true, previewRevision: `${id}-v1`, detailSections: [{ title: 'Accepted details', items: [`Evidence for ${id}`] }], ...extra });
const page = (photos) => ({ photos, totalCount: photos.length, offset: 0, limit: 60 });
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };

function harness() {
  let document;
  class Node {
    constructor(tag = 'div', value = '') {
      this.tagName = tag.toUpperCase(); this.children = []; this.parentNode = null; this._text = value;
      this.value = ''; this.dataset = {}; this.attributes = {}; this.listeners = {}; this.style = {};
      this.hidden = false; this.disabled = false; this.open = false; this.className = '';
      this.classList = { toggle: (name, on) => { const names = new Set(this.className.split(' ').filter(Boolean)); on ? names.add(name) : names.delete(name); this.className = [...names].join(' '); } };
    }
    append(...nodes) { for (let node of nodes) { if (typeof node === 'string') node = new Node('#text', node); if (node.tagName === 'FRAGMENT') { this.append(...[...node.children]); continue; } node.remove(); node.parentNode = this; this.children.push(node); } }
    insertBefore(node, before) { node.remove(); const at = before ? this.children.indexOf(before) : this.children.length; assert(at >= 0); this.children.splice(at, 0, node); node.parentNode = this; }
    replaceChildren(...nodes) { for (const child of this.children) child.parentNode = null; this.children = []; this._text = ''; this.append(...nodes); }
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((child) => child !== this); this.parentNode = null; }
    setAttribute(key, value) { this.attributes[key] = String(value); }
    removeAttribute(key) { delete this.attributes[key]; }
    addEventListener(name, listener) { (this.listeners[name] ||= []).push(listener); }
    async fire(name, extra = {}) { for (const listener of this.listeners[name] || []) await listener({ target: this, key: '', preventDefault() {}, ...extra }); }
    click(extra) { return this.disabled ? Promise.resolve() : this.fire('click', extra); }
    focus() { document.activeElement = this; }
    scrollIntoView() {}
    contains(node) { return node === this || this.children.some((child) => child.contains(node)); }
    showModal() { this.open = true; }
    close() { if (!this.open) return; this.open = false; queueMicrotask(() => this.fire('close')); }
    querySelector(selector) { return descendants(this).find((node) => selector.startsWith('.') ? node.className.split(' ').includes(selector.slice(1)) : node.tagName === selector.toUpperCase()) || null; }
    querySelectorAll(selector) { return descendants(this).filter((node) => selector === '[data-detail-action]' && node.dataset.detailAction); }
    get firstChild() { return this.children[0]; }
    get textContent() { return this._text + this.children.map((child) => child.textContent).join(''); }
    set textContent(value) { this.replaceChildren(); this._text = String(value); }
  }
  const descendants = (node) => node.children.flatMap((child) => [child, ...descendants(child)]);
  const html = fs.readFileSync(path.join(__dirname, '../web-companion/index.html'), 'utf8');
  const ids = new Map([...html.matchAll(/\bid="([^"]+)"/g)].map((match) => [match[1], new Node()]));
  const body = new Node('body'); for (const [id, node] of ids) { node.id = id; body.append(node); }
  document = { body, activeElement: null, getElementById: (id) => ids.get(id), createElement: (tag) => new Node(tag),
    createTextNode: (value) => new Node('#text', value), createDocumentFragment: () => new Node('fragment') };
  const events = new Map();
  class CustomEvent { constructor(type, { detail } = {}) { this.type = type; this.detail = detail; } }
  const window = { addEventListener(name, listener) { if (!events.has(name)) events.set(name, []); events.get(name).push(listener); },
    dispatchEvent(event) { for (const listener of events.get(event.type) || []) listener(event); } };
  const calls = [], createdURLs = [], revokedURLs = [], handlers = {};
  let library = page([photo('a'), photo('b'), photo('c')]);
  const fetch = async (route, request) => {
    const input = JSON.parse(request.body); calls.push({ route, input });
    if (handlers[route]) { const result = await handlers[route](input, request); return result instanceof Response ? result : Response.json(result); }
    if (route === '/api/pair') return Response.json({ token: 'a'.repeat(64), apiVersion: 4, instanceId: 'b'.repeat(64), sessionLifetime: 'process' });
    if (route === '/api/state') return Response.json({ galleryRevision: 1, libraryGeneration: 0 });
    if (route === '/api/library') return Response.json(library);
    if (route === '/api/photo') return Response.json(photo(input.photoId));
    if (route === '/api/preview') return new Response(Buffer.from(`pixels-${input.photoId}`), { headers: { 'Content-Type': 'image/png' } });
    if (route === '/api/action' || route === '/api/disconnect') return Response.json({ id: 'job-synthetic' });
    throw Error(`Unhandled route ${route}`);
  };
  const urls = { createObjectURL() { const url = `blob:fixture-${createdURLs.length}`; createdURLs.push(url); return url; }, revokeObjectURL(url) { revokedURLs.push(url); } };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../web-companion/app.js'), 'utf8'), {
    window, document, CustomEvent, fetch, URL: urls, AbortController, DOMException, Blob, Response,
    setTimeout, clearTimeout, queueMicrotask, Intl, Date, getComputedStyle: () => ({ gridTemplateColumns: '1fr 1fr' }),
  }, { filename: 'app.js' });
  const flush = async () => { for (let i = 0; i < 12; i++) await new Promise((resolve) => setImmediate(resolve)); };
  return { ids, window, calls, createdURLs, revokedURLs, handlers, flush,
    findPreviewButton(label) { const found = descendants(ids.get('preview-details')).find((node) => node.tagName === 'BUTTON' && node.textContent === label); assert(found, `Missing preview button: ${label}`); return found; },
    setLibrary(value) { library = value; },
    async pair() { ids.get('pair-code').value = 'test-code'; await ids.get('pair-form').fire('submit'); await flush(); },
    async close() { await ids.get('disconnect').click(); await flush(); },
  };
}

test('preview navigates the frozen current page, zooms from fit to 3×, and keeps gallery selection', async () => {
  const ui = harness();
  try {
    await ui.pair(); const gallery = ui.ids.get('gallery');
    await gallery.children[0].click(); await ui.flush();
    const before = ui.window.PhotoFindry.getScope();
    await gallery.children[0].fire('dblclick'); await ui.flush();
    assert.equal(ui.ids.get('preview-position').textContent, '1 of 3 on this page');
    assert.equal(ui.ids.get('preview-previous').disabled, true);
    await ui.ids.get('preview-next').click(); await ui.flush();
    assert.equal(ui.ids.get('preview-title').textContent, 'b.jpg');
    await ui.ids.get('preview-dialog').fire('keydown', { key: 'ArrowRight' }); await ui.flush();
    assert.equal(ui.ids.get('preview-position').textContent, '3 of 3 on this page');
    assert.equal(ui.ids.get('preview-next').disabled, true);
    for (let i = 0; i < 12; i++) await ui.ids.get('preview-zoom-in').click();
    assert.equal(ui.ids.get('preview-zoom-level').textContent, '3.00×');
    assert.equal(ui.ids.get('preview-content').querySelector('.preview-image-stage').style.width, '300%');
    await ui.ids.get('preview-dialog').fire('keydown', { key: '0' });
    assert.equal(ui.ids.get('preview-zoom-level').textContent, 'Fit');
    const stage = ui.ids.get('preview-content').querySelector('.preview-image-stage');
    const imageURL = stage.querySelector('img').src;
    await ui.window.PhotoFindry.refreshState(); await ui.flush();
    assert.equal(ui.ids.get('preview-content').querySelector('.preview-image-stage'), stage);
    assert.equal(stage.querySelector('img').src, imageURL);
    assert(!ui.revokedURLs.includes(imageURL), 'a metric/state tick must not revoke the open preview');
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], ['a']);
    assert.equal(JSON.stringify(ui.window.PhotoFindry.getScope()), JSON.stringify(before));
    assert.equal(ui.ids.get('preview-dialog').open, true);
  } finally { await ui.close(); assert.deepEqual([...ui.revokedURLs].sort(), [...ui.createdURLs].sort()); }
});

test('an off-page People or activity photo opens its complete details without changing browse context', async () => {
  const ui = harness();
  try {
    await ui.pair(); const before = ui.window.PhotoFindry.getScope();
    ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { detailSections: [{ title: 'Accepted details', items: ['A precise saved fact'] }], personNames: ['Tedi'], personIds: ['person-one'], error: 'Synthetic tagging issue' });
    const diagnostics = []; ui.window.addEventListener('photofindry:tagging-diagnostic', (event) => diagnostics.push(event.detail));
    await ui.window.PhotoFindry.showPhoto('off-page'); await ui.flush();
    assert.equal(ui.ids.get('preview-position').textContent, 'Single photograph');
    assert.equal(ui.ids.get('preview-previous').disabled, true); assert.equal(ui.ids.get('preview-next').disabled, true);
    assert.match(ui.ids.get('preview-details').textContent, /A precise saved fact/);
    assert.match(ui.ids.get('preview-details').textContent, /Tedi/);
    await ui.findPreviewButton('View tagging diagnostic…').click();
    assert.equal(JSON.stringify(diagnostics), JSON.stringify([{ photoId: 'off-page' }]));
    await ui.findPreviewButton('Retag photograph…').click();
    await ui.findPreviewButton('Confirm retag').click(); await ui.flush();
    const retag = ui.calls.find((call) => call.route === '/api/action' && call.input.action === 'enqueueJob');
    assert.equal(JSON.stringify(retag.input.params), JSON.stringify({ kind: 'tagging', params: { photoIds: ['off-page'], force: true } }));
    assert.deepEqual([...ui.window.PhotoFindry.selectedIDs()], []);
    assert.equal(JSON.stringify(ui.window.PhotoFindry.getScope()), JSON.stringify(before));
    assert.deepEqual(ui.ids.get('gallery').children.map((tile) => tile.dataset.photoId), ['a', 'b', 'c']);
  } finally { await ui.close(); }
});

test('late pixels and details from a previous preview cannot overwrite the next photograph', async () => {
  const ui = harness();
  try {
    await ui.pair();
    const oldPixels = deferred(), oldDetails = deferred();
    ui.handlers['/api/preview'] = ({ photoId }) => photoId === 'a' ? oldPixels.promise
      : new Response(Buffer.from('new-pixels'), { headers: { 'Content-Type': 'image/png' } });
    ui.handlers['/api/photo'] = ({ photoId }) => photoId === 'a' ? oldDetails.promise : photo(photoId);
    const opening = ui.ids.get('gallery').children[0].fire('dblclick'); await ui.flush();
    await ui.ids.get('preview-next').click(); await ui.flush();
    assert.equal(ui.ids.get('preview-title').textContent, 'b.jpg');
    oldPixels.resolve(new Response(Buffer.from('old-pixels'), { headers: { 'Content-Type': 'image/png' } }));
    oldDetails.resolve(photo('a', { detailSections: [{ title: 'Old', items: ['Stale details'] }] }));
    await opening; await ui.flush();
    assert.equal(ui.ids.get('preview-title').textContent, 'b.jpg');
    assert.match(ui.ids.get('preview-details').textContent, /Evidence for b/);
    assert.doesNotMatch(ui.ids.get('preview-details').textContent, /Stale details/);
    assert.equal(ui.ids.get('preview-content').querySelector('img')?.src, ui.createdURLs.at(-1));
  } finally { await ui.close(); }
});

test('saved crop uses its current photo/crop identity and returns to the retained full preview', async () => {
  const ui = harness();
  try {
    await ui.pair();
    ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { detailCrops: [
      { id: 'saved-crop', label: 'Small subject', matched: true, box: {}, hasPreview: true },
      { id: 'withheld-crop', label: 'Unavailable', matched: false, box: {}, hasPreview: false },
    ] });
    await ui.window.PhotoFindry.showPhoto('off-page'); await ui.flush();
    const overview = ui.ids.get('preview-content').querySelector('img').src;
    const unavailable = ui.findPreviewButton('Open saved crop · Unavailable');
    assert.equal(unavailable.disabled, true);
    await ui.findPreviewButton('Open saved crop · Small subject').click(); await ui.flush();
    assert.equal(ui.ids.get('preview-back').hidden, false);
    assert.match(ui.ids.get('preview-title').textContent, /Small subject/);
    assert(ui.calls.some((call) => call.route === '/api/preview' && call.input.photoId === 'off-page' && call.input.cropId === 'saved-crop'));
    const crop = ui.ids.get('preview-content').querySelector('img').src;
    assert.notEqual(crop, overview); assert(!ui.revokedURLs.includes(overview));
    await ui.ids.get('preview-back').click(); await ui.flush();
    assert.equal(ui.ids.get('preview-content').querySelector('img').src, overview);
    assert(ui.revokedURLs.includes(crop)); assert(!ui.revokedURLs.includes(overview));
  } finally { await ui.close(); assert.deepEqual([...ui.revokedURLs].sort(), [...ui.createdURLs].sort()); }
});

test('returning to the full photo fences a late crop response without creating a new blob', async () => {
  const ui = harness();
  try {
    await ui.pair();
    ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { detailCrops: [{ id: 'slow-crop', label: 'Slow detail', hasPreview: true }] });
    const late = deferred();
    ui.handlers['/api/preview'] = ({ cropId }) => cropId ? late.promise
      : new Response(Buffer.from('overview'), { headers: { 'Content-Type': 'image/png' } });
    await ui.window.PhotoFindry.showPhoto('off-page'); await ui.flush();
    const before = ui.createdURLs.length;
    const opening = ui.findPreviewButton('Open saved crop · Slow detail').click(); await ui.flush();
    await ui.ids.get('preview-back').click();
    late.resolve(new Response(Buffer.from('late-crop'), { headers: { 'Content-Type': 'image/png' } }));
    await opening; await ui.flush();
    assert.equal(ui.ids.get('preview-back').hidden, true);
    assert.equal(ui.createdURLs.length, before);
    assert.equal(ui.ids.get('preview-content').querySelector('img').src, ui.createdURLs.at(-1));
  } finally { await ui.close(); }
});

test('preparing a pending photo preview keeps the open, zoomed modal across stale and fresh page refreshes', async () => {
  const ui = harness();
  const source = { id: 'source-a', mode: 'connected', status: 'available' };
  try {
    ui.setLibrary(page([photo('pending', { status: 'pending', hasPreview: false, source })]));
    let prepared = false;
    ui.handlers['/api/action'] = ({ action }) => { if (action === 'ensurePreview') prepared = true; return { reason: 'prepared' }; };
    ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { status: 'pending', hasPreview: prepared, source });
    await ui.pair();
    await ui.ids.get('gallery').children[0].fire('dblclick'); await ui.flush();
    const prepare = ui.ids.get('preview-content').querySelector('button');
    assert.match(prepare.textContent, /Prepare this photo/);
    await prepare.click(); await ui.flush();
    assert.equal(prepared, true); assert.equal(ui.ids.get('preview-dialog').open, true);
    const openImage = ui.ids.get('preview-content').querySelector('img');
    assert(openImage);
    await ui.ids.get('preview-zoom-in').click();
    assert.equal(ui.ids.get('preview-zoom-level').textContent, '1.25×');
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.ids.get('preview-dialog').open, true, 'old hasPreview=false page must not discard a newly authorized preview');
    assert.equal(ui.ids.get('preview-content').querySelector('img'), openImage);
    ui.setLibrary(page([photo('pending', { status: 'pending', hasPreview: true, source })]));
    await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
    assert.equal(ui.ids.get('preview-dialog').open, true);
    assert.equal(ui.ids.get('preview-zoom-level').textContent, '1.25×');
    assert(!ui.revokedURLs.includes(openImage.src));
  } finally { await ui.close(); }
});

test('a real fingerprint, source-policy or membership change withdraws the open preview', async () => {
  for (const changed of [
    [photo('a', { previewRevision: 'changed-fingerprint', source: { id: 'source-a', mode: 'connected', status: 'available' } })],
    [photo('a', { source: { id: 'source-a', mode: 'metadata', status: 'available' } })],
    [],
  ]) {
    const ui = harness();
    try {
      ui.setLibrary(page([photo('a', { source: { id: 'source-a', mode: 'connected', status: 'available' } })]));
      ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { source: { id: 'source-a', mode: 'connected', status: 'available' } });
      await ui.pair();
      await ui.ids.get('gallery').children[0].fire('dblclick'); await ui.flush();
      const imageURL = ui.ids.get('preview-content').querySelector('img').src;
      ui.setLibrary(page(changed));
      await ui.window.PhotoFindry.refreshCurrentPage(); await ui.flush();
      assert.equal(ui.ids.get('preview-dialog').open, false);
      assert(ui.revokedURLs.includes(imageURL));
    } finally { await ui.close(); }
  }
});

test('full preview preserves ranking evidence and labels filenames as metadata after one-record hydration', async () => {
  const ui = harness();
  try {
    ui.setLibrary(page([photo('a', { matchKind: 'ai-ranked', matchText: 'Two visible bicycles.', matchEvidence: [{ text: 'Two bicycles.' }, { text: 'Both beside a wall.' }] }),
      photo('b', { matchKind: 'exact', matchSection: 'filename', filenameMatch: 'exact', matchText: 'b.jpg' })]));
    await ui.pair();
    await ui.ids.get('gallery').children[0].fire('dblclick'); await ui.flush();
    const detail = ui.ids.get('preview-details');
    assert.match(detail.textContent, /Refined candidate.*Two visible bicycles.*Saved details used for ranking.*Both beside a wall/s);
    await ui.ids.get('preview-next').click(); await ui.flush();
    assert.match(detail.textContent, /Filename match.*b\.jpg.*Filename metadata, not visual evidence/s);
    assert.doesNotMatch(detail.textContent, /Two visible bicycles/);
  } finally { await ui.close(); }
});

test('off-page review displays bounded changes and distinct question coverage; Undo keeps browse selection', async () => {
  const ui = harness();
  let undone = false;
  ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, photoId === 'outside' ? { improvement: {
    decisionProvider: 'intern-decision-4b', decisionClaimCount: 8, decisionCheckedTotal: 16, decisionMoreQuestions: true,
    summaryState: undone ? 'undone' : 'suggestions', canUndo: !undone, changes: undone ? [] : [
      { kind: 'uncertain', text: 'Contact is obscured.', previousText: 'A person holds a cup.' },
      { kind: 'suggestion', text: '<script>A possible red cup.</script>', previousText: 'A cup.' }],
    changeCount: undone ? 0 : 15, notes: ['Saved locally.'],
  } } : {});
  ui.handlers['/api/action'] = ({ action, params }) => { if (action === 'undoImprovement') { assert.equal(params.photoId, 'outside'); undone = true; } return { id: 'review-job' }; };
  try {
    await ui.pair(); await ui.ids.get('gallery').children[0].click(); await ui.flush();
    const selected = ui.window.PhotoFindry.selectedIDs(), scope = ui.window.PhotoFindry.getScope();
    await ui.window.PhotoFindry.showPhoto('outside'); await ui.flush();
    const detail = ui.ids.get('preview-details');
    assert.match(detail.textContent, /Intern-Decision 4B.*8 questions checked this review.*16 distinct questions.*Review again can check more/s);
    assert.match(detail.textContent, /Uncertain details.*not confirmed as new facts.*Earlier: A person holds a cup/s);
    assert.match(detail.textContent, /Showing 2 of 15 recorded changes/);
    await ui.findPreviewButton('Review again').click(); await ui.flush();
    assert.deepEqual(ui.calls.filter(call => call.route === '/api/action' && call.input.action === 'enqueueJob').at(-1).input.params,
      { kind: 'improve', params: { photoIds: ['outside'], force: true } });
    await ui.findPreviewButton('Undo review').click(); await ui.flush();
    assert.match(detail.textContent, /Review undone/); assert.doesNotMatch(detail.textContent, /Contact is obscured/);
    assert.deepEqual(ui.window.PhotoFindry.selectedIDs(), selected); assert.deepEqual(ui.window.PhotoFindry.getScope(), scope);
  } finally { await ui.close(); }
});

test('per-photo metadata and face workflows use the off-page photo identity and preserve saved provenance', async () => {
  const ui = harness(); const opened = [];
  ui.window.PhotoFindryPanels = { show: (name, context) => opened.push({ name, context: JSON.parse(JSON.stringify(context)) }) };
  ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { archiveImport: true, metadataStatus: 'ready', faceScanLimited: true, source: { id: 'source-one', status: 'available', accessOriginals: true } });
  try {
    await ui.pair(); await ui.window.PhotoFindry.showPhoto('outside'); await ui.flush();
    const detail = ui.ids.get('preview-details');
    assert.match(detail.textContent, /Original pixels have not been reverified/);
    assert.match(detail.textContent, /Read complete. This original has no enabled camera or location fields/);
    assert.match(detail.textContent, /Some faces may be missing/);
    await ui.findPreviewButton('Read camera & location').click(); await ui.flush();
    await ui.findPreviewButton('Find faces in this photo').click(); await ui.flush();
    const submitted = ui.calls.filter(call => call.route === '/api/action' && call.input.action === 'enqueueJob').map(call => call.input.params);
    assert.deepEqual(submitted, [{ kind: 'metadata', params: { photoIds: ['outside'], force: true } }, { kind: 'faces', params: { photoIds: ['outside'] } }]);
    await ui.findPreviewButton('Name or correct people in this photo…').click(); await ui.flush();
    assert.deepEqual(opened, [{ name: 'people', context: { photoId: 'outside' } }]);
    assert.equal(ui.ids.get('preview-dialog').open, false);
  } finally { await ui.close(); }
});

test('off-page preview withdraws cached pixels when a source or fingerprint changes', async () => {
  const ui = harness(); let revision = 1, accessible = true;
  ui.handlers['/api/state'] = () => ({ galleryRevision: revision, libraryGeneration: 0 });
  ui.handlers['/api/photo'] = ({ photoId }) => photo(photoId, { source: { id: 'source-one', mode: accessible ? 'connected' : 'metadata', accessOriginals: accessible } });
  try {
    await ui.pair(); await ui.window.PhotoFindry.showPhoto('outside'); await ui.flush();
    assert.equal(ui.ids.get('preview-dialog').open, true);
    accessible = false; revision++;
    await ui.window.PhotoFindry.refreshState(); await ui.flush();
    assert.equal(ui.ids.get('preview-dialog').open, false);
    assert.equal(ui.ids.get('preview-content').children.length, 0);
  } finally { await ui.close(); }
});
