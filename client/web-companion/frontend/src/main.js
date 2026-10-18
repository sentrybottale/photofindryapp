import { flushSync, mount, unmount } from 'svelte';
import AppShell from './AppShell.svelte';
import PhotoSummary from './PhotoSummary.svelte';

let component = null;
let mountedTarget = null;
const photoSummaries = new Set();

function mountPhotoSummary(target, data) {
  if (!component) return null;
  if (!(target instanceof Element) || target.ownerDocument !== document || !target.classList.contains('photo-summary-slot')) throw new Error('The photo summary target is unavailable.');
  if (photoSummaries.size >= 240) throw new Error('The photo summary page exceeded its display bound.');
  let summary;
  flushSync(() => { summary = mount(PhotoSummary, { target, props: { data } }); });
  const handle = {
    update(next) { if (summary) flushSync(() => summary.update(next)); },
    async destroy() { if (!summary) return; const previous = summary; summary = null; photoSummaries.delete(handle); await unmount(previous); },
  };
  photoSummaries.add(handle);
  return Object.freeze(handle);
}

function mountShell(target = document.getElementById('photofindry-root')) {
  if (!(target instanceof Element) || target.ownerDocument !== document || target.id !== 'photofindry-root') throw new Error('The local interface mount point is unavailable. Reopen PhotoFindry.');
  if (component) {
    if (target !== mountedTarget) throw new Error('The local interface is already mounted.');
    return getState();
  }
  const fallback = [...target.childNodes];
  target.replaceChildren();
  mountedTarget = target;
  try { flushSync(() => { component = mount(AppShell, { target }); }); }
  catch (error) {
    const failed = component;
    component = null; mountedTarget = null;
    if (failed) void unmount(failed);
    target.replaceChildren(...fallback);
    throw error;
  }
  return getState();
}

async function destroyShell() {
  if (!component) return;
  await Promise.all([...photoSummaries].map(summary => summary.destroy()));
  const previous = component;
  component = null; mountedTarget = null;
  await unmount(previous);
}

function getState() {
  return { framework: 'svelte', version: __PF_SVELTE_VERSION__, mounted: component !== null, ...(component?.getState() || {}) };
}

Object.defineProperty(window, 'PhotoFindryShell', {
  value: Object.freeze({
    mount: mountShell,
    destroy: destroyShell,
    getState,
    showDetails: () => component?.showDetails(),
    closeDrawers: () => component?.closeDrawers(),
    mountPhotoSummary,
    get ownsNavigation() { return component !== null; },
  }),
  writable: false,
  configurable: false,
});
