'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../web-companion/model-monitor.js'), 'utf8');
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const metric = (changes = {}) => ({
  modelActivityVersion: 1, modelActivityRevision: 1,
  phase: 'generating', requestId: 'request-one', requestModelId: 'model-one', requestModelKind: 'tagger',
  requestModelName: 'local.gguf', label: 'Tagging photograph', tokensMeasured: true,
  generatedTokens: 2, promptTokens: 8, appInputTokens: 8, appOutputTokens: 2,
  appTotalTokens: 10, appRequests: 1, appUnmeasuredRequests: 0,
  tokensPerSecond: 12.5, promptTokensPerSecond: 30, timeToFirstTokenMs: 200,
  elapsedMs: 1000, contextUsed: 10, contextCapacity: 8192,
  activeModel: { id: 'model-one', kind: 'tagger', name: 'local.gguf', status: 'ready', contextCapacity: 8192 },
  loadedModels: [{ id: 'model-one', kind: 'tagger', name: 'local.gguf', status: 'ready', contextCapacity: 8192 }],
  resources: { sampledAtMs: 1, complete: true, currentBytes: 1024 ** 3, peakBytes: 2 * 1024 ** 3,
    budgetBytes: 8 * 1024 ** 3, pressureLevel: 1, swapUsedBytes: 0, paused: false,
    processes: [{ role: 'model', active: true, footprintBytes: 1024 ** 3, rssBytes: 512 * 1024 ** 2, peakBytes: 2 * 1024 ** 3 }] },
  ...changes,
});

function harness() {
  let document;
  class Node {
    constructor(tag = 'div', value = '') {
      this.tagName = tag.toUpperCase(); this.children = []; this.parentNode = null;
      this._text = value; this.className = ''; this.attributes = {}; this.listeners = {};
      this.hidden = false; this.open = false; this.scrollTop = 0;
    }
    append(...nodes) { for (let node of nodes) { if (typeof node === 'string') node = new Node('#text', node); node.remove(); node.parentNode = this; this.children.push(node); } }
    after(node) { assert(this.parentNode); node.remove(); const at = this.parentNode.children.indexOf(this); this.parentNode.children.splice(at + 1, 0, node); node.parentNode = this.parentNode; }
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(child => child !== this); this.parentNode = null; }
    setAttribute(key, value) { this.attributes[key] = String(value); }
    addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
    async click() { for (const handler of this.listeners.click || []) await handler(); }
    focus() { document.activeElement = this; }
    showModal() { this.open = true; }
    close() { this.open = false; }
    querySelector(selector) { return descendants(this).find(node => node.tagName === selector.toUpperCase()) || null; }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this._text = String(value); this.children = []; }
  }
  const descendants = node => node.children.flatMap(child => [child, ...descendants(child)]);
  const body = new Node('body'), sidebar = new Node('div'); body.append(sidebar);
  const button = new Node('button'), status = new Node('span'); button.append(new Node('strong', 'Model activity'), status); sidebar.append(button);
  const gallery = new Node('div'); body.append(gallery);
  document = { body, activeElement: null, createElement: tag => new Node(tag), getElementById: id => ({ 'open-model-monitor': button, 'sidebar-model-status': status, gallery })[id] };
  const events = new Map(), timers = new Map(), delays = [], calls = [];
  let nextTimer = 0, connected = false, fullState = { workerGeneration: 0, monitor: metric(), runtime: { status: 'ready' } };
  let apiHandler = () => ({ workerGeneration: 0, monitor: metric(), runtime: { status: 'ready' } });
  const window = {
    addEventListener(name, handler) { (events.get(name) || events.set(name, []).get(name)).push(handler); },
  };
  const bridge = {
    isConnected: () => connected, getState: () => fullState,
    action: async (action, params = {}) => { calls.push({ action, params }); return {}; },
    api: (route, body, options) => { calls.push({ route, body, options }); return apiHandler(route, body, options); },
    refreshState: () => { throw Error('Monitor must not request a full library snapshot.'); },
  };
  window.PhotoFindry = bridge;
  const timeout = (fn, ms) => { const id = ++nextTimer; timers.set(id, { fn, ms }); delays.push(ms); return id; };
  vm.runInNewContext(source, { window, document, Intl, Number, Object, JSON, Array, String, Set, Map, Promise, AbortController, setTimeout: timeout, clearTimeout: id => timers.delete(id) }, { filename: 'model-monitor.js' });
  const emit = (name, detail) => { for (const handler of events.get(name) || []) handler({ detail }); };
  return { window, document, body, button, status, gallery, calls, delays, timers, all: () => descendants(body),
    emit, setAPI(fn) { apiHandler = fn; }, setFullState(value) { fullState = value; },
    connect() { connected = true; emit('photofindry:session', { connected: true }); },
    disconnect() { connected = false; emit('photofindry:session', { connected: false }); },
    find(label) { return descendants(body).find(node => node.textContent === label); },
    byLabel(label) { return descendants(body).find(node => node.attributes['aria-label'] === label); },
  };
}

