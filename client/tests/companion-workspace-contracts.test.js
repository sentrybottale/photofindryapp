'use strict';

// Run the real browser event handlers with an isolated DOM and recorded local API.
// No worker, model, original file, download or support service is contacted.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { webcrypto } = require('node:crypto');
const source = fs.readFileSync(path.join(__dirname, '../web-companion/workspace.js'), 'utf8');

function harness(overrides = {}) {
  let document;
  class Node {
    constructor(tag = 'div', value = '') {
      this.tagName = tag.toUpperCase(); this.children = []; this._text = value; this.value = ''; this.dataset = {};
      this.attributes = {}; this.listeners = {}; this.checked = false; this.disabled = false; this.hidden = false; this.open = false; this.isConnected = true;
      this.className = ''; this.classList = { add: (...names) => { this.className += ` ${names.join(' ')}`; }, toggle() {} };
    }
    append(...nodes) { for (let node of nodes) { if (typeof node === 'string') node = new Node('#text', node); this.children.push(node); node.parentNode = this; } }
    replaceChildren(...nodes) { this.children = []; this._text = ''; this.append(...nodes); }
    setAttribute(key, value) { this.attributes[key] = value; }
    addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
    async fire(name, extra = {}) { for (const handler of this.listeners[name] || []) await handler({ target: this, preventDefault() {}, ...extra }); }
    click() { return this.disabled ? Promise.resolve() : this.fire('click'); }
    focus() { document.activeElement = this; }
    select() {} scrollIntoView() {} after() {}
    showModal() { this.open = true; }
    close() { if (!this.open) return; this.open = false; queueMicrotask(() => this.fire('close')); }
    remove() { this.isConnected = false; if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((node) => node !== this); }
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    querySelectorAll(selector) { return descendants(this).filter(node => selector === '[data-jobs-focus]' && node.dataset.jobsFocus); }
    closest(selector) { let at = this.parentNode; while (at) { if (selector === 'dialog' && at.tagName === 'DIALOG') return at; at = at.parentNode; } return null; }
    getContext() { return { fillRect() {}, set fillStyle(_) {} }; }
    get textContent() { return this._text + this.children.map((node) => node.textContent).join(''); }
    set textContent(value) { this._text = String(value); this.children = []; }
  }
  const descendants = (node) => node.children.flatMap((child) => [child, ...descendants(child)]);
  const body = new Node('body');
  const ids = new Map([...fs.readFileSync(path.join(__dirname, '../web-companion/index.html'), 'utf8').matchAll(/\bid="([^"]+)"/g)].map((match) => [match[1], new Node()]));
  for (const [id, node] of ids) { node.id = id; body.append(node); }
  document = {
    body, activeElement: null, createElement: (tag) => new Node(tag), createTextNode: (value) => new Node('#text', value), createDocumentFragment: () => new Node('fragment'),
    getElementById: (id) => ids.get(id) || descendants(body).find((node) => node.id === id),
  };
  const state = {
    settings: { modelConfigured: true, projectorConfigured: true, rerankerModelConfigured: true, modelLabel: 'saved.gguf', projectorLabel: 'saved-projector.gguf', jinja: true },
    jobs: { jobs: [], notifications: [] }, folders: [], sources: [], runtime: { status: 'stopped' },
    ...overrides.state,
  };
  const scope = { sourceId: 'source-a', scopeId: '', status: 'all', sort: 'library', query: 'dog', metadataFilters: {}, locationOptions: {}, includeRelated: true };
  let selected = ['photo-a']; let aiId = overrides.aiId || null; let page = overrides.page || null;
  const calls = [], errors = [], notices = [], events = new Map(), createdURLs = [], revokedURLs = [];
  const invoke = async (name, params = {}) => {
    calls.push({ name, params: structuredClone(params) });
    if (overrides.responses?.[name]) return overrides.responses[name](params);
    if (name === 'feedbackEnvironment') return { appVersion: 'CLI companion 0.1.0', systemVersion: 'macOS 15.7', architecture: 'aarch64' };
    if (name === '/api/model-files/check') return { canSave: true, files: [], summary: 'Quick file checks only. No model was started.' };
    if (name === '/api/review-files/check') return { canSave: true, files: [], summary: 'Reviewer files checked.', physicalMemoryBytes: 32 * 1024 ** 3 };
    if (name === 'enqueueJob') return { id: 'new-job' };
    return {};
  };
  const bridge = {
    action: invoke, api: async (_path, body) => _path === '/api/action' ? invoke(body.action, body.params || {}) : invoke(_path, body),
    pick: overrides.pick || (async (kind) => ({ kind, grantId: `grant-${kind}`, label: `chosen-${kind}.gguf` })),
    refreshState: async () => state, refreshLibrary: async () => {}, refreshCurrentPage: async () => { calls.push({ name: 'refreshCurrentPage', params: structuredClone(scope) }); },
    getState: () => state, getScope: () => structuredClone(scope), selectedIDs: () => [...selected],
    setScope: async (change) => { calls.push({ name: 'setScope', params: structuredClone(change) }); Object.assign(scope, change); },
    getAIJobId: () => aiId, getPage: () => page,
    openAIResults: async (id) => { aiId = id; }, returnToLibrary: async () => { aiId = null; },
    setCandidateView: async (view) => { calls.push({ name: 'candidateView', params: { view } }); },
    showNotice: (message) => notices.push(message), showError: (message) => errors.push(message),
    previewBlob: async () => { throw Error('No fixture image'); }, activityPreviewBlob: async () => { throw Error('No fixture image'); },
  };
  class CustomEvent { constructor(type, { detail } = {}) { this.type = type; this.detail = detail; } }
  const window = {
    PhotoFindry: bridge, PhotoFindryPanels: { show() {} },
    addEventListener(name, handler) { if (!events.has(name)) events.set(name, []); events.get(name).push(handler); },
    dispatchEvent(event) { for (const handler of events.get(event.type) || []) handler(event); },
    matchMedia: () => ({ matches: true, addEventListener() {} }),
  };
  const urls = { createObjectURL: () => { const url = `blob:fixture-${createdURLs.length}`; createdURLs.push(url); return url; }, revokeObjectURL: (url) => revokedURLs.push(url) };
  const context = { window, document, CustomEvent, AbortController, Blob, URL: urls, Uint8Array, atob, Intl, Date, Map, Set, Promise, Number, String, Object, JSON, Array, Error, crypto: webcrypto, navigator: { clipboard: { writeText: async () => {} } }, queueMicrotask, setTimeout, clearTimeout, setInterval, clearInterval };
  vm.runInNewContext(source, context, { filename: 'workspace.js' });
  if (overrides.withPanels) vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../web-companion/panels.js'), 'utf8'), context, { filename: 'panels.js' });
  const emit = (name, detail) => window.dispatchEvent(new CustomEvent(`photofindry:${name}`, { detail }));
  emit('state', state);
  const all = () => descendants(body);
  const findButton = (label) => { const node = all().find((node) => node.tagName === 'BUTTON' && node.textContent === label); assert(node, `Missing button: ${label}`); return node; };
  const findField = (label) => {
    const wrapper = all().find((node) => node.tagName === 'LABEL' && (node.children[0]?.textContent === label || node.children[1]?.textContent === label));
    assert(wrapper, `Missing field: ${label}`); return wrapper.children.find((node) => ['INPUT', 'SELECT', 'TEXTAREA'].includes(node.tagName));
  };
  return { state, scope, calls, errors, notices, createdURLs, revokedURLs, all, emit, findButton, findField, ids, window, document,
    setPage(value) { page = value; emit('page', value); },
    select(ids) { selected = ids; emit('selection', { ids }); },
    async close() { emit('session', { connected: false }); await new Promise((resolve) => setTimeout(resolve, 0)); },
  };
}

test('Files navigation and feedback labels retain the established source contracts', async () => {
  const ui = harness(), opened = [];
  ui.window.PhotoFindryPanels.show = name => opened.push(name);
  try {
    await ui.findButton('Files').click(); assert.deepEqual(opened, ['sources']);
    assert(!ui.ids.get('workspace-navigation').children.some(node => node.textContent === 'Sources'));
    ui.emit('feedback'); const area = ui.findField('Area');
    assert(area.children.some(option => option.textContent === 'Files' && option.value === 'Sources'));
    assert(!ui.calls.some(call => call.name === 'sendFeedback'));
  } finally { await ui.close(); }
});

test('retag freezes a nonempty explicit selection across its confirmation', async () => {
  const ui = harness();
  try {
    ui.select(['photo-a', 'photo-b']); await ui.findButton('Retag').click();
    ui.select(['photo-c']);
    await ui.findButton('Retag selected').click();
    const submissions = ui.calls.filter((call) => call.name === 'enqueueJob');
    assert.deepEqual(submissions, [{ name: 'enqueueJob', params: { kind: 'tagging', params: { photoIds: ['photo-a', 'photo-b'], force: true } } }]);
    ui.select([]); assert.equal(ui.ids.get('selection-actions').hidden, true);
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('Jobs owns queue, notices and history; dismissing notifications never deletes saved work', async () => {
  const ui = harness({ state: { jobs: {
    jobs: [{ id: 'queued-one', kind: 'tagging', title: 'Pending photos', state: 'paused', total: 3, completed: 1 },
      { id: 'done-one', kind: 'tagging', title: 'Saved photo batch', state: 'completed', total: 2, completed: 2 }],
    notifications: [{ id: 'notice-one', title: 'Batch saved', message: 'Two accepted photos.', read: false }],
  } } });
  try {
    assert(!ui.ids.get('workspace-navigation').children.some(node => ['Activity', 'Model activity'].includes(node.textContent)));
    await ui.findButton('Jobs').click();
    const pane = ui.all().find(node => node.className === 'jobs-pane');
    const queueTab = ui.findButton('Queue (1)'), activityTab = ui.findButton('Activity (1 unread)');
    assert.equal(queueTab.attributes['aria-selected'], 'true');
    assert.match(pane.textContent, /Pending photos/); assert.doesNotMatch(pane.textContent, /Batch saved|Saved photo batch|Dismiss all notifications/);
    await activityTab.click();
    assert.equal(activityTab.attributes['aria-selected'], 'true'); assert.equal(queueTab.attributes['aria-selected'], 'false');
    assert.match(pane.textContent, /Batch saved.*Two accepted photos.*Saved photo batch/s); assert.doesNotMatch(pane.textContent, /Pending photos/);
    await ui.findButton('Dismiss all notifications').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction'), [{ name: 'jobAction', params: { action: 'dismissAll' } }]);
    assert.equal(ui.state.jobs.jobs.length, 2);
    ui.state.jobs.notifications = []; ui.emit('state', ui.state);
    assert(ui.findButton('Dismiss all notifications').disabled); assert(ui.findButton('Mark all read').disabled);
    assert.match(pane.textContent, /Saved photo batch/);
    assert.equal(ui.findButton('Activity'), activityTab);
    await activityTab.fire('keydown', { key: 'ArrowLeft' });
    assert.equal(queueTab.attributes['aria-selected'], 'true'); assert.match(pane.textContent, /Pending photos/);
  } finally { await ui.close(); }
});

test('model file draft survives native snapshots and saves only an opaque picker grant', async () => {
  const ui = harness();
  try {
    await ui.findButton('Settings').click();
    const row = ui.all().find((node) => node.className === 'model-file-row' && node.children[0]?.textContent === 'Vision model');
    const choose = row.children[2].children.find((node) => node.textContent === 'Choose…'); await choose.click();
    const template = ui.findField('Use the model’s chat formatting (recommended)'); template.checked = false; await template.fire('change');
    ui.state.settings.modelLabel = 'another-snapshot.gguf'; ui.emit('state', ui.state);
    assert.equal(row.children[1].textContent, 'chosen-model.gguf'); assert.equal(template.checked, false);
    await ui.findButton('Save settings').click();
    const configure = ui.calls.find((call) => call.name === 'configure');
    assert.deepEqual(configure.params, { modelGrant: 'grant-model', jinja: false, smartCrops: false });
    assert(!ui.calls.some((call) => call.name === 'startModel'));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('optional improvement crops save as a model preference without starting tagging or review', async () => {
  const ui = harness({ state: { settings: { modelConfigured: true, projectorConfigured: true, smartCrops: false } } });
  try {
    await ui.findButton('Settings').click();
    const option = ui.findField('Inspect small details during improvement');
    option.checked = true; await option.fire('change');
    await ui.findButton('Save settings').click();
    assert.deepEqual(ui.calls.filter((call) => call.name === 'configure').map((call) => call.params), [{ jinja: true, smartCrops: true }]);
    assert(!ui.calls.some((call) => ['enqueueJob', 'startModel', 'optimizeModel'].includes(call.name)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('saved optimization comparison shows bounded sample, quality and memory facts without a new model request', async () => {
  const ui = harness({ state: { modelOptimization: { running: false, message: 'Checked on this Mac.', result: {
    checkedAt: '2026-10-16T12:00:00Z', sampleCount: 3, secondsPerPhoto: 11.2, speedup: 1.3,
    improved: true, qualityChecked: true, memoryMeasured: true, peakMemoryBytes: 5 * 1024 ** 3,
    reports: [{ label: 'Baseline', accepted: false, secondsPerPhoto: 15.0, reason: 'Slower.' },
      { label: 'Compact', accepted: true, secondsPerPhoto: 11.2, reason: 'Retained details.' }],
  } } } });
  try {
    await ui.findButton('Settings').click();
    assert.match(ui.all().map((node) => node.textContent).join(' '), /3 sample photos checked.*11\.2 s\/photo.*1\.30× measured speed.*Saved detail retention checked.*Peak 5\.00 GB/);
    assert(ui.findButton('Recheck performance'));
    assert(!ui.calls.some((call) => call.name === 'optimizeModel'));
    ui.state.modelOptimization = { running: true, completed: 2, total: 3, message: 'Checking sample.' };
    ui.emit('state', ui.state);
    assert.match(ui.all().map((node) => node.textContent).join(' '), /2 of 3 sample checks/);
    assert.equal(ui.findButton('Check performance').disabled, true);
    assert.equal(ui.findButton('Stop performance check').hidden, false);
  } finally { await ui.close(); }
});

test('tagging diagnostics are fetched once on explicit photo identity and show the final attempt as text', async () => {
  const ui = harness({ responses: { taggingDiagnostic: ({ photoId }) => ({ photoId, photoName: 'Synthetic photo',
    summary: 'Final repair was invalid.', model: 'Local model', recordedAt: '2026-10-16T12:00:00Z',
    attempts: [{ number: 1, finishReason: 'length', raw: 'Earlier truncation' },
      { number: 2, finishReason: 'stop', raw: '<script>Final invalid content</script>', outputTokens: 18, tokenLimit: 64 }],
    findings: [{ path: '$.subjects[0]', message: 'Final response failed schema validation.' }] }) } });
  try {
    ui.state.monitor = { generatedTokens: 3 }; ui.emit('state', ui.state);
    assert(!ui.calls.some((call) => call.name === 'taggingDiagnostic'));
    ui.emit('tagging-diagnostic', { photoId: 'photo-one' }); await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(ui.calls.filter((call) => call.name === 'taggingDiagnostic'), [{ name: 'taggingDiagnostic', params: { photoId: 'photo-one' } }]);
    const diagnostic = ui.all().find((node) => node.className === 'diagnostic-content');
    assert.match(diagnostic.textContent, /Final repair was invalid.*Attempt 2.*Final invalid content.*Final response failed schema validation/s);
    assert(!ui.all().some((node) => node.tagName === 'SCRIPT'));
    ui.state.monitor.generatedTokens = 8; ui.emit('state', ui.state);
    assert.equal(ui.calls.filter((call) => call.name === 'taggingDiagnostic').length, 1);
  } finally { await ui.close(); }
});

test('stopping a paused AI job cancels owned remaining work and never deletes results', async () => {
  const ui = harness({ aiId: 'ai-job', page: { aiJobId: 'ai-job', aiResultsReady: true, aiCandidateCount: 90, aiRefinedCount: 12, aiViewTotalCount: 12, aiCandidateView: 'best' }, state: { jobs: { jobs: [{ id: 'ai-job', kind: 'aiSearch', title: 'Find dog', state: 'paused', total: 90, completed: 12 }], notifications: [] } } });
  try {
    await ui.findButton('Stop search').click(); await ui.findButton('Stop remaining work').click();
    assert.deepEqual(ui.calls.filter((call) => call.name === 'jobAction'), [{ name: 'jobAction', params: { action: 'cancel', id: 'ai-job' } }]);
    assert(!ui.calls.some((call) => /discard|delete|remove/i.test(call.name)));
    await ui.findButton('All candidates').click(); assert(ui.calls.some((call) => call.name === 'candidateView' && call.params.view === 'all'));
  } finally { await ui.close(); }
});

test('a new AI job never presents the previous job’s candidate summary while preparing', async () => {
  const ui = harness({ aiId: 'old-job', page: { aiJobId: 'old-job', aiResultsReady: true, aiCandidateCount: 16, aiRefinedCount: 16, aiViewTotalCount: 16, aiCandidateView: 'best' }, state: { jobs: { jobs: [{ id: 'old-job', kind: 'aiSearch', state: 'completed', total: 16, completed: 16 }], notifications: [] } } });
  try {
    assert.match(ui.ids.get('ai-results-controls').textContent, /16 of 16 candidates/);
    await ui.findButton('Run AI Search again').click();
    const controls = ui.ids.get('ai-results-controls');
    assert.match(controls.textContent, /Preparing saved AI search candidates/);
    assert.doesNotMatch(controls.textContent, /16 of 16|16 refined|Broaden/);
    assert(!ui.all().some((node) => node.tagName === 'BUTTON' && node.textContent === 'All candidates'));
    assert.equal(ui.calls.filter((call) => call.name === 'enqueueJob').at(-1).params.params.broaden, false);

    ui.state.jobs.jobs.push({ id: 'new-job', kind: 'aiSearch', state: 'running', phase: 'checkingSearchModel', message: 'Checking local search model…', aiResultsReady: false });
    ui.setPage({ aiJobId: 'new-job', aiResultsPending: true, aiResultsReady: false, photos: [], totalCount: 0 });
    ui.emit('state', ui.state);
    assert.match(controls.textContent, /Checking local search model/);
    assert.doesNotMatch(controls.textContent, /16 refined/);
    assert(ui.findButton('Stop search'));

    ui.setPage({ aiJobId: 'new-job', aiResultsPending: false, aiResultsReady: true, aiCandidateCount: 5, aiRefinedCount: 2, aiViewTotalCount: 2, aiCandidateView: 'best', semanticLimited: true });
    assert.match(controls.textContent, /2 of 5 candidates · 2 refined/);
    assert.match(controls.textContent, /Related evidence indexing is incomplete/);
    assert(ui.findButton('All candidates'));
  } finally { await ui.close(); }
});

test('mismatched and terminal empty AI pages cannot borrow candidate counts from another job', async () => {
  const ui = harness({ aiId: 'new-job', page: { aiJobId: 'old-job', aiCandidateCount: 16, aiRefinedCount: 16, aiViewTotalCount: 16 }, state: { jobs: { jobs: [{ id: 'new-job', kind: 'aiSearch', state: 'running', message: 'Preparing saved evidence…', aiResultsReady: false }], notifications: [] } } });
  try {
    assert.match(ui.ids.get('ai-results-controls').textContent, /Preparing saved AI search candidates/);
    assert.doesNotMatch(ui.ids.get('ai-results-controls').textContent, /16 refined/);
    ui.setPage({ aiJobId: 'new-job', aiResultsPending: false, aiResultsReady: false, aiError: 'Search failed before candidate capture.', photos: [], totalCount: 0 });
    ui.state.jobs.jobs[0].state = 'failed'; ui.emit('state', ui.state);
    assert.match(ui.ids.get('ai-results-controls').textContent, /No candidates were saved/);
    assert.match(ui.ids.get('ai-results-controls').textContent, /Search failed before candidate capture/);
    assert.doesNotMatch(ui.ids.get('ai-results-controls').textContent, /16 refined|Preparing saved AI search candidates/);
  } finally { await ui.close(); }
});

test('metric-only snapshots preserve AI controls and pre-checkpoint phases do not refetch results', async () => {
  const pending = harness({ aiId: 'pending-job', page: { aiJobId: 'pending-job', aiResultsPending: true, aiResultsReady: false, photos: [], totalCount: 0 }, state: { jobs: { jobs: [{ id: 'pending-job', kind: 'aiSearch', state: 'running', phase: 'retrieving', message: 'Finding saved evidence…', aiResultsReady: false }], notifications: [] } } });
  try {
    pending.state.jobs.jobs[0].phase = 'checkingSearchModel'; pending.state.jobs.jobs[0].message = 'Checking local search model…'; pending.emit('state', pending.state);
    pending.state.jobs.jobs[0].phase = 'preparingCandidates'; pending.state.jobs.jobs[0].message = 'Preparing candidate evidence…'; pending.emit('state', pending.state);
    await new Promise((resolve) => setTimeout(resolve, 520));
    assert.equal(pending.calls.filter((call) => call.name === 'refreshCurrentPage').length, 0);
    pending.state.jobs.jobs[0].aiResultsReady = true; pending.emit('state', pending.state);
    await new Promise((resolve) => setTimeout(resolve, 520));
    assert.equal(pending.calls.filter((call) => call.name === 'refreshCurrentPage').length, 1);
  } finally { await pending.close(); }

  const ready = harness({ aiId: 'ready-job', page: { aiJobId: 'ready-job', aiResultsReady: true, aiCandidateCount: 16, aiRefinedCount: 16, aiViewTotalCount: 16, aiCandidateView: 'best' }, state: { jobs: { jobs: [{ id: 'ready-job', kind: 'aiSearch', state: 'running', phase: 'refining', completed: 16, total: 16, aiResultsReady: true }], notifications: [] } } });
  try {
    const allCandidates = ready.findButton('All candidates');
    await new Promise((resolve) => setTimeout(resolve, 520));
    const refreshes = ready.calls.filter((call) => call.name === 'refreshCurrentPage').length;
    ready.state.monitor = { generatedTokens: 10 }; ready.emit('state', ready.state);
    ready.state.monitor.generatedTokens = 20; ready.emit('state', ready.state);
    await new Promise((resolve) => setTimeout(resolve, 520));
    assert.equal(ready.findButton('All candidates'), allCandidates);
    assert.equal(ready.calls.filter((call) => call.name === 'refreshCurrentPage').length, refreshes);
  } finally { await ready.close(); }
});

test('a pending AI checkpoint gets one delayed recovery probe, without phase-driven polling', async () => {
  const ui = harness({ aiId: 'pending-job', page: { aiJobId: 'pending-job', aiResultsPending: true, aiResultsReady: false, photos: [], totalCount: 0 }, state: { jobs: { jobs: [{ id: 'pending-job', kind: 'aiSearch', state: 'running', phase: 'checkingSearchModel', aiResultsReady: false }], notifications: [] } } });
  try {
    await new Promise((resolve) => setTimeout(resolve, 2100));
    assert.equal(ui.calls.filter((call) => call.name === 'refreshCurrentPage').length, 1);
    ui.state.jobs.jobs[0].phase = 'preparingCandidates'; ui.emit('state', ui.state);
    await new Promise((resolve) => setTimeout(resolve, 520));
    assert.equal(ui.calls.filter((call) => call.name === 'refreshCurrentPage').length, 1);
  } finally { await ui.close(); }
});

test('feedback retries preserve submission identity and never submit before explicit consent', async () => {
  let attempts = 0;
  const ui = harness({ responses: { sendFeedback: async () => { attempts += 1; throw Error('Receipt unavailable.'); } } });
  try {
    ui.emit('feedback');
    for (const [label, value] of [['Short summary', 'Fixture report'], ['What happened or what would help?', 'No real request is sent.'], ['Email for follow-up', 'fixture@example.invalid']]) {
      const field = ui.findField(label); field.value = value; await field.fire('input');
    }
    const send = ui.findButton('Send feedback'); assert.equal(send.disabled, true); await send.click(); assert.equal(attempts, 0);
    const consent = ui.findField('Send these fields to PhotoFindry support'); consent.checked = true; await consent.fire('change');
    await send.click(); await send.click();
    const sent = ui.calls.filter((call) => call.name === 'sendFeedback'); assert.equal(sent.length, 2);
    assert.equal(sent[0].params.submissionId, sent[1].params.submissionId); assert.equal(sent[0].params.confirmSend, true);
    const details = ui.findField('What happened or what would help?'); details.value += ' Edited.'; await details.fire('input');
    assert.equal(consent.checked, false); assert.equal(send.disabled, true);
    consent.checked = true; await consent.fire('change'); await send.click();
    const final = ui.calls.filter((call) => call.name === 'sendFeedback').at(-1);
    assert.notEqual(final.params.submissionId, sent[0].params.submissionId);
    assert.equal(ui.findField('Short summary').value, 'Fixture report');
  } finally { await ui.close(); }
});

test('configured stopped models and absent measurements are presented without false setup or zero coverage', async () => {
  const ui = harness({ state: { runtime: { status: 'stopped', message: 'Choose a vision model and matching projector.' } } });
  try {
    await ui.findButton('Settings').click();
    const content = () => ui.all().filter((node) => node.tagName === 'DIALOG').map((node) => node.textContent).join(' ');
    assert.match(content(), /Configured; loads automatically when needed/);
    assert.doesNotMatch(content(), /Choose a vision model and matching projector/);
    assert.match(content(), /Saved preview count not measured/);
    assert.match(content(), /Photo coverage not measured/);
    assert.match(content(), /Detail coverage not measured/);
    assert.doesNotMatch(content(), /0 saved previews|0 of 0 photos|0 of 0 details/);
    ui.state.runtime = { status: 'error', message: 'Model file could not be read.' }; ui.emit('state', ui.state);
    assert.match(content(), /Model file could not be read/);
  } finally { await ui.close(); }
});

test('ordinary query refreshes once on source-job completion with unchanged gallery revision, excluding metric ticks and face-only scans', async () => {
  const ui = harness({ state: { galleryRevision: 7, jobs: { jobs: [{ id: 'tag-off-page', kind: 'tagging', state: 'running', total: 1, completed: 0 }], notifications: [] } } });
  try {
    ui.state.monitor = { generatedTokens: 10 }; ui.emit('state', ui.state);
    assert(!ui.calls.some((call) => call.name === 'refreshCurrentPage'));
    ui.state.jobs.jobs[0].state = 'completed'; ui.state.jobs.jobs[0].completed = 1; ui.emit('state', ui.state);
    ui.state.monitor.generatedTokens = 20; ui.emit('state', ui.state);
    await new Promise((resolve) => setTimeout(resolve, 380));
    const refreshes = ui.calls.filter((call) => call.name === 'refreshCurrentPage');
    assert.equal(refreshes.length, 1); assert.equal(refreshes[0].params.query, 'dog'); assert.equal(refreshes[0].params.sourceId, 'source-a');
    ui.state.jobs.jobs.push({ id: 'face-only', kind: 'faces', state: 'running', total: 1, completed: 0 }); ui.emit('state', ui.state);
    ui.state.jobs.jobs[1].state = 'completed'; ui.emit('state', ui.state);
    await new Promise((resolve) => setTimeout(resolve, 380));
    assert.equal(ui.calls.filter((call) => call.name === 'refreshCurrentPage').length, 1);
  } finally { await ui.close(); }
});

test('feedback screenshots require displayed previews and consent, send grants only, and revoke preview URLs', async () => {
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZlSAAAAAASUVORK5CYII=', 'base64');
  const ui = harness({ responses: {
    prepareFeedbackScreenshot: async () => ({ pngBase64: png.toString('base64'), bytes: png.length, width: 1, height: 1, sha256: 'a'.repeat(64) }),
    sendFeedback: async () => { throw Error('Mock receipt unavailable.'); },
  } });
  try {
    ui.emit('feedback');
    for (const [label, value] of [['Short summary', 'Screenshot fixture'], ['What happened or what would help?', 'Prepared screenshot review.'], ['Email for follow-up', 'fixture@example.invalid']]) {
      const field = ui.findField(label); field.value = value; await field.fire('input');
    }
    await ui.findButton('Choose screenshot…').click();
    const review = ui.findField('I reviewed these screenshots and want to include them');
    const consent = ui.findField('Send these fields to PhotoFindry support');
    consent.checked = true; await consent.fire('change'); review.checked = true; await review.fire('change');
    const send = ui.findButton('Send feedback'); assert.equal(send.disabled, true);
    const preview = ui.all().find((node) => node.tagName === 'IMG' && node.alt === 'Prepared screenshot 1'); assert(preview); await preview.fire('load');
    assert.equal(send.disabled, false); await send.click(); await send.click();
    const sent = ui.calls.filter((call) => call.name === 'sendFeedback'); assert.equal(sent.length, 2);
    assert.deepEqual(sent[0].params.screenshotGrants, ['grant-screenshot']); assert.equal(sent[0].params.submissionId, sent[1].params.submissionId);
    assert(!('screenshots' in sent[0].params)); assert(!('pngBase64' in sent[0].params));
    await ui.findButton('Remove screenshot 1').click(); assert.equal(consent.checked, false); assert.equal(send.disabled, true);
    assert.deepEqual(ui.revokedURLs, ui.createdURLs);
    consent.checked = true; await consent.fire('change'); await send.click();
    const changed = ui.calls.filter((call) => call.name === 'sendFeedback').at(-1); assert.notEqual(changed.params.submissionId, sent[0].params.submissionId); assert.deepEqual(changed.params.screenshotGrants, []);
    await ui.findButton('Choose screenshot…').click();
  } finally { await ui.close(); assert.deepEqual(ui.revokedURLs, ui.createdURLs); }
});

test('whole-folder retag uses the frozen registered scope rather than gallery selection or filters', async () => {
  const ui = harness({ state: { folders: [{ scopeId: 'folder-a', sourceId: 'source-a', name: 'Archive', count: 500 }] } });
  try {
    await ui.findButton('Subfolders').click();
    await ui.findButton('Retag this folder…').click();
    ui.select(['unrelated-current-page']); ui.scope.scopeId = 'different-folder'; ui.scope.query = 'filtered query';
    await ui.findButton('Retag whole folder').click();
    const request = ui.calls.find((call) => call.name === 'enqueueJob');
    assert.deepEqual(request.params, { kind: 'tagging', params: { scopeId: 'folder-a', sourceId: 'source-a', force: true } });
  } finally { await ui.close(); }
});

test('file guidance explains the adapter and optional text template, and checks never start inference', async () => {
  const ui = harness();
  try {
    await ui.findButton('Settings').click(); await new Promise(resolve=>setImmediate(resolve));
    const content=ui.all().find(node=>node.className==='dialog-body').textContent;
    assert.match(content,/Vision adapter \(mmproj\)/); assert.match(content,/Usually leave empty/); assert.match(content,/Never select a \.gguf here/);
    await ui.findButton('Check files').click();
    assert(ui.calls.some(call=>call.name==='/api/model-files/check'));
    assert(!ui.calls.some(call=>['configure','startModel','enqueueJob','optimizeModel'].includes(call.name)));
  } finally { await ui.close(); }
});

test('a mistaken template blocks save and clearing it recovers without changing saved model paths', async () => {
  const ui=harness({responses:{'/api/model-files/check':params=>({canSave:!params.templateGrant,files:[{role:'template',status:params.templateGrant?'error':'empty',message:params.templateGrant?'This is a model-weight file. Choose text or clear the override.':'Built-in format will be used.'}],summary:'Quick check only.'})}});
  try {
    await ui.findButton('Settings').click();
    const row=ui.all().find(node=>node.className==='model-file-row'&&node.children[0]?.textContent==='Custom chat template');
    await row.children[2].children[0].click();
    await ui.findButton('Save settings').click();
    assert(!ui.calls.some(call=>call.name==='configure')); assert(ui.errors.some(message=>message.includes('saved settings are unchanged')));
    assert.match(row.textContent,/model-weight file/);
    await ui.findButton('Use built-in').click(); await ui.findButton('Save settings').click();
    const saved=ui.calls.find(call=>call.name==='configure'); assert.deepEqual(saved.params,{clearTemplate:true,jinja:true,smartCrops:false});
    assert(!ui.calls.some(call=>call.name==='startModel'));
  } finally { await ui.close(); }
});

test('late file checks cannot overwrite a newer choice or save a superseded draft', async () => {
  let delayed=false,finish;
  const ui=harness({responses:{'/api/model-files/check':async()=>{if(delayed){delayed=false;return new Promise(resolve=>{finish=resolve;});}return {canSave:true,files:[{role:'model',status:'ok',message:'New choice checked.'}],summary:'Current check.'};}}});
  try {
    await ui.findButton('Settings').click();await new Promise(resolve=>setImmediate(resolve));
    const row=ui.all().find(node=>node.className==='model-file-row'&&node.children[0]?.textContent==='Vision model');await row.children[2].children[0].click();
    delayed=true;const saving=ui.findButton('Save settings').click();await new Promise(resolve=>setImmediate(resolve));
    await ui.findButton('Check files').click();finish({canSave:true,files:[{role:'model',status:'error',message:'Stale report'}],summary:'Stale'});await saving;
    assert(!ui.calls.some(call=>call.name==='configure'));assert(!row.textContent.includes('Stale report'));assert.match(row.textContent,/New choice checked/);
  } finally { await ui.close(); }
});

test('preview settings preserve the native 800px size and an existing fractional GiB budget', async () => {
  const ui = harness({ state: { storagePreferences: { keepOnBrowse: true, dimension: 800, budgetBytes: 96 * 1024 ** 2 } } });
  try {
    await ui.findButton('Settings').click();
    const dimension = ui.findField('Longest edge');
    assert.deepEqual(dimension.children.map(node => node.value), ['800', '1024']);
    assert.equal(dimension.value, '800');
    assert.equal(ui.findField('Preview storage budget · GiB').value, '0.09375');
    await ui.findButton('Save settings').click();
    assert.deepEqual(ui.calls.find(call => call.name === 'configureStorage').params, { keepOnBrowse: true, dimension: 800, budgetBytes: 96 * 1024 ** 2 });
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('automatic source waiting can be paused without retrying, while settled missing originals expose only targeted recovery', async () => {
  const waiting = { id: 'waiting', kind: 'tagging', title: 'Waiting batch', state: 'paused', pauseReason: 'source-waiting', sourceLifecycleVersion: 1, waitingForAvailability: true, sourceNextAction: { title: 'Reconnect originals' }, sourceId: 'source-a', completed: 5, total: 10, failed: 1, skippedOriginalCount: 1, waitingOriginalCount: 5 };
  const ui = harness({ state: { jobs: { jobs: [waiting], notifications: [] } } });
  try {
    const card = ui.window.PhotoFindryJobs.card(waiting);
    assert.match(card.textContent, /Waiting for originals/);
    assert.match(card.textContent, /3 tagged.*5 originals waiting.*1 need attention.*1 removed from library/);
    assert(card.children.at(-1).children.some(node => node.textContent === 'Pause'));
    assert(!card.children.at(-1).children.some(node => node.textContent === 'Resume'));
    await card.children.at(-1).children.find(node => node.textContent === 'Pause').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction'), [{ name: 'jobAction', params: { action: 'pause', id: 'waiting' } }]);
    const settled = { ...waiting, id: 'missing', pauseReason: 'source-missing', phase: 'available-finished', missingOriginalCount: 5 };
    const settledCard = ui.window.PhotoFindryJobs.card(settled);
    const labels = settledCard.children.at(-1).children.map(node => node.textContent);
    assert(labels.includes('Tag recovered photos'));
    assert(!labels.some(label => ['Resume', 'Stop', 'Run next', 'Schedule', 'Pause'].includes(label)));
    await settledCard.children.at(-1).children.find(node => node.textContent === 'Tag recovered photos').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction').at(-1), { name: 'jobAction', params: { action: 'retryMissing', id: 'missing' } });
  } finally { await ui.close(); }
});

test('terminal activity summary stays visible until dismissed and never exposes older history when dismissed', async () => {
  const ui = harness({ state: { jobs: { jobs: [
    { id: 'old', kind: 'tagging', title: 'Old batch', state: 'completed', total: 8, completed: 8, updatedAt: '2026-10-15T12:00:00Z' },
    { id: 'done', kind: 'tagging', title: 'Latest batch', state: 'completed', total: 4, completed: 4, updatedAt: '2026-10-16T12:00:00Z' },
  ], notifications: [] } } });
  try {
    const strip = ui.ids.get('job-strip');
    assert.equal(strip.hidden, false); assert.match(strip.textContent, /Latest batch.*Tagging complete/);
    assert(!strip.children.some(node => node.textContent === 'Stop'));
    await ui.findButton('Dismiss summary').click(); assert.equal(strip.hidden, true);
    ui.state.monitor = { generatedTokens: 25 }; ui.emit('state', ui.state); assert.equal(strip.hidden, true);
    ui.state.jobs.jobs.push({ id: 'new', kind: 'improve', title: 'New review', state: 'running', total: 3, completed: 0, createdAt: '2026-10-16T13:00:00Z' });
    ui.state.jobs.activeJobId = 'new'; ui.emit('state', ui.state);
    assert.equal(strip.hidden, false); assert.match(strip.textContent, /New review/); assert.doesNotMatch(strip.textContent, /Old batch|Latest batch/);
  } finally { await ui.close(); }
});

test('calibration sample progress is independent of photo counts and manual checks remain visible with stop control', async () => {
  const job = { id: 'tag', kind: 'tagging', title: 'Tag pending', state: 'running', completed: 0, total: 40 };
  const ui = harness({ state: { jobs: { activeJobId: 'tag', jobs: [job], notifications: [] }, modelOptimization: { running: true, total: 3, completed: 2, phase: 'checking' } } });
  try {
    const strip = ui.ids.get('job-strip');
    assert.match(strip.textContent, /Checking model performance.*2 of 3 sample checks · 0 of 40 processed/s);
    const meter = ui.all().find(node => node.id === 'activity-meter'); assert.equal(meter.value, 2); assert.equal(meter.max, 3);
    ui.state.jobs = { jobs: [], notifications: [] }; ui.emit('state', ui.state);
    assert.match(strip.textContent, /Model performance.*2 of 3 sample checks/s); assert.doesNotMatch(strip.textContent, /40 processed/);
    const stop = ui.findButton('Stop check'); await stop.click();
    assert(ui.calls.some(call => call.name === 'cancelModelOptimization'));
    ui.state.modelOptimization.phase = 'saving'; ui.emit('state', ui.state); assert(ui.findButton('Stop check').disabled);
  } finally { await ui.close(); }
});

test('Jobs preserves saved-model failures, ignores old source progress in a new phase and counts refinement attempts', async () => {
  const ui = harness();
  try {
    const failed = ui.window.PhotoFindryJobs.card({ id: 'failure', kind: 'tagging', title: 'Failed batch', state: 'failed', modelName: 'saved-with-job.gguf', failureSummary: ['Final response failed validation.'], total: 4, completed: 2 });
    assert.match(failed.textContent, /Final response failed validation.*Model saved with job: saved-with-job\.gguf/s);
    const review = ui.window.PhotoFindryJobs.card({ id: 'review-failure', kind: 'improve', state: 'failed', failureSummary: ['Invalid evidence review: visual age requires an explicitly approximate range for the same person.'] });
    assert.match(review.textContent, /Saved tags are kept.*Retry to use automatic correction/s);
    assert.equal(review.children.find(node => node.tagName === 'DETAILS').open, false);
    assert.match(review.children.find(node => node.tagName === 'DETAILS').textContent, /visual age requires/);
    const checking = ui.window.PhotoFindryJobs.card({ id: 'source', kind: 'sourceRecovery', title: 'Check source', state: 'running', phase: 'checking', progressDetails: { phase: 'match', checked: 500, total: 500 } });
    assert.match(checking.textContent, /Checking folder access/); assert.doesNotMatch(checking.textContent, /500/);
    const ranked = ui.window.PhotoFindryJobs.card({ id: 'rank', kind: 'aiSearch', state: 'running', aiRanking: true, aiRankingStage: 'refining', aiRefinedCount: 2, aiRefineProcessedCount: 7, completed: 2, total: 10 });
    assert.match(ranked.textContent, /7 of 10 refinement checks · 2 refined/);
    assert.equal(ranked.children.find(node => node.tagName === 'PROGRESS').value, 7);
  } finally { await ui.close(); }
});

test('folder improvement and screening preserve opaque subtree scope and overnight preference', async () => {
  const folder = { scopeId: 'folder-scope', sourceId: 'source-a', name: 'Pictures', count: 30 };
  const ui = harness({ state: { folders: [folder], improvementPreferences: { start: '23:30', end: '06:15' } } });
  try {
    await ui.findButton('Subfolders').click();
    await ui.findButton('Improve overnight').click();
    await ui.findButton('Screen saved tags').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'enqueueJob'), [
      { name: 'enqueueJob', params: { kind: 'improve', params: { scopeId: 'folder-scope', sourceId: 'source-a', force: false }, schedule: { mode: 'overnight', start: '23:30', end: '06:15' } } },
      { name: 'enqueueJob', params: { kind: 'reviewScreen', params: { scopeId: 'folder-scope', sourceId: 'source-a' } } },
    ]);
  } finally { await ui.close(); }
});

test('interrupted deletion exposes bounded native recovery and blocks ordinary queue resume', async () => {
  const ui = harness({ state: { jobs: { paused: true, jobs: [], notifications: [], purgeRecovery: [{ id: 'begun-plan', startedCount: 2, untouchedCount: 18, state: 'paused' }] } } });
  try {
    await ui.findButton('Jobs').click();
    assert(ui.findButton('Resume queue').disabled);
    assert.match(ui.all().map(node => node.textContent).join(' '), /2 entries started · 18 untouched entries kept/);
    await ui.findButton('Finish cleanup').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction'), [{ name: 'jobAction', params: { action: 'finishDeletion', deletionId: 'begun-plan' } }]);
    ui.state.jobs.purgeRecovery[0].state = 'running'; ui.emit('state', ui.state);
    assert(ui.findButton('Finishing…').disabled);
    assert(!ui.calls.some(call => ['deleteRemovedEntries', 'enqueueJob'].includes(call.name)));
  } finally { await ui.close(); }
});

test('bounded queue counts and notification actions retain the native full-queue and saved-result identities', async () => {
  const ui = harness({ state: { jobs: { counts: { queued: 20, attention: 1 }, jobs: [{ id: 'failed-ai', kind: 'aiSearch', title: 'Saved search', state: 'failed', completed: 2, total: 20 }, { id: 'next', kind: 'tagging', state: 'queued' }], notifications: [{ id: 'note', jobId: 'failed-ai', title: 'Search needs attention', read: false }] } } });
  try {
    await ui.findButton('Jobs').click(); assert(ui.findButton('Queue (21)'));
    assert.match(ui.all().map(node => node.textContent).join(' '), /Showing next 2 of 21 jobs/);
    await ui.findButton('Activity (1 unread)').click();
    const notice = ui.all().find(node => node.className === 'notification-card');
    assert.match(notice.textContent, /Retry.*Dismiss from queue.*View results/);
    const retry = notice.children.find(node => node.className === 'button-row').children.find(node => node.textContent === 'Retry'); await retry.click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction').at(-1), { name: 'jobAction', params: { action: 'retry', id: 'failed-ai' } });
  } finally { await ui.close(); }
});

test('feedback diagnostics are opt-in, previewed exactly and frozen across unchanged retries', async () => {
  let snapshots = 0, attempts = 0;
  const ui = harness({ responses: {
    feedbackEnvironment: () => ({ appVersion: 'CLI companion 0.1.0', systemVersion: 'macOS 15.7', architecture: 'aarch64' }),
    feedbackDiagnostics: () => ({ schema: 1, consent: true, events: [{ secondsAgo: snapshots++, component: 'tagging', state: 'paused', phase: 'tagging', metrics: { completed: 2, total: 4, failed: 0 } }] }),
    sendFeedback: () => { attempts++; throw Error('Fixture has no network.'); },
  } });
  try {
    ui.emit('feedback'); await new Promise(resolve => setImmediate(resolve));
    assert.equal(snapshots, 0);
    for (const [label, value] of [['Short summary', 'Diagnostic fixture'], ['What happened or what would help?', 'A private local test.'], ['Email for follow-up', 'fixture@example.invalid']]) {
      const node = ui.findField(label); node.value = value; await node.fire('input');
    }
    const include = ui.findField('Include diagnostic log with this report'); include.checked = true; await include.fire('change'); await new Promise(resolve => setImmediate(resolve));
    assert.equal(snapshots, 1);
    const preview = ui.all().find(node => node.className.split(' ').includes('diagnostic-preview')); assert.equal(JSON.parse(preview.textContent).events[0].metrics.completed, 2);
    assert.match(ui.all().find(node => node.className.split(' ').includes('feedback-report-preview')).textContent, /macOS 15.7.*Optional diagnostic log/s);
    const consent = ui.findField('Send these fields to PhotoFindry support'); consent.checked = true; await consent.fire('change');
    await ui.findButton('Send feedback').click(); await ui.findButton('Send feedback').click();
    const sent = ui.calls.filter(call => call.name === 'sendFeedback'); assert.equal(attempts, 2);
    assert.equal(sent[0].params.submissionId, sent[1].params.submissionId);
    assert.deepEqual(sent[0].params.diagnostics, JSON.parse(preview.textContent)); assert.deepEqual(sent[0].params.diagnostics, sent[1].params.diagnostics);
    await ui.findButton('Refresh diagnostic log').click(); assert.equal(snapshots, 2); assert.equal(consent.checked, false);
    include.checked = false; await include.fire('change'); await new Promise(resolve => setImmediate(resolve)); assert.equal(preview.hidden, true);
    consent.checked = true; await consent.fire('change'); await ui.findButton('Send feedback').click();
    assert.equal(ui.calls.filter(call => call.name === 'sendFeedback').at(-1).params.diagnostics, null);
  } finally { await ui.close(); }
});

test('feedback cancellation keeps the submission identity and clear draft explicitly creates a fresh report', async () => {
  let rejectSend; let sends = 0;
  const ui = harness({ responses: {
    sendFeedback: () => { sends++; return sends === 1 ? new Promise((_, reject) => { rejectSend = reject; }) : { reportID: 'PF-0123456789ABCDEF', receivedAt: '2026-10-16T11:00:00Z' }; },
    cancelFeedback: () => { const error = new Error('Cancelled'); error.name = 'AbortError'; rejectSend(error); return { cancelled: true, delivery: 'unknown' }; },
  } });
  try {
    ui.emit('feedback');
    for (const [label, value] of [['Short summary', 'Cancel fixture'], ['What happened or what would help?', 'A private local test.'], ['Email for follow-up', 'fixture@example.invalid']]) {
      const node = ui.findField(label); node.value = value; await node.fire('input');
    }
    const consent = ui.findField('Send these fields to PhotoFindry support'); consent.checked = true; await consent.fire('change');
    const pending = ui.findButton('Send feedback').click(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.findButton('Cancel sending').hidden, false); await ui.findButton('Cancel sending').click(); await pending;
    const first = ui.calls.find(call => call.name === 'sendFeedback');
    assert.equal(ui.calls.find(call => call.name === 'cancelFeedback').params.submissionId, first.params.submissionId);
    assert.match(ui.all().map(node => node.textContent).join(' '), /server may already have received/);
    await ui.findButton('Send feedback').click();
    assert.equal(ui.calls.filter(call => call.name === 'sendFeedback').at(-1).params.submissionId, first.params.submissionId);
    assert.equal(ui.findButton('New report').hidden, false); await ui.findButton('New report').click(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.findField('Short summary').value, '');
    const summary = ui.findField('Short summary'); summary.value = 'Temporary'; await summary.fire('input');
    await ui.findButton('Clear draft…').click(); assert.equal(ui.findField('Short summary').value, 'Temporary');
    await ui.findButton('Clear draft').click(); await new Promise(resolve => setImmediate(resolve)); assert.equal(ui.findField('Short summary').value, '');
  } finally { await ui.close(); }
});

test('closing and reopening feedback retains the pending submission, cancellation and final receipt', async () => {
  let finish;
  const ui = harness({ responses: { sendFeedback: () => new Promise(resolve => { finish = resolve; }) } });
  try {
    ui.emit('feedback');
    for (const [label, value] of [['Short summary', 'Pending fixture'], ['What happened or what would help?', 'A private local test.'], ['Email for follow-up', 'fixture@example.invalid']]) {
      const node = ui.findField(label); node.value = value; await node.fire('input');
    }
    const consent = ui.findField('Send these fields to PhotoFindry support'); consent.checked = true; await consent.fire('change');
    const pending = ui.findButton('Send feedback').click(); await new Promise(resolve => setImmediate(resolve));
    const old = ui.all().find(node => node.tagName === 'DIALOG' && node.open); old.close(); await new Promise(resolve => setImmediate(resolve));
    ui.emit('feedback'); await new Promise(resolve => setImmediate(resolve));
    assert.equal(ui.findField('Short summary').disabled, true); assert.equal(ui.findButton('Cancel sending').hidden, false); assert.equal(ui.findButton('Send feedback').disabled, true);
    assert.match(ui.all().find(node => node.className === 'dialog-body').textContent, /Sending your reviewed report/);
    finish({ reportID: 'PF-0123456789ABCDEF', receivedAt: '2026-10-16T11:00:00Z' }); await pending;
    assert.equal(ui.findField('Short summary').disabled, false); assert.equal(ui.findButton('Cancel sending').hidden, true); assert.equal(ui.findButton('New report').hidden, false);
    assert.match(ui.all().find(node => node.className === 'dialog-body').textContent, /Received as PF-0123456789ABCDEF/);
    assert.equal(ui.calls.filter(call => call.name === 'sendFeedback').length, 1);
  } finally { finish?.({ reportID: 'PF-0123456789ABCDEF' }); await ui.close(); }
});

test('Jobs progress preserves the exact job and action focus across reordering, then falls back when that action disappears', async () => {
  const first = { id: 'job-a', kind: 'tagging', state: 'running', title: 'First batch', completed: 1, total: 50 };
  const second = { id: 'job-b', kind: 'improve', state: 'queued', title: 'Second batch', completed: 0, total: 40 };
  const ui = harness({ state: { jobs: { jobs: [first, second], notifications: [] } } });
  const findKey = key => ui.all().find(node => node.dataset.jobsFocus === JSON.stringify(key));
  try {
    await ui.findButton('Jobs').click();
    const pane = ui.all().find(node => node.className === 'jobs-pane'); pane.parentNode.scrollTop = 250;
    const oldPause = findKey(['job', 'job-b', 'pause']); oldPause.focus();
    first.completed = 2; ui.state.jobs.jobs = [second, first]; ui.emit('state', ui.state);
    const updatedPause = findKey(['job', 'job-b', 'pause']);
    assert.notEqual(updatedPause, oldPause); assert.equal(ui.document.activeElement, updatedPause);
    assert.equal(pane.parentNode.scrollTop, 250);
    await updatedPause.click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction').at(-1), { name: 'jobAction', params: { action: 'pause', id: 'job-b' } });
    second.state = 'pausing'; ui.emit('state', ui.state);
    assert.equal(ui.document.activeElement, pane, 'Do not redirect keyboard input to another job’s Pause or the newly available action.');
    assert(findKey(['job', 'job-a', 'pause'])); assert(!findKey(['job', 'job-b', 'pause']));
  } finally { await ui.close(); }
});

test('Jobs keeps focused toolbar choices, notification identity and history disclosure across live updates', async () => {
  const job = { id: 'job-a', kind: 'tagging', state: 'running', title: 'Batch', completed: 1, total: 50 };
  const notes = [{ id: 'notice-a', title: 'A', read: false, createdAt: '2026-10-16T10:00:00Z' }, { id: 'notice-b', title: 'B', read: false, createdAt: '2026-10-16T11:00:00Z' }];
  const ui = harness({ state: { jobs: { jobs: [job], notifications: notes }, powerPreferences: { mode: 'processing' } } });
  const findKey = key => ui.all().find(node => node.dataset.jobsFocus === JSON.stringify(key));
  try {
    await ui.findButton('Jobs').click();
    const oldPower = findKey(['queue', 'keepAwake']); oldPower.focus(); oldPower.value = 'queue';
    job.completed = 2; ui.emit('state', ui.state);
    const power = findKey(['queue', 'keepAwake']); assert.equal(ui.document.activeElement, power); assert.equal(power.value, 'queue');
    assert(!ui.calls.some(call => call.name === 'configurePower'), 'A live update must not commit an in-progress keyboard choice.');
    await power.fire('change'); assert.deepEqual(ui.calls.find(call => call.name === 'configurePower').params, { mode: 'queue' });
    await ui.findButton('Activity (2 unread)').click();
    const oldMark = findKey(['notice', 'notice-b', 'markRead']); oldMark.focus();
    notes[0].createdAt = '2026-10-16T12:00:00Z'; ui.emit('state', ui.state);
    const mark = findKey(['notice', 'notice-b', 'markRead']); assert.notEqual(mark, oldMark); assert.equal(ui.document.activeElement, mark);
    await mark.click(); assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction').at(-1), { name: 'jobAction', params: { action: 'markRead', id: 'notice-b' } });
    notes[1].read = true; ui.emit('state', ui.state); assert.equal(ui.document.activeElement.className, 'jobs-pane');
    const history = findKey(['history', 'toggle']); history.focus();
    ui.state.jobs.jobs.push({ id: 'old', kind: 'metadata', state: 'completed', completed: 1, total: 1 }); ui.emit('state', ui.state);
    assert.equal(ui.document.activeElement, findKey(['history', 'toggle']));
  } finally { await ui.close(); }
});

test('library replacement closes old job, settings and confirmation contexts while keeping independent feedback text', async () => {
  const ui = harness();
  try {
    await ui.findButton('Jobs').click(); await ui.findButton('Settings').click();
    ui.select(['old-photo']); await ui.findButton('Retag').click();
    const oldRetag = ui.findButton('Retag selected'), oldSave = ui.findButton('Save settings');
    ui.emit('feedback'); await new Promise(resolve => setImmediate(resolve));
    const summary = ui.findField('Short summary'); summary.value = 'My unsent feedback'; await summary.fire('input');
    const feedback = summary.closest('dialog');
    ui.emit('library-restored', {}); await new Promise(resolve => setImmediate(resolve));
    assert.equal(feedback.open, true); assert.equal(ui.findField('Short summary').value, 'My unsent feedback');
    assert(ui.all().filter(node => node.tagName === 'DIALOG' && node.open).every(node => node === feedback));
    const before = ui.calls.length;
    await oldRetag.fire('click'); await oldSave.fire('click');
    assert.equal(ui.calls.length, before, 'Detached controls must not resubmit the old library’s selection or settings.');
    assert(!ui.calls.some(call => call.name === 'enqueueJob'));
  } finally { await ui.close(); }
});

test('uninstall checks only on explicit entry and explains an unavailable installation without launching anything', async () => {
  const ui = harness({ responses: { '/api/uninstall/plan': () => ({ available: false, reason: 'The installed package has changed. Open the companion from its verified package and try again.' }) } });
  try {
    await ui.findButton('Settings').click();
    assert(!ui.calls.some(call => call.name.startsWith('/api/uninstall/')));
    await ui.findButton('Review uninstall…').click();
    assert.match(ui.all().find(node => node.className === 'dialog-body' && /installed package has changed/.test(node.textContent)).textContent, /Open the companion from its verified package/);
    assert.equal(ui.findButton('Continue in macOS…').hidden, true);
    assert.equal(ui.findButton('Continue in macOS…').disabled, true);
    await ui.findButton('Continue in macOS…').fire('click');
    assert.deepEqual(ui.calls.filter(call => call.name.startsWith('/api/uninstall/')), [{ name: '/api/uninstall/plan', params: {} }]);
  } finally { await ui.close(); }
});

test('dismissing uninstall preserves the unsaved model draft and never opens a native removal confirmation', async () => {
  const ui = harness({ responses: { '/api/uninstall/plan': () => ({ available: true, pending: false, packageLabel: 'PhotoFindry', libraryLabel: 'My local library', keeps: ['Saved tags and people names', 'Backups and source archives'] }) } });
  try {
    await ui.findButton('Settings').click();
    const model = ui.all().find(node => node.className === 'model-file-row' && node.children[0]?.textContent === 'Vision model');
    await model.children[2].children.find(node => node.textContent === 'Choose…').click();
    await ui.findButton('Review uninstall…').click();
    const continueButton = ui.findButton('Continue in macOS…');
    const review = continueButton.closest('dialog');
    assert.match(review.textContent, /moved to Trash.*Running jobs will stop safely.*other installations are left alone/s);
    assert.match(review.textContent, /My local library.*Original photographs, model files, saved tags, previews and saved settings are preserved/s);
    assert.match(review.textContent, /Saved tags and people names.*Backups and source archives/s);
    await ui.findButton('Cancel').click(); await new Promise(resolve => setImmediate(resolve));
    await continueButton.fire('click');
    assert(!ui.calls.some(call => call.name === '/api/uninstall/open'));
    assert.equal(model.children[1].textContent, 'chosen-model.gguf');
    await ui.findButton('Save settings').click();
    assert.equal(ui.calls.find(call => call.name === 'configure').params.modelGrant, 'grant-model');
  } finally { await ui.close(); }
});

test('uninstall hands off an empty request once, waits for native status and permits retry only after explicit recheck', async () => {
  let pending = false; let failure = '';
  const ui = harness({ responses: {
    '/api/uninstall/plan': () => ({ available: true, pending, lastError: failure, packageLabel: 'PhotoFindry', libraryLabel: 'Local library', keeps: [] }),
    '/api/uninstall/open': () => { pending = true; return { opened: true }; },
  } });
  try {
    await ui.findButton('About').click(); await ui.findButton('Uninstall companion…').click();
    const proceed = ui.findButton('Continue in macOS…');
    await proceed.click(); await proceed.fire('click');
    assert.deepEqual(ui.calls.filter(call => call.name === '/api/uninstall/open'), [{ name: '/api/uninstall/open', params: {} }], 'The browser sends no removal target, path, force or confirmation authority.');
    assert.match(proceed.closest('dialog').textContent, /This page cannot confirm the outcome/);
    assert.equal(ui.findButton('Check uninstall status').hidden, false);
    await ui.findButton('Check uninstall status').click();
    assert.equal(proceed.disabled, true); assert.equal(proceed.hidden, true);
    pending = false; failure = 'Uninstall was cancelled. Your companion remains installed.';
    await ui.findButton('Check uninstall status').click();
    assert.equal(proceed.disabled, false); assert.equal(proceed.hidden, false);
    assert.match(proceed.closest('dialog').textContent, /Uninstall was cancelled/);
    await proceed.click();
    assert.equal(ui.calls.filter(call => call.name === '/api/uninstall/open').length, 2);
    ui.emit('session', { connected: false }); await new Promise(resolve => setImmediate(resolve));
    assert.match(ui.notices.at(-1), /closed connection alone does not confirm removal/);
    await proceed.fire('click'); assert.equal(ui.calls.filter(call => call.name === '/api/uninstall/open').length, 2);
  } finally { await ui.close(); }
});

test('a late uninstall plan cannot revive a dismissed review or submit removal', async () => {
  let finish;
  const ui = harness({ responses: { '/api/uninstall/plan': () => new Promise(resolve => { finish = resolve; }) } });
  try {
    await ui.findButton('About').click();
    const checking = ui.findButton('Uninstall companion…').click();
    await new Promise(resolve => setImmediate(resolve));
    const proceed = ui.findButton('Continue in macOS…');
    await ui.findButton('Cancel').click(); await new Promise(resolve => setImmediate(resolve));
    finish({ available: true, pending: false, packageLabel: 'PhotoFindry', keeps: [] }); await checking;
    assert.equal(proceed.closest('dialog').open, false); assert.equal(proceed.disabled, true);
    await proceed.fire('click'); assert(!ui.calls.some(call => call.name === '/api/uninstall/open'));
  } finally { finish?.({ available: false }); await ui.close(); }
});

test('closing and reopening the uninstall review retains an in-flight native handoff without launching twice', async () => {
  let finish;
  const ui = harness({ responses: {
    '/api/uninstall/plan': () => ({ available: true, pending: false, packageLabel: 'PhotoFindry', keeps: [] }),
    '/api/uninstall/open': () => new Promise(resolve => { finish = resolve; }),
  } });
  try {
    await ui.findButton('About').click(); await ui.findButton('Uninstall companion…').click();
    const first = ui.findButton('Continue in macOS…'); const opening = first.click();
    await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Close review').click(); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Uninstall companion…').click();
    const reopened = ui.findButton('Continue in macOS…');
    assert.notEqual(first, reopened); assert.equal(reopened.disabled, true);
    await reopened.fire('click'); assert.equal(ui.calls.filter(call => call.name === '/api/uninstall/open').length, 1);
    finish({ opened: true }); await opening;
    assert.equal(ui.findButton('Check uninstall status').hidden, false);
    assert.match(reopened.closest('dialog').textContent, /This page cannot confirm the outcome/);
  } finally { finish?.({ opened: true }); await ui.close(); }
});

test('Settings directly configures the decision reviewer through the shared provider, file and memory controls', async () => {
  const pickerKinds = [];
  const ui = harness({ withPanels: true, state: { photoReviewSettings: { provider: 'tagging-model' }, monitor: { resources: { physicalMemoryBytes: 32 * 1024 ** 3 } } }, pick: async requested => {
    pickerKinds.push(requested);
    const kind = requested === 'reviewModel' ? 'model' : requested === 'reviewProjector' ? 'projector' : requested;
    return { kind, grantId: `grant-${kind}`, label: `chosen-${kind}.gguf` };
  } });
  try {
    await ui.findButton('Settings').click(); await new Promise(resolve => setImmediate(resolve));
    const provider = ui.findField('Reviewer');
    assert.deepEqual(provider.children.map(node => [node.value, node.textContent]), [
      ['tagging-model', 'Current tagging model'], ['clef-flash', 'CLEF Flash'], ['intern-decision-4b', 'Intern-Decision 4B'],
    ]);
    const localModels = ui.all().find(node => node.tagName === 'SECTION' && node.children[0]?.textContent === 'Local models');
    const reviewer = localModels.children.find(node => node.children[0]?.textContent === 'Photo detail reviewer (decision model)');
    assert(reviewer?.contains(provider), 'The full editor belongs to Settings, not only a destination in Improvement.');
    const reranker = localModels.children.find(node => node.className === 'model-file-row' && node.children[0]?.textContent === 'Search reranker');
    const advanced = localModels.children.find(node => node.tagName === 'DETAILS');
    assert(localModels.children.indexOf(reranker) < localModels.children.indexOf(reviewer));
    assert(localModels.children.indexOf(reviewer) < localModels.children.indexOf(advanced));
    provider.value = 'intern-decision-4b'; await provider.fire('change'); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Choose review model…').click(); await ui.findButton('Choose matching vision adapter…').click();
    assert.deepEqual(pickerKinds, ['reviewModel', 'reviewProjector'], 'Reviewer choices must open reviewer-specific native guidance, not tagger guidance.');
    const reduced = ui.findField('Reduced-memory mode'); reduced.checked = true; await reduced.fire('change'); await new Promise(resolve => setImmediate(resolve));
    assert.match(reviewer.textContent, /Intern-Decision-4B-Q4_K_M\.gguf.*mmproj-Intern-Decision-4B-F16\.gguf/s);
    assert.match(reviewer.textContent, /512-pixel previews and four questions/);
    ui.state.photoReviewSettings = { provider: 'clef-flash', modelLabel: 'different-snapshot.gguf' };
    ui.state.monitor = { generatedTokens: 50 }; ui.emit('state', ui.state);
    assert.equal(ui.findField('Reviewer'), provider); assert.equal(provider.value, 'intern-decision-4b'); assert.equal(reduced.checked, true);
    assert.match(reviewer.textContent, /chosen-model\.gguf.*chosen-projector\.gguf/s);
    await ui.findButton('Save reviewer settings').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'configurePhotoReview'), [{ name: 'configurePhotoReview', params: { provider: 'intern-decision-4b', lowMemoryTrial: true, modelGrant: 'grant-model', projectorGrant: 'grant-projector' } }]);
    assert(!ui.calls.some(call => ['configure', 'startModel', 'enqueueJob'].includes(call.name)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('Settings reviewer rejects mismatched files and a late picker cannot change or save a dismissed editor', async () => {
  let finishPicker;
  const ui = harness({ withPanels: true,
    state: { photoReviewSettings: { provider: 'intern-decision-4b', modelConfigured: true, projectorConfigured: true } },
    responses: { '/api/review-files/check': () => ({ canSave: false, files: [{ role: 'model', status: 'error', message: 'Choose the pinned review model, not its vision adapter.' }], summary: 'The files do not match this reviewer.' }) },
    pick: () => new Promise(resolve => { finishPicker = resolve; }),
  });
  try {
    await ui.findButton('Settings').click(); await new Promise(resolve => setImmediate(resolve));
    const save = ui.findButton('Save reviewer settings'); await save.click();
    assert(!ui.calls.some(call => call.name === 'configurePhotoReview'));
    assert.match(ui.errors.at(-1), /Fix the highlighted reviewer choices/);
    const picking = ui.findButton('Choose review model…').click(); await new Promise(resolve => setImmediate(resolve));
    save.closest('dialog').close(); await new Promise(resolve => setImmediate(resolve));
    const before = ui.calls.length;
    finishPicker({ grantId: 'stale-review-model', label: 'stale.gguf' }); await picking; await save.fire('click');
    assert.equal(ui.calls.length, before, 'A closed Settings reviewer cannot check a late picker grant or save old choices.');
    assert(!ui.calls.some(call => call.name === 'configurePhotoReview'));
  } finally { finishPicker?.({ cancelled: true }); await ui.close(); }
});

test('background companion settings expose only fixed authenticated lifecycle actions', async () => {
  const ui = harness({ responses: {
    '/api/lifecycle/status': () => ({ installed: true, version: '0.2.0-beta.15', build: 113, previousBuild: 112, autostart: true, automaticUpdates: true, available: { build: 114 }, status: { phase: 'ready', message: 'Ready' } }),
    '/api/lifecycle/action': () => ({ started: true }),
  } });
  try {
    await ui.findButton('Settings').click(); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Install signed update').click();
    await ui.findButton('Turn off login startup').click();
    const actions = ui.calls.filter(call => call.name === '/api/lifecycle/action');
    assert.deepEqual(JSON.parse(JSON.stringify(actions.map(call => call.params))), [{ action: 'update' }, { action: 'disable-autostart' }]);
    assert(!ui.calls.some(call => ['configure', 'startModel', 'enqueueJob'].includes(call.name)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});


test('macOS read consent recovery gives an actionable Files path and explicit retry without starting work automatically', async () => {
  const job = { id: 'consent', kind: 'tagging', state: 'paused', pauseReason: 'source-waiting', sourceLifecycleVersion: 1, sourceNextAction: { kind: 'allowAccess', sourceId: 'source-a' }, total: 2, completed: 0, failed: 0 };
  const ui = harness({ state: { jobs: { jobs: [job], notifications: [] } } });
  try {
    const card = ui.window.PhotoFindryJobs.card(job);
    assert.match(card.textContent, /choose its originals folder again/);
    const buttons = card.children.at(-1).children;
    assert(buttons.some(node => node.textContent === 'Open Files'));
    assert.equal(ui.calls.length, 0);
    await buttons.find(node => node.textContent === 'Retry access').click();
    assert.deepEqual(ui.calls.filter(call => call.name === 'jobAction'), [{ name: 'jobAction', params: { action: 'resume', id: 'consent' } }]);
  } finally { await ui.close(); }
});
