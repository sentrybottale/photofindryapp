'use strict';

// The installed host owns guide progress and paths. This page keeps only the
// current view, a reviewed sample, and short-lived request identities in memory.
(() => {
  const P = window.PhotoFindry;
  const banner = document.getElementById('onboarding');
  const navigation = document.getElementById('workspace-navigation');
  if (!P || !banner || !navigation) return;

  const chapters = [
    ['Welcome', 'Find the photograph you remember.', 'Choose local models, add a small photo folder, then start a few selected tags when you are ready. Continuing this guide never starts a job.'],
    ['Local models', 'Give your library local eyes.', 'Choose the main vision-model .gguf and its matching mmproj vision-adapter file in Settings. A separate chat-template file is normally unnecessary. A configured model may be unloaded; it restarts when explicit tagging needs it. A search reranker is optional for your first tags.'],
    ['Photo folders', 'Bring a few photographs.', 'Choose a folder through the native Mac dialog. Discovery adds pending records; it never changes your originals or starts tagging the whole archive.'],
    ['Your first tags', 'Start with a reviewed sample.', 'Choose one registered folder, review up to eight pending photographs and select the few you want to tag. Use the tagging button when your sample is ready.'],
    ['Browse & inspect', 'Look closer at saved details.', 'Select a tile to see complete accepted observations. Double-click or press Space for a preview; the short gallery caption is only a summary.'],
    ['Find a photograph', 'Search what has been saved.', 'Ordinary search reads saved evidence and filenames without starting a language model. Local AI Search is a separate action; its candidates are ranked suggestions, not confirmed matches.'],
    ['People', 'Name a face you recognise.', 'Your confirmed name can seed quality-qualified matches. Suggestions remain reviewable, and automatically matched faces never become reference seeds.'],
    ['Jobs & Activity', 'Work at your pace.', 'Pause keeps eligible work to resume. Stop cancels remaining targets and keeps saved tags or reviewed AI results. Reopening a queue leaves it paused until you resume or start work.'],
    ['Files & backups', 'Keep your work portable.', 'Use Files to reconnect originals or remove a source after saving its restorable archive. Backups export saved evidence and selected managed assets; they omit original photographs, model weights and this Mac’s private job history.'],
    ['Your checklist', 'Your library, at your pace.', 'Finishing this guide does not download files, start jobs or mark unfinished setup as complete. You can reopen it from the sidebar at any time.'],
  ];
  const terminal = (job) => ['completed', 'failed', 'cancelled'].includes(job?.state);
  const text = (value, max = 256) => typeof value === 'string' ? value.slice(0, max) : '';
  const safeID = (value) => typeof value === 'string' && value.length > 0 && value.length <= 128 && /^[A-Za-z0-9._:-]+$/.test(value);
  const count = (value) => new Intl.NumberFormat().format(Number.isSafeInteger(value) && value >= 0 ? value : 0);
  const el = (tag, className = '', value) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (value !== undefined) node.textContent = String(value);
    return node;
  };
  const button = (label, run, className = 'secondary-button') => {
    const node = el('button', className, label);
    node.type = 'button';
    let running = false;
    node.addEventListener('click', async () => {
      if (node.disabled || running) return;
      running = true;
      try { await run(); }
      catch (error) { if (error?.name !== 'AbortError') P.showError(error?.message || 'The local guide could not finish this action.'); }
      finally { running = false; }
    });
    return node;
  };

  let state = {};
  let guide = null;
  let libraryInfo = null;
  let loadedId = '';
  let loadPromise = null;
  let loadError = '';
  let epoch = 0;
  // Offering the guide belongs to the authenticated browser session, not a
  // worker generation. Restore/attachment must invalidate data without undoing
  // the user's choice to continue later in this same library.
  const offeredLibraries = new Set();
  let guideDialog = null;
  let infoDialog = null;
  let pageNode = null;
  let chapterButtons = [];
  let liveNodes = {};
  let sample = null;
  let sampleRequest = null;
  let sampleVersion = 0;
  let submitting = false;
  let uncertainSubmission = false;

  const heading = el('h2');
  const detail = el('p');
  const bannerActions = el('div', 'pf-guide-actions');
  const resumeButton = button('Open Getting Started', () => openGuide());
  const settingsButton = button('Choose model files', () => destination('settings'));
  const addButton = button('Add photo folder', addFolder);
  const resetButton = button('Reset guide progress', resetGuide);
  bannerActions.append(resumeButton, settingsButton, addButton, resetButton);
  banner.replaceChildren(heading, detail, bannerActions);

  const guideNav = button('Getting started', () => openGuide(), 'nav-item');
  const libraryNav = button('This library', () => openLibraryInfo(), 'nav-item');
  if (!window.PhotoFindryShell?.ownsNavigation) navigation.append(guideNav, libraryNav);

  function currentId() { return safeID(state.libraryId) ? state.libraryId : ''; }
  function connected(id = currentId(), ticket = epoch) {
    return ticket === epoch && P.isConnected() && id && currentId() === id;
  }
  function configured() { return state.settings?.modelConfigured === true && state.settings?.projectorConfigured === true; }
  function isNewEmptyLibrary() {
    return state.library?.total === 0 && Array.isArray(state.folders)
      && state.folders.length === 0 && !configured();
  }
  function validGuide(value, id) {
    return value?.version === 1 && value.libraryId === id && Number.isInteger(value.step)
      && value.step >= 0 && value.step < chapters.length && typeof value.automaticEnabled === 'boolean'
      && typeof value.completed === 'boolean' && typeof value.hasStarted === 'boolean'
      && ['starterSourceId', 'starterRootId', 'taggingJobID'].every((key) => value[key] === '' || safeID(value[key]));
  }
  function validInfo(value, id) {
    return value && value.libraryId === id && typeof value.ready === 'boolean'
      && (value.error == null || typeof value.error === 'string')
      && typeof value.locationLabel === 'string' && value.locationLabel.length <= 4096;
  }
  function roots() {
    const rows = [];
    const seen = new Set();
    for (const folder of Array.isArray(state.folders) ? state.folders.slice(0, 256) : []) {
      const sourceId = folder?.sourceId;
      const rootId = folder?.sourceRootId;
      const scopeId = folder?.scopeId;
      if (!safeID(sourceId) || !safeID(rootId) || !safeID(scopeId)) continue;
      const key = `${sourceId}:${rootId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      rows.push({ key, sourceId, rootId, scopeId, label: text(folder.name || folder.pathLabel || 'Photo folder', 180), count: folder.count });
    }
    return rows;
  }
  function selectedRoot() {
    return roots().find((root) => root.sourceId === guide?.starterSourceId && root.rootId === guide?.starterRootId) || null;
  }
  function starterJob() {
    const id = guide?.taggingJobID;
    if (!safeID(id)) return null;
    return Array.isArray(state.jobs?.jobs) ? state.jobs.jobs.find((job) => job.id === id) || null : null;
  }
  function otherTaggingActive() {
    return Array.isArray(state.jobs?.jobs) && state.jobs.jobs.some((job) =>
      job.kind === 'tagging' && !terminal(job) && job.id !== guide?.taggingJobID);
  }
  function clearSample() {
    sampleVersion += 1;
    sampleRequest?.abort();
    sampleRequest = null;
    sample = null;
  }
  function closeGuide() {
    clearSample();
    const dialog = guideDialog;
    guideDialog = null;
    pageNode = null;
    chapterButtons = [];
    liveNodes = {};
    if (dialog?.open) dialog.close();
    dialog?.remove();
  }
  function closeInfo() {
    const dialog = infoDialog;
    infoDialog = null;
    if (dialog?.open) dialog.close();
    dialog?.remove();
  }
  function invalidate() {
    epoch += 1;
    loadedId = '';
    loadPromise = null;
    guide = null;
    libraryInfo = null;
    loadError = '';
    submitting = false;
    uncertainSubmission = false;
    closeGuide();
    closeInfo();
    renderBanner();
  }

  async function ensureLoaded(allowAutomatic = false) {
    const id = currentId();
    const ticket = epoch;
    if (!connected(id, ticket)) return false;
    if (loadedId === id && guide && libraryInfo) return true;
    if (loadPromise?.id === id) return loadPromise.work;
    const work = (async () => {
      try {
        const [info, progress] = await Promise.all([
          P.api('/api/library-info', {}), P.api('/api/onboarding', {}),
        ]);
        if (!connected(id, ticket)) return false;
        if (!validInfo(info, id) || !validGuide(progress, id)) throw Error('The local guide returned invalid state.');
        libraryInfo = info;
        guide = progress;
        loadedId = id;
        loadError = '';
        renderBanner();
        if (allowAutomatic && info.ready && !state.libraryError && guide.automaticEnabled && !guide.completed
          && (guide.hasStarted || isNewEmptyLibrary()) && !offeredLibraries.has(id)) {
          offeredLibraries.add(id);
          await openGuide({ loaded: true });
        }
        return true;
      } catch (error) {
        if (!connected(id, ticket)) return false;
        loadError = text(error?.message || 'Local guide progress is unavailable.', 250);
        renderBanner();
        return false;
      } finally {
        if (loadPromise?.work === work) loadPromise = null;
      }
    })();
    loadPromise = { id, work };
    return work;
  }

  async function persist(changes) {
    const id = guide?.libraryId;
    const ticket = epoch;
    if (!connected(id, ticket)) throw Error('Reconnect to this local library first.');
    const next = await P.api('/api/onboarding/update', { libraryId: id, changes });
    if (!connected(id, ticket)) return false;
    if (!validGuide(next, id)) throw Error('The local guide returned invalid progress.');
    guide = next;
    renderBanner();
    updateLive();
    return true;
  }

  async function resetGuide() {
    const id = currentId();
    const ticket = epoch;
    if (!connected(id, ticket)) return;
    const [next, info] = await Promise.all([
      P.api('/api/onboarding/reset', { libraryId: id }), P.api('/api/library-info', {}),
    ]);
    if (!connected(id, ticket)) return;
    if (!validGuide(next, id) || !validInfo(info, id)) throw Error('The local guide reset returned invalid progress.');
    guide = next;
    libraryInfo = info;
    loadedId = id;
    loadError = '';
    offeredLibraries.add(id);
    renderBanner();
    await openGuide({ loaded: true });
  }

  function renderBanner() {
    // The workspace clears transient content on disconnect; restore these stable
    // nodes when the next paired session receives a library snapshot.
    if (heading.parentNode !== banner) banner.replaceChildren(heading, detail, bannerActions);
    if (!P.isConnected() || !currentId()) { banner.hidden = true; return; }
    if (loadError) {
      banner.hidden = false;
      heading.textContent = 'Getting Started needs attention';
      detail.textContent = loadError;
      resumeButton.hidden = true; settingsButton.hidden = true; addButton.hidden = true; resetButton.hidden = false;
      return;
    }
    if (!guide || !libraryInfo?.ready || state.libraryError) { banner.hidden = true; return; }
    const noSource = state.library?.total === 0 && (state.sources?.length || 0) === 0
      && (state.folders?.length || 0) === 0;
    const needsModel = !configured();
    const unfinished = guide.hasStarted && !guide.completed;
    banner.hidden = !unfinished && !needsModel && !noSource;
    if (banner.hidden) return;
    heading.textContent = unfinished ? 'Your place in Getting Started is saved' : noSource ? 'Add a photo folder to begin' : 'Choose local model files when you want to tag';
    detail.textContent = unfinished ? `Continue with ${chapters[guide.step][0]}. Your saved library and jobs remain available.`
      : noSource ? 'PhotoFindry can discover photos without tagging or changing the originals.'
        : 'Your saved library is available now. An unloaded configured model never needs setup again.';
    resumeButton.textContent = unfinished ? 'Continue guide' : 'Getting Started';
    resumeButton.hidden = false; settingsButton.hidden = !needsModel; addButton.hidden = !noSource; resetButton.hidden = true;
  }

  async function addFolder() {
    const id = currentId();
    const ticket = epoch;
    const picked = await P.pick('folder');
    if (!connected(id, ticket) || picked?.cancelled || !safeID(picked?.grantId)) return;
    await P.action('addFolders', { folderGrants: [picked.grantId] });
    if (connected(id, ticket)) await P.refreshLibrary();
  }

  function destination(panel) {
    closeGuide();
    closeInfo();
    if (!P.isConnected()) return;
    queueMicrotask(() => window.dispatchEvent(new CustomEvent('photofindry:guide-destination', { detail: { panel } })));
  }

  async function openGuide({ loaded = false } = {}) {
    if (!loaded && !await ensureLoaded(false)) return;
    const id = guide?.libraryId;
    if (!connected(id) || !libraryInfo?.ready) return;
    offeredLibraries.add(id);
    if (!guide.hasStarted && !await persist({ hasStarted: true })) return;
    if (guideDialog?.open) { guideDialog.focus?.(); return; }
    closeInfo();
    const dialog = el('dialog', 'pf-guide-dialog');
    const top = el('div', 'pf-guide-top');
    const topActions = actionRow(button('Start guide again', replay, 'text-button'),
      button('Continue later', closeGuide, 'text-button'));
    top.append(el('div', '', 'PHOTOFINDRY / GETTING STARTED'), topActions);
    const frame = el('div', 'pf-guide-frame');
    const rail = el('nav', 'pf-guide-rail');
    rail.setAttribute('aria-label', 'Getting Started chapters');
    chapterButtons = chapters.map(([title], index) => {
      const node = button(`${index + 1}. ${title}`, () => changeStep(index), 'pf-guide-chapter');
      rail.append(node);
      return node;
    });
    pageNode = el('section', 'pf-guide-page');
    frame.append(rail, pageNode);
    const footer = el('div', 'pf-guide-footer');
    const back = button('Back', () => changeStep(Math.max(0, guide.step - 1)));
    const next = button('Continue', () => guide.step === chapters.length - 1
      ? finishGuide() : changeStep(guide.step + 1), 'primary-button');
    footer.append(back, next);
    liveNodes = { back, next };
    dialog.append(top, frame, footer);
    document.body.append(dialog);
    guideDialog = dialog;
    dialog.showModal();
    renderChapter();
  }

  async function changeStep(step) {
    if (!guide || !Number.isInteger(step) || step < 0 || step >= chapters.length) return;
    if (step === guide.step) return;
    if (!await persist({ step, hasStarted: true })) return;
    clearSample();
    renderChapter();
  }

  function actionRow(...nodes) {
    const row = el('div', 'pf-guide-actions');
    row.append(...nodes);
    return row;
  }
  function guideAction(label, panel) { return button(label, () => destination(panel)); }
  function note(value) { return el('p', 'pf-guide-note', value); }
  function jobStatus() {
    const id = guide?.taggingJobID;
    if (!id) return 'No starter job has been submitted.';
    const job = starterJob();
    if (!job) return 'The saved starter job is not in recent history. Check Jobs and saved tags before starting any new work.';
    const done = count(job.completed);
    const total = count(job.total);
    return `Starter job ${text(job.state, 30)} · ${done} of ${total} processed${job.failed ? ` · ${count(job.failed)} failed` : ''}. Accepted tags appear in the library.`;
  }

  function renderChapter() {
    if (!guideDialog?.open || !pageNode || !guide) return;
    const [title, headingText, intro] = chapters[guide.step];
    const page = el('div', 'pf-guide-copy');
    page.append(el('p', 'pf-guide-kicker', `CHAPTER ${guide.step + 1} OF ${chapters.length} · ${title.toUpperCase()}`),
      el('h2', '', headingText), el('p', 'pf-guide-intro', intro));
    liveNodes = { back: liveNodes.back, next: liveNodes.next };
    if (guide.step === 1) {
      const status = note(''); liveNodes.model = status; page.append(status, actionRow(guideAction('Choose model files', 'settings')));
    } else if (guide.step === 2) {
      page.append(actionRow(button('Add photo folder', addFolder, 'primary-button'), guideAction('Open Files', 'sources')));
      const status = note(''); liveNodes.folders = status; page.append(status);
    } else if (guide.step === 3) {
      renderTaggingChapter(page);
    } else if (guide.step === 4 || guide.step === 5) {
      page.append(actionRow(guideAction('Try it in my library', 'library')));
    } else if (guide.step === 6) {
      page.append(actionRow(guideAction('Open People', 'people')));
    } else if (guide.step === 7) {
      page.append(actionRow(guideAction('Open Jobs & Activity', 'jobs')));
    } else if (guide.step === 8) {
      page.append(actionRow(guideAction('Open Files', 'sources'), guideAction('Backups & archives', 'portability')));
    } else if (guide.step === 9) {
      const list = el('ul', 'pf-guide-checklist');
      const model = el('li'); const source = el('li'); const tags = el('li');
      list.append(model, source, tags); liveNodes.checklist = { model, source, tags };
      const automatic = el('input'); automatic.type = 'checkbox'; automatic.checked = guide.automaticEnabled;
      const automaticLabel = el('label', 'pf-guide-toggle');
      automaticLabel.append(automatic, el('span', '', 'Show this guide at launch until I finish it'));
      automatic.addEventListener('change', () => {
        const selected = automatic.checked;
        void persist({ automaticEnabled: selected }).catch((error) => {
          automatic.checked = !selected;
          P.showError(error.message || 'The guide preference could not be saved.');
        });
      });
      page.append(list, automaticLabel, actionRow(button('Finish tour', finishGuide, 'primary-button')));
    }
    pageNode.replaceChildren(page);
    chapterButtons.forEach((node, index) => node.setAttribute('aria-current', index === guide.step ? 'step' : 'false'));
    liveNodes.back.disabled = guide.step === 0;
    liveNodes.next.textContent = guide.step === chapters.length - 1 ? 'Finish tour' : 'Continue';
    updateLive();
  }

  async function finishGuide() {
    if (await persist({ completed: true })) closeGuide();
  }

  function renderTaggingChapter(page) {
    const selector = el('select', 'pf-guide-select');
    selector.setAttribute('aria-label', 'Starter photo folder');
    const empty = el('option', '', 'Choose a registered photo folder'); empty.value = ''; selector.append(empty);
    const options = roots();
    options.forEach((root, index) => {
      const option = el('option', '', root.label);
      option.value = String(index);
      selector.append(option);
    });
    const selected = options.findIndex((root) => root.sourceId === guide.starterSourceId && root.rootId === guide.starterRootId);
    selector.value = selected < 0 ? '' : String(selected);
    const status = note('');
    const sampleList = el('div', 'pf-guide-sample');
    const sampleCount = el('p', 'pf-guide-note');
    const start = button('Start selected sample', startSample, 'primary-button');
    const reload = button('Refresh pending sample', loadSample);
    page.append(el('label', 'pf-guide-label', 'Starter folder'), selector, status, sampleList, sampleCount,
      actionRow(start, reload, guideAction('Open Jobs', 'jobs')));
    liveNodes.tagging = { selector, options, status, sampleList, sampleCount, start, reload };
    selector.addEventListener('change', () => {
      void (async () => {
        const chosen = options[Number(selector.value)];
        clearSample();
        renderSample();
        if (!chosen || selector.value === '') {
          await persist({ starterSourceId: '', starterRootId: '' });
        } else if (roots().some((root) => root.key === chosen.key && root.scopeId === chosen.scopeId)) {
          if (await persist({ starterSourceId: chosen.sourceId, starterRootId: chosen.rootId })) await loadSample();
        }
        updateLive();
      })().catch((error) => P.showError(error.message || 'Choose this folder again.'));
    });
    if (selected >= 0) void loadSample().catch((error) => P.showError(error.message));
  }

  async function loadSample() {
    const root = selectedRoot();
    const id = currentId();
    const ticket = epoch;
    const tagging = liveNodes.tagging;
    if (!root || !tagging || !connected(id, ticket)) { clearSample(); renderSample(); return; }
    clearSample();
    const version = sampleVersion;
    const controller = new AbortController();
    sampleRequest = controller;
    tagging.sampleCount.textContent = 'Reading a bounded pending page from this folder…';
    try {
      const page = await P.api('/api/library', { sourceId: root.sourceId, scopeId: root.scopeId,
        status: 'pending', sort: 'library', offset: 0 }, { signal: controller.signal });
      if (!connected(id, ticket) || version !== sampleVersion || controller.signal.aborted || guide?.step !== 3
        || selectedRoot()?.key !== root.key || liveNodes.tagging !== tagging) return;
      if (!Array.isArray(page?.photos) || page.photos.length > 60 || !Number.isSafeInteger(page.totalCount)
        || page.totalCount < 0) throw Error('The local pending page was invalid.');
      const seen = new Set();
      const photos = page.photos.slice(0, 8).filter((photo) => {
        if (!safeID(photo?.id) || seen.has(photo.id)) return false;
        seen.add(photo.id); return true;
      }).map((photo) => ({ id: photo.id, label: text(photo.name || photo.pathLabel || photo.id, 180) }));
      sample = { libraryId: id, key: root.key, scopeId: root.scopeId, photos, selected: new Set(photos.slice(0, 3).map((photo) => photo.id)), total: page.totalCount };
      renderSample();
    } finally {
      if (sampleRequest === controller) sampleRequest = null;
    }
  }

  function renderSample() {
    const nodes = liveNodes.tagging;
    if (!nodes) return;
    nodes.sampleList.replaceChildren();
    if (!sample?.photos.length) {
      nodes.sampleCount.textContent = sample ? 'No pending photographs were found in this folder. Choose another folder or check Files.' : '';
      updateTaggingStatus(); return;
    }
    nodes.sampleList.append(el('p', 'pf-guide-sample-title', `Review up to ${sample.photos.length} pending photographs`));
    for (const photo of sample.photos) {
      const row = el('label', 'pf-guide-sample-row');
      const input = el('input'); input.type = 'checkbox'; input.checked = sample.selected.has(photo.id);
      input.addEventListener('change', () => {
        if (input.checked) sample.selected.add(photo.id); else sample.selected.delete(photo.id);
        updateTaggingStatus();
      });
      row.append(input, el('span', '', photo.label));
      nodes.sampleList.append(row);
    }
    updateTaggingStatus();
  }

  function updateTaggingStatus() {
    const nodes = liveNodes.tagging;
    if (!nodes) return;
    const job = starterJob();
    const unknown = Boolean(guide?.taggingJobID && !job);
    const active = Boolean(job && !terminal(job));
    const otherActive = otherTaggingActive();
    const root = selectedRoot();
    nodes.status.textContent = jobStatus();
    if (!configured()) nodes.status.textContent += ' Choose a vision model and matching projector in Local models before starting this sample.';
    if (!root && guide?.starterSourceId) nodes.status.textContent += ' The saved starter folder is no longer registered; choose a current folder.';
    if (otherActive) nodes.status.textContent += ' Another tagging job is active. Check Jobs before starting a new sample.';
    const validSample = sample && root && sample.libraryId === currentId() && sample.key === root.key && sample.scopeId === root.scopeId;
    const selected = validSample ? sample.selected.size : 0;
    if (validSample) nodes.sampleCount.textContent = `${count(selected)} selected for this sample · ${count(sample.total)} pending in the chosen folder. Only the photographs selected here will be tagged.`;
    nodes.start.textContent = `Tag ${selected} selected ${selected === 1 ? 'photo' : 'photos'}`;
    nodes.start.disabled = !selected || selected > 8 || active || unknown || otherActive || submitting || uncertainSubmission || !configured() || !root || !validSample;
    nodes.reload.disabled = !root || submitting;
  }

  async function startSample() {
    const root = selectedRoot();
    const id = currentId();
    const ticket = epoch;
    const current = sample;
    const job = starterJob();
    if (!connected(id, ticket) || !root || !current || current.libraryId !== id || current.key !== root.key
      || current.scopeId !== root.scopeId || (guide.taggingJobID && !job) || (job && !terminal(job))
      || otherTaggingActive() || submitting || uncertainSubmission || !configured()) return;
    const allowed = new Set(current.photos.map((photo) => photo.id));
    const ids = [...current.selected].filter((photoId) => allowed.has(photoId));
    if (!ids.length || ids.length > 8 || ids.length !== current.selected.size) return;
    submitting = true;
    updateTaggingStatus();
    try {
      let result;
      try {
        result = await P.action('enqueueJob', { kind: 'tagging', params: {
          photoIds: ids, sourceId: root.sourceId, scopeId: root.scopeId,
        } });
      } catch (_) {
        uncertainSubmission = true;
        throw Error('The starter job may have been submitted. Check Jobs before trying again.');
      }
      if (!connected(id, ticket)) return;
      if (!safeID(result?.id)) {
        uncertainSubmission = true;
        throw Error('The starter job may have been submitted. Check Jobs before trying again.');
      }
      // Keep the local lock if the host accepted work but guide persistence fails.
      uncertainSubmission = true;
      if (!await persist({ taggingJobID: result.id })) return;
      uncertainSubmission = false;
      clearSample();
      renderSample();
      P.showNotice('Starter sample added to Jobs. Saved tags appear as photos complete.');
    } finally {
      submitting = false;
      updateTaggingStatus();
    }
  }

  function updateLive() {
    if (!guideDialog?.open) return;
    if (liveNodes.model) liveNodes.model.textContent = configured()
      ? `Model files selected: ${text(state.settings?.modelLabel || 'vision model', 100)} and ${text(state.settings?.projectorLabel || 'projector', 100)}. They need not be loaded now.`
      : 'Select both a vision model and its matching projector. Ordinary saved-evidence browsing remains available.';
    if (liveNodes.folders) liveNodes.folders.textContent = `${count(state.library?.total)} photographs across ${count(state.folders?.length)} registered folders. Discovery does not start tagging.`;
    if (liveNodes.checklist) {
      liveNodes.checklist.model.textContent = `${configured() ? '✓' : '○'} Vision model and projector configured`;
      liveNodes.checklist.source.textContent = `${(state.folders?.length || 0) > 0 ? '✓' : '○'} Photo folder registered`;
      liveNodes.checklist.tags.textContent = `${(state.library?.tagged || 0) > 0 ? '✓' : '○'} Accepted tags saved`;
    }
    updateTaggingStatus();
  }

  async function openLibraryInfo() {
    const id = currentId();
    const ticket = epoch;
    if (!connected(id, ticket)) return;
    const info = await P.api('/api/library-info', {});
    if (!connected(id, ticket) || !validInfo(info, id)) return;
    libraryInfo = info;
    closeGuide();
    if (infoDialog?.open) { infoDialog.focus?.(); return; }
    const dialog = el('dialog', 'pf-library-dialog');
    const head = el('div', 'pf-library-head');
    head.append(el('h2', '', 'This library on this Mac'), button('Close', closeInfo, 'text-button'));
    const body = el('div', 'pf-library-body');
    const sourceLabels = Array.isArray(state.sources) ? state.sources.slice(0, 5).map((source) => text(source.label || source.id, 100)).filter(Boolean) : [];
    const modelLabels = [text(state.settings?.modelLabel, 100), text(state.settings?.projectorLabel, 100)].filter(Boolean);
    if (!info.ready) body.append(note(text(info.error || 'The local library is unavailable.', 400)));
    body.append(el('p', '', 'The active PhotoFindry data folder holds saved tags, people, jobs and rebuildable caches.'),
      el('div', 'pf-library-path', text(info.locationLabel, 4096)),
      actionRow(button('Show data folder in Finder', () => P.action('revealLibrary', {}), 'primary-button')),
      el('h3', '', 'What stays where'),
      el('p', '', `Original photographs remain in your selected source folders (${count(state.sources?.length)} registered). PhotoFindry never moves or rewrites them.`),
      sourceLabels.length ? el('p', '', `Registered sources: ${sourceLabels.join(', ')}${state.sources.length > sourceLabels.length ? ', and more' : ''}.`) : note('No source folder is registered yet.'),
      el('p', '', 'Retained previews are reduced-size cached copies, not original-photo backups. Model files stay in the folders you chose and their saved settings are independent of whether the model is loaded.'),
      el('p', '', modelLabels.length ? `Selected model files: ${modelLabels.join(' · ')}.` : 'No model files have been selected on this Mac.'),
      el('p', '', 'Portable backups carry saved evidence and selected managed assets. They omit original photographs, model weights and this Mac’s private jobs and guide preferences.'),
      actionRow(guideAction('Open Files', 'sources'), guideAction('Backups & archives', 'portability')));
    const remove = button('Remove source from gallery…', async () => {
      if (!connected(id, ticket)) return;
      closeInfo();
      await window.PhotoFindryPanels?.removeSource?.();
    });
    remove.disabled = !(Array.isArray(state.sources) && state.sources.some((source) => source.mode !== 'archived'));
    body.append(note('Remove a photo source after saving a verified, restorable archive. Its originals stay in their folders.'), actionRow(remove));
    dialog.append(head, body);
    document.body.append(dialog);
    infoDialog = dialog;
    dialog.showModal();
  }

  async function replay() {
    if (!await ensureLoaded(false)) return;
    if (!await persist({ step: 0, hasStarted: true })) return;
    closeGuide();
    await openGuide({ loaded: true });
  }

  window.addEventListener('photofindry:state', (event) => {
    const next = event.detail || {};
    if ((loadedId && next.libraryId !== loadedId) || (currentId() && next.libraryId !== currentId())
      || (next.loading === true && state.loading !== true)) invalidate();
    state = next;
    if (!P.isConnected() || !currentId() || state.loading === true || state.libraryError) { renderBanner(); return; }
    if (loadedId !== currentId()) void ensureLoaded(true);
    else { renderBanner(); updateLive(); }
  });
  window.addEventListener('photofindry:session', (event) => {
    if (event.detail?.connected === false) { offeredLibraries.clear(); invalidate(); }
  });
  window.addEventListener('photofindry:library-restored', invalidate);

  window.PhotoFindryOnboarding = Object.freeze({ open: openGuide, replay, openLibraryInfo });
})();