test('metric poll renders current and last requests without inventing token usage or republishing gallery state', async () => {
  const ui = harness();
  ui.connect();
  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ rawOutput: 'SECRET MODEL RESPONSE', prompt: 'SECRET PROMPT' }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  await ui.button.click();
  const dialog = ui.all().find(node => node.tagName === 'DIALOG');
  assert(dialog.open); assert.match(dialog.textContent, /Current request/);
  assert.match(dialog.textContent, /10/); assert.match(dialog.textContent, /First token/);
  assert.match(dialog.textContent, /12\.5 tok\/s/);
  assert.doesNotMatch(ui.body.textContent, /SECRET/);
  assert.equal(ui.gallery.children.length, 0);
  assert(ui.calls.every(call => call.route === '/api/monitor'));
  assert(ui.calls.every(call => call.options?.signal instanceof AbortSignal));
  assert(ui.delays.includes(250), 'Active requests use the metric-only four-per-second cadence.');

  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ phase: 'error', tokensMeasured: false, generatedTokens: 0, appUnmeasuredRequests: 1, requestId: 'request-one', modelActivityRevision: 2 }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(dialog.textContent, /Last request · failed/);
  assert.match(dialog.textContent, /Not fully reported/);
  assert.match(dialog.textContent, /requests did not report complete usage/);
  assert.match(ui.status.textContent, /Model ready/);
  assert(ui.delays.includes(1200), 'Idle monitoring slows down.');
  ui.disconnect();
});

test('the permanent main-page monitor shows live status, loaded model, tokens and memory before opening details', async () => {
  const ui = harness(); ui.connect();
  await ui.window.PhotoFindryModelMonitor.refresh();
  const dialog = ui.all().find(node => node.tagName === 'DIALOG');
  assert.equal(dialog.open, false);
  assert.match(ui.button.textContent, /Model activity.*Generating.*local\.gguf.*10 tokens · this session.*RAM 1\.00 GiB/);
  assert.equal(ui.all().filter(node => node.tagName === 'BUTTON' && node.textContent.includes('tokens · this session')).length, 1);
  const card = ui.button;
  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ modelActivityRevision: 2, appTotalTokens: 22 }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(card, ui.button); assert.match(card.textContent, /22 tokens · this session/);
  await card.click(); assert(dialog.open); assert.equal(card.attributes['aria-expanded'], 'true');
  ui.disconnect();
});

test('one outstanding poll and session reset fence a delayed metric response', async () => {
  const ui = harness(), late = deferred();
  ui.setAPI(() => late.promise);
  ui.connect();
  const first = ui.window.PhotoFindryModelMonitor.refresh();
  const second = ui.window.PhotoFindryModelMonitor.refresh();
  await Promise.resolve();
  assert.equal(first, second); assert.equal(ui.calls.length, 1);
  const signal = ui.calls[0].options.signal;
  ui.disconnect();
  assert(signal.aborted);
  late.resolve({ workerGeneration: 0, monitor: metric({ appTotalTokens: 999, activeModel: { id: 'secret', name: 'previous-session.gguf', status: 'ready' } }), runtime: { status: 'ready' } });
  await first;
  assert.match(ui.status.textContent, /Companion unavailable/);
  assert.doesNotMatch(ui.body.textContent, /999|previous-session/);
  assert.equal(ui.timers.size, 0);
});

