'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../../web-companion/panels.js'),'utf8');
function harness(initial = {}, overrides = {}) {
  let document;
  class Node {
    constructor(tag, value = '') {
      this.tagName = tag.toUpperCase(); this.children = []; this.dataset = {}; this.attributes = {}; this.listeners = {};
      this._text = value; this.value = ''; this.checked = false; this.disabled = false; this.hidden = false;
      this.className = ''; this.open = false; this.isConnected = true; this.inert = false;
      this.classList = { add: (...names) => { this.className += ` ${names.join(' ')}`; }, toggle() {} };
    }
    append(...children) { for (const child of children) { this.children.push(child); child.parentNode = this; } }
    prepend(...children) { for (const child of children) child.parentNode = this; this.children.unshift(...children); }
    replaceChildren(...children) { this.children = []; this.append(...children); }
    replaceWith(next) { const owner = this.parentNode; if (owner) { const at = owner.children.indexOf(this); owner.children.splice(at, 1, next); next.parentNode = owner; } }
    setAttribute(name, value) { this.attributes[name] = value; }
    addEventListener(name, listener) { (this.listeners[name] ||= []).push(listener); }
    async fire(name, extra = {}) { for (const listener of this.listeners[name] || []) await listener({ target: this, preventDefault() {}, ...extra }); }
    click(extra = {}) { return this.disabled ? Promise.resolve() : this.fire('click', extra); }
    focus() { document.activeElement = this; }
    scrollIntoView() {}
    contains(node) { return node === this || this.children.some((child) => child.contains(node)); }
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((child) => child !== this); this.isConnected = false; }
    showModal() { this.open = true; }
    close() { if (!this.open) return; this.open = false; queueMicrotask(() => this.fire('close')); }
    querySelectorAll(selector) { return descendants(this).filter((node) => selector === '[data-panel]' ? node.dataset.panel : selector.startsWith('.') && node.className.split(' ').includes(selector.slice(1))); }
    querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
    get lastChild() { return this.children.at(-1); }
    get textContent() { return this._text + this.children.map((child) => child.textContent).join(''); }
    set textContent(value) { this._text = String(value); this.children = []; }
  }
  const descendants = (node) => node.children.flatMap((child) => [child, ...descendants(child)]);
  const app = new Node('div'); app.id = 'app';
  document = { body: new Node('body'), activeElement: null, createElement: (tag) => new Node(tag), createTextNode: (value) => new Node('#text', value), getElementById: (id) => id === 'app' ? app : null };
  document.body.append(app);
  const state = {
    sources: [{ id: '12345678-1234-4234-8234-123456789abc', label: 'Test source', photoCount: 1, status: 'available', roots: [] }],
    people: { people: [], faces: [], canUndo: false }, jobs: { jobs: [] }, ...initial,
  };
  const calls = [], errors = [], notices = [], events = {};
  const responses = {
    '/api/review-files/check': { canSave: true, files: [], summary: 'Review files checked.', physicalMemoryBytes: 32 * 1024 ** 3 },
    sourceDetails: { candidates: [], canUndo: false, totalCount: 0 },
    browsePeople: { peoplePage: { photoIds: [], total: 0, offset: 0, limit: 240 }, peoplePhotos: [], people: { faces: [] } },
    beginCleanupSession: { active: true, ready: true }, cleanupSession: { active: true, ready: true }, endCleanupSession: { active: false, ready: false },
    cleanupReview: { items: [], totalCount: 0, offset: 0 }, cleanupRemoved: { items: [], totalCount: 0, offset: 0 }, ...overrides,
  };
  const invoke = async (action, params) => {
    calls.push({ action, params: structuredClone(params) });
    if (typeof responses[action] === 'function') return responses[action](params);
    return structuredClone(responses[action] ?? {});
  };
  const window = {
    PhotoFindry: {
      getState: () => state, getScope: overrides.getScope || (() => ({})), selectedIDs: overrides.selectedIDs || (() => []), action: invoke,
      api: async (route, body) => route === '/api/action' ? invoke(body.action, body.params) : invoke(route, body),
      showPhoto: async photoId => invoke('showPhoto', { photoId }),
      showError: (error) => errors.push(error), showNotice: (notice) => notices.push(notice),
      previewBlob: async () => { throw Error('No retained image in this fixture.'); }, refreshLibrary: async () => {}, setScope: async () => {},
    },
    addEventListener: (name, listener) => { (events[name] ||= []).push(listener); },
  };
  const context = { window, document, AbortController, Blob, URL, Intl, Date, Map, Set, Promise, Number, String, Object, JSON, Array, Error, Uint8Array, atob, setTimeout, clearTimeout };
  vm.runInNewContext(source, context, { filename: 'panels.js' });
  const all = () => descendants(document.body);
  const findButton = (label) => { const node = all().find((node) => node.tagName === 'BUTTON' && node.textContent === label); assert(node, `Missing button: ${label}`); return node; };
  const field = (label) => { const owner = all().find((node) => node.tagName === 'LABEL' && node.children[0]?.textContent === label); assert(owner, `Missing field: ${label}`); return owner.children[1]; };
  return { window, document, app, state, calls, errors, notices, all, findButton, field, emit(name, detail) { for (const listener of events[name] || []) listener({ detail }); }, async close() { window.PhotoFindryPanels.close(); await new Promise((resolve) => setTimeout(resolve, 0)); } };
}

module.exports={harness};
