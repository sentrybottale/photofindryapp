import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { build } from 'esbuild';
import { compile } from 'svelte/compiler';
import { Window } from 'happy-dom';

const root = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  stdin: { contents: `import { mount, unmount, flushSync } from 'svelte'; import PhotoSummary from './src/PhotoSummary.svelte';
    window.photoSummaryFixture = { mount(target, data) { let component; flushSync(() => { component = mount(PhotoSummary, { target, props: { data } }); });
    return { update(data) { flushSync(() => component.update(data)); }, destroy() { return unmount(component); } }; } };`, resolveDir: root, sourcefile: 'photo-summary-fixture.js' },
  bundle: true, write: false, format: 'iife', platform: 'browser', conditions: ['browser', 'production'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [{ name: 'compile-photo-summary', setup(builder) {
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path: filename }) => {
      const result = compile(await readFile(filename, 'utf8'), { filename, generate: 'client', css: 'external', runes: true, dev: false, hmr: false, discloseVersion: false });
      assert.deepEqual(result.warnings, []);
      return { contents: result.js.code, loader: 'js', resolveDir: path.dirname(filename) };
    });
  } }],
});
const data = extra => ({ name: 'holiday.jpg', caption: 'A saved description.', sourceLabel: 'Travel', sourceStatus: '',
  statusLabel: 'Tagged · 41 observations', statusTone: 'success', reviewLabel: '', reviewTone: 'neutral', reviewHint: '',
  matchLabel: '', matchText: '', matchHint: '', matchTone: 'direct', locationEvidence: '', onPreview() {}, ...extra });
function fixture() {
  const window = new Window({ url: 'http://127.0.0.1:50001/' });
  let requests = 0;
  window.fetch = () => { requests += 1; throw new Error('Photo summaries must never request data.'); };
  window.eval(bundle.outputFiles[0].text);
  const target = window.document.createElement('div'); window.document.body.append(target);
  return { window, target, mount: input => window.photoSummaryFixture.mount(target, input), requests: () => requests,
    close: () => window.happyDOM.abort() };
}

test('compiled photo summary renders source, saved status and review icons with explicit candidate evidence', async () => {
  const ui = fixture();
  const component = ui.mount(data({ sourceStatus: 'Confirm original location', reviewLabel: 'Review suggestions', reviewTone: 'review',
    matchLabel: 'Refined candidate', matchText: 'A person next to a tree.', matchTone: 'related', matchHint: 'Ranking orders candidates; it does not confirm a match.' }));
  try {
    assert.match(ui.target.textContent, /Travel · Confirm original location/);
    assert.match(ui.target.textContent, /Tagged · 41 observations/); assert.match(ui.target.textContent, /Review suggestions/);
    assert.match(ui.target.textContent, /Refined candidate/); assert.match(ui.target.textContent, /A person next to a tree/);
    assert.equal(ui.target.querySelectorAll('svg[aria-hidden="true"]').length, 4);
    assert.match(ui.target.querySelector('.photo-match').title, /does not confirm a match/);
    assert.equal(ui.target.querySelectorAll('button').length, 2); assert.equal(ui.target.querySelectorAll('input, img').length, 0);
    assert.equal(ui.requests(), 0);
  } finally { await component.destroy(); await ui.close(); }
});

test('compiled summary updates in place, escapes saved strings and leaves image/selection ownership outside its root', async () => {
  const ui = fixture(); const component = ui.mount(data());
  try {
    const name = ui.target.querySelector('.photo-name'), icon = ui.target.querySelector('.pf-photo-icon');
    component.update(data({ name: '<img src=x onerror=alert(1)>', caption: '<script>stolen()</script>', statusLabel: 'Needs attention', statusTone: 'warning' }));
    assert.equal(ui.target.querySelector('.photo-name'), name); assert.equal(ui.target.querySelector('.pf-photo-icon'), icon);
    assert.equal(ui.target.querySelector('.photo-name').textContent, '<img src=x onerror=alert(1)>');
    assert.match(ui.target.textContent, /<script>stolen\(\)<\/script>/); assert.equal(ui.target.querySelectorAll('img, script, [onclick]').length, 0);
    assert.equal(ui.target.querySelector('.photo-state').dataset.tone, 'warning'); assert.equal(ui.requests(), 0);
  } finally { await component.destroy(); assert.equal(ui.target.childElementCount, 0); await ui.close(); }
});

test('one page of compiled summaries is bounded and all DOM ownership is released on removal', async () => {
  const ui = fixture(); const components = [];
  try {
    for (let index = 0; index < 60; index += 1) {
      const host = ui.window.document.createElement('div'); ui.target.append(host);
      components.push(ui.window.photoSummaryFixture.mount(host, data({ name: `photo-${index}.jpg` })));
    }
    assert.equal(ui.target.querySelectorAll('.pf-photo-summary').length, 60);
    assert(ui.target.querySelectorAll('*').length < 1800); assert.equal(ui.requests(), 0);
    for (const component of components) await component.destroy();
    assert.equal(ui.target.querySelectorAll('.pf-photo-summary').length, 0);
    assert.equal(ui.target.querySelectorAll('*').length, 60);
  } finally { await ui.close(); }
});

test('review and matching-detail buttons open the ordinary preview without also selecting or double-opening the tile', async () => {
  const ui = fixture(); let selections = 0, previews = 0, doubles = 0;
  // The real tile wraps the framework's metadata mount point. Svelte delegates
  // at that mount point, before the controller-owned parent tile receives it.
  ui.target.parentNode.addEventListener('click', () => { selections += 1; });
  ui.target.parentNode.addEventListener('dblclick', () => { doubles += 1; });
  const component = ui.mount(data({ reviewLabel: 'Photo detail review', matchLabel: 'Refined candidate', matchText: 'Saved evidence.', onPreview() { previews += 1; } }));
  try {
    const buttons = [...ui.target.querySelectorAll('button')];
    assert.equal(buttons.length, 2);
    for (const button of buttons) {
      assert.equal(button.type, 'button'); assert.match(button.getAttribute('aria-label'), /holiday.jpg/);
      button.click(); button.dispatchEvent(new ui.window.MouseEvent('dblclick', { bubbles: true }));
    }
    assert.equal(previews, 2); assert.equal(selections, 0); assert.equal(doubles, 0);
    component.update(data({ reviewLabel: 'Review undone', onPreview() { previews += 10; } }));
    ui.target.querySelector('button').click(); assert.equal(previews, 12); assert.equal(selections, 0);
  } finally { await component.destroy(); await ui.close(); }
});