test('incomplete process samples distinguish unavailable and stopped measurements', async () => {
  const ui = harness(); ui.connect();
  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ resources: {
    sampledAtMs: 2, complete: false, currentBytes: 0, peakBytes: 0, budgetBytes: null,
    pressureLevel: null, swapUsedBytes: null, paused: false,
    processes: [{ role: 'model', active: true, footprintBytes: null, rssBytes: null, peakBytes: 0 },
      { role: 'face', active: false, footprintBytes: 0, rssBytes: 0, peakBytes: 0 }],
  } }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  await ui.button.click();
  assert.match(ui.body.textContent, /RAM Not measured · partial/);
  assert.match(ui.body.textContent, /Some process measurements are unavailable/);
  assert.match(ui.body.textContent, /Vision modelUnavailable/);
  assert.match(ui.body.textContent, /Face matchingStopped/);
  assert.doesNotMatch(ui.body.textContent, /PhotoFindry footprint0 B/);
  ui.disconnect();
});

test('worker generation resets lower revisions, memory time, trends and engine-session totals', async () => {
  const ui = harness(); ui.connect();
  let response = { workerGeneration: 0, monitor: metric({ modelActivityRevision: 90, appTotalTokens: 900,
    resources: { ...metric().resources, sampledAtMs: 9000 } }), runtime: { status: 'ready' } };
  ui.setAPI(() => response);
  await ui.window.PhotoFindryModelMonitor.refresh();
  response = { workerGeneration: 0, monitor: metric({ modelActivityRevision: 91, generatedTokens: 3,
    tokensPerSecond: 13, appTotalTokens: 901, resources: { ...metric().resources, sampledAtMs: 9001 } }), runtime: { status: 'ready' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  const memoryTrend = ui.byLabel('Recent PhotoFindry memory usage');
  const speedTrend = ui.byLabel('Recent token generation speed');
  assert.equal(memoryTrend.textContent.length, 3);
  assert.equal(speedTrend.textContent.length, 2);

  response = { workerGeneration: 1, monitor: metric({ modelActivityRevision: 1, requestId: 'new-request',
    appTotalTokens: 3, appInputTokens: 2, appOutputTokens: 1,
    resources: { ...metric().resources, sampledAtMs: 100 } }), runtime: { status: 'ready' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /3 tokens · this session/);
  assert.doesNotMatch(ui.body.textContent, /900 tokens · this session/);
  assert.match(memoryTrend.textContent, /Not enough samples yet/);
  assert.match(speedTrend.textContent, /Not enough samples yet/);

  response = { workerGeneration: 0, monitor: metric({ modelActivityRevision: 999, appTotalTokens: 9999 }), runtime: { status: 'ready' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /3 tokens · this session/);
  assert.doesNotMatch(ui.body.textContent, /9999 tokens · this session/);

  response = { workerGeneration: 1, monitor: metric({ modelActivityRevision: 2, requestId: 'new-request',
    generatedTokens: 4, tokensPerSecond: 14, appTotalTokens: 4,
    resources: { ...metric().resources, sampledAtMs: 101 } }), runtime: { status: 'ready' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /4 tokens · this session/);
  assert.equal(memoryTrend.textContent.length, 2);
  assert.equal(speedTrend.textContent.length, 2);
  ui.disconnect();
});

test('a replacement state fences an old delayed poll and untagged fallback cannot mix sessions', async () => {
  const ui = harness(); ui.connect();
  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ modelActivityRevision: 80, appTotalTokens: 800 }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();

  const old = deferred(); ui.setAPI(() => old.promise);
  const inFlight = ui.window.PhotoFindryModelMonitor.refresh();
  await Promise.resolve();
  const signal = ui.calls.at(-1).options.signal;
  const replacement = { workerGeneration: 1, monitor: metric({ modelActivityRevision: 1, requestId: 'new-worker', appTotalTokens: 2,
    resources: { ...metric().resources, sampledAtMs: 100 } }), runtime: { status: 'ready' } };
  ui.setFullState(replacement); ui.emit('photofindry:state', replacement);
  assert(signal.aborted);
  assert.match(ui.body.textContent, /2 tokens · this session/);

  old.resolve({ workerGeneration: 0, monitor: metric({ modelActivityRevision: 900, appTotalTokens: 9000 }), runtime: { status: 'ready' } });
  await inFlight;
  assert.match(ui.body.textContent, /2 tokens · this session/);
  assert.doesNotMatch(ui.body.textContent, /9000 tokens · this session/);

  ui.setAPI(() => ({ workerGeneration: 1, monitor: metric({ modelActivityRevision: 2, requestId: 'new-worker', appTotalTokens: 3 }), runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /3 tokens · this session/);
  ui.setFullState({ monitor: metric({ modelActivityRevision: 1000, appTotalTokens: 10000 }), runtime: { status: 'ready' } });
  ui.setAPI(() => { throw Error('Metric endpoint temporarily unavailable.'); });
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /3 tokens · this session/);
  assert.doesNotMatch(ui.body.textContent, /10000 tokens · this session/);
  ui.disconnect();
});

test('fallback snapshots, stable dialog nodes, and bounded trends preserve focus and scroll', async () => {
  const ui = harness();
  ui.setFullState({ workerGeneration: 0, monitor: metric({ appTotalTokens: 17 }), runtime: { status: 'ready' } });
  ui.setAPI(() => { throw Error('Monitor endpoint temporarily unavailable.'); });
  ui.connect();
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /17 tokens · this session/);
  await ui.button.click();
  const dialog = ui.all().find(node => node.tagName === 'DIALOG');
  const scroll = ui.all().find(node => node.className === 'pfm-scroll');
  const close = ui.byLabel('Close model activity');
  close.focus(); scroll.scrollTop = 87;

  let count = 0;
  ui.setAPI(() => ({ workerGeneration: 0, monitor: metric({ modelActivityRevision: ++count, generatedTokens: count + 2, tokensPerSecond: 10 + count,
    appTotalTokens: 17, resources: { ...metric().resources, sampledAtMs: count, currentBytes: (count + 1) * 1024 ** 2 } }), runtime: { status: 'ready' } }));
  for (let i = 0; i < 75; i++) await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(ui.document.activeElement, close);
  assert.equal(scroll.scrollTop, 87);
  assert.equal(ui.all().find(node => node.tagName === 'DIALOG'), dialog);
  assert.match(ui.body.textContent, /17 tokens · this session/);
  const memoryTrend = ui.byLabel('Recent PhotoFindry memory usage');
  const speedTrend = ui.byLabel('Recent token generation speed');
  assert.equal(memoryTrend.textContent.length, 60);
  assert.equal(speedTrend.textContent.length, 50);

  const repeated = metric({ modelActivityRevision: count, generatedTokens: count + 2, tokensPerSecond: 10 + count,
    appTotalTokens: 17, resources: { ...metric().resources, sampledAtMs: count, currentBytes: (count + 1) * 1024 ** 2 } });
  ui.setAPI(() => ({ workerGeneration: 0, monitor: repeated, runtime: { status: 'ready' } }));
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(memoryTrend.textContent.length, 60);
  assert.equal(speedTrend.textContent.length, 50);
  ui.disconnect();
  assert(!dialog.open); assert.doesNotMatch(ui.body.textContent, /local\.gguf|17 tokens · this session/);
});

test('main-page runtime controls respect model and queue state and reveal only owned memory logs', async () => {
  const ui = harness();
  const stopped = { workerGeneration: 0, settings: { modelConfigured: true, projectorConfigured: true }, jobs: { jobs: [] }, monitor: metric({ phase: 'complete', activeModel: null, loadedModels: [] }), runtime: { status: 'stopped' } };
  ui.setFullState(stopped); ui.setAPI(() => stopped); ui.connect(); await ui.window.PhotoFindryModelMonitor.refresh();
  const start = ui.byLabel('Start tagging model'), unload = ui.byLabel('Unload active model');
  assert.equal(start.disabled, false); assert.equal(unload.disabled, true);
  await start.click(); assert.deepEqual(ui.calls.find(call => call.action === 'startModel'), { action: 'startModel', params: {} });
  const active = { ...stopped, jobs: { jobs: [{ kind: 'improve', state: 'running' }] }, monitor: metric({ requestModelKind: 'decision', requestModelName: 'last-reviewer.gguf', activeModel: { id: 'model-one', kind: 'decision', status: 'ready', name: 'active-reviewer.gguf' } }) };
  ui.setFullState(active); ui.emit('photofindry:state', active); ui.setAPI(() => active); await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(start.disabled, true); assert.equal(unload.disabled, false);
  await unload.click(); assert(ui.calls.some(call => call.action === 'stopModel'));
  await ui.find('Show memory logs').click(); assert.deepEqual(ui.calls.find(call => call.action === 'revealMemoryLogs'), { action: 'revealMemoryLogs', params: {} });
  assert.match(ui.body.textContent, /Review output tokens/);
  assert.match(ui.body.textContent, /last-reviewer\.gguf/);
  ui.disconnect(); assert(start.disabled); assert(unload.disabled);
});

test('a source-held tagging job exposes its scoped recovery even when the model is ready', async () => {
  const ui = harness(), opened = [];
  ui.window.PhotoFindryPanels = { show: (...args) => opened.push(args) };
  const job = { id: 'held-tagging', kind: 'tagging', state: 'paused', pauseReason: 'source-waiting',
    sourceNextAction: { kind: 'reviewIdentity', sourceId: 'source-one', rootId: 'root-one' } };
  const state = { workerGeneration: 0, settings: { modelConfigured: true, projectorConfigured: true },
    jobs: { jobs: [job] }, monitor: metric({ phase: 'idle' }), runtime: { status: 'ready' } };
  ui.setFullState(state); ui.setAPI(() => state); ui.connect(); await ui.window.PhotoFindryModelMonitor.refresh();
  assert.match(ui.body.textContent, /Tagging is waiting for originals/);
  const recovery = ui.find('Review originals'); assert.equal(recovery.hidden, false);
  await recovery.click();
  assert.equal(JSON.stringify(opened), JSON.stringify([['sources', { jobId: 'held-tagging', sourceId: 'source-one', rootId: 'root-one' }]]));
  assert(!ui.calls.some(call => call.action), 'Opening recovery must not confirm originals or resume the job.');
  const resumed = { ...state, jobs: { jobs: [{ ...job, state: 'running', pauseReason: null }] } };
  ui.emit('photofindry:state', resumed);
  assert.equal(recovery.hidden, true); assert.doesNotMatch(ui.body.textContent, /Tagging is waiting for originals/);
  await recovery.click(); assert.equal(opened.length, 1, 'A stale control cannot reopen an old job.');
  ui.disconnect();
});

test('loading and failed startup remain visible without an active model, and retry can reach ready', async () => {
  const ui = harness();
  let state = { workerGeneration: 0, settings: { modelConfigured: true, projectorConfigured: true }, jobs: { jobs: [] },
    monitor: metric({ phase: 'idle', activeModel: null, loadedModels: [] }), runtime: { status: 'starting' } };
  ui.setFullState(state); ui.setAPI(() => state); ui.connect(); await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(ui.status.textContent, 'Loading model…');
  state = { ...state, runtime: { status: 'error', message: 'The selected model file could not be opened.' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(ui.status.textContent, 'Model needs attention');
  assert.match(ui.body.textContent, /selected model file could not be opened.*Retry Start tagger/);
  await ui.byLabel('Start tagging model').click(); assert(ui.calls.some(call => call.action === 'startModel'));
  state = { ...state, monitor: metric({ phase: 'idle', modelActivityRevision: 2 }), runtime: { status: 'ready' } };
  await ui.window.PhotoFindryModelMonitor.refresh();
  assert.equal(ui.status.textContent, 'Model ready');
  assert.doesNotMatch(ui.body.textContent, /selected model file could not be opened/);
  ui.disconnect();
});
