'use strict';

// No input contents are observed. Time requires trusted input in the visible,
// focused library; a foreground change or missed sample requires fresh input.
(() => {
  const P = () => window.PhotoFindry;
  const clientId = crypto.randomUUID();
  let connected = false, handled = false, inputSinceFocus = false, lastInput = null, lastSample = null;
  let generation = 0;
  let timer = null, lastReportedInput = null, pending = false, snapshot = {}, dialog = null;
  const now = () => performance.now();
  const foreground = () => connected && document.visibilityState === 'visible' && document.hasFocus();
  const request = async (event, quiet = false) => {
    if (!connected || (handled && event !== 'manual')) return null;
    const epoch = generation;
    const result = await P().api('/api/support', { clientId, event, foreground: foreground(), quiet });
    if (!connected || epoch !== generation) { const error = Error('Local support view changed.'); error.name = 'AbortError'; throw error; }
    return result;
  };
  function suspend() {
    inputSinceFocus = false; lastInput = lastSample = null; lastReportedInput = null;
    if (connected && !handled) void request('suspend').catch(() => {});
  }
  function quietLibrary() {
    if (!foreground() || !inputSinceFocus || lastInput === null || now() - lastInput < 30000 || now() - lastInput > 300000) return false;
    if (dialog?.open || document.querySelector('dialog[open]') || (document.getElementById('advanced-panel') && !document.getElementById('advanced-panel').hidden)) return false;
    if (document.getElementById('onboarding')?.hidden === false || document.getElementById('app')?.hidden !== false) return false;
    if (!snapshot.library || !(snapshot.library.total > 0) || snapshot.libraryError || snapshot.loading) return false;
    if (['global-error', 'library-error', 'loading-state'].some(id => { const node = document.getElementById(id); return node && !node.hidden; })) return false;
    const active = document.activeElement;
    if (active && (['INPUT', 'TEXTAREA', 'SELECT'].includes(active.tagName) || active.isContentEditable)) return false;
    if ([snapshot.job, snapshot.improvementJob, snapshot.aiSearch, snapshot.sourceTransfer, snapshot.storageJob, snapshot.modelOptimization, snapshot.faceJob, snapshot.semanticIndex, snapshot.metadataJob, snapshot.previewJob, snapshot.discoveryJob].some(job => job?.running === true)) return false;
    if (['starting', 'loading', 'switching'].includes(snapshot.runtime?.status)) return false;
    return !(snapshot.jobs?.jobs || []).some(job => ['running', 'pausing'].includes(job.state));
  }
  function schedule() { clearTimeout(timer); if (connected && !handled) timer = setTimeout(sample, 15000); }
  async function sample() {
    timer = null;
    const current = now();
    if (!foreground() || lastSample !== null && (current - lastSample < 0 || current - lastSample > 30000)) { suspend(); schedule(); return; }
    lastSample = current;
    if (pending) { schedule(); return; }
    pending = true;
    try {
      const result = await request('sample', quietLibrary());
      handled = result?.handled === true;
      if (result?.showInvitation && connected && quietLibrary()) show(true);
    } catch { /* Support cannot interrupt library access. */ }
    finally { pending = false; schedule(); }
  }
  function input(event) {
    if (!event.isTrusted || !foreground() || handled) return;
    lastInput = now(); inputSinceFocus = true;
    // Input requests are throttled to one per sampling interval; event content,
    // target identity, key values and pointer coordinates never leave this handler.
    if (lastReportedInput === null || now() - lastReportedInput >= 15000) {
      lastReportedInput = now(); if (lastSample === null) lastSample = now();
      void request('input').then(result => { if (result?.handled) { handled = true; clearTimeout(timer); } }).catch(() => {});
    }
  }
  async function open() {
    if (!connected) return;
    const result = await request('manual');
    if (!result?.handled) throw Error('The support invitation preference could not be saved. Please try again.');
    handled = true; clearTimeout(timer); show(false);
  }
  function show(invitation) {
    if (dialog?.open) return;
    const make = (tag, text) => { const node = document.createElement(tag); if (text) node.textContent = text; return node; };
    dialog = make('dialog'); dialog.className = 'workspace-dialog support-dialog'; dialog.setAttribute('aria-label', 'Optional support');
    const title = make('h2', invitation ? 'Enjoying PhotoFindry?' : 'Support PhotoFindry');
    const paragraph = make('p', 'PhotoFindry is free, with every feature included. If it is useful to you, a coffee helps support its continued development.');
    const link = make('a', 'Buy me a coffee ↗'); link.href = 'https://buymeacoffee.com/photorecall'; link.target = '_blank'; link.rel = 'noopener noreferrer'; link.className = 'primary-button';
    const qr = make('img'); qr.src = '/support-qr.png'; qr.alt = 'QR code for the PhotoFindry Buy Me a Coffee page'; qr.width = 112; qr.height = 112;
    const copy = make('button', 'Copy support link'); copy.type = 'button'; copy.className = 'secondary-button';
    copy.addEventListener('click', async () => { try { await navigator.clipboard.writeText('https://buymeacoffee.com/photorecall'); copy.textContent = 'Link copied'; } catch { copy.textContent = 'buymeacoffee.com/photorecall'; } });
    const close = make('button', invitation ? 'No thanks' : 'Done'); close.type = 'button'; close.className = 'secondary-button'; close.addEventListener('click', () => dialog?.close());
    const content = make('div'); content.className = 'dialog-body'; content.append(title, paragraph, link, make('p', 'Optional. Opens Buy Me a Coffee only when you choose the link.'), qr, copy, close); dialog.append(content);
    const current = dialog; current.addEventListener('close', () => { current.remove(); if (dialog === current) dialog = null; });
    document.body.append(current); current.showModal();
  }
  for (const event of ['pointerdown', 'pointermove', 'keydown', 'wheel']) document.addEventListener(event, input, { passive: true });
  document.addEventListener('visibilitychange', suspend); window.addEventListener('blur', suspend); window.addEventListener('focus', suspend); window.addEventListener('pagehide', suspend);
  window.addEventListener('photofindry:state', event => { snapshot = event.detail || {}; });
  window.addEventListener('photofindry:session', event => {
    generation += 1;
    const previous = connected; connected = event.detail?.connected === true;
    inputSinceFocus = false; lastInput = lastSample = null; lastReportedInput = null;
    clearTimeout(timer);
    if (!connected) { if (previous) dialog?.close(); snapshot = {}; }
    else { handled = false; schedule(); }
  });
  window.PhotoFindrySupport = Object.freeze({ open });
})();
