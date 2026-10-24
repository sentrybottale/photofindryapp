'use strict';

// Local domain workflows. Filesystem access is represented only by picker grants
// or source scopes minted by the installed companion, never browser-supplied paths.
(() => {
  const bridge = () => window.PhotoFindry;
  const el = (tag, className = '', value) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (value !== undefined) node.textContent = String(value);
    return node;
  };
  const list = (value) => Array.isArray(value) ? value : [];
  const text = (value, fallback = '') => typeof value === 'string' && value ? value : fallback;
  const count = (value) => new Intl.NumberFormat().format(Number.isFinite(value) ? value : 0);
  const date = (value) => value && Number.isFinite(Date.parse(value)) ? new Date(value).toLocaleString() : 'Not recorded';
  const size = (value) => { const n = Number(value) || 0; return n < 1048576 ? `${count(Math.ceil(n / 1024))} KB` : `${(n / 1048576).toFixed(1)} MB`; };
  const snapshot = () => bridge()?.getState?.() || view.snapshot || {};
  const sources = () => list(snapshot().sources);
  const jobs = () => list(snapshot().jobs?.jobs);
  const selected = () => list(bridge()?.selectedIDs?.()).filter((id) => typeof id === 'string');
  const terminal = (job) => ['completed', 'failed', 'cancelled'].includes(job.state);
  const view = { name: null, generation: 0, controller: null, urls: new Set(), snapshot: {}, sourceId: '', personId: '', photoId: '', peopleOffset: 0,
    cleanupSource: '', cleanupTab: 'review', cleanupPlan: null, cleanupPage: null, cleanupSelected: new Set(), cleanupAnchor: null,
    cleanupReady: false, cleanupLife: Promise.resolve(), cleanupLastJob: null, cleanupLastCount: 0, confirmation: null,
    requestVersion: 0, sourceOffset: 0, recoveryJobId: null, sourceDrafts: new Map(), sourceRoots: new Map(), sourceChecks: new Map(), pending: 0, refreshTimer: null, sourceRefreshNeeded: false };
  let root, body, status, title, previousFocus;
  let transferStatusSignature = '', previousTransfers = new Map();
  const underlayInert = new Map();

  function isolatePanel() {
    const app = document.getElementById('app');
    if (!app || root.parentNode !== app) return;
    for (const sibling of app.children) {
      if (sibling === root) continue;
      if (!underlayInert.has(sibling)) underlayInert.set(sibling, Boolean(sibling.inert));
      sibling.inert = true;
    }
  }
  function restoreUnderlay() {
    for (const [sibling, inert] of underlayInert) sibling.inert = inert;
    underlayInert.clear();
    const target = previousFocus; previousFocus = null;
    if (target?.isConnected && !target.closest?.('[hidden], [inert]')) target.focus?.({ preventScroll: true });
    else {
      const fallback = document.getElementById(document.getElementById('app')?.hidden ? 'pair-code' : 'gallery');
      fallback?.focus?.({ preventScroll: true });
    }
  }

  function button(label, handler, tone = '') {
    const node = el('button', `pfp-button ${tone}`, label); node.type = 'button';
    if (handler) node.addEventListener('click', async () => {
      if (node.disabled) return;
      const generation = view.generation;
      node.disabled = true; view.pending += 1;
      try { await handler(); }
      catch (error) { if (error?.name !== 'AbortError') { bridge().showError(error?.message || 'The action could not finish.'); showProblem(error?.message); } }
      finally { view.pending -= 1; if (generation === view.generation && node.isConnected) node.disabled = false; if (!view.pending && view.sourceRefreshNeeded) scheduleSourceRefresh(); }
    });
    return node;
  }
  // These controls outlive view generations. Navigation must stay available even
  // while an old panel read is pending; show() aborts and fences that old view.
  function navigationButton(label, handler) {
    const node = button(label);
    node.addEventListener('click', handler);
    return node;
  }
  function row(...nodes) { const node = el('div', 'pfp-actions'); node.append(...nodes.filter(Boolean)); return node; }
  function card(heading, detail = '') {
    const node = el('section', 'pfp-card');
    if (heading) node.append(el('h3', '', heading));
    if (detail) node.append(el('p', 'pfp-muted', detail));
    return node;
  }
  function disclosure(label, ...nodes) {
    const node = el('details', 'pfp-card'); node.append(el('summary', '', label), ...nodes.filter(Boolean)); return node;
  }
  function sourceLocation(rootInfo) { return text(rootInfo?.locationLabel, text(rootInfo?.path, text(rootInfo?.pathLabel, text(rootInfo?.label, 'No saved folder')))); }
  function notice(message, warning = false) { return el('p', warning ? 'pfp-warning' : 'pfp-note', message); }
  function field(label, control, help) {
    const node = el('label', 'pfp-field'); node.append(el('span', '', label), control);
    if (help) node.append(el('small', '', help));
    return node;
  }
  function input(value = '', maxLength = 80) { const node = el('input', 'pfp-input'); node.value = value; node.maxLength = maxLength; return node; }
  function select(options, value = '') {
    const node = el('select', 'pfp-input');
    for (const [id, label] of options) { const option = el('option', '', label); option.value = id; node.append(option); }
    node.value = value; return node;
  }
  function checkbox(label, checked = false) {
    const node = el('label', 'pfp-check'); const control = el('input'); control.type = 'checkbox'; control.checked = checked;
    node.append(control, document.createTextNode(label)); return { node, control };
  }
  function sourceSelect(value = '', all = true) {
    return select([...(all ? [['', 'All sources']] : []), ...sources().map((source) => [source.id, source.label || 'Untitled source'])], value);
  }
  function showProblem(message) {
    if (!root || root.hidden || !message) return;
    const old = root.querySelector('.pfp-problem'); old?.remove();
    const error = el('p', 'pfp-problem', message); error.setAttribute('role', 'alert'); body.prepend(error);
  }
  // An explicit face scan may join the unfinished job that already owns
  // those photographs; say where the work is instead of claiming a new queue.
  const faceScanNotice = (fallback) => (value) => !value?.reusedExisting ? fallback
    : value.state === 'paused' ? 'These photographs are already in a paused face scan. Resume it in Jobs when ready; saved faces are kept.'
      : 'Face discovery for these photographs is already queued or running. Its progress is in Jobs.';
  async function command(name, params = {}, success) {
    const value = await bridge().action(name, params);
    const message = typeof success === 'function' ? success(value) : success;
    if (message) {
      bridge().showNotice(message);
      if (root && !root.hidden) { root.querySelector('.pfp-local-notice')?.remove(); const banner = notice(message); banner.classList.add('pfp-local-notice'); body.prepend(banner); }
    }
    return value;
  }
  async function read(name, params = {}) { return bridge().api('/api/action', { action: name, params }, { signal: view.controller?.signal }); }
  async function choose(kind) {
    const grant = await bridge().pick(kind);
    return grant?.cancelled || !grant?.grantId ? null : grant;
  }
  function appendSummary(parent, entries) {
    const node = el('dl', 'pfp-summary');
    for (const [label, value] of entries) node.append(el('dt', '', label), el('dd', '', value));
    parent.append(node);
  }
  function pager(parent, offset, total, limit, change, itemCount) {
    const previous = button('← Previous', () => change(Math.max(0, offset - limit)));
    const next = button('Next →', () => change(offset + limit));
    previous.disabled = offset <= 0; next.disabled = offset + limit >= total;
    parent.append(row(previous, el('span', 'pfp-muted', total ? `${count(offset + 1)}–${count(Math.min(total, offset + (itemCount ?? limit)))} of ${count(total)}` : '0 entries'), next));
  }
  function ensureRoot() {
    if (root) return;
    root = el('section', 'pfp-shell'); root.id = 'advanced-panel'; root.hidden = true; root.setAttribute('aria-label', 'Library management');
    const heading = el('header', 'pfp-header'); title = el('h2', '', 'Library management');
    heading.append(title, navigationButton('Back to photos', close));
    const nav = el('nav', 'pfp-nav'); nav.setAttribute('aria-label', 'Library management');
    for (const [name, label] of [['sources', 'Files'], ['people', 'People'], ['cleanup', 'Library cleanup'], ['portability', 'Backups & archives'], ['improvement', 'Photo improvement']]) {
      const item = navigationButton(label, () => show(name)); item.dataset.panel = name; nav.append(item);
    }
    status = el('div', 'pfp-status'); status.setAttribute('aria-live', 'polite');
    body = el('div', 'pfp-body'); root.append(heading, nav, status, body);
    (document.getElementById('app') || document.body).append(root);
  }
  function clearContent() {
    view.controller?.abort(); view.controller = new AbortController(); view.requestVersion += 1;
    for (const url of view.urls) URL.revokeObjectURL(url); view.urls.clear();
    body.replaceChildren();
  }
  function renderStatus() {
    if (!status) return;
    const state = snapshot();
    const transfers = [['source', state.sourceTransfer], ['library', state.storageJob]];
    for (const [kind, job] of transfers) {
      if (previousTransfers.get(kind)?.running && job?.phase === 'error') bridge().showError(text(job.message, text(job.error, 'The local transfer could not finish. Open Backups & archives to review its status.')));
    }
    previousTransfers = new Map(transfers.map(([kind, job]) => [kind, { running: job?.running === true, phase: job?.phase }]));
    const work = transfers.filter(([, job]) => job && job.kind !== 'idle' && job.kind !== 'purgePrepare' && (job.kind !== 'sourceReview' || job.running)
      && (job.running || ['complete', 'error', 'cancelled', 'cleanupPending', 'readyToRestart'].includes(job.phase)));
    const completedResults = list(state.transferResults).filter(result => ['library', 'source'].includes(result?.kind) && /^[a-f0-9]{64}$/i.test(result?.id || ''));
    const signature = JSON.stringify([work, completedResults]);
    if (signature === transferStatusSignature) return;
    transferStatusSignature = signature;
    status.replaceChildren(); status.hidden = !work.length && !completedResults.length;
    const otherTransferRunning = transfers.some(([, job]) => job?.running);
    for (const [kind, job] of work) {
      const failed = job.phase === 'error' || job.phase === 'cleanupPending';
      const fallback = job.running ? 'Working locally…' : job.phase === 'complete' ? 'Transfer complete.' : job.phase === 'cancelled' ? 'Transfer cancelled.' : job.phase === 'cleanupPending' ? 'Source cache cleanup needs attention. Saved metadata remains available.' : 'The transfer could not finish.';
      const line = row(el('span', failed ? 'pfp-warning' : '', text(job.message, fallback)),
        job.running && job.total > 0 ? el('span', 'pfp-muted', `${count(job.completed)} / ${count(job.total)}`) : null);
      if (job.running) {
        if (job.kind === 'sourceReview') line.append(button('Cancel folder review', async () => { await command('cancelSourceConnectionReview', {}, 'Stopping the folder review. Saved photographs and tags are kept.'); view.sourceRefreshNeeded = true; }));
        else if (job.cancellable !== false && job.phase !== 'restoring') line.append(button('Cancel', () => command(kind === 'source' ? 'cancelSourceTransfer' : 'cancelTransfer')));
        else line.append(el('span', 'pfp-muted', 'Finishing the source update; this step cannot be cancelled.'));
      } else if (kind === 'source' && job.phase === 'cleanupPending') {
        const retry = button('Retry cleanup', async () => {
          const latest = snapshot();
          if (latest.sourceTransfer?.phase !== 'cleanupPending' || latest.sourceTransfer?.running || latest.storageJob?.running) throw Error('The transfer state changed. Review its current status before retrying.');
          await command('retrySourceTransfer', {}, 'Rechecking the verified archive before retrying its remaining cache cleanup.');
        });
        retry.disabled = otherTransferRunning; line.append(retry);
      }
      if (job.error && job.error !== job.message) line.append(el('span', 'pfp-warning', text(job.error)));
      if (job.phase === 'complete' && job.pathLabel) line.append(el('span', 'pfp-muted', `Saved as ${text(job.pathLabel)} in the destination you chose.`));
      if (job.cleanup && Number.isFinite(job.cleanup.deletedFiles) && Number.isFinite(job.cleanup.retainedFiles)) line.append(el('span', 'pfp-muted', `${count(job.cleanup.deletedFiles)} image copies removed · ${count(job.cleanup.retainedFiles)} retained. Original photographs are unchanged.`));
      status.append(line);
    }
    for (const result of completedResults) status.append(row(el('span', 'pfp-muted', `Saved ${result.kind === 'library' ? 'backup' : 'source archive'} · ${text(result.label, 'Verified export')}`),
      button(result.kind === 'library' ? 'Show backup in Finder' : 'Show archive in Finder', () => command('revealTransferResult', { kind: result.kind, id: result.id }))));
  }
  async function show(name = 'sources', context = {}) {
    ensureRoot();
    const normalized = ({ removed: 'cleanup', backup: 'portability', archives: 'portability', reviews: 'improvement' })[name] || name;
    if (!['sources', 'people', 'cleanup', 'portability', 'improvement'].includes(normalized)) return;
    if (view.name === 'cleanup' && normalized !== 'cleanup') releaseCleanup();
    if (root.hidden) previousFocus = document.activeElement;
    view.name = normalized; view.generation += 1; root.hidden = false; isolatePanel(); clearContent();
    if (context.sourceId !== undefined) view.sourceId = context.sourceId;
    if (normalized === 'sources' && typeof context.sourceId === 'string' && typeof context.rootId === 'string') view.sourceRoots.set(context.sourceId, context.rootId);
    else if (normalized === 'sources' && typeof context.sourceId === 'string' && context.jobId) view.sourceRoots.delete(context.sourceId);
    if (context.personId !== undefined) { view.personId = context.personId || ''; view.peopleOffset = 0; }
    if (normalized === 'people') view.photoId = typeof context.photoId === 'string' && context.photoId.length <= 128 ? context.photoId : '';
    if (normalized === 'sources') view.recoveryJobId = typeof context.jobId === 'string' ? context.jobId : null;
    if (normalized === 'cleanup' && context.sourceId !== undefined) view.cleanupSource = context.sourceId;
    if (name === 'removed') view.cleanupTab = 'removed';
    title.textContent = ({ sources: 'Files', people: 'People', cleanup: 'Library cleanup', portability: 'Backups & source archives', improvement: 'Photo improvement' })[normalized];
    for (const item of root.querySelectorAll('[data-panel]')) item.setAttribute('aria-current', item.dataset.panel === normalized ? 'page' : 'false');
    renderStatus(); title.tabIndex = -1; title.focus({ preventScroll: true });
    const generation = view.generation;
    try {
      if (normalized === 'sources') await renderSources();
      if (normalized === 'people') await renderPeople();
      if (normalized === 'cleanup') await renderCleanup();
      if (normalized === 'portability') renderPortability();
      if (normalized === 'improvement') await renderImprovement();
    } catch (error) { if (generation === view.generation && error?.name !== 'AbortError') showProblem(error.message); }
  }
  function close() {
    if (!root || root.hidden) return;
    if (view.name === 'cleanup') releaseCleanup();
    view.generation += 1; view.name = null; clearContent(); root.hidden = true;
    view.confirmation?.close(); restoreUnderlay();
  }
  function confirmation(heading, content, label, { destructive = false, acknowledgement = null } = {}) {
    return new Promise((resolve) => {
      const dialog = el('dialog', 'pfp-dialog'); view.confirmation = dialog;
      dialog.append(el('h2', '', heading));
      for (const message of list(content)) dialog.append(typeof message === 'string' ? el('p', '', message) : message);
      const accept = button(label, () => { dialog.returnValue = 'accept'; dialog.close(); }, destructive ? 'danger' : 'primary');
      if (acknowledgement) {
        const check = checkbox(acknowledgement); accept.disabled = true;
        check.control.addEventListener('change', () => { accept.disabled = !check.control.checked; }); dialog.append(check.node);
      }
      dialog.append(row(button('Cancel', () => dialog.close()), accept));
      dialog.addEventListener('close', () => { const approved = dialog.returnValue === 'accept'; if (view.confirmation === dialog) view.confirmation = null; dialog.remove(); resolve(approved); }, { once: true });
      document.body.append(dialog); dialog.showModal();
    });
  }
  function preview(photoId, parent, faceId, label = 'Saved preview') {
    const signal = view.controller.signal;
    const placeholder = el('div', 'pfp-image-placeholder', 'Loading saved preview…'); parent.append(placeholder);
      bridge().previewBlob(photoId, signal, faceId)
        .then((blob) => {
          if (signal.aborted || !parent.isConnected) return;
          const url = URL.createObjectURL(blob); view.urls.add(url);
          const image = el('img', 'pfp-preview'); image.alt = label; image.src = url; image.decoding = 'async';
          image.addEventListener('error', () => { image.replaceWith(el('span', 'pfp-muted', 'Saved preview unavailable')); URL.revokeObjectURL(url); view.urls.delete(url); }, { once: true });
          placeholder.replaceWith(image);
        }).catch(() => { if (!signal.aborted && placeholder.isConnected) placeholder.textContent = 'No saved preview'; });
  }
  function photoButton(photo, label) { return button(label || text(photo.name, 'Open photo'), () => bridge().showPhoto(photo.id)); }
  function jobCard(job) {
    if (window.PhotoFindryJobs?.card) return window.PhotoFindryJobs.card(job);
    const node = card(text(job.title, text(job.kind, 'Local job')), `${text(job.state)} · ${text(job.message)}`);
    if (job.total > 0 || job.progress?.total > 0) node.append(el('p', '', `${count(job.completed ?? job.progress?.completed)} / ${count(job.total ?? job.progress?.total)}`));
    const actions = row();
    if (['running', 'queued'].includes(job.state)) actions.append(button('Pause', () => command('jobAction', { id: job.id, action: 'pause' })));
    if (['paused', 'failed'].includes(job.state)) actions.append(button(job.state === 'failed' ? 'Retry' : 'Resume', () => command('jobAction', { id: job.id, action: job.state === 'failed' ? 'retry' : 'runNext' })));
    if (!terminal(job)) actions.append(button('Stop', () => command('jobAction', { id: job.id, action: 'cancel' })));
    node.append(actions); return node;
  }

  const automaticSource = source => source?.automaticLifecycleVersion === 1;
  const sourceConflict = source => source?.needsReview > 0 || ['needsReview', 'changed'].includes(source?.status);
  function nativeAccessStatus(parent, source, roots, selectedRoot, existingPermissionAction) {
    for (const rootInfo of selectedRoot ? [selectedRoot] : roots) {
      const status = rootInfo.nativeAccessStatus;
      const copy = {
        restoring: ['Restoring folder access', 'Saved permission is being restored automatically. Your photographs and saved work are kept.'],
        unavailable: ['Folder access is temporarily unavailable', 'PhotoFindry will retry automatically. This does not mean your originals were deleted.'],
        permissionDenied: ['Folder read access needed', 'macOS denied access to this folder. Choose it again to allow reading; saved work is kept.'],
        invalidGrant: ['Saved folder permission needs renewal', 'This folder’s saved permission is no longer usable. Choose the folder again; saved work is kept.'],
        ownerUnavailable: ['Companion access recovery needed', 'The installed folder access component is missing or changed. Use verified companion recovery; choosing the folder again cannot repair this component.'],
      }[status];
      if (!copy) continue;
      const note = card(copy[0], copy[1]);note.classList.add('pfp-native-access-status');
      if (roots.length > 1) appendSummary(note, [['Folder', sourceLocation(rootInfo)]]);
      if (['permissionDenied', 'invalidGrant'].includes(status) && !(existingPermissionAction && selectedRoot?.id === rootInfo.id)) {
        note.append(row(button('Allow folder access…', () => locate(source, rootInfo), 'primary')));
      } else if (status === 'ownerUnavailable') {
        const recover = el('a', 'pfp-button', 'Recover companion');recover.href = 'photofindry-companion://open';
        const setup = el('a', 'pfp-button', 'Verified setup help');setup.href = 'https://photofindry.com/#install';setup.target = '_blank';setup.rel = 'noopener noreferrer';
        note.append(row(recover, setup));
      }
      parent.append(note);
    }
  }
  function automaticSourceConnection(source, rootInfo, check) {
    const roots = list(source.roots);
    const status = text(rootInfo?.status, source.status);
    const selected = rootInfo ? rootInfo.selected === true && status !== 'detached' : source.originalsFolderSelected === true && roots.every(root => root.selected === true || root.status === 'detached');
    const conflicts = Number.isFinite(rootInfo?.needsReview) ? rootInfo.needsReview : source.needsReview;
    const missing = rootInfo?.missing ?? source.missing;
    const unchecked = rootInfo?.unchecked ?? source.unchecked;
    const identityConflict = ['needsReview', 'changed'].includes(status);
    // The selected-folder receipt is not a processing job. Once its retained
    // selection reaches the source snapshot, only source availability drives UI.
    const unavailable = ['offline', 'permission', 'onlineOnly', 'unavailable'].includes(status);
    const pendingSelection = rootInfo ? rootInfo.selectionPending === true : source.folderSelectionPending === true;
    if (check?.jobKind === 'sourceSelection' && (selected || pendingSelection)) check.selectionObserved = true;
    const needsChoice = (!selected || source.mode === 'metadata') && !pendingSelection;
    const checking = !identityConflict && !['permission', 'unavailable', 'onlineOnly'].includes(status) && Boolean(check?.jobId && check.jobKind === 'sourceSelection' && !check.selectionObserved && !selected && !pendingSelection
      || !unavailable && (pendingSelection || selected && status === 'checking'));
    const permission = status === 'permission';
    const ownerUnavailable = (rootInfo ? [rootInfo] : roots).some(root => root.nativeAccessStatus === 'ownerUnavailable');
    let heading = 'Originals folder connected';
    let detail = 'Folder changes are checked in the background. Connected, moved and returning originals are matched automatically; saved photographs and tags stay available.';
    if (ownerUnavailable && (permission || needsChoice)) {
      heading = 'Companion access recovery needed';
      detail = 'Use verified companion recovery to restore the folder access component. Saved photographs and work are kept.';
    } else if (checking) {
      heading = 'Checking originals automatically';
      detail = 'Checking the folder you selected. Matching originals become usable automatically; no second confirmation is needed.';
    } else if (permission) {
      heading = 'Folder access needed';
      detail = 'macOS needs permission to read this folder. Choose it to allow access; saved photographs and completed work are kept.';
    } else if (needsChoice) {
      heading = 'Choose this Mac’s originals folder once';
      detail = 'Select the matching folder on this Mac. PhotoFindry then watches its availability and file changes automatically. Saved tags and search remain available.';
    } else if (status === 'offline') {
      heading = 'Waiting for the folder to return';
      detail = 'This folder is disconnected. It stays selected and is checked automatically when its drive returns. Saved photos, tags and retained previews are kept.';
    } else if (status === 'onlineOnly') {
      heading = 'Waiting for cloud originals';
      detail = 'These originals are stored by your cloud provider. Changes are watched automatically; make originals available in Finder when you want to process them.';
    } else if (status === 'unavailable') {
      heading = 'Some originals are temporarily unavailable';
      detail = 'Saved locations and returning files are checked automatically. Saved photos and tags are kept; temporary absence does not mean permanent deletion.';
    } else if (identityConflict || conflicts > 0) {
      heading = 'Some original identities need review';
      detail = 'Only conflicting or uncertain originals need a decision. Other verified photographs continue automatically; a disconnected drive is never treated as deleted photos.';
    } else if (status === 'missing' || missing > 0) {
      heading = 'Some originals are temporarily unavailable';
      detail = 'Saved locations and returning files are checked automatically. Saved photos and tags are kept; temporary absence does not mean permanent deletion.';
    } else if (['checking', 'unchecked'].includes(status) || unchecked > 0) {
      heading = 'Checking originals automatically';
      detail = 'Folder availability and file changes are checked in the background. No manual folder confirmation is needed for matching originals.';
    }
    const connection = card(heading, detail); connection.classList.add('pfp-automatic-source');
    connection.setAttribute('aria-live', 'polite');
    for (const root of roots) appendSummary(connection, [[root.selected ? 'Selected folder' : 'Saved folder location', sourceLocation(root)]]);
    if (check?.chosenLocationLabel && checking) appendSummary(connection, [['Selected on this Mac', check.chosenLocationLabel]]);
    if ((needsChoice || permission) && !checking && !ownerUnavailable) {
      if (roots.length > 1) {
        const rootChoice = select([['', 'Choose a saved originals folder…'], ...roots.map(root => [root.id, sourceLocation(root)])], rootInfo?.id || '');
        rootChoice.addEventListener('change', () => { view.sourceRoots.set(source.id, rootChoice.value); renderSourcesFresh().catch(error => showProblem(error.message)); });
        connection.append(field('Saved originals folder', rootChoice));
      }
      const chooseFolder = button(permission ? 'Allow folder access…' : 'Choose originals folder…', () => locate(source, rootInfo), 'primary');
      chooseFolder.classList.add('pfp-choose-originals'); chooseFolder.disabled = roots.length > 1 && !rootInfo;
      connection.append(row(chooseFolder));
    } else if ((selected || identityConflict) && (identityConflict || conflicts > 0) && !unavailable) {
      connection.append(row(button('Review identity conflicts', () => {
        if (roots.length > 1 && !rootInfo) throw Error('Select the affected saved folder in Source options first.');
        return connectionPlan(source, rootInfo);
      })));
    }
    const reviewHost = el('div', 'pfp-connection-review'); connection.append(reviewHost);
    const checked = rootInfo?.observationChecked ?? source.observationChecked;
    const total = rootInfo?.observationTotal ?? source.observationTotal;
    if (checking && Number.isFinite(total) && total > 0 && Number.isFinite(checked)) appendSummary(connection, [['Locations checked', `${count(Math.min(total, Math.max(0, checked)))} of ${count(total)}`]]);
    const available = Number.isFinite(rootInfo?.available) ? rootInfo.available : source.available;
    appendSummary(connection, [['Photographs', count(rootInfo?.photoCount ?? source.photoCount)], [roots.length > 1 && !Number.isFinite(rootInfo?.available) ? 'Available in source' : 'Available', count(available)],
      ['Missing at checked locations', count(missing)], needsChoice ? ['Identity check', 'After folder selection'] : checking && !(conflicts > 0) ? ['Identity check', 'In progress'] : ['Identity conflicts', count(conflicts)]]);
    nativeAccessStatus(connection, source, roots, rootInfo, permission && !checking && !ownerUnavailable);
    return connection;
  }

  async function renderSources(preservedName = null) {
    const generation = view.generation;
    const add = button('Add photo folder…', async () => {
      const grant = await choose('folder'); if (!grant || generation !== view.generation) return;
      await command('addFolders', { folderGrants: [grant.grantId] }, 'Folder added. Its originals check and metadata work appear in Jobs.'); await renderSourcesFresh();
    }, sources().length ? '' : 'primary');
    const libraryActions = disclosure('Add or import photos', row(add, button('Import library backup…', inspectBackup), button('Attach source archive…', inspectArchive)),
      el('p', 'pfp-muted', 'Import a whole .photofindrybackup folder to replace this library, with a recovery backup kept first. Attach a .photofindrysource.zip file to add a source alongside the existing gallery.'));
    if (!sources().length) {
      libraryActions.open = true; body.append(card('No photo folders yet', 'Add a photo folder or import your saved library to begin.'), libraryActions); return;
    }
    if (!sources().some((source) => source.id === view.sourceId)) view.sourceId = sources()[0].id;
    const chooser = sourceSelect(view.sourceId, false); chooser.addEventListener('change', () => { view.sourceId = chooser.value; view.sourceOffset = 0; view.recoveryJobId = null; renderSourcesFresh().catch((error) => showProblem(error.message)); });
    body.append(field('Source', chooser));
    const source = sources().find((item) => item.id === view.sourceId);
    const roots = list(source.roots);
    const summary = card(source.label, automaticSource(source) ? 'Folder availability is managed automatically.' : text(source.statusLabel, text(source.status)));
    if (source.mode === 'archived') {
      summary.append(notice('This source is archived and is not part of the searchable library. Attach its verified archive to bring it back; an originals folder alone does not restore its saved work.'),
        button('Inspect & attach archive…', inspectArchive));
      for (const rootInfo of roots) summary.append(el('p', 'pfp-muted', `Archived location: ${sourceLocation(rootInfo)}`));
      summary.append(disclosure('Archive options', button('Archive QR label…', () => showArchiveLabel(source))));
      if (sources().some(item => item.mode !== 'archived')) libraryActions.append(button('Remove source from gallery…', removeSourceFromGallery));
      body.append(summary, libraryActions, sourceArchiveLabels()); return;
    }
    const rootInfo = roots.find(item => item.id === view.sourceRoots.get(source.id)) || (roots.length === 1 ? roots[0] : null);
    if (rootInfo) view.sourceRoots.set(source.id, rootInfo.id);
    const savedCheck = view.sourceChecks.get(source.id);
    const check = savedCheck?.rootId === rootInfo?.id ? savedCheck : null;
    const checkJob = check?.jobId ? jobs().find(job => job.id === check.jobId && job.sourceId === source.id && (!check.rootId || !job.sourceRootId || job.sourceRootId === check.rootId)) : null;
    if (['confirming', 'confirmationFailed'].includes(check?.phase) && checkJob) check.phase = terminal(checkJob)
      ? checkJob.state === 'completed' ? 'confirmed' : 'confirmationFailed' : 'confirming';
    const latestRootCheck = rootInfo ? jobs().filter(job => job.kind === 'sourceRecovery' && job.sourceId === source.id && (job.sourceRootId === rootInfo.id || !job.sourceRootId && job.sourceAction === 'confirmLocations')
      && !job.background && [undefined, 'check', 'locate', 'confirmLocations'].includes(job.sourceAction))
      .sort((a, b) => text(b.updatedAt, text(b.createdAt)).localeCompare(text(a.updatedAt, text(a.createdAt))) || text(b.id).localeCompare(text(a.id)))[0] : null;
    const activeRootCheck = [checkJob, latestRootCheck].find(job => job && !terminal(job));
    const checkedFolder = latestRootCheck?.sourceAction === 'locate' && latestRootCheck.state === 'completed' && latestRootCheck.sourceVerifyContents !== true
      && (source.needsReview > 0 || source.mode === 'metadata' || source.accessOriginals === false);
    if (automaticSource(source)) summary.append(automaticSourceConnection(source, rootInfo, check));
    else {
    const needsConnection = source.mode === 'metadata' || source.accessOriginals === false || ['offline', 'permission', 'unavailable', 'needsReview', 'changed', 'unchecked', 'missing'].includes(source.status)
      || [source.needsReview, source.offline, source.missing, source.unchecked].some(value => value > 0);
    const confirming = check?.phase === 'confirming' || activeRootCheck?.sourceAction === 'confirmLocations';
    const connection = card(confirming ? 'Connecting originals folder' : (needsConnection && check?.phase !== 'confirmed' || check?.plan) ? 'Confirm this Mac’s originals folder' : 'Originals folder',
      confirming ? 'Publishing the locations you confirmed. This finishes automatically; saved photographs and tags stay available.' : needsConnection && check?.phase !== 'confirmed' ? 'Choose the originals folder, then confirm the matching files.' : 'Your saved photographs and their original folder stay linked.');
    if (roots.length > 1) {
      const rootChoice = select([['', 'Choose a saved originals folder…'], ...roots.map(item => [item.id, sourceLocation(item)])], rootInfo?.id || '');
      rootChoice.addEventListener('change', () => { view.sourceRoots.set(source.id, rootChoice.value); renderSourcesFresh().catch(error => showProblem(error.message)); });
      connection.append(field('Saved originals folder', rootChoice));
    }
    if (rootInfo) appendSummary(connection, [['Saved folder location', sourceLocation(rootInfo)]]);
    else if (roots.length > 1) connection.append(notice('Select which saved folder you are reconnecting, then choose its matching location on this Mac.'));
    else appendSummary(connection, [['Saved folder location', 'Not selected']]);
    if (check && (!rootInfo || check.rootId === rootInfo.id)) {
      if (check.chosenLocationLabel) appendSummary(connection, [['Chosen on this Mac', check.chosenLocationLabel]]);
      if (check.phase === 'confirmed') connection.append(notice('Folder confirmation finished. Browse this source to see your photographs. Any remaining unavailable locations are shown below.'));
      else if (check.phase === 'confirmationFailed') connection.append(notice(checkJob?.state === 'cancelled'
        ? 'Connection stopped. Saved photographs and tags are kept. Choose the originals folder again when you are ready to reconnect it.'
        : 'Connecting the folder did not finish. Use Retry or Resume below to finish this job; your saved photographs and tags are kept.', true));
      else if (confirming) { const progress = notice(checkJob?.state === 'paused' ? 'Connection is paused. Resume the connection job below to finish.' : text(checkJob?.message, 'Connecting the confirmed locations. Progress and Stop are available below.')); progress.classList.add('pfp-connection-progress'); progress.dataset.jobId = check?.jobId || activeRootCheck?.id || ''; connection.append(progress); }
      else if (!check.plan && (!checkJob || ['queued', 'running', 'pausing'].includes(checkJob.state))) connection.append(notice(check.verifyContents
        ? 'Comparing complete original file contents against saved checksums. This optional comparison reads the photographs and can take time on a cloud or external drive.'
        : 'Checking the chosen folder’s locations, sizes and saved dates. The matching locations will appear here for your confirmation. Photo contents are not read.'));
      if (checkJob?.state === 'cancelled') connection.append(notice('Folder check stopped. Saved photographs and tags are kept.'));
    }
    if (!check && activeRootCheck) connection.append(notice(activeRootCheck.sourceVerifyContents === true
      ? 'The optional full-file comparison is in progress. It reads complete original contents and can take time on this drive.'
      : 'The folder check is in progress. Its matching locations will be ready to review here after it finishes.'));
    const chooseFolder = button('Choose originals folder…', () => locate(source, rootInfo), needsConnection || check ? 'primary' : '');
    chooseFolder.classList.add('pfp-choose-originals');
    chooseFolder.disabled = (roots.length > 1 && !rootInfo) || Boolean(activeRootCheck) || confirming;
    if (checkedFolder && !check?.plan && !check?.phase) {
      chooseFolder.className = chooseFolder.className.replace(/\bprimary\b/g, '');
      const review = button('Review checked folder…', () => connectionPlan(source, rootInfo), 'primary'); review.classList.add('pfp-review-checked');
      connection.append(notice('The folder check finished. Review its matching locations, then confirm that these are the same originals.'), row(review, chooseFolder));
    } else connection.append(row(chooseFolder));
    const reviewHost = el('div', 'pfp-connection-review'); connection.append(reviewHost);
    summary.append(connection);
    }
    const browsing = row(button('Browse this source', async () => { close(); await bridge().setScope({ sourceId: source.id, scopeId: null }); }));
    summary.append(browsing);
    if (!automaticSource(source)) appendSummary(summary, [['Photographs', count(source.photoCount)], ['Available', count(source.available)], ['Missing', count(source.missing)], ['Need review', count(source.needsReview)]]);
    const label = preservedName?.sourceId === source.id ? preservedName.input : input(view.sourceDrafts.get(source.id) ?? source.label);
    if (!label.dataset.sourceName) {
      label.dataset.sourceName = source.id;label.classList.add('pfp-source-name');
      label.addEventListener('input', () => { if (label.value === sources().find(item => item.id === source.id)?.label) view.sourceDrafts.delete(source.id); else view.sourceDrafts.set(source.id, label.value); });
    }
    const sourceOptions = disclosure('Source options', field('Source name', label), row(button('Save name', async () => { if (!label.value.trim()) throw Error('Enter a source name.'); await command('renameSource', { sourceId: source.id, label: label.value.trim() }); view.sourceDrafts.delete(source.id); await renderSourcesFresh(); }),
      button(automaticSource(source) ? 'Check now' : 'Check locations', async () => { await command('checkSources', { sourceId: source.id, automatic: false }, 'Checking saved locations.'); await renderSourcesFresh(); }),
      ...(!automaticSource(source) || sourceConflict(source) ? [button('Review originals', () => { if (roots.length > 1 && !rootInfo) throw Error('Select the saved originals folder above first.'); return connectionPlan(source, rootInfo); })] : []), button('Review missing', () => show('cleanup', { sourceId: source.id })),
      button('Find new photographs', async () => { await command('discoverFolders', { recursive: true }, 'Finding new photographs in registered folders. This checks file attributes and adds pending records; it does not start tagging.'); await renderSourcesFresh(); })));
    for (const item of roots) sourceOptions.append(el('p', 'pfp-muted', sourceLocation(item)),
      button('Show folder in Finder', () => command('revealSource', { sourceId: source.id, rootId: item.id })));
    if (automaticSource(source)) {
      if (roots.length > 1) {
        const rootChoice = select([['', 'Choose a saved originals folder…'], ...roots.map(root => [root.id, sourceLocation(root)])], rootInfo?.id || '');
        rootChoice.addEventListener('change', () => { view.sourceRoots.set(source.id, rootChoice.value); renderSourcesFresh().catch(error => showProblem(error.message)); });
        sourceOptions.append(field('Folder to change or review', rootChoice));
      }
      sourceOptions.append(button('Change originals folder…', () => {
        if (roots.length > 1 && !rootInfo) throw Error('Select a saved originals folder first.');
        return locate(source, rootInfo);
      }));
    }
    sourceOptions.append(row(button('Export source archive…', () => archivePlan(source, false)), button('Archive & disconnect…', () => archivePlan(source, true)),
      button('Remove source from gallery…', removeSourceFromGallery), button('Archive QR label…', () => showArchiveLabel(source))));
    if (snapshot().sourceTransfer?.phase === 'cleanupPending') sourceOptions.append(button('Retry interrupted transfer', () => command('retrySourceTransfer')));
    if (source.mode !== 'metadata' && source.accessOriginals !== false) sourceOptions.append(button('Read camera & location metadata', () => command('enqueueJob', { kind: 'metadata', params: { sourceId: source.id, force: true } }, 'Metadata read queued for this source. Originals are never changed.')));
    if (view.recoveryJobId && (!automaticSource(source) || sourceConflict(source) || source.status === 'permission')) {
      const id = view.recoveryJobId;
      summary.append(button('Recheck originals for the selected job', async () => {
        const result = await command('recoverJobOriginals', { jobId: id }, 'The job’s original-file recovery is being checked. Its completed work is preserved.');
        if (result.sourceId) view.sourceId = result.sourceId;
        if (result.sourceId && result.rootId) view.sourceRoots.set(result.sourceId, result.rootId);
        await renderSourcesFresh();
      }));
    }
    body.append(summary);
    for (const job of jobs().filter(job => job.sourceId === source.id && ['sourceRecovery', 'discovery', 'metadata'].includes(job.kind) && !['completed', 'cancelled'].includes(job.state)).slice(0, 8)) {
      const host = el('div', 'pfp-source-job'); host.dataset.jobId = job.id; host.dataset.signature = JSON.stringify(job); host.append(jobCard(job)); body.append(host);
    }
    body.append(sourceOptions, libraryActions, sourceArchiveLabels());
    if (preservedName?.input === label && preservedName.focused) label.focus({ preventScroll: true });
    if (!automaticSource(source) && check && !check.phase && check.rootId === rootInfo?.id && (check.plan || checkJob?.state === 'completed')) await connectionPlan(source, rootInfo, check.plan);
    const detail = await read('sourceDetails', { sourceId: source.id, offset: view.sourceOffset, limit: 100 });
    if (generation !== view.generation || view.name !== 'sources' || source.id !== view.sourceId) return;
    if (detail.canUndo) sourceOptions.append(button('Undo last source relocation', async () => {
      if (await confirmation('Undo source relocation?', ['Restore the preceding saved original locations. Saved tags and human names are preserved.'], 'Undo relocation')) { await command('undoSourceRelink', { sourceId: source.id }); view.sourceChecks.delete(source.id); await renderSourcesFresh(); }
    }));
    if (list(detail.candidates).length) {
      const candidates = disclosure('Review individual original locations', el('p', 'pfp-muted', 'Review uncertain candidates individually. Matching folder names alone do not prove that a photograph is the same original.'));
      for (const group of detail.candidates) {
        const item = card(text(group.filename, 'Saved photograph'));
        for (const candidate of list(group.candidates)) item.append(row(el('span', '', `${text(candidate.locationLabel, text(candidate.label, text(candidate.pathLabel, 'Possible original')))} · ${text(candidate.reason)}`),
          ...(source.mode !== 'metadata' && source.accessOriginals !== false ? [button('Preview candidate', () => previewSourceCandidate(source.id, group.photoId, candidate.id, item)),
          button('Show candidate in Finder', () => command('revealSourceCandidate', { sourceId: source.id, photoId: group.photoId, candidateId: candidate.id }))] : []),
          button('Confirm this original', async () => {
            if (!await confirmation('Confirm this original?', [`Use the reviewed candidate for ${text(group.filename, 'this photograph')}? Confirm only if it is the same original. Saved evidence is preserved.`, text(candidate.locationLabel, text(candidate.label, text(candidate.pathLabel)))], 'Confirm original')) return;
            await command('confirmSourceMatch', { sourceId: source.id, photoId: group.photoId, candidateId: candidate.id }); await renderSourcesFresh();
          })));
        candidates.append(item);
      }
      pager(candidates, detail.offset || 0, detail.totalCount || 0, 100, async (offset) => { view.sourceOffset = offset; await renderSourcesFresh(); }, detail.candidates.length); body.append(candidates);
    }
  }
  function sourceArchiveLabels() {
    const archiveID = input('', 128); archiveID.placeholder = 'Full archive UUID or PhotoFindry QR link';
    const labels = disclosure('Open an archive label', row(field('Open a saved archive label', archiveID), button('Open saved source', () => openArchiveCode(archiveID.value)),
      button('Read QR image…', () => scanArchiveCode('image')), button('Scan with camera…', () => scanArchiveCode('camera'))));
    // Removal remains available when every source is archived, with its native
    // active-source selection and refusal contract unchanged.
    if (sources().every(source => source.mode === 'archived')) labels.append(button('Remove source from gallery…', removeSourceFromGallery));
    return labels;
  }
  async function renderSourcesFresh(preserveName = false) {
    if (view.name !== 'sources') return;
    const input = preserveName ? body.querySelector('.pfp-source-name') : null;
    const name = input ? {input, sourceId: input.dataset.sourceName, focused: document.activeElement === input} : null;
    view.generation += 1; clearContent(); await renderSources(name);
  }
  async function previewSourceCandidate(sourceId, photoId, candidateId, parent) {
    const generation = view.generation;
    const blob = await bridge().api('/api/source-candidate-preview', { sourceId, photoId, candidateId }, { binary: true, signal: view.controller.signal });
    if (generation !== view.generation || view.name !== 'sources' || !parent.isConnected) return;
    const url = URL.createObjectURL(blob); view.urls.add(url);
    const preview = el('div', 'pfp-candidate-preview'); const image = el('img', 'pfp-preview'); image.alt = 'Candidate original — confirm only if it is the same photograph'; image.src = url;
    preview.append(image, button('Close candidate preview', () => { URL.revokeObjectURL(url); view.urls.delete(url); preview.remove(); }));
    parent.append(preview);
  }
  async function openArchiveCode(value) {
    const match = /^(?:photofindry:\/\/archive\/v1\/)?([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$/.exec(value.trim());
    if (!match) throw Error('Enter a complete archive UUID or an exact PhotoFindry archive QR link.');
    const id = match[1].toLowerCase();
    const source = sources().find((source) => source.id.toLowerCase() === id);
    if (!source) throw Error('This library does not contain that source. Attach its archive first; scanning a label does not download a catalog or connect a disk.');
    await show('sources', { sourceId: source.id });
  }
  async function scanArchiveCode(mode) {
    const result = await command('sourceLabelScan', { mode });
    if (!result.cancelled) await openArchiveCode(text(result.payload, text(result.sourceId)));
  }
  async function showArchiveLabel(source) {
    const result = await read('sourceLabel', { sourceId: source.id, operation: 'show' });
    if (result.sourceId !== source.id || typeof result.pngBase64 !== 'string' || result.pngBase64.length > 350000) throw Error('The native label renderer returned an invalid label.');
    const bytes = Uint8Array.from(atob(result.pngBase64), (character) => character.charCodeAt(0));
    const url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }));
    const image = el('img', 'pfp-qr-image'); image.src = url; image.alt = `Archive QR label for ${source.label}`;
    const content = [image, el('h3', '', source.label), el('p', 'pfp-code', source.id.toUpperCase()),
      el('p', '', 'The QR contains only this archive ID. It works with a library that already includes this source. It contains no photographs or filenames and grants no access to a drive.'),
      row(button('Print label…', () => command('sourceLabel', { sourceId: source.id, operation: 'print' })), button('Save label as PDF…', () => command('sourceLabel', { sourceId: source.id, operation: 'savePDF' })))];
    try { await confirmation('Archive label', content, 'Done'); } finally { URL.revokeObjectURL(url); }
  }
  async function locate(source, rootInfo, verify = false) {
    const generation = view.generation;
    const recoveryJobId = view.recoveryJobId;
    // Starting a new choice supersedes every preceding plan before any native
    // picker or queue submission can wait. An old acknowledgement has no effect.
    const check = { rootId: rootInfo?.id, previousLocationLabel: sourceLocation(rootInfo), chosenLocationLabel: '', jobId: null, verifyContents: verify };
    view.sourceChecks.set(source.id, check);
    body.querySelector('.pfp-originals-review')?.remove(); view.confirmation?.close();
    const current = () => generation === view.generation && view.name === 'sources' && source.id === view.sourceId && view.sourceChecks.get(source.id) === check;
    const cancelChoice = async () => { if (current()) { view.sourceChecks.delete(source.id); await renderSourcesFresh(); } };
    if (verify && !await confirmation('Compare complete original files?', ['This optional check reads complete original contents and compares saved checksums. It can take a long time on an external or network drive. Ordinary folder confirmation does not need it.'], 'Choose folder & compare')) { await cancelChoice(); return; }
    if (!current()) return;
    const grant = await choose('sourceFolder'); if (!grant || !current()) { await cancelChoice(); return; }
    check.chosenLocationLabel = text(grant.locationLabel, text(grant.label));
    try {
      const result = await command('locateSource', { sourceId: source.id, ...(rootInfo?.id ? { rootId: rootInfo.id } : {}), folderGrant: grant.grantId, ...(verify ? { verifyContents: true } : {}), ...(recoveryJobId ? { resumeJobId: recoveryJobId } : {}) }, verify ? 'Full-file comparison queued.' : automaticSource(source) ? 'Folder selected. Matching originals connect automatically; availability and file changes are checked in the background.' : 'Checking the chosen originals folder. Review and confirm its matching locations here when the check finishes.');
      check.jobId = result?.submittedJob?.id;
      check.jobKind = result?.submittedJob?.kind;
      if (check.jobKind === 'sourceSelection') {
        const responseSource = list(result?.sources).find(item => item.id === source.id);
        const responseRoot = list(responseSource?.roots).find(item => item.id === check.rootId);
        check.selectionObserved = responseRoot ? responseRoot.selected === true || responseRoot.selectionPending === true
          : responseSource?.originalsFolderSelected === true || responseSource?.folderSelectionPending === true;
      }
      if (automaticSource(source) && !check.jobId) throw Error('The background folder check was not returned. Saved photographs and tags are kept; choose the folder again to retry.');
      if (!current()) return;
      await renderSourcesFresh();
    } catch (error) {
      if (view.sourceChecks.get(source.id) === check) view.sourceChecks.delete(source.id);
      if (generation !== view.generation || view.name !== 'sources' || source.id !== view.sourceId) return;
      // A failed new choice must restore the primary folder action after the
      // superseded review was removed; keep the original failure for display.
      await renderSourcesFresh().catch(() => {}); throw error;
    }
  }
  async function connectionPlan(source, rootInfo, existingPlan = null) {
    const generation = view.generation;
    const initialCheck = view.sourceChecks.get(source.id);
    const isCurrent = () => generation === view.generation && view.name === 'sources' && source.id === view.sourceId && view.sourceChecks.get(source.id) === initialCheck;
    let plan = existingPlan;
    if (!plan) {
      // A previous read can still be draining after navigation or cancellation.
      // Wait only for this read-only operation; never retry a confirmation.
      const preparing = card('Preparing matching locations'); preparing.classList.add('pfp-originals-review');
      const progress = el('p', 'pfp-note', 'Checking this folder’s matching locations. The result will appear here automatically. Photo contents are not read.');
      let cancelled = false;
      const cancel = button('Cancel review', async () => {
        cancelled = true; await command('cancelSourceConnectionReview', {}, 'Stopping the folder review. Saved photographs and tags are kept.');
        if (isCurrent()) { view.sourceChecks.delete(source.id); await renderSourcesFresh(); }
      });
      preparing.append(progress, row(cancel));
      body.querySelector('.pfp-originals-review')?.remove(); (body.querySelector('.pfp-connection-review') || body).append(preparing);
      for (let attempt = 0; attempt < 6 && isCurrent() && !cancelled; attempt += 1) {
        try { plan = await read('prepareSourceConnection', { sourceId: source.id, ...(rootInfo?.id ? { rootId: rootInfo.id } : {}) }); break; }
        catch (error) {
          if (!isCurrent() || cancelled || error?.name === 'AbortError') return;
          const busy = /folder review is still finishing|source review worker is busy/i.test(error?.message || '')
            || snapshot().storageJob?.kind === 'sourceReview' && snapshot().storageJob?.running === true && /already running/i.test(error?.message || '');
          if (busy && attempt < 5) {
            progress.textContent = 'Finishing the previous folder review. This review will continue automatically. You can cancel it below.';
            await new Promise(resolve => setTimeout(resolve, 500)); continue;
          }
          progress.textContent = busy ? 'The previous folder review is taking longer. Cancel review to stop it, then retry the checked folder here.'
            : `The folder review could not finish. ${text(error?.message)} Retry the checked folder or choose another folder.`;
          preparing.append(row(button('Retry folder review', () => connectionPlan(source, rootInfo)), button('Choose another folder…', () => locate(source, rootInfo))));
          return;
        }
      }
      if (!isCurrent() || cancelled || !plan) return;
      preparing.remove();
    }
    if (!isCurrent()) return;
    root.querySelector('.pfp-problem')?.remove();
    const pendingCheck = initialCheck && initialCheck.rootId === rootInfo?.id ? initialCheck : { rootId: rootInfo?.id, verifyContents: false };
    pendingCheck.plan = plan; view.sourceChecks.set(source.id, pendingCheck);
    const currentReview = () => generation === view.generation && view.name === 'sources' && source.id === view.sourceId
      && view.sourceChecks.get(source.id) === pendingCheck && pendingCheck.plan?.planId === plan.planId;
    const chooseOriginals = body.querySelector('.pfp-choose-originals'); if (chooseOriginals) chooseOriginals.hidden = true;
    const reviewChecked = body.querySelector('.pfp-review-checked'); if (reviewChecked) reviewChecked.hidden = true;
    const previous = text(plan.previousRootLocationLabel, text(plan.previousRootPath, text(plan.previousRootLabel, pendingCheck?.previousLocationLabel || sourceLocation(rootInfo))));
    const current = text(plan.rootLocationLabel, text(plan.rootPath, text(plan.rootLabel, pendingCheck?.chosenLocationLabel || sourceLocation(rootInfo))));
    const matches = Number.isFinite(plan.count) && plan.count > 0;
    const content = [
      `Saved folder: ${previous}`,
      `On this Mac: ${current}`,
      `${count(plan.count)} matching locations · ${count(plan.unresolved)} unresolved.`,
      'The quick check compares relative paths, file sizes and saved modification dates without reading photo contents. Confirm only if this is the same originals folder. This is not a checksum verification.',
    ];
    const node = card(matches ? 'Confirm the matching originals' : 'No matching originals in this folder');
    node.classList.add('pfp-originals-review');
    appendSummary(node, [['Saved folder', previous], ['On this Mac', current], ['Matching photographs', count(plan.count)], ['Other locations in this source still unresolved', count(plan.unresolved)]]);
    if (matches) node.append(el('p', 'pfp-muted', content[3]));
    else node.append(notice('No matching files. Choose the correct originals folder.', true));
    if (list(plan.examples).length) node.append(disclosure('Example matching locations', ...list(plan.examples).slice(0, 8).map(example => el('p', 'pfp-muted', text(example.relativePath, text(example.name, 'Saved photograph'))))));
    const confirm = button('Confirm matching locations', async () => {
      if (!matches || !currentReview()) return;
      if (!await confirmation('Confirm matching original locations?', content, `Confirm ${count(plan.count)} locations`, { acknowledgement: 'I confirm this is the same originals folder.' })) return;
      if (!currentReview()) return;
      // Retain the exact submitted job until it finishes. A policy flip during
      // its commit must refresh this panel, not close it or re-offer old proof.
      pendingCheck.phase = 'confirming'; pendingCheck.jobId = null;
      pendingCheck.chosenLocationLabel = current; delete pendingCheck.plan;
      await renderSourcesFresh();
      try {
        const result = await command('confirmSourceConnection', { sourceId: plan.sourceId, planId: plan.planId, confirmed: true, ...(view.recoveryJobId ? { resumeJobId: view.recoveryJobId } : {}) }, 'Connecting the original locations you confirmed.');
        if (view.sourceChecks.get(source.id) !== pendingCheck) return;
        pendingCheck.jobId = result?.submittedJob?.id;
        if (!pendingCheck.jobId) { view.sourceChecks.delete(source.id); throw Error('The connection job was not returned. Review the folder again before confirming.'); }
        if (view.name === 'sources' && source.id === view.sourceId) await renderSourcesFresh();
      } catch (error) {
        if (view.sourceChecks.get(source.id) === pendingCheck) { view.sourceChecks.delete(source.id); if (view.name === 'sources' && source.id === view.sourceId) await renderSourcesFresh(); }
        throw error;
      }
    }, matches ? 'primary' : '');
    confirm.disabled = !matches;
    node.append(row(matches ? confirm : button('Choose originals folder…', () => locate(source, rootInfo), 'primary'),
      matches ? button('Choose another folder…', () => locate(source, rootInfo)) : confirm,
      button('Cancel review', async () => { await command('cancelSourceConnectionReview'); view.sourceChecks.delete(source.id); node.remove(); if (chooseOriginals) chooseOriginals.hidden = false; if (reviewChecked) reviewChecked.hidden = false; })));
    if (plan.verificationCount > 0) node.append(disclosure('Compare file contents (optional)', el('p', 'pfp-muted', 'This reads complete original files with saved checksums. It is not needed for folder confirmation.'), button('Optional full-file comparison…', () => locate(source, rootInfo, true))));
    body.querySelector('.pfp-originals-review')?.remove();
    const host = body.querySelector('.pfp-connection-review'); (host || body).append(node);
    node.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  async function renderPeople() {
    if (view.photoId) return renderPhotoPeople();
    const generation = view.generation;
    body.append(notice('Names and corrections are yours. Only faces you confirm become reference faces for confident automatic naming; uncertain matches remain for review.'));
    const people = list(snapshot().people?.people);
    if (view.personId && !people.some((person) => person.id === view.personId)) view.personId = '';
    const chooser = select([['', 'Unnamed faces'], ...people.map((person) => [person.id, `${person.name} · ${count(person.count)} photos`])], view.personId);
    chooser.addEventListener('change', () => { view.personId = chooser.value; view.peopleOffset = 0; renderPeopleFresh().catch((error) => showProblem(error.message)); });
    const toolbar = row(field('Person', chooser), button('Refresh people', () => renderPeopleFresh()), button('Undo last people edit', async () => { await command('editPerson', { undo: true }); await renderPeopleFresh(); }));
    toolbar.lastChild.disabled = snapshot().people?.canUndo !== true; body.append(toolbar);
    const discovery = card('Face discovery', text(snapshot().faceJob?.message, 'Automatic discovery covers new and retagged photographs.'));
    if (snapshot().faceCoverage) appendSummary(discovery, [['Scanned', count(snapshot().faceCoverage.scanned)], ['Pending', count(snapshot().faceCoverage.pending)], ['Need attention', count(snapshot().faceCoverage.failed)]]);
    discovery.append(row(button(snapshot().facePreferences?.enabled === false ? 'Resume automatic discovery' : 'Pause automatic discovery', async () => {
      if (snapshot().facePreferences?.enabled === false) await command('configureFaceScan', { enabled: true }); else await command('cancelFaces'); await renderPeopleFresh();
    }), button('Scan selected photos', async () => { const ids = selected(); if (!ids.length) throw Error('Select photographs in the gallery first.'); await command('enqueueJob', { kind: 'faces', params: { photoIds: ids } }, faceScanNotice('Face scan queued for the selected photographs.')); }),
    button('Scan whole library…', async () => { if (await confirmation('Scan faces in the whole library?', ['This explicitly reads available originals throughout the library, including imported archives. Unavailable originals stay deferred.'], 'Start face scan')) await command('configureFaceScan', { enabled: true, scope: 'library' }, 'Library face discovery enabled.'); })));
    const preferences = snapshot().facePreferences || {};
    const folderChoices = list(snapshot().folders).filter((folder) => folder.scopeId)
      .map((folder) => [folder.scopeId, text(folder.name, text(folder.pathLabel, 'Saved folder'))]);
    const savedFolders = list(preferences.scopeIds).filter((id) => typeof id === 'string' && id);
    const savedFolderChoice = preferences.scope === 'folders' && savedFolders.length === 1
      && folderChoices.some(([id]) => id === savedFolders[0]) ? savedFolders[0] : 'saved-folders';
    const currentScope = preferences.scope === 'library' ? 'library' : preferences.scope === 'folders' ? savedFolderChoice : 'new';
    const savedNames = savedFolders.slice(0, 3).map((id) => folderChoices.find(([candidate]) => candidate === id)?.[1] || 'unavailable folder');
    const savedLabel = `Current saved folder scope · ${count(savedFolders.length)} ${savedFolders.length === 1 ? 'folder' : 'folders'}${savedNames.length ? ` · ${savedNames.join(', ').slice(0, 100)}` : ''}`;
    const choices = [['new', 'New and retagged photos'], ['library', 'Whole library'],
      ...(currentScope === 'saved-folders' ? [['saved-folders', savedLabel]] : []),
      ...folderChoices];
    const discoveryScope = select(choices, currentScope);
    const applyScope = button('Apply discovery scope', async () => {
      const choice = discoveryScope.value;
      if (choice === currentScope || choice === 'saved-folders') return;
      const selectedLabel = choices.find(([id]) => id === choice)?.[1] || 'selected scope';
      if (!await confirmation('Change automatic face discovery scope?', [
        `Replace the current ${preferences.scope === 'folders' ? 'saved folder' : preferences.scope === 'library' ? 'whole-library' : 'new-photo'} scope with ${selectedLabel}?`,
        'This authorizes reading available originals in the selected scope. Completed scans are reused; unavailable originals remain deferred.',
      ], 'Apply scope')) return;
      await command('configureFaceScan', { enabled: true, scope: ['new', 'library'].includes(choice) ? choice : 'folders',
        scopeIds: ['new', 'library'].includes(choice) ? [] : [choice] }, 'Face discovery scope saved.');
    });
    applyScope.disabled = true;
    discoveryScope.addEventListener('change', () => { applyScope.disabled = discoveryScope.value === currentScope || discoveryScope.value === 'saved-folders'; });
    discovery.append(row(field('Automatic discovery scope', discoveryScope), applyScope));
    if (selected().length === 1) discovery.append(button('Add a person label to the selected photograph…', nameSelectedPhoto));
    body.append(discovery);
    const person = people.find((item) => item.id === view.personId);
    if (person) body.append(row(button('Show in library', async () => { close(); await bridge().searchLibrary(person.name); }), button('Edit name & aliases…', () => editPerson(person)), button('Review possible matches', () => showSuggestions(person, false)), button('Review broader matches', () => showSuggestions(person, true))));
    const pageState = await read('browsePeople', { personId: view.personId || null, offset: view.peopleOffset });
    if (generation !== view.generation || view.name !== 'people') return;
    const page = pageState.peoplePage || {};
    const pageIds = new Set(list(page.photoIds));
    const photos = list(pageState.peoplePhotos || pageState.photos).filter((photo) => pageIds.has(photo.id));
    const faces = list(pageState.people?.faces);
    if (!person) body.append(button('Show groups of similar unnamed faces', () => showFaceGroups(faces)));
    const grid = el('div', 'pfp-photo-grid');
    if (person) {
      for (const photo of photos) {
        const node = card(text(photo.name, 'Photograph')); preview(photo.id, node);
        node.append(photoButton(photo, 'Open photograph'));
        for (const face of list(photo.faces).length ? photo.faces : faces.filter((face) => face.photoId === photo.id)) {
          if (face.personId !== person.id) continue;
          node.append(row(el('span', 'pfp-muted', face.assignmentSource === 'automatic' ? 'Confident automatic match' : 'User-confirmed name'), button(`Not ${person.name}`, async () => {
            if (!await confirmation('Correct this face name?', [`Remove ${person.name} from this particular face and exclude this match? Other people in the photograph stay unchanged.`], 'Correct this face')) return;
            await command('editPerson', { personId: person.id, faceId: face.id, reject: true }); await renderPeopleFresh();
          })));
        }
        grid.append(node);
      }
    } else {
      const unnamed = faces.filter((face) => !face.personId && pageIds.has(face.photoId));
      for (const face of unnamed) {
        const photo = photos.find((photo) => photo.id === face.photoId);
        const node = card(text(photo?.name, 'Unnamed face')); preview(face.photoId, node, face.id, 'Saved face crop');
        node.append(row(button('Name this face…', () => nameFace(face, photo)), button('Open photograph', () => bridge().showPhoto(face.photoId)))); grid.append(node);
      }
      if (!unnamed.length) grid.append(card('No unnamed faces on this page', 'Run face discovery for the photos you want to name. Saved names remain independent of factual tags.'));
    }
    if (person && !photos.length) grid.append(card('No photographs on this page', 'Refresh the person or choose another name.'));
    body.append(grid); pager(body, page.offset || 0, page.total || 0, page.limit || 240, async (offset) => { view.peopleOffset = offset; await renderPeopleFresh(); }, person ? photos.length : Math.min(page.limit || 240, Math.max(0, (page.total || 0) - (page.offset || 0))));
  }
  async function renderPeopleFresh() { if (view.name !== 'people') return; view.generation += 1; clearContent(); await renderPeople(); }
  async function renderPhotoPeople() {
    const generation = view.generation, photoId = view.photoId;
    const photo = await bridge().api('/api/photo', { photoId }, { signal: view.controller.signal });
    if (generation !== view.generation || view.name !== 'people' || view.photoId !== photoId) return;
    if (photo?.id !== photoId) throw Error('This photograph changed. Open its People controls again.');
    body.append(card(`People in ${text(photo.name, 'this photograph')}`, 'Face names and age labels are your observations. Confirming a face creates a human reference; model appearance estimates never establish identity.'));
    const undo = button('Undo last people edit', async () => { await command('editPerson', { undo: true }); await renderPeopleFresh(); }); undo.disabled = snapshot().people?.canUndo !== true;
    const controls = row(button('All people', () => show('people')), button('Open photograph', () => bridge().showPhoto(photoId)), undo,
      button('Add person label to this photograph…', () => nameSelectedPhoto(photoId)));
    if (photo.source?.accessOriginals !== false) controls.append(button('Scan faces in this photograph', () => command('enqueueJob', { kind: 'faces', params: { photoIds: [photoId] } }, faceScanNotice('Face scan queued for this photograph.'))));
    body.append(controls);
    if (list(photo.personNames).length) body.append(notice(`Saved names: ${photo.personNames.join(', ')}`));
    const grid = el('div', 'pfp-photo-grid');
    for (const face of list(photo.faces)) {
      const person = list(snapshot().people?.people).find(item => item.id === face.personId);
      const node = card(text(person?.name, text(face.name, face.personId ? 'Named face' : 'Unnamed face')));
      preview(photoId, node, face.id, 'Saved face crop');
      node.append(el('p', 'pfp-muted', [face.assignmentSource === 'automatic' ? 'Confident automatic match' : face.personId ? 'User-confirmed name' : 'Awaiting your confirmation', face.ageBand ? `Your age label: ${face.ageBand}` : ''].filter(Boolean).join(' · ')));
      node.append(button(face.personId ? 'Change this face’s name or age…' : 'Name this face…', () => nameFace(face, photo)));
      if (face.personId) node.append(button(`Not ${text(person?.name, 'this person')}`, async () => {
        if (!await confirmation('Correct this face name?', ['Remove this name from this particular face and exclude this match. Other faces and photo labels remain unchanged.'], 'Correct this face')) return;
        await checkedFace(face);
        await command('editPerson', { personId: face.personId, faceId: face.id, reject: true }); await renderPeopleFresh();
      }));
      grid.append(node);
    }
    if (!list(photo.faces).length) grid.append(card('No saved faces', 'Scan this photograph when its original is available, or add a photograph-level person label without claiming a face-recognition reference.'));
    body.append(grid);
  }
  function sameFace(actual, expected) { return actual && actual.id === expected.id && actual.photoId === expected.photoId && actual.fingerprint === expected.fingerprint && actual.model === expected.model && JSON.stringify(actual.box) === JSON.stringify(expected.box); }
  async function checkedFace(face, unassigned = false) {
    const photo = await bridge().api('/api/photo', { photoId: face.photoId });
    const actual = list(photo.faces).find((item) => item.id === face.id);
    if (!sameFace(actual, face) || text(actual.personId) !== text(face.personId) || (unassigned && actual.personId)) throw Error('This face changed since the page was opened. Refresh People before confirming it.');
    return actual;
  }
  async function nameFace(face, photo) {
    const name = input(); const existing = select([['', 'Create a new name'], ...list(snapshot().people?.people).map((person) => [person.id, person.name])], face.personId || ''); name.disabled = Boolean(existing.value);
    const age = select([['', 'Keep existing age label'], ['unknown', 'Unknown'], ['infant', 'Infant'], ['child', 'Child'], ['teen', 'Teen'], ['young adult', 'Young adult'], ['adult', 'Adult'], ['older adult', 'Older adult']], face.ageBand || '');
    const content = [el('p', '', `Name only the selected face in ${text(photo?.name, 'this photograph')}. This confirms a reference for future confident matches.`), field('Existing person', existing), field('New name', name), field('Your age label (optional)', age)];
    existing.addEventListener('change', () => { name.disabled = !!existing.value; });
    if (!await confirmation('Name this face', content, 'Save confirmed name')) return;
    if (!existing.value && !name.value.trim()) throw Error('Enter a name or choose an existing person.');
    await checkedFace(face);
    await command('assignPerson', { faceIds: [face.id], name: name.value.trim(), personId: existing.value || null, ageBand: age.value || null }, 'Confirmed name saved. Quality-qualified matches can be named automatically.');
    await renderPeopleFresh();
  }
  async function nameSelectedPhoto(explicitPhotoId) {
    const ids = typeof explicitPhotoId === 'string' ? [explicitPhotoId] : selected(); if (ids.length !== 1) throw Error('Select one photograph in the gallery first.');
    const photo = await bridge().api('/api/photo', { photoId: ids[0] });
    const name = input(); const existing = select([['', 'Create a new name'], ...list(snapshot().people?.people).map((person) => [person.id, person.name])]);
    const age = select([['', 'Keep existing age label'], ['unknown', 'Unknown'], ['infant', 'Infant'], ['child', 'Child'], ['teen', 'Teen'], ['young adult', 'Young adult'], ['adult', 'Adult'], ['older adult', 'Older adult']]);
    existing.addEventListener('change', () => { name.disabled = !!existing.value; });
    if (!await confirmation('Add a person to this photograph', [text(photo.name, 'Selected photograph'), field('Existing person', existing), field('New name', name), field('Your age label (optional)', age), 'This is your photograph-level label. It does not identify a particular detected face or create a face-recognition reference. Use Name this face for a confirmed face reference.'], 'Save person label')) return;
    if (!existing.value && !name.value.trim()) throw Error('Enter a name or choose an existing person.');
    await command('assignPerson', { photoId: ids[0], name: name.value.trim(), personId: existing.value || null, ageBand: age.value || null }, 'Your person label is saved.'); await renderPeopleFresh();
  }
  async function showFaceGroups(pageFaces) {
    const generation = view.generation, raw = await read('peopleGroups');
    if (generation !== view.generation) return;
    const region = card('Similar unnamed faces', 'These groups are suggestions. Open each photograph and choose the individual face to confirm its name. Similarity alone never creates a reference.');
    let shown = 0;
    for (const group of list(raw)) {
      const faces = list(group.faceIds).map((id) => pageFaces.find((face) => face.id === id && !face.personId)).filter(Boolean);
      if (!faces.length) continue;
      const groupCard = card(`${count(faces.length)} similar faces on this page`), grid = el('div', 'pfp-photo-grid');
      for (const face of faces) {
        const tile = card(); preview(face.photoId, tile, face.id, 'Unconfirmed similar face');
        tile.append(row(button('Name this face…', () => nameFace(face)), button('Open photograph', () => bridge().showPhoto(face.photoId)))); grid.append(tile);
      }
      groupCard.append(grid); region.append(groupCard); shown += faces.length;
    }
    if (!shown) region.append(el('p', 'pfp-muted', 'No groups are available on this page. Individual unnamed faces can still be named.'));
    region.append(button('Close groups', () => region.remove())); body.prepend(region);
  }
  async function editPerson(person) {
    const name = input(person.name); const aliases = input(list(person.aliases).join(', '), 1000);
    const merge = select([['', 'Keep this person separate'], ...list(snapshot().people?.people).filter((item) => item.id !== person.id).map((item) => [item.id, `Merge into ${item.name}`])]);
    if (!await confirmation('Edit person', [field('Name', name), field('Aliases, separated by commas', aliases), field('Merge duplicate person', merge), 'Merging combines the selected people. Manual corrections and undo history are retained.'], 'Save changes')) return;
    if (!merge.value && !name.value.trim()) throw Error('A person needs a name.');
    await command('editPerson', merge.value ? { personId: person.id, mergeInto: merge.value } : { personId: person.id, name: name.value.trim(), aliases: aliases.value.split(',').map((value) => value.trim()).filter(Boolean) }); await renderPeopleFresh();
  }
  async function showSuggestions(person, broad) {
    const generation = view.generation; const raw = await read('peopleSuggestions', { personId: person.id, broad });
    if (generation !== view.generation) return;
    const suggestions = list(raw).filter((face) => !face.personId).slice(0, 30);
    const region = card(`Possible matches for ${person.name}`, 'These are suggestions, not confirmed identities. Select only faces you can identify. At most 30 candidates are shown at a time.');
    const ids = new Set(), checkboxes = new Map(); const grid = el('div', 'pfp-photo-grid');
    for (const face of suggestions) {
      const node = card('Possible match'); preview(face.photoId, node, face.id, `Possible ${person.name} match`);
      const choice = checkbox('Select this face'); checkboxes.set(face.id, choice.control); choice.control.addEventListener('change', () => { if (choice.control.checked) ids.add(face.id); else ids.delete(face.id); });
      node.append(choice.node, button('Open photograph', () => bridge().showPhoto(face.photoId))); if (face.reviewReason) node.append(el('p', 'pfp-muted', face.reviewReason)); grid.append(node);
    }
    region.append(grid);
    const decide = async (decision) => {
      const chosen = suggestions.filter((face) => ids.has(face.id)); if (!chosen.length) throw Error('Select the candidate faces to review.');
      if (!await confirmation(decision === 'confirm' ? 'Confirm these reference faces?' : 'Exclude these matches?', [`${count(chosen.length)} selected faces ${decision === 'confirm' ? `will be confirmed as ${person.name}` : `will be excluded from ${person.name}`}. Other faces remain unchanged.`], decision === 'confirm' ? 'Confirm selected' : 'Exclude selected')) return;
      for (const face of chosen) await checkedFace(face, true);
      await command('reviewPeople', { personId: person.id, faceIds: chosen.map((face) => face.id), decision }); await renderPeopleFresh(); await showSuggestions(person, broad);
    };
    if (!suggestions.length) region.append(el('p', '', 'No unconfirmed candidates are available.'));
    else region.append(row(button('Select all shown', () => { for (const face of suggestions) { ids.add(face.id); checkboxes.get(face.id).checked = true; } }), button('Clear selection', () => { ids.clear(); for (const box of checkboxes.values()) box.checked = false; }), button('Confirm selected', () => decide('confirm'), 'primary'), button('Not this person', () => decide('exclude'))));
    region.append(button('Close possible matches', () => region.remove())); body.prepend(region);
  }

  function releaseCleanup() {
    view.cleanupReady = false;
    view.cleanupLife = view.cleanupLife.catch(() => {}).then(() => bridge().api('/api/action', { action: 'endCleanupSession', params: {} })).catch(() => {});
  }
  async function acquireCleanup(generation) {
    if (view.cleanupReady) return true;
    view.cleanupLife = view.cleanupLife.catch(() => {}).then(async () => {
      if (generation !== view.generation || view.name !== 'cleanup') return false;
      let result = await bridge().api('/api/action', { action: 'beginCleanupSession', params: {} });
      while (result.active && !result.ready && generation === view.generation && view.name === 'cleanup') {
        await new Promise((resolve) => setTimeout(resolve, 250));
        result = await bridge().api('/api/action', { action: 'cleanupSession', params: {} });
      }
      if (generation !== view.generation || view.name !== 'cleanup') return false;
      if (!result.active || !result.ready) throw Error('Processing could not pause for cleanup. Close this panel and try again.');
      view.cleanupReady = true; return true;
    });
    return view.cleanupLife;
  }
  async function renderCleanup(offset = 0) {
    const generation = view.generation;
    if (list(snapshot().jobs?.purgeRecovery).length) {
      body.append(notice('Permanent deletion has interrupted work to finish. The saved deletion plan remains authoritative; open Jobs to finish its recovery before starting new cleanup.', true), button('Open cleanup recovery in Jobs', () => { close(); window.PhotoFindryWorkspace?.openJobs?.(); }));
    }
    body.append(notice('Cleanup changes library records, never original files. A saved preview may be your only remaining image; save it before permanent deletion.', true));
    const chooser = sourceSelect(view.cleanupSource); chooser.addEventListener('change', () => { view.cleanupSource = chooser.value; view.cleanupPlan = null; view.cleanupSelected.clear(); renderCleanupFresh().catch((error) => showProblem(error.message)); });
    body.append(row(field('Scope', chooser), button('Review missing', async () => { view.cleanupTab = 'review'; view.cleanupSelected.clear(); await renderCleanupFresh(); }, view.cleanupTab === 'review' ? 'primary' : ''), button('Removed from library', async () => { view.cleanupTab = 'removed'; view.cleanupSelected.clear(); await renderCleanupFresh(); }, view.cleanupTab === 'removed' ? 'primary' : '')));
    const waiting = notice('Pausing processing safely before opening the cleanup review…'); body.append(waiting);
    if (!await acquireCleanup(generation)) return; waiting.remove();
    const scope = view.cleanupSource ? { sourceId: view.cleanupSource } : {};
    const isRemoved = view.cleanupTab === 'removed';
    if (!isRemoved) body.append(row(button('Prepare fresh review', async () => {
      const prepared = await command('prepareCleanup', scope, 'Missing-original review queued. Nothing is removed by preparing a review.');
      view.cleanupPlan = prepared.planId; view.cleanupSelected.clear(); await renderCleanupFresh();
    }, 'primary'), button('Refresh review', () => renderCleanupFresh(offset))));
    else body.append(button('Refresh Removed', () => renderCleanupFresh(offset)));
    const page = await read(isRemoved ? 'cleanupRemoved' : 'cleanupReview', { ...scope, ...(view.cleanupPlan && !isRemoved ? { planId: view.cleanupPlan } : {}), offset, limit: 100 });
    if (generation !== view.generation || view.name !== 'cleanup') return;
    view.cleanupPage = page; if (!isRemoved) view.cleanupPlan = page.plan?.id || view.cleanupPlan;
    const plan = page.plan;
    const canApply = isRemoved || (plan?.status === 'ready' && (!plan.expiresAt || Date.parse(plan.expiresAt) > Date.now()));
    if (plan) {
      const progress = card(`Review: ${text(plan.status)}`); appendSummary(progress, [['Checked', `${count(plan.completed)} / ${count(plan.total)}`], ['Eligible missing originals', count(page.eligibleCount ?? plan.eligible)], ['Protected entries', count(plan.excluded)]]);
      for (const [reason, amount] of Object.entries(plan.reasons || {})) if (amount) progress.append(el('p', 'pfp-muted', `${reason.replace(/([A-Z])/g, ' $1')}: ${count(amount)}`));
      body.append(progress);
    }
    const related = jobs().filter((job) => job.kind === 'libraryCleanup' && (!view.cleanupSource || job.sourceId === view.cleanupSource) && !['completed', 'cancelled'].includes(job.state));
    for (const job of related.slice(0, 6)) body.append(jobCard(job));
    if (!list(page.items).length) body.append(card(isRemoved ? 'No removed entries' : 'No confirmed missing originals to display', isRemoved ? 'Removed entries can be restored here until you explicitly delete them permanently.' : 'Only missing originals from a complete, available source check qualify. Offline, cloud-only, unverified and permission-blocked originals stay in the library. Prepare or refresh a review when its source is available.'));
    const selectionLabel = el('span', 'pfp-muted'); const updateSelection = () => { selectionLabel.textContent = `${count(view.cleanupSelected.size)} selected`; };
    updateSelection(); const controls = row(selectionLabel, button('Select eligible page', () => { for (const item of list(page.items)) if (item.eligible !== false && view.cleanupSelected.size < 1000) view.cleanupSelected.add(item.photo.id); updateBoxes(); }), button('Clear selection', () => { view.cleanupSelected.clear(); updateBoxes(); }));
    const grid = el('div', 'pfp-photo-grid'); const boxes = [];
    const updateBoxes = () => { for (const [id, box] of boxes) box.checked = view.cleanupSelected.has(id); updateSelection(); };
    for (const item of list(page.items)) {
      const photo = item.photo; if (!photo?.id) continue;
      const node = card(text(photo.name, 'Saved photograph')); preview(photo.id, node);
      const choice = checkbox('Select entry', view.cleanupSelected.has(photo.id)); choice.control.disabled = item.eligible === false;
      if (item.eligible === false) view.cleanupSelected.delete(photo.id);
      boxes.push([photo.id, choice.control]);
      choice.control.addEventListener('click', (event) => {
        if (event.shiftKey && view.cleanupAnchor) {
          const eligible = list(page.items).filter((entry) => entry.eligible !== false).map((entry) => entry.photo.id);
          const a = eligible.indexOf(view.cleanupAnchor), b = eligible.indexOf(photo.id);
          if (a >= 0 && b >= 0) for (const id of eligible.slice(Math.min(a, b), Math.max(a, b) + 1)) if (view.cleanupSelected.size < 1000) view.cleanupSelected.add(id);
        } else if (choice.control.checked && view.cleanupSelected.size < 1000) view.cleanupSelected.add(photo.id); else view.cleanupSelected.delete(photo.id);
        view.cleanupAnchor = photo.id; updateBoxes();
      });
      node.append(choice.node, el('p', 'pfp-muted', text(item.reason)), button('Save cached preview…', () => downloadPreview(photo)));
      if (!isRemoved) node.append(photoButton(photo, 'Open saved photograph'));
      grid.append(node);
    }
    const explicitIDs = () => { const ids = [...view.cleanupSelected]; if (!ids.length) throw Error('Choose the entries first.'); if (ids.length > 1000) throw Error('Choose at most 1,000 entries per operation.'); return ids; };
    if (isRemoved) {
      controls.append(button('Restore selected', async () => { const ids = explicitIDs(); await command('restoreLibraryEntries', { photoIds: ids, confirmed: true }, 'Restoration queued.'); view.cleanupSelected.clear(); await renderCleanupFresh(offset); await bridge().refreshLibrary(); }, 'primary'),
        button('Delete selected permanently…', () => deleteRemoved(explicitIDs()), 'danger'), button('Empty Removed…', () => deleteRemoved(null), 'danger'));
    } else {
      const remove = button('Move selected to Removed…', async () => { const ids = explicitIDs(); await removeEntries({ planId: plan.id, photoIds: ids, confirmed: true }, ids.length); }, 'danger'); remove.disabled = !canApply;
      const removeAll = button('Move all reviewed missing entries…', () => removeEntries({ planId: plan.id, all: true, expectedCount: page.eligibleCount, confirmed: true }, page.eligibleCount), 'danger'); removeAll.disabled = !canApply || !page.eligibleCount;
      controls.append(remove, removeAll);
    }
    if (view.cleanupLastJob) controls.append(button('Undo last completed removal', async () => {
      const result = await read('cleanupRemovalResult', { jobId: view.cleanupLastJob });
      const amount = result.removedCount ?? result.removed ?? result.count ?? 0;
      if (!amount) throw Error('No completed removal is available to undo yet. Refresh after the job finishes.');
      await command('restoreLibraryEntries', { removalJobId: view.cleanupLastJob, expectedCount: amount, confirmed: true }, 'Undo queued for the records actually removed.'); await renderCleanupFresh();
    }));
    body.append(controls, grid); pager(body, page.offset || 0, page.totalCount || 0, 100, renderCleanupFresh, list(page.items).length); updateSelection();
  }
  async function renderCleanupFresh(offset = 0) { if (view.name !== 'cleanup') return; view.generation += 1; clearContent(); await renderCleanup(offset); }
  async function removeEntries(params, amount) {
    if (!amount) throw Error('No eligible missing entries remain. Refresh the review.');
    const generation = view.generation;
    if (!await confirmation(`Move ${count(amount)} entries to Removed?`, ['These entries, tags and saved previews will disappear from active gallery and search. You can restore them later. Original files are untouched.', 'A preview may be your only remaining image. Changed, returned or unverified originals are kept.'], 'Move to Removed', { destructive: true })) return;
    if (generation !== view.generation || params.planId !== view.cleanupPlan) throw Error('The review changed. Prepare the selection again.');
    const result = await command('removeLibraryEntries', params, 'Approved removal queued. Saved records remain recoverable in Removed.');
    view.cleanupLastJob = result.id || result.jobId || result.submittedJob?.id || null; view.cleanupSelected.clear(); await renderCleanupFresh(); await bridge().refreshLibrary();
  }
  async function deleteRemoved(ids) {
    const source = view.cleanupSource, generation = view.generation;
    const preparation = await read('prepareRemovedDeletion', { ...(source ? { sourceId: source } : {}), ...(ids ? { photoIds: ids } : {}) });
    if (generation !== view.generation || view.cleanupTab !== 'removed') return;
    if (!preparation.count) throw Error('No removed records remain in that selection.');
    if (!await confirmation(`Permanently delete ${count(preparation.count)} saved entries?`, [
      'This deletes the approved removed entries, their saved tagging data, people information and unshared cached previews. It cannot be undone from Removed. Original photographs are never deleted.',
      'A retained preview may be your only image. Save it first. Shared or unsafe files are retained and reported. Existing exported backups are unchanged.',
    ], 'Delete permanently', { destructive: true, acknowledgement: 'I understand this permanently deletes these saved library entries.' })) return;
    if (generation !== view.generation || source !== view.cleanupSource || view.cleanupTab !== 'removed' || (preparation.expiresAt && Date.parse(preparation.expiresAt) <= Date.now())) throw Error('The frozen deletion review changed or expired. Review it again.');
    await command('deleteRemovedEntries', { deletionId: preparation.id, confirmed: true }, 'The approved permanent-deletion job is queued.'); view.cleanupSelected.clear(); await renderCleanupFresh();
  }
  async function downloadPreview(photo) {
    const blob = await bridge().previewBlob(photo.id, view.controller.signal);
    const url = URL.createObjectURL(blob), link = el('a');
    link.href = url; link.download = `${text(photo.name, 'saved-preview').replace(/\.[^.]*$/, '').replace(/[\\/:*?"<>|]/g, '_')}-saved-preview.${blob.type === 'image/png' ? 'png' : 'jpg'}`;
    document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 60000);
    bridge().showNotice('Saved preview offered as a new download. It is a reduced-size cached copy, not the original.');
  }

  function renderPortability() {
    const backup = card('Portable library backup', 'Import the whole .photofindrybackup folder exported by PhotoFindry, not a file inside it. Legacy .photorecallbackup folders are accepted too. Import replaces this library after verification and keeps a recovery backup of the current library first.');
    backup.append(el('p', 'pfp-muted', 'Backups include saved evidence, people, corrections and review history. Original photographs and model weights are excluded. Host model settings stay on this Mac.'));
    const images = checkbox('Include available cached previews', true); backup.append(images.node, row(button('Export library backup…', async () => {
      const destination = await choose('backupSave'); if (!destination) return;
      await command('exportLibrary', { destinationGrant: destination.grantId, includePreviews: images.control.checked }, 'Library export started. The verified package is published atomically.');
    }, 'primary'), button('Import library backup…', inspectBackup)));
    const archive = card('Source archives', 'Choose a .photofindrysource.zip file to add one source and its saved work alongside the existing gallery. Whole-library .photofindrybackup folders use Import library backup instead. Exported ZIPs are not synchronized automatically.');
    const source = sourceSelect(view.sourceId, false); archive.append(field('Source to export', source), row(button('Export source…', () => { const selectedSource = sources().find((item) => item.id === source.value); if (!selectedSource) throw Error('Choose a source first.'); return archivePlan(selectedSource, false); }),
      button('Archive & disconnect…', () => { const selectedSource = sources().find((item) => item.id === source.value); if (!selectedSource) throw Error('Choose a source first.'); return archivePlan(selectedSource, true); }), button('Inspect & attach archive…', inspectArchive)));
    body.append(backup, archive, notice('Restore verifies and stages a package before replacing the current library, preserving a rollback backup. Imported progress remains paused until you explicitly start it.'));
  }
  async function inspectBackup() {
    const generation = view.generation;
    const current = () => generation === view.generation;
    const grant = await choose('backupOpen'); if (!grant || !current()) return;
    const details = await read('inspectBackup', { sourceGrant: grant.grantId });
    if (!current()) return;
    if (!await confirmation('Restore this library backup?', [
      `${grant.label || 'Selected backup'} · ${count(details.photos)} photos · ${count(details.people)} people · ${size(details.bytes)}`,
      `Created ${date(details.createdAt)}. ${details.includePreviews ? 'Includes cached previews.' : 'Saved metadata only.'}`,
      'Restore replaces the current library after validating and staging the package. The current library is backed up for recovery. Original photographs, installed models and host model settings are preserved.',
    ], 'Restore verified backup', { destructive: true, acknowledgement: 'I want to replace this library with the inspected backup.' })) return;
    if (!current()) return;
    const result = await command('restoreLibrary', { sourceGrant: grant.grantId, confirmReplace: true }, 'Restore approved. Follow its progress here and in Jobs.');
    if (result.restartRequired) bridge().showNotice('The verified restore was staged. The companion reconnects to the restored library automatically.');
    await bridge().refreshLibrary();
  }
  async function inspectArchive() {
    const generation = view.generation;
    const current = () => generation === view.generation;
    const grant = await choose('archiveOpen'); if (!grant || !current()) return;
    const details = await read('inspectSourceArchive', { sourceGrant: grant.grantId });
    if (!current()) return;
    if (!await confirmation('Attach this source archive?', [
      `${text(details.label, grant.label)} · ${count(details.photos)} photos · ${count(details.people)} people · ${count(details.faces)} faces`,
      `Created ${date(details.createdAt)}. ${details.includeImageCaches ? 'Includes cached images.' : 'Metadata-only archive.'}`,
      details.alreadyAttached ? 'This source already exists here. The engine will validate compatibility and preserve the existing source identity.' : 'This adds the source and its saved work. Existing sources remain. Original access and unfinished work do not silently resume.',
    ], 'Attach verified source')) return;
    if (!current()) return;
    const result = await command('attachSourceArchive', { sourceGrant: grant.grantId, confirm: true }, 'Source attachment approved.');
    if (result.restartRequired) bridge().showNotice('The verified attachment was staged. The companion reconnects to the updated library automatically.');
    if (view.name === 'sources') await renderSourcesFresh(); await bridge().refreshLibrary();
  }
  async function removeSourceFromGallery() {
    const generation = view.generation;
    const available = sources().filter((source) => source.mode !== 'archived');
    if (!available.length) throw Error('No active source remains in the gallery. Attach an archive or add a photo folder in Files.');
    let id = available.some((source) => source.id === view.sourceId) ? view.sourceId : available[0].id;
    if (available.length > 1) {
      const chooser = select(available.map((source) => [source.id, `${source.label || 'Untitled source'} · ${count(source.photoCount)} photos`]), id);
      if (!await confirmation('Choose a source to remove from the gallery', [field('Source to remove', chooser),
        'A restorable archive is saved and verified before this source leaves the gallery. Other sources and original photographs stay in place.'], 'Review source removal')) return;
      id = chooser.value;
    }
    if (generation !== view.generation) return;
    const source = sources().find((item) => item.id === id && item.mode !== 'archived');
    if (!source || !available.some((item) => item.id === id)) throw Error('The selected source changed. Review it again in Files.');
    await archivePlan(source, true, true);
  }
  async function openSourceRemoval() {
    const opening = show('sources');
    const generation = view.generation;
    await opening;
    if (generation === view.generation && view.name === 'sources' && !root.hidden) await removeSourceFromGallery();
  }
  async function archivePlan(source, disconnect, removeFromGallery = false) {
    if (source.mode === 'archived') throw Error('Attach this source archive before exporting it again.');
    const generation = view.generation;
    const current = () => generation === view.generation && sources().some((item) => item.id === source.id && item.mode !== 'archived');
    if (!current()) return;
    const images = checkbox('Include cached images', true);
    const mode = select([['detach', 'Archive and remove from library'], ['metadata', 'Keep saved details; remove cached images']], 'detach');
    const content = [el('p', '', text(source.label)), disconnect ? field('After the archive is verified', mode) : images.node,
      el('p', '', disconnect ? 'Disconnecting stops original access. Archive mode removes active source records after the verified archive is safe. Saved-details mode keeps evidence searchable and clears only verified unshared cached images.' : 'Export this source snapshot while leaving its active records and cached images in place.'),
      notice('Original photographs are excluded and never changed. A retained preview may be your only remaining image.', disconnect)];
    if (!removeFromGallery && !await confirmation(disconnect ? 'Choose archive & disconnect mode' : 'Choose source export contents', content, 'Prepare export review')) return;
    if (!current()) return;
    const plan = await read('prepareSourceArchive', { sourceId: source.id, mode: disconnect ? mode.value : 'detach', includeImageCaches: disconnect ? mode.value === 'detach' : images.control.checked });
    if (!current()) return;
    const summary = [
      text(source.label, 'Selected source'),
      `${count(plan.photos)} photos · ${count(plan.removedPhotos)} removed records · ${count(plan.people)} people · ${count(plan.faces)} faces`,
      `${plan.includeImageCaches ? `${count(plan.previews)} recorded previews` : 'Cached images excluded'} · ${size(plan.bytes)} recorded preview storage. Final archive size is calculated during export.`,
      `${count(plan.missingImages)} photographs have no recorded preview.`, ...list(plan.warnings),
      disconnect ? 'Only after verifying the archive will PhotoFindry clear proved unshared cached images. Shared or unverified files are retained. A preview may be your only remaining image; keep the archive somewhere safe.' : 'The source remains connected. Subsequent edits are not synchronized into this exported snapshot.',
    ];
    if (removeFromGallery) summary.push('PhotoFindry saves and verifies a restorable source archive before removing this source’s active entries and verified unshared cached copies from the gallery. Other sources and original files are unchanged. Attach the archive in Files to bring this source back.');
    if (!await confirmation(removeFromGallery ? 'Remove this source from the gallery?' : disconnect ? 'Export and disconnect this source?' : 'Export this source archive?', summary,
      removeFromGallery ? 'Save archive & remove source' : disconnect ? 'Choose destination & disconnect' : 'Choose destination & export',
      { destructive: disconnect, acknowledgement: disconnect ? 'I understand which saved images and active records this mode removes.' : null })) return;
    if (!current()) return;
    const destination = await choose('archiveSave'); if (!destination) return;
    if (!current()) return;
    const result = await command('exportSourceArchive', { planId: plan.planId, destinationGrant: destination.grantId, disconnect, confirmClearImages: disconnect }, removeFromGallery ? 'Saving and verifying the archive before removing this source from the gallery.' : disconnect ? 'Verified source export and disconnect requested.' : 'Source export requested.');
    if (result.restartRequired) bridge().showNotice('The source change was staged. The companion reconnects to the updated library automatically.');
    if (view.name === 'sources') await renderSourcesFresh();
  }

  // One reviewer editor is shared by Settings and Improvement. Its picker grants
  // and pending checks belong to the mounted view, never to a later library/view.
  function mountReviewSettings(parent, { signal, isCurrent = () => parent.isConnected } = {}) {
    const current = () => !signal?.aborted && isCurrent();
    const button = (label, operation, tone = '') => {
      const node = el('button', `pfp-button ${tone}`, label); node.type = 'button';
      node.addEventListener('click', async () => {
        if (!current() || node.disabled || !node.isConnected) return;
        node.disabled = true;
        try { await operation(); }
        catch (error) { if (current() && error?.name !== 'AbortError') bridge().showError(error?.message || 'The reviewer settings could not be updated.'); }
        finally { if (current() && node.isConnected) node.disabled = false; }
      });
      return node;
    };
    const settings = snapshot().photoReviewSettings || {};
    const reviewer = card('Photo detail reviewer (decision model)', 'Choose how Improvement checks saved photo details. It asks up to two bounded batches per photo, follows up uncertain counts and relationships, and checks missing object colours. Checked questions are saved; Review again can explore more. Tagging and Local AI Search keep their own models.');
    const provider = select([['tagging-model', 'Current tagging model'], ['clef-flash', 'CLEF Flash'], ['intern-decision-4b', 'Intern-Decision 4B']], settings.provider || 'tagging-model');
    const lowMemory = checkbox('Reduced-memory mode', settings.lowMemoryTrial === true);
    const modelLabel = el('span', 'pfp-muted', text(settings.modelLabel, settings.modelConfigured ? 'Saved model selected' : 'No review model selected'));
    const projectorLabel = el('span', 'pfp-muted', text(settings.projectorLabel, settings.projectorConfigured ? 'Saved adapter selected' : 'No adapter selected'));
    const guidance = el('p', 'pfp-muted'), memoryGuidance = el('p', 'pfp-muted'), checkStatus = el('p', 'pfp-note');
    checkStatus.setAttribute('role', 'status');
    const fileChecks = el('div'), files = el('div');
    let modelGrant = null, projectorGrant = null, checkVersion = 0, checkController = null;
    let physicalMemory = snapshot().monitor?.resources?.physicalMemoryBytes;
    const parameters = () => ({ provider: provider.value, lowMemoryTrial: provider.value === 'intern-decision-4b' && lowMemory.control.checked, ...(modelGrant ? { modelGrant } : {}), ...(projectorGrant ? { projectorGrant } : {}) });
    const updateProvider = () => {
      files.hidden = provider.value === 'tagging-model'; lowMemory.node.hidden = provider.value !== 'intern-decision-4b';
      const intern = provider.value === 'intern-decision-4b';
      guidance.textContent = intern
        ? 'Choose Intern-Decision-4B-Q4_K_M.gguf and its matching mmproj-Intern-Decision-4B-F16.gguf. These are separate from your tagging model. Files remain in your chosen folder.'
        : 'Choose the CLEF Flash 9B Q8 model and its matching vision adapter, or use the verified CLEF Flash pair from Settings → Verified model downloads. No Ollama service is needed.';
      memoryGuidance.textContent = intern
        ? lowMemory.control.checked ? 'Reduced memory: 512-pixel previews and four questions per batch; at least 8 GiB Mac memory. Small details may be harder to judge. The 8 GiB hardware profile still needs qualification; memory protection remains active.' : 'Normal profile: 1024-pixel previews and eight questions per batch; at least 16 GiB Mac memory. Reduced-memory mode is available for 8 GiB Macs.'
        : 'CLEF Flash requires at least 24 GiB Mac memory. Memory protection can defer a review while other work uses too much memory.';
      for (const option of provider.children) option.disabled = Number.isFinite(physicalMemory) && (option.value === 'clef-flash' && physicalMemory < 24 * 1024 ** 3 || option.value === 'intern-decision-4b' && physicalMemory < 8 * 1024 ** 3);
    };
    async function checkReviewer() {
      if (!current()) return null;
      const version = ++checkVersion, changes = parameters();
      checkController?.abort(); checkController = new AbortController();
      const controller = checkController;
      checkStatus.textContent = 'Checking local reviewer files… No model is being started.';
      try {
        const report = await bridge().api('/api/review-files/check', changes, { signal: controller.signal });
        if (!current() || version !== checkVersion || controller.signal.aborted) return null;
        physicalMemory = report.physicalMemoryBytes ?? physicalMemory; updateProvider();
        fileChecks.replaceChildren(...list(report.files).map(file => el('p', file.status === 'error' ? 'pfp-warning' : 'pfp-muted', `${file.role === 'projector' ? 'Vision adapter' : 'Review model'}: ${text(file.message)}`)));
        checkStatus.textContent = text(report.summary, report.canSave ? 'Quick checks passed. Exact model compatibility is checked when review starts.' : 'Check the highlighted reviewer choices.');
        return { report, changes, version };
      } catch (error) {
        if (!current() || version !== checkVersion || controller.signal.aborted) return null;
        checkStatus.textContent = `File check could not finish: ${error.message}. Saved reviewer settings are unchanged.`;
        return null;
      }
    }
    files.append(row(button('Choose review model…', async () => { const grant = await choose('reviewModel'); if (grant && current()) { modelGrant = grant.grantId; modelLabel.textContent = grant.label || 'Model selected'; await checkReviewer(); } }), modelLabel),
      row(button('Choose matching vision adapter…', async () => { const grant = await choose('reviewProjector'); if (grant && current()) { projectorGrant = grant.grantId; projectorLabel.textContent = grant.label || 'Adapter selected'; await checkReviewer(); } }), projectorLabel), guidance, lowMemory.node, memoryGuidance);
    const providerSource = el('a', 'pfp-link', 'Reviewer source & licence'); providerSource.target = '_blank'; providerSource.rel = 'noopener noreferrer';
    const refreshSource = () => { providerSource.href = provider.value === 'intern-decision-4b' ? 'https://huggingface.co/internlm/Intern-Decision-4B' : 'https://ollama.com/library/clef-flash/9b-q8_0'; };
    files.append(providerSource);
    provider.addEventListener('change', () => { updateProvider(); refreshSource(); void checkReviewer(); });
    lowMemory.control.addEventListener('change', () => { updateProvider(); void checkReviewer(); }); updateProvider(); refreshSource();
    reviewer.append(field('Reviewer', provider), files, fileChecks, checkStatus, row(button('Check reviewer files', () => checkReviewer()), button('Use current tagging model', () => { provider.value = 'tagging-model'; updateProvider(); void checkReviewer(); }), button('Save reviewer settings', async () => {
      const checked = await checkReviewer();
      if (!checked) throw Error('Wait for the local reviewer file check, then save again.');
      if (!checked.report.canSave) throw Error('Fix the highlighted reviewer choices before saving. Your saved settings are unchanged.');
      if (checked.version !== checkVersion || JSON.stringify(parameters()) !== JSON.stringify(checked.changes)) throw Error('The reviewer choices changed during the check. Save the current choices again.');
      if (!current()) return;
      await bridge().action('configurePhotoReview', checked.changes);
      if (current()) bridge().showNotice('Photo detail reviewer settings saved.');
    }, 'primary')));
    const dispose = () => { checkVersion += 1; checkController?.abort(); };
    signal?.addEventListener('abort', dispose, { once: true });
    parent.append(reviewer); void checkReviewer();
    return { element: reviewer, dispose };
  }

  async function renderImprovement() {
    const generation = view.generation;
    body.append(notice('Improvement reviews saved observations locally. Uncertain or conflicting findings are not new proof. Accepted runs and human names remain preserved; the latest eligible review can be undone.'));
    const preferences = snapshot().improvementPreferences || {};
    const automatic = card('Automatic improvement', 'Automatic work covers newly tagged photographs and previously reviewed photos whose evidence changed. It does not silently enqueue the whole legacy archive.');
    const enabled = checkbox('Enable automatic improvement', preferences.enabled !== false);
    const interleave = checkbox('Take bounded turns between tagging batches', preferences.interleave !== false);
    const mode = select([['overnight', 'During an overnight window'], ['now', 'When queued']], preferences.mode || 'overnight');
    const start = input(preferences.start || '22:00'); start.type = 'time';
    const end = input(preferences.end || '07:00'); end.type = 'time';
    const updateTiming = () => { start.disabled = end.disabled = mode.value !== 'overnight'; }; mode.addEventListener('change', updateTiming); updateTiming();
    automatic.append(enabled.node, interleave.node, row(field('Run automatically', mode), field('From', start), field('Until', end)),
      el('p', 'pfp-muted', 'Uses this Mac’s local clock. The companion must remain running and the Mac awake. Paused queues and individually held work stay paused; Run now in Jobs is the explicit schedule override.'),
      button('Save automatic improvement', async () => {
        if (mode.value === 'overnight' && (!/^\d{2}:\d{2}$/.test(start.value) || !/^\d{2}:\d{2}$/.test(end.value) || start.value === end.value)) throw Error('Choose different valid start and end times.');
        await command('configureImprovement', { enabled: enabled.control.checked, interleave: interleave.control.checked, mode: mode.value, start: start.value, end: end.value }, 'Automatic improvement preferences saved.');
      }, 'primary'));
    body.append(automatic);
    mountReviewSettings(body, { signal: view.controller.signal, isCurrent: () => generation === view.generation && view.name === 'improvement' });
    const ids = selected().slice(0, 1000), currentScope = bridge().getScope?.() || {};
    const source = sourceSelect(currentScope.sourceId || '');
    const folder = checkbox('Use the current folder and its subfolders', Boolean(currentScope.scopeId));
    folder.node.hidden = !currentScope.scopeId;
    const selectedFolder = list(snapshot().folders).find(item => item.scopeId === currentScope.scopeId);
    const folderName = text(selectedFolder?.name || selectedFolder?.pathLabel, 'the current folder');
    const updateFolderScope = () => { folder.control.disabled = source.value !== (currentScope.sourceId || ''); };
    source.addEventListener('change', updateFolderScope); updateFolderScope();
    const scopeParams = () => ({ ...(source.value ? { sourceId: source.value } : {}), ...(folder.control.checked && !folder.control.disabled && currentScope.scopeId ? { scopeId: currentScope.scopeId } : {}) });
    const scheduleMode = select([['now', 'Run now'], ['overnight', `Overnight · ${preferences.start || '22:00'}–${preferences.end || '07:00'}`]], 'now');
    const schedule = () => ({ mode: scheduleMode.value, start: preferences.start || '22:00', end: preferences.end || '07:00' });
    const toolbar = card('Start a review', ids.length ? `${count(ids.length)} gallery photographs selected.` : 'Select photographs in the gallery to review individual saved results.');
    toolbar.append(field('Source for scoped work', source), folder.node, currentScope.scopeId ? el('p', 'pfp-muted', `Current folder: ${folderName}. Scoped work includes tagged photos beyond this page and search filters.`) : el('p', 'pfp-muted', 'Scoped work includes tagged photos beyond this page and search filters.'), field('When to review', scheduleMode), row(button('Improve selected photographs', async () => {
      if (!ids.length) throw Error('Select tagged photographs in the gallery first.');
      await command('enqueueJob', { kind: 'improve', params: { photoIds: ids, force: false }, schedule: schedule() }, 'Selected-photo improvement queued.');
    }, 'primary'), button('Review selected again', async () => {
      if (!ids.length) throw Error('Select tagged photographs first.');
      await command('enqueueJob', { kind: 'improve', params: { photoIds: ids, force: true }, schedule: schedule() }, 'A new bounded review is queued.');
    }), button('Screen saved tags in scope', () => command('enqueueJob', { kind: 'reviewScreen', params: scopeParams() }, 'Saved-evidence screening queued. Originals are read only when visual review begins.')),
    button('Improve whole scope…', async () => {
      const params = scopeParams(), frozenSchedule = schedule();
      if (!await confirmation('Improve saved details in this scope?', [`Review tagged photographs in ${params.scopeId ? folderName + ' and its subfolders' : source.value ? text(sources().find((item) => item.id === source.value)?.label, 'the selected source') : 'the whole library'} using the configured local model. This may read available originals. ${frozenSchedule.mode === 'overnight' ? `Runs during ${frozenSchedule.start}–${frozenSchedule.end}.` : 'Runs when the processing lane is available.'} Jobs provides Pause and Stop.`], 'Queue improvement')) return;
      await command('enqueueJob', { kind: 'improve', params: { ...params, force: false }, schedule: frozenSchedule }, 'Scope improvement queued.');
    })));
    body.append(toolbar);
    const liveJobs = el('div'); body.append(liveJobs);
    let liveJobsSignature = '';
    const updateLiveJobs = () => {
      const current = jobs().filter(job => ['improve', 'reviewScreen'].includes(job.kind) && !terminal(job)).slice(0, 6);
      const signature = JSON.stringify([snapshot().jobs?.paused, current]);
      if (signature !== liveJobsSignature) { liveJobsSignature = signature; liveJobs.replaceChildren(...current.map(jobCard)); }
    };
    updateLiveJobs();
    const reviewIds = ids.slice(0, 30);
    if (ids.length > reviewIds.length) body.append(notice(`Showing saved reviews for the first ${reviewIds.length} selected photos. Processing actions above keep the complete ${ids.length}-photo selection.`));
    const reviewRows = new Map(), undoButtons = new Map(), reviewVersions = new Map(), refreshPending = new Set();
    const panelSignal = view.controller.signal;
    let refreshing = false;
    const currentPanel = () => generation === view.generation && view.name === 'improvement' && !panelSignal.aborted;
    const modelWorkRunning = () => jobs().some(job => ['running', 'pausing'].includes(job.state) && ['tagging', 'improve', 'aiSearch'].includes(job.kind)) || snapshot().job?.running === true || snapshot().improvementJob?.running === true || snapshot().aiSearch?.running === true;
    const updateUndoButtons = () => { for (const undo of undoButtons.values()) { undo.disabled = modelWorkRunning(); undo.title = undo.disabled ? 'Pause active model work in Jobs before undoing a review.' : 'Restore the preceding effective evidence; original observations, names and review history remain.'; } };
    const refreshReview = async id => {
      if (!reviewIds.includes(id) || !currentPanel()) return;
      reviewVersions.set(id, (reviewVersions.get(id) || 0) + 1); refreshPending.add(id);
      if (refreshing) return;
      refreshing = true;
      try {
        while (refreshPending.size && currentPanel()) {
          const next = refreshPending.values().next().value; refreshPending.delete(next);
          const version = reviewVersions.get(next);
          try {
            const photo = await bridge().api('/api/photo', { photoId: next }, { signal: panelSignal });
            if (!currentPanel()) return;
            if (version !== reviewVersions.get(next)) continue;
            const node = reviewCard(next, photo), old = reviewRows.get(next);
            if (old) old.replaceWith(node); else body.append(node);
            reviewRows.set(next, node); updateUndoButtons();
          } catch (error) { if (currentPanel() && error.name !== 'AbortError') showProblem(error.message || 'The saved review could not be refreshed.'); }
        }
      } finally { refreshing = false; }
    };
    function reviewCard(id, photo) {
      undoButtons.delete(id);
      const review = photo.improvement, node = card(text(photo.name, 'Saved photograph'));
      node.append(photoButton(photo, 'Open photograph'));
      if (photo.improvementIssue) node.append(notice(photo.improvementIssue, true));
      if (!review) node.append(el('p', 'pfp-muted', 'No saved review yet.'));
      else {
        appendSummary(node, [['Outcome', text(review.summaryState, text(review.outcome, 'Saved review'))], ['Reviewed', date(review.reviewedAt)]]);
        if (Number.isFinite(review.decisionClaimCount)) node.append(el('p', 'pfp-muted', `${count(review.decisionClaimCount)} questions checked this review · accepted tags kept.`));
        if (Number.isFinite(review.decisionCheckedTotal)) node.append(el('p', 'pfp-muted', `${count(review.decisionCheckedTotal)} distinct questions checked for this photo and review profile. ${review.decisionMoreQuestions ? 'Review again can check more.' : 'This bounded review is finished.'}`));
        for (const change of list(review.changes)) {
          const section = card(({ added: 'Added detail', uncertain: 'Uncertain', conflicting: 'Conflicting detail', suggestion: 'Review suggestion' })[change.kind] || 'Saved change');
          if (['uncertain', 'conflicting'].includes(change.kind)) section.append(el('p', 'pfp-muted', 'Flagged for review, not confirmed as a new fact.'));
          if (change.kind === 'suggestion') section.append(el('p', 'pfp-muted', 'Accepted tags are unchanged. Supported suggestions can appear as labeled search hints.'));
          section.append(el('p', '', text(change.text))); if (change.previousText) section.append(el('p', 'pfp-muted', `Previously: ${change.previousText}`)); node.append(section);
        }
        if (review.changeCount > list(review.changes).length) node.append(el('p', 'pfp-muted', `Showing ${count(list(review.changes).length)} of ${count(review.changeCount)} recorded changes.`));
        if (!list(review.changes).length && (review.summaryState === 'unchanged' || ['no_change', 'reviewed', 'supported'].includes(review.outcome))) node.append(el('p', 'pfp-muted', 'Reviewed with no changes to accepted details.'));
        if (list(review.decisionNotes).length && !list(review.changes).some(change => change.kind === 'suggestion')) { const hints = card('Review suggestions', 'These may inform labeled search hints. They do not replace accepted tags or establish confirmed facts.'); for (const note of review.decisionNotes) hints.append(el('p', '', note)); node.append(hints); }
        for (const note of list(review.notes)) node.append(el('p', 'pfp-muted', note));
        if (review.canUndo) { const undo = button('Undo latest review', async () => {
          if (!await confirmation('Undo this review?', ['Restore the preceding effective evidence while preserving original observations, human names and review history. Pause active model work in Jobs first.'], 'Undo review')) return;
          if (modelWorkRunning()) throw Error('Pause active model work in Jobs before undoing this review.');
          await command('undoImprovement', { photoId: id }, 'Review undone. Saved original observations are preserved.'); await refreshReview(id); await bridge().refreshLibrary();
        });
          undoButtons.set(id, undo); node.append(undo);
        }
      }
      node.append(button('Review this photo again', () => command('enqueueJob', { kind: 'improve', params: { photoIds: [id], force: true } }, 'Photo review queued.'))); return node;
    }
    for (const id of reviewIds) { await refreshReview(id); if (!currentPanel()) return; }
    let previousStates = new Map(jobs().map(job => [job.id, job.state]));
    let committedStory = JSON.stringify([snapshot().improvementStory?.jobID, snapshot().improvementStory?.sequence]);
    window.addEventListener('photofindry:state', () => {
      if (!currentPanel()) return;
      updateLiveJobs(); updateUndoButtons();
      const finished = jobs().some(job => job.kind === 'improve' && terminal(job) && previousStates.has(job.id) && previousStates.get(job.id) !== job.state);
      previousStates = new Map(jobs().map(job => [job.id, job.state]));
      const story = snapshot().improvementStory, identity = JSON.stringify([story?.jobID, story?.sequence]);
      if (identity !== committedStory) {
        committedStory = identity;
        const id = story?.lastCompleted?.photoID || story?.lastCompleted?.photoId;
        if (id && jobs().some(job => job.id === (story.jobID || story.jobId) && job.kind === 'improve')) void refreshReview(id);
      }
      if (finished) for (const id of reviewIds) void refreshReview(id);
    }, { signal: panelSignal });
  }

  function scheduleSourceRefresh() {
    clearTimeout(view.refreshTimer);
    view.refreshTimer = setTimeout(() => {
      if (view.name !== 'sources' || root?.hidden) { view.sourceRefreshNeeded = false; return; }
      if (view.pending || view.confirmation || view.sourceDrafts.has(view.sourceId)) return;
      view.sourceRefreshNeeded = false;
      renderSourcesFresh().catch(error => showProblem(error.message));
    }, 200);
  }
  function onState(value) {
    if (!value || typeof value !== 'object') return;
    const imagePolicy = state => JSON.stringify(list(state.sources).map(source => [source.id, source.mode, source.accessOriginals]).sort((a,b)=>String(a[0]).localeCompare(String(b[0]))));
    const hadPolicy = Array.isArray(view.snapshot.sources);
    const changedImagePolicy = hadPolicy && imagePolicy(view.snapshot) !== imagePolicy(value);
    const previousSource = list(view.snapshot.sources).find(source => source.id === view.sourceId);
    const oldJobs = new Map(list(view.snapshot.jobs?.jobs).map((job) => [job.id, job.state])); view.snapshot = value;
    if (changedImagePolicy && ['sources', 'people', 'cleanup'].includes(view.name)) {
      // Cached pixels must be withdrawn even while a confirmation or draft is
      // active. Saved source-name drafts and owned recovery jobs remain local.
      if (view.name === 'sources') {
        // Withdraw pixels and stale acknowledgements immediately, then keep the
        // selected source and its owned confirmation job visible in fresh state.
        for (const check of view.sourceChecks.values()) delete check.plan;
        view.confirmation?.close();
        renderStatus();
        renderSourcesFresh(true).catch(error => showProblem(error.message));
        return;
      }
      close(); bridge().showNotice('Source image access changed. Reopen the panel to review its current state.'); return;
    }
    renderStatus();
    if (view.name === 'sources') {
      // Metric ticks update only the job controls, keeping folder choices,
      // acknowledgements and candidate images stable.
      for (const host of body.querySelectorAll('.pfp-source-job')) {
        const job = list(value.jobs?.jobs).find(job => job.id === host.dataset.jobId);
        if (job && JSON.stringify(job) !== host.dataset.signature) { host.dataset.signature = JSON.stringify(job); host.replaceChildren(jobCard(job)); }
      }
      for (const progress of body.querySelectorAll('.pfp-connection-progress')) {
        const job = list(value.jobs?.jobs).find(job => job.id === progress.dataset.jobId);
        if (job && !terminal(job)) progress.textContent = job.state === 'paused' ? 'Connection is paused. Resume the connection job below to finish.' : text(job.message, 'Connecting the confirmed locations. Progress and Stop are available below.');
      }
      const changed = list(value.jobs?.jobs).some(job => job.kind === 'sourceRecovery' && job.sourceId === view.sourceId && oldJobs.get(job.id) !== job.state);
      const sourceSettled = !list(value.jobs?.jobs).some(job => job.kind === 'sourceRecovery' && job.sourceId === view.sourceId && !terminal(job));
      const currentSource = list(value.sources).find(source => source.id === view.sourceId);
      const sourceChanged = previousSource && JSON.stringify(previousSource) !== JSON.stringify(currentSource);
      if (sourceChanged && automaticSource(currentSource) && !view.pending && !view.confirmation) {
        // Background observations must stay visible while a source-name draft
        // is being edited. Replace only the availability card, never the input.
        const host = body.querySelector('.pfp-automatic-source');
        if (host) {
          const roots = list(currentSource.roots);
          const selectedRoot = roots.find(item => item.id === view.sourceRoots.get(currentSource.id)) || (roots.length === 1 ? roots[0] : null);
          const saved = view.sourceChecks.get(currentSource.id);
          const check = saved?.rootId === selectedRoot?.id ? saved : null;
          host.replaceWith(automaticSourceConnection(currentSource, selectedRoot, check));
        }
      }
      if (changed || sourceSettled && sourceChanged) { view.sourceRefreshNeeded = true; scheduleSourceRefresh(); }
    }
    if (!view.name || view.pending || view.confirmation) return;
    const changed = list(value.jobs?.jobs).some((job) => terminal(job) && oldJobs.has(job.id) && oldJobs.get(job.id) !== job.state);
    if (changed && ['cleanup', 'people'].includes(view.name)) {
      clearTimeout(view.refreshTimer);
      view.refreshTimer = setTimeout(() => {
        if (view.pending || view.confirmation || root?.hidden || (view.name === 'sources' && view.sourceDrafts.has(view.sourceId)) || root?.contains(document.activeElement) && ['INPUT', 'SELECT', 'TEXTAREA'].includes(document.activeElement?.tagName)) return;
        const refresh = { sources: renderSourcesFresh, cleanup: () => renderCleanupFresh(view.cleanupPage?.offset || 0), people: renderPeopleFresh }[view.name];
        refresh?.().catch((error) => showProblem(error.message));
      }, 200);
    }
  }
  window.addEventListener('photofindry:state', (event) => onState(event.detail));
  window.addEventListener('photofindry:disconnect', close);
  window.addEventListener('photofindry:session', (event) => { if (event.detail?.connected === false) { close(); view.sourceDrafts.clear(); view.sourceRoots.clear(); view.sourceChecks.clear(); view.snapshot = {}; view.cleanupSelected.clear(); } });
  window.addEventListener('photofindry:library-restored', () => {
    close(); view.sourceDrafts.clear(); view.sourceRoots.clear(); view.sourceChecks.clear(); view.cleanupSelected.clear(); view.snapshot = {};
    view.sourceId = ''; view.personId = ''; view.photoId = ''; view.peopleOffset = 0; view.sourceOffset = 0;
    view.cleanupSource = ''; view.cleanupPlan = null; view.cleanupPage = null;
    view.cleanupLastJob = null; view.cleanupLastCount = 0; view.cleanupAnchor = null; view.recoveryJobId = null;
  });
  window.addEventListener('pagehide', () => { view.controller?.abort(); for (const url of view.urls) URL.revokeObjectURL(url); });
  window.PhotoFindryPanels = Object.freeze({ show, close, hide: close, onState, openArchiveCode, mountReviewSettings, removeSource: openSourceRemoval });
})();
