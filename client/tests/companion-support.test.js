'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
function harness() {
  let clock = 0, nextTimer = 1, focused = true, invitations = 0;
  const timers = new Map(), windowEvents = new Map(), documentEvents = new Map(), calls = [], nodes = [];
  const listener = (map, name, callback) => { if (!map.has(name)) map.set(name, []); map.get(name).push(callback); };
  class Node {
    constructor(tag, text = '') { this.tagName = tag.toUpperCase(); this.children = []; this.textContent = text; this.hidden = true; this.open = false; this.events = new Map(); }
    append(...items) { this.children.push(...items); }
    setAttribute() {} remove() { this.removed = true; }
    addEventListener(name, callback) { listener(this.events, name, callback); }
    showModal() { this.open = true; invitations++; }
    close() { this.open = false; for (const callback of this.events.get('close') || []) callback(); }
  }
  const app = new Node('div'); app.hidden = false; const guide = new Node('div'); const body = new Node('body');
  const document = { body, visibilityState: 'visible', activeElement: null, hasFocus: () => focused,
    addEventListener: (name, callback) => listener(documentEvents, name, callback),
    createElement: tag => { const node = new Node(tag); nodes.push(node); return node; },
    getElementById: id => id === 'app' ? app : id === 'onboarding' ? guide : null,
    querySelector: () => nodes.find(node => node.tagName === 'DIALOG' && node.open) || null,
  };
  const window = { addEventListener: (name, callback) => listener(windowEvents, name, callback), PhotoFindry: {
    api: async (route, payload) => { calls.push({ route, payload: JSON.parse(JSON.stringify(payload)) }); return { handled: payload.event === 'manual' || payload.quiet, showInvitation: payload.event === 'sample' && payload.quiet }; },
  } };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../web-companion/support.js'), 'utf8'), { window, document, crypto: { randomUUID: () => '00000000-0000-0000-0000-000000000001' }, performance: { now: () => clock }, setTimeout: (callback, ms) => { const id = nextTimer++; timers.set(id, { callback, due: clock + ms }); return id; }, clearTimeout: id => timers.delete(id) });
  const emit = (map, name, value = {}) => { for (const callback of map.get(name) || []) callback(value); };
  return { window, document, calls, nodes, guide, get invitations() { return invitations; },
    connect() { emit(windowEvents, 'photofindry:session', { detail: { connected: true } }); emit(windowEvents, 'photofindry:state', { detail: { library: { total: 1 } } }); },
    disconnect() { emit(windowEvents, 'photofindry:session', { detail: { connected: false } }); },
    state(value) { emit(windowEvents, 'photofindry:state', { detail: { library: { total: 1 }, ...value } }); },
    input(trusted = true) { emit(documentEvents, 'keydown', { isTrusted: trusted, key: 'PRIVATE KEY CONTENT' }); },
    blur() { focused = false; emit(windowEvents, 'blur'); },
    focus() { focused = true; emit(windowEvents, 'focus'); },
    async advance(ms) { clock += ms; const due = [...timers].filter(([, timer]) => timer.due <= clock); for (const [id, timer] of due) { timers.delete(id); await timer.callback(); } await Promise.resolve(); },
  };
}
test('support invitation needs trusted foreground input and a quiet library; no key contents are collected', async () => {
  const ui = harness(); ui.connect(); ui.input(false); await ui.advance(15000); assert.equal(ui.invitations, 0); assert(!ui.calls.some(call => call.payload.event === 'input'));
  ui.input(); await ui.advance(15000); assert.equal(ui.invitations, 0); await ui.advance(15000); assert.equal(ui.invitations, 1);
  assert(!JSON.stringify(ui.calls).includes('PRIVATE')); assert(ui.calls.every(call => Object.keys(call.payload).sort().join(',') === 'clientId,event,foreground,quiet'));
});
test('support sampling does not invite during work, editing, backgrounding or a missed sleep interval', async () => {
  const ui = harness(); ui.connect(); ui.input(); ui.state({ job: { running: true } }); await ui.advance(15000); await ui.advance(15000); assert.equal(ui.invitations, 0);
  ui.state({}); ui.document.activeElement = { tagName: 'INPUT' }; await ui.advance(15000); assert.equal(ui.invitations, 0);
  ui.document.activeElement = null; ui.blur(); await ui.advance(15000); ui.focus(); await ui.advance(15000); assert.equal(ui.invitations, 0);
  ui.input(); await ui.advance(60000); await ui.advance(15000); assert.equal(ui.invitations, 0);
  ui.input(); await ui.advance(15000); await ui.advance(15000); assert.equal(ui.invitations, 1);
});
test('manual support saves permanent suppression before presentation and opens no external request itself', async () => {
  const ui = harness(); ui.connect(); await ui.window.PhotoFindrySupport.open(); assert.equal(ui.calls[0].payload.event, 'manual'); assert.equal(ui.invitations, 1);
  const link = ui.nodes.find(node => node.tagName === 'A'); assert.equal(link.href, 'https://buymeacoffee.com/photorecall'); assert.equal(link.rel, 'noopener noreferrer');
  await ui.advance(60000); assert.equal(ui.calls.length, 1);
});

test('a late manual support response cannot reopen a disconnected library', async () => {
  const ui = harness(); ui.connect(); let finish;
  ui.window.PhotoFindry.api = () => new Promise(resolve => { finish = resolve; });
  const open = ui.window.PhotoFindrySupport.open().then(() => 'shown', error => error.name); ui.disconnect();
  finish({ handled: true, showInvitation: false }); assert.equal(await open, 'AbortError'); assert.equal(ui.invitations, 0);
});
