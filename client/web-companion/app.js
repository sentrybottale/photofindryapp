'use strict';

// This client is bundled with the companion. It never loads code or data from a website.
(() => {
  // The native launcher alone supplies this one-use ticket. Remove it before
  // requests, state restoration or any other interface initialization.
  let bootstrapTicket = null;
  let pendingLaunchArchive = null;
  let bootstrapError = '';
  let launchFragment = window.location?.hash || '';
  if (launchFragment) {
    try {
      window.history.replaceState(null, '', `${window.location.pathname || '/'}${window.location.search || ''}`);
      const match = /^#bootstrap=([a-f0-9]{64})(?:&archive=([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}))?$/.exec(launchFragment);
      if (match) { bootstrapTicket = match[1]; pendingLaunchArchive = match[2]?.toLowerCase() || null; }
      else bootstrapError = 'This local launch link is invalid. Open PhotoFindry again.';
    } catch {
      bootstrapError = 'The browser could not clear this local launch link. Open PhotoFindry again in a new tab.';
    }
  }
  launchFragment = '';
  // Mount only after removing the launch capability from browser history. The
  // compiled shell creates the controller targets synchronously and restores
  // the static entry screen if initialization fails.
  try { if (!window.location?.hash) window.PhotoFindryShell?.mount(document.getElementById('photofindry-root')); }
  catch {
    bootstrapTicket = null;
    bootstrapError = 'The interface could not start. Open PhotoFindry again using the button above.';
  }
  const $ = (id) => document.getElementById(id);
  const state = {
    token: null,
    pairController: null,
    pageController: null,
    pageRequestKey: null,
    pageContextKey: null,
    pageRefreshQueued: false,
    pageRecoveryBlockedKey: null,
    detailController: null,
    previewController: null,
    pageVersion: 0,
    detailVersion: 0,
    previewVersion: 0,
    queryTimer: null,
    photos: [],
    total: 0,
    offset: 0,
    limit: 60,
    query: '',
    selected: new Set(),
    activeId: null,
    anchorId: null,
    tiles: new Map(),
    previewURL: null,
    previewMainStage: null,
    previewPhotoId: null,
    previewIdentity: null,
    previewPhotoName: '',
    previewIDs: [],
    previewFromPage: false,
    previewZoom: 1,
    previewCropId: null,
    previewCropURL: null,
    previewCropController: null,
    previewCropVersion: 0,
    detailContentKey: null,
    searchGuidanceKey: null,
    previewPhoto: null,
    detailGalleryRevision: null,
    selectionSignature: '',
    loading: false,
    previewQueue: [],
    activePreviews: 0,
    nativeState: {},
    stateTimer: null,
    stateRequest: null,
    stateController: null,
    galleryRefreshTimer: null,
    galleryRevision: null,
    libraryIdentity: null,
    libraryGeneration: null,
    workerGeneration: null,
    photoWorkflowKey: null,
    libraryLoaded: false,
    scope: { sourceId: '', scopeId: '', status: 'all', sort: 'library', metadataFilters: {}, locationOptions: {}, includeRelated: true },
    aiJobId: null,
    aiSort: 'relevance',
    candidateView: 'best',
    lastPage: null,
    aiRequestIDs: new Map(),
    lastAIInvalidation: '',
    restoreAIContext: false,
    requestGeneration: 0,
    requests: new Set(),
    pageSuspended: false,
  };
  const number = new Intl.NumberFormat();
  const text = (value) => typeof value === 'string' ? value : '';
  const strings = (value) => Array.isArray(value) ? value.filter((item) => typeof item === 'string') : [];
  const abortError = () => new DOMException('Request cancelled', 'AbortError');
  const isAbort = (error) => error?.name === 'AbortError';
  const element = (tag, className, value) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (value !== undefined) node.textContent = value;
    return node;
  };
  const searchRecoveryJobs = element('button', 'secondary-button', 'Open Jobs');
  searchRecoveryJobs.type = 'button'; searchRecoveryJobs.hidden = true;
  searchRecoveryJobs.addEventListener('click', () => notifyEvent('guide-destination', { panel: 'jobs' }));
  $('library-error').append(searchRecoveryJobs);

  class SessionError extends Error {}
  const SESSION_KEY = 'photofindry.local-session.v2';
  const SESSION_ENDED = 'Local access ended. Open PhotoFindry to reconnect to the verified companion. Your saved library is kept.';
  let resumeRecord = null;
  let resumeTask = null;
  const retrySession = element('button', 'secondary-button', 'Retry local connection');
  retrySession.type = 'button'; retrySession.hidden = true;
  $('pair-form').append(retrySession);
  // A dead process cannot authenticate a replacement port. Keep its browser
  // credential local and offer the registered native trust boundary instead of
  // scanning ports, sending it to the public site or replaying the last action.
  const connection = { lost: false, attempt: 0, timer: null, task: null, expected: '' };
  const reconnectDelays = [1000, 2000, 4000, 8000, 15000];
  const connectionPanel = element('section', 'connection-recovery');
  connectionPanel.hidden = true;
  connectionPanel.setAttribute('aria-label', 'Companion connection');
  const connectionMessage = element('p');
  connectionMessage.setAttribute('role', 'status');
  const connectionActions = element('div', 'button-row');
  const retryConnection = element('button', 'secondary-button', 'Retry connection');
  retryConnection.type = 'button';
  const reopenCompanion = element('a', 'primary-button', 'Open PhotoFindry');
  reopenCompanion.href = 'photofindry-companion://open';
  const repairCompanion = element('a', 'secondary-button', 'Repair installation');
  repairCompanion.href = 'https://app.photofindry.com/#installation';
  repairCompanion.target = '_blank'; repairCompanion.rel = 'noopener noreferrer';
  repairCompanion.referrerPolicy = 'no-referrer';
  const pairRepairCompanion = element('a', 'secondary-button', 'Repair installation');
  pairRepairCompanion.href = repairCompanion.href; pairRepairCompanion.target = '_blank';
  pairRepairCompanion.rel = 'noopener noreferrer'; pairRepairCompanion.referrerPolicy = 'no-referrer';
  $('pair-form').append(pairRepairCompanion);
  connectionActions.append(retryConnection, reopenCompanion, repairCompanion);
  connectionPanel.append(connectionMessage, connectionActions);
  $('library-error').parentNode.insertBefore(connectionPanel, $('library-error'));

  function sessionOrigin() {
    const origin = window.location?.origin;
    return typeof origin === 'string' && /^http:\/\/127\.0\.0\.1:\d{1,5}$/.test(origin) ? origin : null;
  }
  function validSession(record) {
    return record && typeof record === 'object' && !Array.isArray(record)
      && Object.keys(record).sort().join(',') === 'instanceId,origin,token,version'
      && record.version === 2 && record.origin === sessionOrigin() && record.origin !== null
      && typeof record.token === 'string' && /^[a-f0-9]{64}$/.test(record.token)
      && typeof record.instanceId === 'string' && /^[a-f0-9]{64}$/.test(record.instanceId);
  }
  function forgetSession() {
    resumeRecord = null;
    try {
      window.sessionStorage?.removeItem(SESSION_KEY);
      window.sessionStorage?.removeItem('photofindry.local-session.v1');
    } catch { /* An inaccessible store cannot authorize a request. */ }
  }
  function savedSession() {
    if (validSession(resumeRecord)) return resumeRecord;
    try {
      const raw = window.sessionStorage?.getItem(SESSION_KEY);
      if (typeof raw === 'string' && raw.length <= 1024) {
        const record = JSON.parse(raw);
        if (validSession(record)) { resumeRecord = record; return record; }
      }
    } catch { /* Pairing remains available when browser storage is denied. */ }
    forgetSession(); return null;
  }
  function saveSession(token, instanceId) {
    resumeRecord = { version: 2, origin: sessionOrigin(), token, instanceId };
    if (!validSession(resumeRecord)) { resumeRecord = null; return false; }
    try {
      if (!window.sessionStorage) return false;
      window.sessionStorage.removeItem('photofindry.local-session.v1');
      window.sessionStorage.setItem(SESSION_KEY, JSON.stringify(resumeRecord));
      return true;
    } catch { return false; }
  }

  function notifyEvent(name, detail) { window.dispatchEvent(new CustomEvent(`photofindry:${name}`, { detail })); }
  function showMessage(id, message) {
    if (id === 'global-error' && connection.lost && /^(The companion connection was interrupted\.|PhotoFindry is restarting\.|The companion did not respond in time\.|The request finished, but the latest state could not be read\.)/.test(text(message))) return;
    const target = $(id);
    target.replaceChildren(element('span', '', text(message)));
    const close = element('button', 'icon-button', '×');
    close.type = 'button'; close.setAttribute('aria-label', 'Dismiss message');
    close.addEventListener('click', () => { target.hidden = true; target.replaceChildren(); });
    target.append(close); target.hidden = false;
  }
  async function applyLaunchArchive(snapshot) {
    if (!pendingLaunchArchive || !state.token || !state.libraryLoaded || snapshot?.loading
      || !Number.isSafeInteger(snapshot?.libraryGeneration) || typeof snapshot?.libraryId !== 'string'
      || !Array.isArray(snapshot?.sources) || typeof window.PhotoFindryPanels?.openArchiveCode !== 'function') return;
    const id=pendingLaunchArchive, token=state.token, generation=state.requestGeneration;
    pendingLaunchArchive=null;
    try { await window.PhotoFindryPanels.openArchiveCode(id); }
    catch(error) {
      if (!isAbort(error) && state.token===token && generation===state.requestGeneration) {
        showMessage('global-error',error.message || 'This library does not contain that source. Open Files to review its saved archives.');
      }
    }
  }
  async function refreshState() {
    if (!state.token) return state.nativeState;
    if (state.stateRequest) return state.stateRequest;
    const token = state.token;
    const controller = new AbortController();
    state.stateController = controller;
    const task = (async () => {
      const response = await api('/api/state', {}, { signal: controller.signal });
      if (controller.signal.aborted || state.token !== token) return state.nativeState;
      const snapshot = response.state || response;
      const changedLibrary = (state.libraryIdentity !== null && typeof snapshot.libraryId === 'string' && snapshot.libraryId !== state.libraryIdentity)
        || (state.libraryGeneration !== null && Number.isSafeInteger(snapshot.libraryGeneration) && snapshot.libraryGeneration !== state.libraryGeneration);
      const restartedWorker = state.workerGeneration !== null && Number.isSafeInteger(snapshot.workerGeneration) && snapshot.workerGeneration !== state.workerGeneration;
      if (changedLibrary || restartedWorker) invalidateLibraryContext(changedLibrary);
      if (typeof snapshot.libraryId === 'string') state.libraryIdentity = snapshot.libraryId;
      if (Number.isSafeInteger(snapshot.libraryGeneration)) state.libraryGeneration = snapshot.libraryGeneration;
      if (Number.isSafeInteger(snapshot.workerGeneration)) state.workerGeneration = snapshot.workerGeneration;
      state.nativeState = snapshot;
      const workflowKey = JSON.stringify([snapshot.job?.running, snapshot.improvementJob?.running, snapshot.aiSearch?.running,
        (Array.isArray(snapshot.jobs?.jobs) ? snapshot.jobs.jobs : []).filter(job => job.kind === 'metadata').map(job => [job.id, job.state, job.waitingForSchedule])]);
      if (state.photoWorkflowKey !== null && workflowKey !== state.photoWorkflowKey) {
        if (state.activeId) void loadDetails(state.activeId, { preserve: true });
        if (state.previewPhoto) void refreshPhotoPresentations(state.previewPhoto).catch(() => {});
      }
      state.photoWorkflowKey = workflowKey;
      const revision = response.galleryRevision ?? snapshot.galleryRevision;
      if (revision != null && state.galleryRevision != null && revision !== state.galleryRevision && state.libraryLoaded
        && (!state.aiJobId || state.lastPage?.aiResultsReady === true)) {
        clearTimeout(state.galleryRefreshTimer);
        state.galleryRefreshTimer = setTimeout(() => loadLibrary(state.offset, true, { automatic: true }), 700);
        if (state.previewPhoto && !state.previewFromPage) {
          const previewVersion = state.previewVersion;
          void refreshPhotoPresentations(state.previewPhoto).catch(() => { if (state.previewVersion === previewVersion) closePreview(); });
        }
      }
      state.galleryRevision = revision;
      const aiEvent = snapshot.aiResults;
      const eventJobId = aiEvent?.result?.aiJobId || aiEvent?.aiJobId;
      const ownsEvent = state.aiJobId && (eventJobId === state.aiJobId || (aiEvent?.requestId && state.aiRequestIDs.get(state.aiJobId) === aiEvent.requestId));
      if (ownsEvent && aiEvent.invalidated) {
        const identity = JSON.stringify([state.aiJobId, aiEvent.requestId, revision]);
        if (identity !== state.lastAIInvalidation) {
          state.lastAIInvalidation = identity;
          clearPage(); state.lastPage = null;
          showMessage('global-notice', 'Library evidence changed. Invalid live results were withdrawn; saved candidates will be shown with their current status.');
          clearTimeout(state.galleryRefreshTimer);
          state.galleryRefreshTimer = setTimeout(() => loadLibrary(state.offset), 0);
        }
      }
      notifyEvent('state', snapshot);
      setConnection(true);
      await applyLaunchArchive(snapshot);
      return snapshot;
    })();
    state.stateRequest = task;
    try { return await task; }
    finally { if (state.stateRequest === task) state.stateRequest = null; }
  }
  function invalidateLibraryContext(replaced) {
    state.requestGeneration += 1;
    for (const controller of state.requests) controller.abort();
    state.requests.clear();
    clearTimeout(state.queryTimer); clearTimeout(state.stateTimer); clearTimeout(state.galleryRefreshTimer);
    clearPage(); state.libraryLoaded = false; state.galleryRevision = null; state.photoWorkflowKey = null;
    state.aiJobId = null; state.aiRequestIDs.clear(); state.lastAIInvalidation = ''; state.offset = 0; state.total = 0;
    if (replaced) {
      state.scope = { sourceId: '', scopeId: '', status: 'all', sort: 'library', metadataFilters: {}, locationOptions: {}, includeRelated: true };
      $('search').value = ''; state.query = '';
      $('result-count').textContent = ''; $('page-label').textContent = '';
      $('global-error').textContent = ''; $('global-error').hidden = true;
      $('library-error-text').textContent = ''; $('library-error').hidden = true;
      showMessage('global-notice', 'Backup imported. Open Files to choose and confirm the originals folders on this Mac. Saved tags remain available.');
      notifyEvent('library-restored', {});
    }
    notifyEvent('page', null); notifyEvent('scope', getScope());
    if (!state.pageSuspended && state.token) {
      state.galleryRefreshTimer = setTimeout(() => loadLibrary(0), 0);
      state.stateTimer = setTimeout(pollState, 1200);
    }
  }

  async function pollState() {
    if (connection.lost) return;
    const token = state.token;
    const generation = state.requestGeneration;
    try { await refreshState(); }
    catch (error) { if (!isAbort(error) && !(error instanceof SessionError) && !connection.lost && state.token) showMessage('global-error', error.message); }
    finally { if (generation === state.requestGeneration && !state.pageSuspended && !connection.lost && state.token && state.token === token) { clearTimeout(state.stateTimer); state.stateTimer = setTimeout(pollState, 1200); } }
  }
  async function action(name, params = {}) {
    const generation = state.requestGeneration;
    const current = () => { if (generation !== state.requestGeneration || state.pageSuspended || !state.token) throw abortError(); };
    const result = await api('/api/action', { action: name, params });
    current();
    // A poll that started before the action is not its committed state.
    if (state.stateRequest) { try { await state.stateRequest; } catch { /* Fetch a fresh state below. */ } }
    current();
    try { await refreshState(); }
    catch (error) { if (state.token && !isAbort(error) && !(error instanceof SessionError)) showMessage('global-error', 'The request finished, but the latest state could not be read. Refresh to reconnect; do not repeat the completed action.'); }
    current();
    return result;
  }
  function getScope() { return { ...state.scope, metadataFilters: { ...state.scope.metadataFilters }, locationOptions: { ...state.scope.locationOptions }, query: $('search').value }; }
  function setScope(changes, { reload = true } = {}) {
    if (state.aiJobId && Object.keys(changes).every((key) => key === 'sort')) state.aiSort = changes.sort;
    else { state.scope = { ...state.scope, ...changes }; state.aiJobId = null; }
    notifyEvent('scope', getScope());
    if (reload) return loadLibrary(0);
  }
  function searchLibrary(query) {
    if (typeof query !== 'string' || query.length > 1000 || query.includes('\0')) throw Error('Choose a saved name or a shorter search.');
    state.query = query.trim(); $('search').value = state.query; state.aiJobId = null;
    state.scope = { ...state.scope, sourceId: '', scopeId: '' };
    notifyEvent('scope', getScope()); return loadLibrary(0);
  }
  async function showPhoto(photoId) {
    const photo = await api('/api/photo', { photoId });
    if (photo?.id !== photoId) throw new Error('The companion returned a different photo.');
    return openPreview(photo, { ids: [photoId], fromPage: false, loaded: true });
  }
  window.PhotoFindry = Object.freeze({
    api, action, pick: (kind) => api('/api/pick', { kind }, { timeoutMs: 310000 }), refreshState,
    refreshLibrary: (offset = 0) => loadLibrary(offset),
    refreshCurrentPage: () => loadLibrary(state.offset, true),
    showError: (message) => showMessage('global-error', message),
    showNotice: (message) => showMessage('global-notice', message),
    selectedIDs: () => [...state.selected], getState: () => state.nativeState,
    getScope, setScope, showPhoto, searchLibrary,
    getPage: () => state.aiJobId && state.lastPage?.aiJobId !== state.aiJobId ? null : state.lastPage,
    getAIJobId: () => state.aiJobId,
    openAIResults: (id, requestId) => {
      state.aiJobId = id; state.aiSort = 'relevance'; state.candidateView = 'best'; state.restoreAIContext = true;
      if (requestId) { state.aiRequestIDs.set(id, requestId); if (state.aiRequestIDs.size > 32) state.aiRequestIDs.delete(state.aiRequestIDs.keys().next().value); }
      return loadLibrary(0);
    },
    setCandidateView: (view) => { if (!['best', 'refined', 'all'].includes(view)) return; state.candidateView = view; return loadLibrary(0); },
    returnToLibrary: () => { state.aiJobId = null; return loadLibrary(0); },
    previewBlob: (photoId, signal, faceId) => fetchPreview(photoId, signal, { params: { photoId, ...(faceId ? { faceId } : {}) } }),
    activityPreviewBlob: (jobId, photoId, signal, kind = 'tagging') => fetchPreview(photoId, signal, { path: '/api/activity-preview', params: { jobId, photoId, kind } }),
    isConnected: () => Boolean(state.token) && !connection.lost,
    expectCompanionRestart: (operation) => { if (['update', 'rollback', 'recover'].includes(operation)) connection.expected = operation; },
    finishCompanionRestart: () => { if (!connection.lost) connection.expected = ''; },
  });

  function setConnection(available) {
    const recovered = available && connection.lost;
    if (available) {
      connection.lost = false; connection.attempt = 0;
      clearTimeout(connection.timer); connection.timer = null;
      connectionPanel.hidden = true;
      if (recovered) {
        connection.expected = '';
        clearTimeout(state.stateTimer);
        if (state.token && !state.pageSuspended) state.stateTimer = setTimeout(pollState, 1200);
        notifyEvent('connection', { available: true });
      }
    }
    const badge = $('connection-status');
    badge.replaceChildren(element('span', 'local-dot'), document.createTextNode(available ? 'Connected to this Mac' : 'Companion unavailable'));
    badge.firstChild.setAttribute('aria-hidden', 'true');
    badge.classList.toggle('unavailable', !available);
  }

  function connectionHelp(exhausted = false) {
    connectionMessage.textContent = connection.expected
      ? 'PhotoFindry is restarting for the requested change. Saved work is kept. The updated workspace opens when it is ready. Use Open PhotoFindry to follow its progress or recover.'
      : exhausted
        ? 'This browser could not reconnect. Open PhotoFindry to start or recover the verified companion. If it cannot open, use Repair installation. Your saved library stays in place.'
        : 'The companion connection was interrupted. Reconnecting automatically… Your current view and unsaved inputs are kept. Open PhotoFindry can start or recover it.';
  }
  function connectionLost() {
    if (!state.token || state.pageSuspended || connection.lost) return;
    connection.lost = true; connection.attempt = 0;
    clearTimeout(state.stateTimer);
    setConnection(false); connectionHelp(); connectionPanel.hidden = false;
    notifyEvent('connection', { available: false });
    scheduleReconnect();
  }
  function scheduleReconnect() {
    if (!connection.lost || connection.timer || connection.task || !state.token || state.pageSuspended) return;
    if (connection.attempt >= reconnectDelays.length) { connectionHelp(true); return; }
    connection.timer = setTimeout(() => { connection.timer = null; void reconnect(); }, reconnectDelays[connection.attempt]);
  }
  async function reconnect() {
    if (connection.task || !connection.lost || !state.token || state.pageSuspended) return;
    const token = state.token, generation = state.requestGeneration, record = savedSession();
    connection.attempt += 1; retryConnection.disabled = true;
    const task = (async () => {
      try {
        const result = await api('/api/session', {}, { token, reconnecting: true, timeoutMs: 4000 });
        if (token !== state.token || generation !== state.requestGeneration || state.pageSuspended) return;
        if (!validSessionResponse(result) || !record || result.instanceId !== record.instanceId) {
          resetSession('The local companion changed. Open PhotoFindry to reconnect securely. Your saved library is kept.'); return;
        }
        // Validate before resuming reads. Never repeat the mutation that lost
        // its response: it may already have committed in the durable queue.
        await refreshState();
        if (token === state.token && generation === state.requestGeneration && !state.pageSuspended) await loadLibrary(state.offset, true);
      } catch (error) {
        if (token !== state.token || generation !== state.requestGeneration || state.pageSuspended || isAbort(error)) return;
        if (error instanceof SessionError) return;
        connectionHelp(connection.attempt >= reconnectDelays.length);
      } finally {
        if (connection.task === task) connection.task = null;
        retryConnection.disabled = false;
        scheduleReconnect();
      }
    })();
    connection.task = task;
    await task;
  }
  retryConnection.addEventListener('click', () => {
    clearTimeout(connection.timer); connection.timer = null; connection.attempt = 0;
    return reconnect();
  });

  function requestTimeout(path, body) {
    if (path === '/api/pick') return 310000;
    if (path === '/api/state' || path === '/api/library') return 120000;
    if (path !== '/api/action') return 30000;
    if (['sourceLabel', 'sourceLabelScan', 'startDownload'].includes(body.action)) return 310000;
    if (['startModel', 'switchModel', 'configure', 'optimizeModel', 'prepareSourceArchive', 'inspectSourceArchive', 'attachSourceArchive', 'exportSourceArchive', 'inspectBackup', 'restoreLibrary', 'exportLibrary'].includes(body.action)) return 1900000;
    return 120000;
  }
  async function api(path, body, { signal, binary = false, pairing = false, token = state.token, timeoutMs, reconnecting = false } = {}) {
    if (state.pageSuspended) throw abortError();
    const generation = state.requestGeneration;
    const controller = new AbortController();
    const abort = () => controller.abort();
    if (signal?.aborted) throw abortError();
    signal?.addEventListener('abort', abort, { once: true });
    state.requests.add(controller);
    const current = () => {
      if (generation !== state.requestGeneration || state.pageSuspended || controller.signal.aborted) throw abortError();
    };
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; controller.abort(); }, timeoutMs ?? requestTimeout(path, body));
    try {
      const headers = { 'Content-Type': 'application/json' };
      // Bind actions to the library whose state this tab has actually applied.
      // A restore in another tab cannot authorize stale photo IDs or drafts.
      if (['/api/action', '/api/model-files/check', '/api/review-files/check'].includes(path)) {
        if (!Number.isSafeInteger(state.libraryGeneration) || state.libraryGeneration < 0) throw new Error('Wait for the local library state to finish opening before changing it.');
        headers['X-PhotoFindry-Library-Generation'] = String(state.libraryGeneration);
      }
      if (!pairing) {
        if (!token) throw new SessionError('Pair with the companion to continue.');
        headers.Authorization = `Bearer ${token}`;
      }
      const response = await fetch(path, {
        method: 'POST', headers, body: JSON.stringify(body), signal: controller.signal,
        cache: 'no-store', credentials: 'omit', redirect: 'error', referrerPolicy: 'no-referrer',
      });
      current();
      if (!response.ok) {
        let detail;
        try { detail = await response.json(); } catch { /* Use a local error below. */ }
        current();
        if (response.status === 401 && !pairing) {
          if (token === state.token) resetSession(SESSION_ENDED);
          throw new SessionError('Your session ended.');
        }
        const error = new Error(text(detail?.error) || `The companion could not complete this request (${response.status}).`);
        error.status = response.status;
        throw error;
      }
      if (binary) {
        const blob = await response.blob();
        current();
        if (!['image/png', 'image/jpeg'].includes(blob.type) || blob.size > 24 * 1024 * 1024) {
          throw new Error('The companion returned an unsupported preview.');
        }
        return blob;
      }
      const result = await response.json();
      current();
      return result;
    } catch (error) {
      if (generation !== state.requestGeneration || state.pageSuspended) throw abortError();
      if (timedOut) {
        if (!reconnecting && state.token === token && !pairing) connectionLost();
        throw new Error('The companion did not respond in time. Reconnecting automatically; use Open PhotoFindry in the connection panel if needed.');
      }
      if (isAbort(error) || error instanceof SessionError) throw error;
      if (error instanceof TypeError || error?.name === 'TypeError') {
        if (!reconnecting && state.token === token && !pairing) connectionLost();
        throw new Error(connection.expected
          ? 'PhotoFindry is restarting. Use Open PhotoFindry to follow its progress; saved work is kept.'
          : 'The companion connection was interrupted. Reconnecting automatically. Your last action was not repeated; use Open PhotoFindry if it does not return.');
      }
      if (error instanceof SyntaxError) throw new Error('The companion returned an unreadable response.');
      throw error;
    } finally {
      clearTimeout(timer);
      state.requests.delete(controller);
      signal?.removeEventListener('abort', abort);
    }
  }

  // One queue covers gallery and full-preview requests, including overlapping page changes.
  function fetchPreview(photoId, signal, { path = '/api/preview', params = { photoId } } = {}) {
    return new Promise((resolve, reject) => {
      if (signal.aborted) { reject(abortError()); return; }
      const task = { photoId, path, params, signal, resolve, reject, cancel: null };
      task.cancel = () => {
        const index = state.previewQueue.indexOf(task);
        if (index !== -1) state.previewQueue.splice(index, 1);
        reject(abortError());
      };
      signal.addEventListener('abort', task.cancel, { once: true });
      state.previewQueue.push(task);
      pumpPreviews();
    });
  }

  function pumpPreviews() {
    while (state.activePreviews < 4 && state.previewQueue.length) {
      const task = state.previewQueue.shift();
      task.signal.removeEventListener('abort', task.cancel);
      if (task.signal.aborted) { task.reject(abortError()); continue; }
      state.activePreviews += 1;
      api(task.path, task.params, { signal: task.signal, binary: true })
        .then(task.resolve, task.reject)
        .finally(() => { state.activePreviews -= 1; pumpPreviews(); });
    }
  }

  function clearPage({ keepPreview = false } = {}) {
    state.pageVersion += 1;
    state.pageController?.abort();
    state.pageController = null;
    state.pageRequestKey = null;
    state.pageContextKey = null;
    state.pageRefreshQueued = false;
    state.pageRecoveryBlockedKey = null;
    state.lastPage = null;
    state.searchGuidanceKey = null;
    $('query-notice').replaceChildren(); $('query-notice').hidden = true;
    clearSelection();
    if (!keepPreview) closePreview();
    $('gallery').replaceChildren();
    for (const tile of state.tiles.values()) disposeTile(tile);
    state.tiles.clear();
    state.photos = [];
  }

  function resetSession(message = '', { preserveCredential = false } = {}) {
    clearTimeout(connection.timer); connection.timer = null;
    connection.task = null; connection.lost = false; connection.attempt = 0; connection.expected = '';
    connectionPanel.hidden = true;
    state.requestGeneration += 1;
    for (const controller of state.requests) controller.abort();
    state.requests.clear();
    resumeTask = null;
    if (!preserveCredential) { forgetSession(); bootstrapTicket = null; bootstrapError = ''; }
    state.token = null;
    clearTimeout(state.queryTimer);
    clearTimeout(state.stateTimer);
    clearTimeout(state.galleryRefreshTimer);
    state.stateController?.abort();
    state.nativeState = {};
    state.stateRequest = null;
    state.libraryLoaded = false;
    state.galleryRevision = null;
    state.libraryIdentity = state.libraryGeneration = state.workerGeneration = state.photoWorkflowKey = null;
    state.aiJobId = null;
    state.lastPage = null;
    state.aiRequestIDs.clear(); state.lastAIInvalidation = '';
    state.scope = { sourceId: '', scopeId: '', status: 'all', sort: 'library', metadataFilters: {}, locationOptions: {}, includeRelated: true };
    state.pairController?.abort();
    state.pairController = null;
    clearPage();
    state.total = 0;
    state.offset = 0;
    state.query = '';
    state.loading = false;
    $('search').value = '';
    $('query-notice').textContent = '';
    $('query-notice').hidden = true;
    $('library-error-text').textContent = '';
    $('library-error').hidden = true;
    searchRecoveryJobs.hidden = true;
    $('result-count').textContent = '';
    $('page-label').textContent = '';
    $('pair-code').value = '';
    $('pair-code').disabled = false;
    $('pair-submit').disabled = false;
    $('pair-submit').replaceChildren(document.createTextNode('Open local library '), element('span', '', '→'));
    $('pair-error').textContent = message;
    $('pair-error').hidden = !message;
    retrySession.hidden = true;
    $('app').hidden = true;
    $('pair-screen').hidden = false;
    if (!state.pageSuspended) $('pair-code').focus();
    $('global-error').hidden = true;
    $('global-notice').hidden = true;
    notifyEvent('session', { connected: false });
  }

  function validSessionResponse(result, withToken = false) {
    return result && typeof result === 'object' && !Array.isArray(result)
      && Object.keys(result).sort().join(',') === (withToken ? 'apiVersion,instanceId,sessionLifetime,token' : 'apiVersion,instanceId,sessionLifetime')
      && result.apiVersion === 4 && result.sessionLifetime === 'process'
      && typeof result.instanceId === 'string' && /^[a-f0-9]{64}$/.test(result.instanceId)
      && (!withToken || typeof result.token === 'string' && /^[a-f0-9]{64}$/.test(result.token));
  }
  async function connectSession(token, instanceId) {
    if (!/^[a-f0-9]{64}$/.test(token) || !/^[a-f0-9]{64}$/.test(instanceId)) {
      throw new Error('This interface and companion are incompatible. Restart with a matching companion build.');
    }
    const retained = saveSession(token, instanceId);
    state.token = token;
    $('pair-screen').hidden = true;
    $('app').hidden = false;
    retrySession.hidden = true;
    setConnection(true);
    notifyEvent('session', { connected: true });
    if (!retained) showMessage('global-notice', 'This browser cannot save access for a reload. You can keep using this page; reopening may require a new pairing code.');
    pollState();
    $('search').focus();
    await loadLibrary(0);
    void api('/api/lifecycle/status', {}).then(value => {
      const messages=[value?.openUpdate?.message,value?.linkRepair?.message].filter(message => typeof message==='string' && message.length>0);
      if (state.token && messages.length) showMessage('global-notice', messages.join(' '));
    }).catch(() => {});
    if (pendingLaunchArchive) { await refreshState(); await applyLaunchArchive(state.nativeState); }
  }

  function restoreSession() {
    if (state.pageSuspended || state.token) return Promise.resolve();
    if (resumeTask) return resumeTask;
    if (bootstrapError) {
      $('pair-error').textContent = bootstrapError; $('pair-error').hidden = false;
      return Promise.resolve();
    }
    const ticket = bootstrapTicket;
    const record = ticket ? null : savedSession();
    if (!ticket && !record) return Promise.resolve();
    const generation = state.requestGeneration;
    $('pair-screen').hidden = false; $('app').hidden = true;
    $('pair-code').disabled = true; $('pair-submit').disabled = true;
    $('pair-error').textContent = 'Reconnecting to your local companion…'; $('pair-error').hidden = false;
    retrySession.hidden = true;
    const task = (async () => {
      try {
        const result = ticket
          ? await api('/api/bootstrap', { ticket }, { pairing: true })
          : await api('/api/session', {}, { token: record.token });
        if (generation !== state.requestGeneration || state.pageSuspended) return;
        if (!validSessionResponse(result, Boolean(ticket))) {
          resetSession('This interface and companion are incompatible. Restart with a matching companion build.'); return;
        }
        if (record && record.instanceId !== result.instanceId) {
          resetSession('The local companion changed. Open PhotoFindry again to reconnect.'); return;
        }
        bootstrapTicket = null;
        await connectSession(ticket ? result.token : record.token, result.instanceId);
      } catch (error) {
        if (generation !== state.requestGeneration || isAbort(error)) return;
        if (ticket && error.status === 401) {
          bootstrapTicket = null;
          // A stale launch ticket cannot discard an independently valid tab
          // credential. Revalidate that credential through the ordinary route.
          if (savedSession()) { resumeTask = null; await restoreSession(); return; }
          resetSession('This local launch expired or was already used. Open PhotoFindry again.'); return;
        }
        if (error instanceof SessionError) { resetSession(SESSION_ENDED); return; }
        $('pair-error').textContent = 'The companion could not be reached; retry the local connection, or use Open PhotoFindry above to start or recover it. Your saved library is kept.';
        $('pair-error').hidden = false; retrySession.hidden = false;
      } finally {
        if (resumeTask === task) resumeTask = null;
        if (generation === state.requestGeneration && !state.pageSuspended) {
          $('pair-code').disabled = false; $('pair-submit').disabled = false;
        }
      }
    })();
    resumeTask = task;
    return task;
  }
  retrySession.addEventListener('click', restoreSession);

  $('pair-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const code = $('pair-code').value.trim();
    if (!code) return;
    resetSession();
    const controller = new AbortController();
    state.pairController = controller;
    $('pair-code').value = '';
    $('pair-code').disabled = true;
    $('pair-submit').disabled = true;
    $('pair-submit').textContent = 'Connecting…';
    $('pair-error').hidden = true;
    try {
      const result = await api('/api/pair', { code }, { signal: controller.signal, pairing: true });
      if (controller.signal.aborted || state.pairController !== controller) return;
      if (!validSessionResponse(result, true)) {
        throw new Error('This interface and companion are incompatible. Restart with a matching companion build.');
      }
      await connectSession(result.token, result.instanceId);
    } catch (error) {
      if (isAbort(error) || error instanceof SessionError) return;
      $('pair-error').textContent = error.message;
      $('pair-error').hidden = false;
    } finally {
      if (state.pairController === controller) {
        state.pairController = null;
        $('pair-code').disabled = false;
        $('pair-submit').disabled = false;
        $('pair-submit').replaceChildren(document.createTextNode('Open local library '), element('span', '', '→'));
        if (!state.token) $('pair-code').focus();
      }
    }
  });

  $('disconnect').addEventListener('click', async () => {
    const token = state.token;
    resetSession('Disconnected. Open PhotoFindry again when you want to reconnect.');
    if (!token) return;
    try { await api('/api/disconnect', {}, { token }); }
    catch { if (!state.token) $('pair-error').textContent = 'Browser access was cleared here. If the companion is unavailable, quit it to revoke its remaining browser access.'; }
  });

  $('quit-companion').addEventListener('click', async () => {
    if (!state.token || !window.confirm('Quit PhotoFindry on this Mac? Current processing will stop; saved work is kept. You can reopen it from Open PhotoFindry.')) return;
    const token = state.token;
    resetSession('PhotoFindry is stopping. Open PhotoFindry to return to this library.');
    try { await api('/api/quit', {}, { token }); }
    catch { if (!state.token) $('pair-error').textContent = 'Browser access was cleared here. If PhotoFindry is still running, reopen it to retry Quit.'; }
  });

  function updatePagination() {
    const pages = Math.max(1, Math.ceil(state.total / state.limit));
    $('page-label').textContent = state.lastPage?.aiResultsPending ? 'Preparing' : state.total ? `${number.format(Math.floor(state.offset / state.limit) + 1)} / ${number.format(pages)}` : state.aiJobId ? '0 candidates' : '0 photos';
    $('previous-page').disabled = state.loading || state.offset === 0;
    $('next-page').disabled = state.loading || state.offset + state.limit >= state.total;
    $('refresh').disabled = state.loading;
  }

  function pageContext(aiJobId, scope, offset) {
    return JSON.stringify(aiJobId
      ? { aiJobId, offset, sort: state.aiSort, candidateView: state.candidateView }
      : { scope, offset });
  }

  // These are stale read-only search attempts, not failures to open a library.
  // Keep the worker's content fences and repeat the whole search with fresh data.
  function isSearchInvalidation(error) {
    return [
      'The operation was cancelled.',
      'The library changed during search. Run it again to use current details.',
      'The library changed during search.',
      'Saved People changed during search. Try again.',
      'The library changed while preparing GPS search. Try again.',
      'The recorded GPS changed while preparing search. Try again.',
      'Recorded metadata changed while filtering. Run the search again.',
      'The searchable library changed during ranking.',
    ].includes(error?.message);
  }

  function waitForPageRetry(milliseconds, signal) {
    return new Promise((resolve, reject) => {
      if (signal.aborted) { reject(abortError()); return; }
      const abort = () => { clearTimeout(timer); signal.removeEventListener('abort', abort); reject(abortError()); };
      const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve(); }, milliseconds);
      signal.addEventListener('abort', abort, { once: true });
    });
  }

  async function loadLibrary(offset = 0, preserveSelection = false, { automatic = false } = {}) {
    if (!state.token) return;
    clearTimeout(state.galleryRefreshTimer);
    const aiJobId = state.aiJobId;
    const scope = getScope();
    const requestedOffset = Math.max(0, offset);
    const contextKey = pageContext(aiJobId, scope, requestedOffset);
    // Repeated state ticks must not restart an exhausted recovery cycle forever.
    // An explicit retry, query change or scope change starts a fresh attempt.
    if (automatic && state.pageRecoveryBlockedKey === contextKey) return;
    if (!automatic) state.pageRecoveryBlockedKey = null;
    if (state.loading && state.pageRequestKey === contextKey) {
      state.pageRefreshQueued = true;
      return;
    }
    const samePage = contextKey === state.pageContextKey;
    clearTimeout(state.queryTimer);
    if (!samePage) {
      clearPage();
      state.total = 0;
      notifyEvent('page', null);
    } else {
      state.pageController?.abort();
      state.pageVersion += 1;
    }
    const controller = new AbortController();
    state.pageController = controller;
    state.pageRequestKey = contextKey;
    state.pageRefreshQueued = false;
    const version = state.pageVersion;
    const current = () => !controller.signal.aborted && version === state.pageVersion && state.token
      && aiJobId === state.aiJobId && contextKey === pageContext(state.aiJobId, getScope(), requestedOffset);
    state.query = scope.query;
    state.offset = requestedOffset;
    state.loading = true;
    $('gallery').setAttribute('aria-busy', 'true');
    const keepingPhotos = samePage && state.photos.length > 0;
    $('loading-state').hidden = keepingPhotos;
    if (!keepingPhotos) $('empty-state').hidden = true;
    $('library-error').hidden = true;
    searchRecoveryJobs.hidden = true;
    if (!keepingPhotos) {
      $('query-notice').hidden = true;
      $('result-count').textContent = aiJobId ? 'Preparing AI search…' : state.query ? 'Searching saved details…' : 'Opening your library…';
    }
    updatePagination();
    try {
      const input = { ...scope, offset: requestedOffset };
      if (!input.scopeId) delete input.scopeId;
      if (!input.sourceId) delete input.sourceId;
      let result;
      let showingUnscoredPool = false;
      const retryDelays = [200, 600, 1400];
      for (let attempt = 0; ; attempt += 1) {
        try {
          result = aiJobId
            ? await api('/api/action', { action: 'jobResults', params: { id: aiJobId, offset: requestedOffset, limit: 60, sort: state.aiSort, candidateView: state.candidateView } }, { signal: controller.signal })
            : await api('/api/library', input, { signal: controller.signal });
          break;
        } catch (error) {
          if (!current()) return;
          if (aiJobId || !isSearchInvalidation(error) || attempt >= retryDelays.length) throw error;
          $('result-count').textContent = 'Updating saved results…';
          await waitForPageRetry(retryDelays[attempt], controller.signal);
          if (!current()) return;
          // This replay already incorporates refreshes requested before it began.
          state.pageRefreshQueued = false;
        }
      }
      if (!current()) return;
      if (aiJobId && state.candidateView === 'best' && result.aiResultsReady === true && result.aiResultsPending !== true && result.totalCount === 0 && result.aiCandidateCount > 0) {
        const all = await api('/api/action', { action: 'jobResults', params: { id: aiJobId, offset: 0, limit: 60, sort: state.aiSort, candidateView: 'all' } }, { signal: controller.signal });
        if (!current()) return;
        if (all.aiJobId !== aiJobId) throw new Error('The companion returned results for a different AI search.');
        if (all.aiCandidateView === 'all' && all.totalCount > 0) { result = all; showingUnscoredPool = true; }
      }
      if (aiJobId && result.aiJobId !== aiJobId) throw new Error('The companion returned results for a different AI search.');
      if (!Array.isArray(result.photos) || !Number.isSafeInteger(result.totalCount) || result.totalCount < 0 ||
          !Number.isSafeInteger(result.offset) || result.offset < 0 || !Number.isSafeInteger(result.limit) || result.limit < 1 || result.limit > 60 || result.photos.length > result.limit ||
          result.photos.some((photo) => typeof photo?.id !== 'string' || !photo.id) || new Set(result.photos.map((photo) => photo.id)).size !== result.photos.length) {
        throw new Error('The companion returned an invalid library page.');
      }
      if (showingUnscoredPool) state.candidateView = 'all';
      const oldActivePhoto = state.photos.find((photo) => photo.id === state.activeId);
      state.photos = result.photos;
      state.total = result.totalCount;
      state.offset = result.offset;
      state.limit = result.limit;
      state.lastPage = result;
      state.pageRecoveryBlockedKey = null;
      state.pageContextKey = contextKey;
      state.libraryLoaded = true;
      if (state.aiJobId && typeof result.aiQuery === 'string') { state.query = result.aiQuery; $('search').value = result.aiQuery; }
      if (state.aiJobId && state.restoreAIContext && result.aiContextUnavailable !== true
        && Object.hasOwn(result, 'aiSourceId') && Object.hasOwn(result, 'aiMetadataFilters') && Object.hasOwn(result, 'aiLocationOptions')) {
        state.scope = { ...state.scope, sourceId: text(result.aiSourceId), scopeId: text(result.aiScopeId), status: 'all', metadataFilters: result.aiMetadataFilters && typeof result.aiMetadataFilters === 'object' ? result.aiMetadataFilters : {}, locationOptions: result.aiLocationOptions && typeof result.aiLocationOptions === 'object' ? result.aiLocationOptions : {} };
        state.restoreAIContext = false; notifyEvent('scope', getScope());
      }
      setConnection(true);
      const range = state.photos.length ? `${number.format(state.offset + 1)}–${number.format(state.offset + state.photos.length)} of ` : '';
      $('result-count').textContent = result.aiResultsPending ? 'Preparing AI search…' : `${range}${number.format(state.total)} ${aiJobId ? (state.total === 1 ? 'candidate' : 'candidates') : state.query ? (state.total === 1 ? 'match' : 'matches') : (state.total === 1 ? 'photo' : 'photos')}`;
      if (!aiJobId && state.query && Number.isSafeInteger(result.directCount) && result.directCount >= 0 && result.directCount <= state.total) {
        $('result-count').append(document.createTextNode(` · ${number.format(result.directCount)} direct`));
      }
      renderSearchGuidance(result);
      $('empty-state').hidden = state.photos.length > 0;
      $('empty-actions').hidden = true;
      $('clear-filters').hidden = true;
      if (!state.photos.length) {
        const filtered = Boolean(state.scope.sourceId || state.scope.scopeId || state.scope.status !== 'all'
          || Object.keys(state.scope.metadataFilters).length || Object.keys(state.scope.locationOptions).length);
        const hasSources = (state.nativeState.sources?.length || state.nativeState.folders?.length || 0) > 0;
        const discovering = state.nativeState.discoveryJob?.running === true;
        $('empty-state').hidden = false;
        $('empty-title').textContent = state.total ? 'This page is no longer available' : state.query ? 'No saved matches' : filtered ? 'No photographs in this view' : discovering ? 'Finding your photographs' : hasSources ? 'No photographs discovered yet' : 'Start your local library';
        $('empty-description').textContent = state.total ? 'The saved library changed. Return to the first page to continue browsing.' : state.query ? `Try another word or a filename, or clear your search.${filtered ? ' Source, status and metadata filters are still applied.' : ''}` : filtered ? 'Your current filters contain no photographs. Clear filters or choose another source to continue.' : discovering ? 'PhotoFindry is discovering file locations. You can follow its progress in Files and Jobs; no tagging starts until you choose it.' : hasSources ? 'Check source availability and discovery in Files, or add another folder. An unavailable source is different from an empty library.' : 'Add a photo folder or restore a portable backup. Originals stay in their folders; This library shows where PhotoFindry stores its saved work.';
        $('clear-search').hidden = !state.query && !state.total;
        $('clear-search').textContent = state.total ? 'Back to first page' : 'Clear search';
        $('clear-filters').hidden = !filtered;
        $('empty-actions').hidden = Boolean(state.query || filtered || state.total);
        $('empty-sources').hidden = !hasSources && !discovering;
        $('empty-restore').hidden = hasSources || discovering;
        if (state.aiJobId) {
          $('empty-title').textContent = result.aiResultsPending ? 'Preparing your AI search' : result.aiError ? 'AI search needs attention' : result.aiStopped ? 'Search stopped' : 'No candidates in this view';
          $('empty-description').textContent = result.aiResultsPending ? 'Your local engine is preparing the search. Candidates appear once they have been saved. You can follow its progress or pause it in Jobs.' : text(result.aiError) || (result.aiStopped ? 'This search stopped before any candidates were saved.' : 'Choose All candidates to browse the retained pool. Jobs shows whether the search is queued, still scoring, paused or complete.');
          $('clear-search').hidden = true;
          $('clear-filters').hidden = true;
          $('empty-actions').hidden = true;
        }
      }
      renderGallery({ retryFailed: !preserveSelection });
      state.selected = new Set([...state.selected].filter((id) => state.photos.some((photo) => photo.id === id)));
      state.activeId = state.selected.has(state.activeId) ? state.activeId : [...state.selected][0] || null;
      if (!state.selected.has(state.anchorId)) state.anchorId = state.activeId;
      updateSelection();
      if (oldActivePhoto) {
        const active = state.photos.find((photo) => photo.id === state.activeId);
        if (!active) clearSelection();
        else if (JSON.stringify(inspectorContent(active)) !== JSON.stringify(inspectorContent(oldActivePhoto)) || state.detailGalleryRevision !== state.galleryRevision) {
          loadDetails(active.id, { preserve: active.id === oldActivePhoto.id });
        }
      }
      if (state.previewPhotoId && state.previewFromPage) {
        const updated = state.photos.find((photo) => photo.id === state.previewPhotoId);
        if (!updated || previewIdentity(updated) !== state.previewIdentity) closePreview();
      }
      notifyEvent('page', result);
    } catch (error) {
      if (isAbort(error) || error instanceof SessionError || !current()) return;
      const invalidated = !aiJobId && isSearchInvalidation(error);
      state.pageRecoveryBlockedKey = contextKey;
      state.pageRefreshQueued = false;
      if (invalidated) $('result-count').textContent = keepingPhotos ? 'Previous results · waiting for saved details' : 'Search is waiting for saved details';
      else if (!keepingPhotos) $('result-count').textContent = aiJobId ? 'AI search results could not be loaded' : state.query ? 'Search could not finish' : 'Library could not be loaded';
      $('library-error-text').textContent = invalidated
        ? 'Saved details kept changing during the search. Your query and filters are kept. Choose Retry search. Open Jobs to check or pause any running work.'
        : `${error.message} Choose ${state.query && !aiJobId ? 'Retry search' : 'Try again'} to retry this view. If it repeats, check Background companion in Settings.`;
      $('retry-library').textContent = (state.query && !aiJobId) || invalidated ? 'Retry search' : 'Try again';
      searchRecoveryJobs.hidden = !invalidated;
      $('library-error').hidden = false;
    } finally {
      if (version === state.pageVersion) {
        state.loading = false;
        state.pageRequestKey = null;
        $('loading-state').hidden = true;
        $('gallery').setAttribute('aria-busy', 'false');
        updatePagination();
        if (state.pageRefreshQueued) {
          state.pageRefreshQueued = false;
          clearTimeout(state.galleryRefreshTimer);
          state.galleryRefreshTimer = setTimeout(() => loadLibrary(state.offset, true, { automatic: true }), 100);
        }
      }
    }
  }

  function renderSearchGuidance(result) {
    const region = $('query-notice');
    const signature = JSON.stringify([state.aiJobId, state.query, state.scope, result.queryNotice, result.suggestion,
      result.filenameOutsideStatusCount, result.semanticLimited, result.semanticAvailable, result.semanticIndexedPhotoCount,
      result.semanticEligiblePhotoCount, result.semanticRerankNotice, result.locationSearch]);
    if (signature === state.searchGuidanceKey) return;
    state.searchGuidanceKey = signature; region.replaceChildren();
    const note = (message) => { if (message) region.append(element('p', '', message)); };
    note(text(result.queryNotice) || text(result.queryNotice?.message));
    if (!state.aiJobId && state.query.trim()) {
      if (state.scope.includeRelated && result.semanticAvailable === false) note('Semantic search is unavailable. Showing matches from saved words and details.');
      else if (result.semanticLimited === true) {
        const done = result.semanticIndexedPhotoCount, total = result.semanticEligiblePhotoCount;
        note(Number.isSafeInteger(done) && Number.isSafeInteger(total)
          ? `Related search covers ${number.format(done)} of ${number.format(total)} photographs with indexed detail vectors. Direct matches cover the full search scope.`
          : 'Related search has incomplete indexed detail coverage. Direct matches cover the full search scope.');
      }
      note(text(result.semanticRerankNotice));
      if (text(result.suggestion) && result.suggestion !== state.query) {
        note('Close spelling matches are included below. Your original query is unchanged.');
        region.append(previewAction(`Search instead for “${result.suggestion}”`, () => {
          $('search').value = result.suggestion; return loadLibrary(0);
        }));
      }
      if (Number.isSafeInteger(result.filenameOutsideStatusCount) && result.filenameOutsideStatusCount > 0) {
        note(`${number.format(result.filenameOutsideStatusCount)} filename matches are outside this status filter.`);
        region.append(previewAction('Show filename matches in all statuses', () => setScope({ status: 'all' })));
      }
    }
    const location = result.locationSearch;
    if (location && typeof location === 'object' && ['ambiguous', 'suggested', 'applied', 'disabled'].includes(location.state)) {
      const choose = (placeId, radiusKm = location.radiusKm) => setScope({ locationOptions: {
        ...state.scope.locationOptions, enabled: true, applied: true,
        ...(typeof placeId === 'string' ? { placeId } : {}),
        radiusKm: Number.isFinite(radiusKm) && radiusKm >= 0.1 && radiusKm <= 1000 ? radiusKm : 30,
      } });
      if (location.state === 'ambiguous') {
        note('Choose the recorded location. Similarity alone does not verify a place.');
        const alternatives = Array.isArray(location.alternatives) ? location.alternatives : [];
        for (const place of alternatives) if (typeof place?.id === 'string' && text(place.name)) {
          region.append(previewAction(`${place.name}, ${text(place.country)}`, () => choose(place.id)));
        }
        if (location.alternativeCount > alternatives.length) note(`${number.format(location.alternativeCount)} places share this name. Add a country to narrow it.`);
      } else if (text(location.place?.name)) {
        note(`Near ${location.place.name}, ${text(location.place.country)} · ${Number.isFinite(location.radiusKm) ? location.radiusKm : 30} km${location.state === 'applied' ? ' · Recorded GPS filter' : ''}`);
        if (location.state !== 'applied') region.append(previewAction('Use GPS filter', () => choose(location.place.id)));
        if (location.state !== 'disabled') {
          const label = element('label', 'field-help', 'Distance from city centre · km ');
          const radius = element('input'); radius.type = 'number'; radius.min = '0.1'; radius.max = '1000'; radius.step = 'any'; radius.value = String(location.radiusKm || 30);
          label.append(radius); region.append(label, previewAction('Apply GPS radius', () => {
            const value = Number(radius.value);
            if (!Number.isFinite(value) || value < 0.1 || value > 1000) throw Error('Choose a distance between 0.1 and 1000 km.');
            return choose(location.place.id, value);
          }));
        }
      }
      if (location.state !== 'disabled') {
        region.append(previewAction('Search without GPS filter', () => setScope({ locationOptions: { ...state.scope.locationOptions, enabled: false, applied: false, placeId: null } })));
        const counts = [location.matchedGPSCount, location.gpsPhotoCount, location.scopePhotoCount];
        if (counts.every(value => Number.isSafeInteger(value) && value >= 0)) note(`${number.format(counts[0])} within the GPS/time area · ${number.format(counts[1])} of ${number.format(counts[2])} photographs have usable GPS.`);
        note(location.state === 'applied' ? 'Missing GPS does not match this filter. Visual words still apply. Offline place names: GeoNames; city proximity is approximate.' : 'GPS matches are labelled separately. Visual similarity does not verify a location. Offline names: GeoNames.');
        if (text(location.daypart)) note(`${location.daypart} uses recorded camera time; its time zone may be unknown.`);
      }
    }
    region.hidden = !region.children.length;
  }

  function appendPhotoContext(fragment, photo) {
    if (photo.archiveImport === true) fragment.append(element('p', 'inspector-context', 'Previously saved descriptions. Original pixels have not been reverified by PhotoFindry.'));
    if (photo.detailsError) fragment.append(detailSection('Saved details need attention', element('p', '', text(photo.detailsError))));
    if (photo.matchText) {
      const match = element('div'); match.append(element('p', '', text(photo.matchText)));
      if (photo.matchKind === 'review-suggestion' || photo.matchSection === 'review-suggestion') match.append(element('p', 'inspector-context', 'The photo reviewer supported this possibility. Accepted tags are unchanged; this is not a confirmed fact.'));
      if (photo.matchKind === 'ai' && photo.matchConfidence === 'possible') match.append(element('p', 'inspector-context', 'Possible activity inferred from saved clues.'));
      if (photo.matchSection === 'filename' || photo.filenameMatch) match.append(element('p', 'inspector-context', 'Filename metadata, not visual evidence.'));
      fragment.append(detailSection(matchLabel(photo.matchKind, photo) || 'Search match', match));
    }
    if (Array.isArray(photo.matchEvidence) && photo.matchEvidence.length > 1) {
      const evidence = element('ul');
      for (const item of photo.matchEvidence) if (text(item?.text)) evidence.append(element('li', '', item.text));
      fragment.append(detailSection(['ai-preview', 'ai-ranked'].includes(photo.matchKind) ? 'Saved details used for ranking' : 'Evidence for this match', evidence));
    }
  }

  function photoWorkflowAction(photo, key, label, callback) {
    const control = previewAction(label, callback); control.dataset.detailAction = `${key}:${photo.id}`; return control;
  }
  async function refreshPhotoPresentations(photo) {
    const detailVersion = state.detailVersion, previewVersion = state.previewVersion;
    const fresh = await api('/api/photo', { photoId: photo.id });
    if (fresh?.id !== photo.id) throw Error('The companion returned a different photograph.');
    const merged = { ...photo, ...fresh };
    if (state.activeId === photo.id && detailVersion === state.detailVersion) { renderDetails(merged); state.detailContentKey = JSON.stringify(inspectorContent(merged)); }
    if (state.previewPhotoId === photo.id && previewVersion === state.previewVersion && $('preview-dialog').open) {
      if (previewIdentity(fresh) !== state.previewIdentity) { closePreview(); return; }
      state.previewPhoto = merged; renderPreviewDetails(merged);
    }
  }
  function appendPhotoWorkflows(fragment, photo) {
    const openSource = () => { closePreview(); window.PhotoFindryPanels?.show('sources', { sourceId: photo.source?.id }); };
    if (photo.status === 'tagged' || photo.improvement || photo.improvementIssue) {
      const content = element('div', 'photo-review');
      const review = photo.improvement;
      if (review && typeof review === 'object') {
        const decision = ['clef-flash', 'intern-decision-4b'].includes(review.decisionProvider) || review.summaryState === 'suggestions';
        const title = review.summaryState === 'undone' || review.outcome === 'undone' ? 'Review undone' : review.summaryState === 'reused' ? 'Saved review reused'
          : decision ? 'Photo detail review saved · accepted details kept' : review.summaryState === 'unchanged' ? 'Reviewed · accepted details unchanged'
          : ({ improved: 'Additional observations saved', unresolved: 'Some details remain uncertain', unclear: 'Some details remain uncertain', unavailable: 'Closer inspection needs the original' })[review.outcome] || 'Review saved';
        content.append(element('p', '', title));
        const provider = ({ 'clef-flash': 'CLEF Flash', 'intern-decision-4b': 'Intern-Decision 4B', 'tagging-model': 'Tagging model' })[review.decisionProvider];
        if (provider) content.append(element('p', 'inspector-context', `Reviewer: ${provider}`));
        if (Number.isSafeInteger(review.decisionClaimCount)) content.append(element('p', 'inspector-context', `${number.format(review.decisionClaimCount)} questions checked this review · saved tags kept`));
        if (Number.isSafeInteger(review.decisionCheckedTotal)) content.append(element('p', 'inspector-context', `${number.format(review.decisionCheckedTotal)} distinct questions checked for this photo and review profile. ${review.decisionMoreQuestions === true ? 'Review again can check more.' : 'This bounded review is finished.'}`));
        if (review.reviewedAt && Number.isFinite(Date.parse(review.reviewedAt))) content.append(element('p', 'inspector-context', `Reviewed ${new Date(review.reviewedAt).toLocaleString()}`));
        const changes = Array.isArray(review.changes) ? review.changes : [];
        for (const [kind, label] of [['added', 'Added details'], ['uncertain', 'Uncertain details'], ['conflicting', 'Conflicting details'], ['suggestion', 'Review suggestions']]) {
          const items = changes.filter(item => item?.kind === kind && text(item.text)); if (!items.length) continue;
          const group = element('div');
          if (kind === 'suggestion') group.append(element('p', 'inspector-context', 'Accepted tags stay unchanged. Supported suggestions may appear as labeled search hints; disputed model claims may stop supporting direct matches.'));
          if (['uncertain', 'conflicting'].includes(kind)) group.append(element('p', 'inspector-context', 'Flagged for review, not confirmed as new facts.'));
          for (const item of items) {
            if (text(item.previousText)) group.append(element('p', 'inspector-context', `${kind === 'suggestion' ? 'Saved detail' : 'Earlier'}: ${item.previousText}`));
            group.append(element('p', '', item.text));
          }
          content.append(detailSection(label, group));
        }
        if (Number.isSafeInteger(review.changeCount) && review.changeCount > changes.length) content.append(element('p', 'inspector-context', `Showing ${number.format(changes.length)} of ${number.format(review.changeCount)} recorded changes.`));
        if (!changes.length && review.summaryState === 'unavailable') content.append(element('p', 'inspector-context', 'Individual changes were not recorded for this earlier review. Its saved details remain below.'));
        if (!changes.some(item => item?.kind === 'suggestion') && strings(review.decisionNotes).length) {
          const notes = element('div'); notes.append(element('p', 'inspector-context', 'Review suggestions are possibilities. Accepted tags stay unchanged.'));
          for (const note of strings(review.decisionNotes)) notes.append(element('p', '', note));
          content.append(detailSection('Review suggestions', notes));
        }
        if (strings(review.notes).length) { const notes = element('ul'); for (const note of strings(review.notes)) notes.append(element('li', '', note)); content.append(detailSection('Review notes', notes)); }
      } else content.append(element('p', 'inspector-context', 'Check saved observations locally. Reviews and corrections are saved on this Mac.'));
      if (photo.improvementIssue) content.append(element('p', 'inspector-error', text(photo.improvementIssue)));
      content.append(photoWorkflowAction(photo, 'improve', photo.improvementIssue ? 'Retry improvement' : review ? 'Review again' : 'Improve photograph', async () => {
        await action('enqueueJob', { kind: 'improve', params: { photoIds: [photo.id], force: Boolean(review || photo.improvementIssue) } });
        showMessage('global-notice', 'Photo improvement queued. Saved observations and human names are preserved.');
      }));
      if (review?.canUndo) {
        const undo = photoWorkflowAction(photo, 'undo-review', 'Undo review', async () => {
          await action('undoImprovement', { photoId: photo.id }); await refreshPhotoPresentations(photo);
          showMessage('global-notice', 'Review undone. Original observations and human names are preserved.');
        });
        undo.disabled = [state.nativeState.job, state.nativeState.improvementJob, state.nativeState.aiSearch].some(job => job?.running === true);
        if (undo.disabled) content.append(element('p', 'inspector-context', 'Pause active model work in Jobs before undoing this review.'));
        content.append(undo);
      }
      fragment.append(detailSection('Photo improvement', content));
    }
    const metadata = element('div');
    const fields = Array.isArray(photo.metadataFields) ? photo.metadataFields : [];
    if (fields.length) {
      const list = element('dl', 'photo-metadata-fields');
      for (const field of fields) { const row = element('div'); row.append(element('dt', '', text(field.label)), element('dd', '', text(field.value))); list.append(row); }
      metadata.append(list);
      metadata.append(element('p', 'inspector-context', 'Recorded metadata. Nearby cities are approximate; capture time has an unknown time zone unless an offset is shown.'));
      if (['unavailable', 'changed'].includes(photo.metadataStatus)) metadata.append(element('p', 'inspector-context', 'The latest metadata read could not complete. Saved fields are retained; check the original before reading again.'));
    }
    const canRead = photo.source?.accessOriginals !== false && (!photo.source || ['available', 'unchecked'].includes(photo.source.status));
    const jobs = Array.isArray(state.nativeState.jobs?.jobs) ? state.nativeState.jobs.jobs : [];
    const scan = jobs.find(job => job.kind === 'metadata' && !['completed', 'cancelled', 'failed'].includes(job.state)
      && (job.metadataPhotoId === photo.id || (!job.metadataPhotoId && photo.source?.id && job.sourceId === photo.source.id)));
    if (scan) {
      metadata.append(element('p', 'inspector-context', `${text(scan.message) || 'Camera & location scan'} · ${text(scan.state)}`));
      const controls = element('div', 'button-row');
      if (scan.state === 'paused') controls.append(photoWorkflowAction(photo, 'resume-metadata', 'Resume metadata scan', () => action('jobAction', { id: scan.id, action: 'runNext' })));
      else if (['running', 'queued'].includes(scan.state)) controls.append(photoWorkflowAction(photo, 'pause-metadata', 'Pause metadata scan', () => action('jobAction', { id: scan.id, action: 'pause' })));
      if (scan.waitingForSchedule) controls.append(photoWorkflowAction(photo, 'run-metadata', 'Read metadata now', () => action('jobAction', { id: scan.id, action: 'runNow' })));
      controls.append(photoWorkflowAction(photo, 'stop-metadata', 'Stop metadata scan', () => action('jobAction', { id: scan.id, action: 'cancel' })));
      metadata.append(controls);
    } else {
      if (!fields.length) metadata.append(element('p', 'inspector-context', ['ready', 'complete'].includes(photo.metadataStatus)
        ? 'Read complete. This original has no enabled camera or location fields.'
        : !canRead ? 'Connect or confirm the original in Files to read camera and location data.'
        : ['unavailable', 'changed'].includes(photo.metadataStatus) ? 'The last metadata read could not complete. Check the original, then try again.' : 'Camera & location has not been read yet.'));
      metadata.append(canRead ? photoWorkflowAction(photo, 'read-metadata', fields.length ? 'Read camera & location again' : 'Read camera & location', async () => {
        await action('enqueueJob', { kind: 'metadata', params: { photoIds: [photo.id], force: true } });
        await refreshPhotoPresentations(photo);
      }) : photoWorkflowAction(photo, 'metadata-source', 'Connect originals in Files…', openSource));
    }
    fragment.append(detailSection('Camera & location', metadata));
    const people = element('div');
    const faces = Array.isArray(photo.faces) ? photo.faces : [];
    people.append(element('p', 'inspector-context', faces.length ? `${number.format(faces.length)} detected faces. Confirmed names are separate from factual model descriptions.` : 'No detected faces in this photograph yet.'));
    if (photo.faceScanLimited) people.append(element('p', 'inspector-context', 'Some faces may be missing: this photograph exceeds the current scan limit.'));
    people.append(photoWorkflowAction(photo, 'people-photo', 'Name or correct people in this photo…', () => { closePreview(); window.PhotoFindryPanels?.show('people', { photoId: photo.id }); }));
    if (!faces.length) people.append(photoWorkflowAction(photo, 'scan-faces', 'Find faces in this photo', async () => { await action('enqueueJob', { kind: 'faces', params: { photoIds: [photo.id] } }); showMessage('global-notice', 'Face discovery queued for this photograph.'); }));
    fragment.append(detailSection('People in this photograph', people));
  }

  function matchLabel(kind, photo = {}) {
    if (photo.aiRankingState === 'unscored') return 'Waiting for preview';
    if (!['ai', 'ai-preview', 'ai-ranked', 'related', 'review-suggestion'].includes(kind)) {
      if (photo.matchSection === 'filename' || photo.filenameMatch) return 'Filename match';
      if (photo.matchSection === 'location') return 'Recorded GPS match';
      if (photo.matchSection === 'metadata') return 'Metadata match';
    }
    const labels = { exact: 'Direct match', direct: 'Direct match', related: 'Related match', filename: 'Filename match', spelling: 'Spelling match', wording: 'Matching wording', metadata: 'Metadata match', 'ai-preview': 'Preview candidate', 'ai-ranked': 'Refined candidate', 'review-suggestion': 'Review-supported suggestion', ai: 'AI-selected saved detail' };
    return Object.hasOwn(labels, kind) ? labels[kind] : text(kind);
  }

  function previewKey(photo) {
    return JSON.stringify([photo.id, photo.status, photo.hasPreview, photo.previewRevision ?? state.galleryRevision,
      photo.source?.id, photo.source?.mode, photo.source?.status]);
  }

  // Image availability may lag a newly prepared preview in a page response.
  // The open modal is fenced by the photo/source identity, not that cache flag.
  function previewIdentity(photo) {
    return JSON.stringify([photo.id, photo.previewRevision ?? state.galleryRevision,
      photo.source?.id, photo.source?.mode, photo.source?.status]);
  }

  function disposeTile(record, { previewOnly = false } = {}) {
    if (!previewOnly) { record.summary?.destroy(); record.summary = null; }
    record.controller?.abort();
    if (record.url) URL.revokeObjectURL(record.url);
    record.url = null;
  }

  function loadTilePreview(record) {
    disposeTile(record, { previewOnly: true });
    const controller = new AbortController();
    record.controller = controller;
    record.failed = false;
    const photo = record.photo;
    const allowed = photo.hasPreview;
    const metadataOnly = ['metadata', 'archived'].includes(photo.source?.mode);
    const untagged = !allowed && !metadataOnly && photo.status === 'pending';
    const missingLabel = metadataOnly ? 'Metadata-only photo' : untagged ? 'Photo not tagged yet' : ['error', 'failed', 'needsAttention'].includes(photo.status) ? 'Tagging needs attention' : 'Preview unavailable';
    const placeholder = element('span', 'preview-placeholder', allowed ? 'Loading preview…' : missingLabel);
    record.preview.replaceChildren(placeholder);
    if (untagged) {
      const scenes = ['landscape', 'spaces', 'shapes'];
      // Stable ID-based variety: metric ticks, paging and reload never reshuffle
      // artwork or imply that this decorative image describes the photograph.
      let hash = 2166136261;
      for (const character of String(photo.id)) hash = Math.imul(hash ^ character.charCodeAt(0), 16777619) >>> 0;
      const artwork = element('img', 'untagged-art');
      artwork.src = `/assets/untagged-${scenes[hash % scenes.length]}.jpg`;
      artwork.alt = ''; artwork.setAttribute('aria-hidden', 'true'); artwork.decoding = 'async';
      placeholder.classList.toggle('untagged-label', true);
      record.preview.replaceChildren(artwork, placeholder);
    }
    if (!allowed) return;
    const current = () => !controller.signal.aborted && state.tiles.get(photo.id) === record && record.controller === controller && state.token;
    fetchPreview(photo.id, controller.signal).then((blob) => {
      if (!current()) return;
      const url = URL.createObjectURL(blob);
      record.url = url;
      const image = element('img');
      image.alt = ''; image.src = url; image.decoding = 'async';
      image.addEventListener('error', () => {
        if (!current()) return;
        placeholder.textContent = 'Preview unavailable';
        record.preview.replaceChildren(placeholder);
        URL.revokeObjectURL(url); record.url = null; record.failed = true; record.retryAfter = Date.now() + 5000;
      }, { once: true });
      record.preview.replaceChildren(image);
    }).catch((error) => {
      if (isAbort(error) || error instanceof SessionError || !current()) return;
      placeholder.textContent = 'Preview unavailable'; record.failed = true; record.retryAfter = Date.now() + 5000;
    });
  }

  function photoStatus(photo) {
    const attention = photo.needsAttention || ['error', 'failed', 'needsAttention'].includes(photo.status) || photo.error || photo.improvementIssue;
    if (attention) return { label: 'Needs attention', tone: 'warning' };
    if (photo.status === 'tagged') return {
      label: Number.isSafeInteger(photo.observationCount) && photo.observationCount >= 0
        ? `Tagged · ${number.format(photo.observationCount)} observations` : 'Tagged', tone: 'success',
    };
    return { label: photo.status === 'pending' ? 'Waiting to be tagged' : 'Saved photograph', tone: 'neutral' };
  }

  function photoSourceStatus(photo) {
    const source = photo.source;
    if (!source) return '';
    const labels = { needsReview: 'Confirm original location', checking: 'Checking originals automatically', changed: 'Original changed', missing: 'Original missing',
      permission: 'Access needed', unavailable: 'Temporarily unavailable', onlineOnly: 'Available from cloud provider' };
    if (Object.hasOwn(labels, source.status)) return labels[source.status];
    if (source.mode === 'archived') return 'Archived';
    if (source.accessOriginals === false || source.mode === 'metadata') return 'Originals disconnected';
    if (['available', 'unchecked'].includes(source.status)) return '';
    const fallback = { offline: 'Drive offline', detached: 'Removed from this library' };
    return text(source.statusLabel) || (Object.hasOwn(fallback, source.status) ? fallback[source.status] : '');
  }

  function photoReviewSummary(photo) {
    const review = photo.improvement;
    if (!review || !review.reviewedAt) return { label: '', tone: 'neutral', hint: '' };
    const changes = Array.isArray(review.changes) ? review.changes : [];
    const decision = ['clef-flash', 'intern-decision-4b'].includes(review.decisionProvider) || review.summaryState === 'suggestions';
    if (review.outcome === 'undone' || review.summaryState === 'undone') return { label: 'Review undone', tone: 'neutral', hint: 'The original observations are restored.' };
    if (review.summaryState === 'reused') return { label: 'Saved review reused', tone: 'neutral', hint: 'The saved review was reused.' };
    if (decision) return { label: review.changeCount > 0 || changes.some(item => item?.kind === 'suggestion') || strings(review.decisionNotes).length ? 'Review suggestions' : 'Photo detail review',
      tone: 'review', hint: 'Reviewer suggestions are possibilities. Accepted details are kept.' };
    if (review.summaryState === 'unchanged') return { label: 'Reviewed', tone: 'neutral', hint: 'Accepted details are unchanged.' };
    const added = Number.isSafeInteger(review.addedCount) && review.addedCount >= 0 ? review.addedCount : changes.filter(item => item?.kind === 'added').length;
    if (added > 0) return { label: `Improved · ${number.format(added)} added`, tone: 'review', hint: 'Additional accepted observations are saved.' };
    if (changes.some(item => ['uncertain', 'conflicting'].includes(item?.kind))) return { label: 'Review flags', tone: 'warning', hint: 'Some saved details need review. Open the photo to see them.' };
    return { label: 'Reviewed', tone: 'neutral', hint: 'Open the photograph to see its saved review.' };
  }

  function photoSummaryData(photo) {
    const status = photoStatus(photo), review = photoReviewSummary(photo);
    const ranked = ['ai-preview', 'ai-ranked'].includes(photo.matchKind) || photo.aiRankingState === 'unscored';
    const suggestion = photo.matchKind === 'review-suggestion' || photo.matchSection === 'review-suggestion';
    return {
      name: text(photo.name) || 'Unnamed photo', caption: text(photo.caption),
      sourceLabel: text(photo.source?.label) || text(photo.folderLabel), sourceStatus: photoSourceStatus(photo),
      statusLabel: status.label, statusTone: status.tone, reviewLabel: review.label, reviewTone: review.tone, reviewHint: review.hint,
      matchLabel: matchLabel(suggestion ? 'review-suggestion' : photo.matchKind, photo), matchText: text(photo.matchText),
      matchTone: suggestion ? 'review' : ranked || photo.matchKind === 'related' ? 'related' : 'direct',
      matchHint: ranked ? 'Ranking orders candidates; it does not confirm a match.' : suggestion ? 'A reviewer-supported possibility, not a confirmed fact.'
        : photo.matchSection === 'filename' || photo.filenameMatch ? 'Filename metadata, not visual evidence.' : '',
      locationEvidence: photo.matchSection === 'location' ? '' : text(photo.locationEvidence),
    };
  }

  function updatePhotoSummary(record, data) {
    const signature = JSON.stringify(data);
    if (record.summaryKey === signature) return;
    if (!record.summaryFailed) {
      try {
        if (record.summary) record.summary.update(data);
        else if (window.PhotoFindryShell?.mountPhotoSummary) record.summary = window.PhotoFindryShell.mountPhotoSummary(record.metadata, data);
      } catch {
        record.summary?.destroy(); record.summary = null; record.summaryFailed = true;
      }
    }
    if (!record.summary) {
      // Retain a complete text fallback for a controller running without the
      // compiled presentation layer (including the isolated protocol tests).
      const name = element('p', 'photo-name', data.name); name.title = data.name;
      record.metadata.replaceChildren(name);
      if (data.sourceLabel) record.metadata.append(element('p', 'photo-source', [data.sourceLabel, data.sourceStatus].filter(Boolean).join(' · ')));
      record.metadata.append(element('p', 'photo-state', data.statusLabel));
      if (data.caption) record.metadata.append(element('p', 'photo-caption', data.caption));
      const previewAction = (label, className) => {
        const button = element('button', `photo-summary-action ${className}`, label); button.type = 'button';
        button.setAttribute('aria-label', `${label}. Open details for ${data.name}`);
        button.addEventListener('click', event => { event.stopPropagation?.(); data.onPreview(); });
        button.addEventListener('dblclick', event => event.stopPropagation?.());
        return button;
      };
      if (data.reviewLabel) record.metadata.append(previewAction(data.reviewLabel, 'photo-review-badge'));
      if (data.locationEvidence) record.metadata.append(element('p', 'photo-location', data.locationEvidence));
      if (data.matchLabel || data.matchText) {
        const matching = previewAction(data.matchLabel || 'Saved matching details', 'photo-match-block');
        if (data.matchText) matching.append(element('span', 'photo-match-evidence', data.matchText));
        record.metadata.append(matching);
      }
    }
    record.summaryKey = signature;
  }

  function renderGallery({ retryFailed = false } = {}) {
    const gallery = $('gallery');
    const ids = new Set(state.photos.map((photo) => photo.id));
    for (const [id, record] of state.tiles) {
      if (ids.has(id)) continue;
      disposeTile(record); record.tile.remove(); state.tiles.delete(id);
    }
    state.photos.forEach((photo, index) => {
      let record = state.tiles.get(photo.id);
      if (!record) {
        const tile = element('div', 'photo-tile');
        const preview = element('div', 'photo-image');
        const metadata = element('div', 'photo-metadata photo-summary-slot');
        tile.append(preview, metadata);
        record = { tile, preview, metadata, photo, summary: null, summaryKey: null, key: null, controller: null, url: null, failed: false };
        record.openPreview = () => openPreview(record.photo);
        tile.addEventListener('click', (event) => {
          selectPhoto(record.photo.id, { toggle: event.metaKey || event.ctrlKey, extend: event.shiftKey });
          gallery.focus({ preventScroll: true });
        });
        tile.addEventListener('dblclick', () => openPreview(record.photo));
        state.tiles.set(photo.id, record);
      }
      record.photo = photo;
      const { tile } = record;
      tile.id = `photo-option-${index}`;
      tile.dataset.photoId = photo.id;
      tile.setAttribute('role', 'option');
      tile.setAttribute('aria-label', `${text(photo.name) || 'Unnamed photo'}, ${photoStatus(photo).label}`);
      updatePhotoSummary(record, { ...photoSummaryData(photo), onPreview: record.openPreview });
      const key = previewKey(photo);
      if (record.key !== key || (record.failed && (retryFailed || Date.now() >= record.retryAfter))) { record.key = key; loadTilePreview(record); }
      // Move only changed positions; unchanged images stay mounted and decoded.
      if (gallery.children[index] !== tile) gallery.insertBefore(tile, gallery.children[index] || null);
    });
  }

  function updateSelection() {
    for (const tile of $('gallery').children) {
      tile.setAttribute('aria-selected', String(state.selected.has(tile.dataset.photoId)));
      tile.classList.toggle('active-tile', tile.dataset.photoId === state.activeId);
      if (tile.dataset.photoId === state.activeId) $('gallery').setAttribute('aria-activedescendant', tile.id);
    }
    if (!state.activeId) $('gallery').removeAttribute('aria-activedescendant');
    $('selection-count').textContent = state.selected.size ? `${number.format(state.selected.size)} selected on this page` : '';
    $('clear-selection').hidden = state.selected.size === 0;
    const signature = JSON.stringify([[...state.selected], state.activeId]);
    if (signature !== state.selectionSignature) {
      state.selectionSignature = signature;
      notifyEvent('selection', { ids: [...state.selected], activeId: state.activeId });
    }
  }

  function clearSelection() {
    state.detailVersion += 1;
    state.detailController?.abort();
    state.detailController = null;
    state.detailContentKey = null;
    state.detailGalleryRevision = null;
    state.selected.clear();
    state.activeId = null;
    state.anchorId = null;
    updateSelection();
    const empty = element('div', 'inspector-empty');
    const symbol = element('span', 'inspector-empty-emblem');
    const icon = detailIcon('photograph'); if (icon) symbol.append(icon); else symbol.textContent = '▧';
    symbol.setAttribute('aria-hidden', 'true');
    empty.append(symbol, element('h3', '', 'A closer look'), element('p', '', 'Select a photo to see its complete saved details.'));
    $('inspector-content').replaceChildren(empty);
  }

  function selectPhoto(id, { toggle = false, extend = false } = {}) {
    const index = state.photos.findIndex((photo) => photo.id === id);
    if (index < 0) return;
    if (extend && state.anchorId) {
      const anchor = state.photos.findIndex((photo) => photo.id === state.anchorId);
      if (!toggle) state.selected.clear();
      for (const photo of state.photos.slice(Math.min(anchor, index), Math.max(anchor, index) + 1)) state.selected.add(photo.id);
    } else if (toggle) {
      if (state.selected.has(id)) state.selected.delete(id);
      else state.selected.add(id);
      state.anchorId = id;
    } else {
      state.selected = new Set([id]);
      state.anchorId = id;
    }
    state.activeId = state.selected.has(id) ? id : [...state.selected].at(-1) || null;
    updateSelection();
    if (state.activeId) loadDetails(state.activeId);
    else clearSelection();
  }

  function inspectorContent(photo) {
    // Scores and ordinal positions change during ranking but are not inspector
    // evidence. Use displayed fields, and refetch omitted details on corpus change.
    return Object.fromEntries(['id', 'name', 'status', 'hasPreview', 'caption', 'tags', 'personNames', 'personIds',
      'matchKind', 'matchText', 'matchSection', 'matchEvidence', 'matchConfidence', 'aiRankingState', 'filenameMatch', 'locationEvidence',
      'detailSections', 'metadataFields', 'metadataStatus', 'error', 'detailsError', 'archiveImport', 'observationCount',
      'improvement', 'improvementIssue', 'faces', 'faceScanLimited', 'source'].map(key => [key, photo[key]]));
  }

  async function loadDetails(id, { preserve = false } = {}) {
    state.detailController?.abort();
    const controller = new AbortController();
    state.detailController = controller;
    const version = ++state.detailVersion;
    const galleryRevision = state.galleryRevision;
    const summary = state.photos.find((photo) => photo.id === id);
    if (!summary) return;
    if (!preserve) $('inspector-content').replaceChildren(element('h3', 'inspector-filename', text(summary.name) || 'Unnamed photo'), element('p', 'inspector-loading', 'Loading complete saved details…'));
    try {
      const photo = await api('/api/photo', { photoId: id }, { signal: controller.signal });
      if (controller.signal.aborted || version !== state.detailVersion || state.activeId !== id || !state.token) return;
      if (photo?.id !== id) throw new Error('The companion returned details for a different photo.');
      const details = { ...summary, ...photo };
      const key = JSON.stringify(inspectorContent(details));
      if (!preserve || key !== state.detailContentKey) { renderDetails(details); state.detailContentKey = key; }
      state.detailGalleryRevision = galleryRevision;
    } catch (error) {
      if (isAbort(error) || error instanceof SessionError || version !== state.detailVersion) return;
      const failure = element('div', 'inspector-error');
      failure.append(element('p', '', error.message));
      const retry = element('button', 'secondary-button', 'Try details again');
      retry.type = 'button';
      retry.addEventListener('click', () => loadDetails(id));
      failure.append(retry);
      $('inspector-content').replaceChildren(element('h3', 'inspector-filename', text(summary.name) || 'Unnamed photo'), failure);
    }
  }

  const detailIcons = {
    photograph: 'M4 4h16v16H4ZM4 16l5-5 4 4 3-3 4 4M15 7h.01',
    description: 'M4 4h16v16H4ZM8 8h8M8 12h8M8 16h5',
    tags: 'M3 3h8l10 10-8 8L3 11ZM7 7h.01',
    people: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM22 21v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8',
    metadata: 'M3 7h4l2-3h6l2 3h4v13H3ZM12 10a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7Z',
    source: 'M3 7V5a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z',
    review: 'M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Zm2 5h10M7 13h6',
    attention: 'M12 8v5m0 3h.01M10.3 3.9 2.5 17.4A2 2 0 0 0 4.2 20h15.6a2 2 0 0 0 1.7-2.6L13.7 3.9a2 2 0 0 0-3.4 0Z',
    match: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15ZM16 16l5 5',
    details: 'M5 4h14v17H5ZM9 8h6M9 12h6M9 16h4',
  };
  function detailIcon(kind) {
    if (typeof document.createElementNS !== 'function') return null;
    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    for (const [name, value] of Object.entries({ viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.7', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', class: 'pf-detail-icon' })) icon.setAttribute(name, value);
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path'); path.setAttribute('d', detailIcons[kind] || detailIcons.details); icon.append(path);
    return icon;
  }
  function detailSection(title, content) {
    const kind = /attention|issue|uncertain|conflicting/i.test(title) ? 'attention' : /review|improvement|added details/i.test(title) ? 'review'
      : /match|ranking|evidence for/i.test(title) ? 'match' : /description/i.test(title) ? 'description'
      : /^tags/i.test(title) ? 'tags' : /people|person|saved names/i.test(title) ? 'people'
      : /camera|location|metadata/i.test(title) ? 'metadata' : /original|source/i.test(title) ? 'source' : 'details';
    const section = element('section', 'detail-section'); section.dataset.section = kind;
    const heading = element('h3'); const icon = detailIcon(kind); if (icon) heading.append(icon);
    heading.append(element('span', '', title)); section.append(heading, content);
    return section;
  }

  function photoDetailHeader(photo) {
    const header = element('div', 'photo-detail-header');
    header.append(element('p', 'photo-detail-eyebrow', 'Photograph'), element('h3', 'inspector-filename', text(photo.name) || 'Unnamed photograph'));
    const badges = element('div', 'photo-detail-badges');
    const status = photoStatus(photo), review = photoReviewSummary(photo);
    const badge = element('span', 'photo-status', status.label); badge.dataset.tone = status.tone; badges.append(badge);
    if (review.label) { const saved = element('span', 'photo-review-badge', review.label); saved.dataset.tone = review.tone; saved.title = review.hint; badges.append(saved); }
    header.append(badges);
    if (photo.source?.label) {
      const source = element('p', 'photo-detail-source'); const icon = detailIcon('source'); if (icon) source.append(icon);
      source.append(element('span', '', [text(photo.source.label), photoSourceStatus(photo)].filter(Boolean).join(' · '))); header.append(source);
    }
    return header;
  }

  function renderDetails(photo) {
    const fragment = document.createDocumentFragment();
    fragment.append(photoDetailHeader(photo));
    const previewButton = element('button', 'secondary-button inspector-preview-button', 'Open saved preview');
    previewButton.dataset.detailAction = 'preview';
    previewButton.type = 'button';
    if (!photo.hasPreview) previewButton.textContent = 'Prepare or open preview';
    previewButton.addEventListener('click', () => openPreview(photo));
    fragment.append(previewButton);
    if (state.selected.size > 1) fragment.append(element('p', 'inspector-context', `Details for the active photo. ${number.format(state.selected.size)} photos selected on this page.`));
    appendPhotoContext(fragment, photo);
    fragment.append(detailSection('Saved description', element('p', 'inspector-caption', text(photo.caption) || 'No description is saved for this photo.')));
    const tags = strings(photo.tags);
    if (tags.length) {
      const list = element('div', 'tag-list');
      tags.forEach((tag) => list.append(element('span', 'tag', tag)));
      fragment.append(detailSection(`Tags · ${number.format(tags.length)}`, list));
    }
    const people = strings(photo.personNames);
    if (people.length) {
      const list = element('ul');
      people.forEach((name, index) => {
        const item = element('li'); const personId = photo.personIds?.[index];
        if (personId) {
          const link = element('button', 'text-button', name); link.type = 'button';
          link.dataset.detailAction = `person:${personId}`;
          link.addEventListener('click', () => window.PhotoFindryPanels?.show('people', { personId })); item.append(link);
        } else item.textContent = name;
        list.append(item);
      });
      fragment.append(detailSection('People · saved names', list));
    }
    for (const section of Array.isArray(photo.detailSections) ? photo.detailSections : []) {
      const items = strings(section?.items);
      if (!items.length) continue;
      const list = element('ul');
      items.forEach((item) => list.append(element('li', '', item)));
      fragment.append(detailSection(text(section.title) || text(section.id) || 'Saved details', list));
    }
    appendPhotoWorkflows(fragment, photo);
    if (photo.error || photo.improvementIssue) {
      const issue = element('div'); issue.append(element('p', '', text(photo.error) || text(photo.improvementIssue)));
      if (photo.error) {
        const diagnostic = element('button', 'secondary-button', 'View tagging diagnostic…'); diagnostic.type = 'button';
        diagnostic.dataset.detailAction = 'tagging-diagnostic';
        diagnostic.addEventListener('click', () => notifyEvent('tagging-diagnostic', { photoId: photo.id })); issue.append(diagnostic);
      }
      fragment.append(detailSection('Needs attention', issue));
    }
    const source = element('div');
    if (photo.source) {
      source.append(element('p', 'inspector-source-name', text(photo.source.label)), element('p', 'inspector-context', photoSourceStatus(photo) || (photo.source.status === 'available' ? 'Original available' : 'Not checked yet')));
      if (photo.source.relativePath) source.append(element('p', 'photo-original-path', text(photo.source.relativePath)));
      const manage = element('button', 'secondary-button', 'View source'); manage.type = 'button';
      manage.dataset.detailAction = `source:${photo.source.id}`;
      manage.addEventListener('click', () => window.PhotoFindryPanels?.show('sources', { sourceId: photo.source.id })); source.append(manage);
    }
    if (photo.source?.accessOriginals !== false && ['available', 'onlineOnly', 'unchecked'].includes(photo.source?.status)) {
      for (const [label, method] of [['Show in Finder', 'revealPhoto'], ['Open original', 'openOriginal']]) {
        const control = element('button', 'secondary-button', label); control.type = 'button';
        control.dataset.detailAction = method;
        control.addEventListener('click', async () => {
          control.disabled = true;
          try { await action(method, { photoId: photo.id }); }
          catch (error) { showMessage('global-error', error.message); }
          finally { control.disabled = false; }
        }); source.append(control);
      }
    }
    if (source.children.length) fragment.append(detailSection('Original & source', source));
    fragment.append(element('p', 'inspector-context', 'Saved library details. Appearance estimates remain estimates, not verified identity.'));
    const inspector = $('inspector-content');
    const focusAction = inspector.contains(document.activeElement) ? document.activeElement?.dataset.detailAction : null;
    const scrollTop = inspector.scrollTop;
    inspector.replaceChildren(fragment);
    if (focusAction) [...inspector.querySelectorAll('[data-detail-action]')].find(node => node.dataset.detailAction === focusAction)?.focus({ preventScroll: true });
    inspector.scrollTop = scrollTop;
  }

  function setPreviewZoom(value) {
    state.previewZoom = Math.max(1, Math.min(3, Math.round(value * 4) / 4));
    const stage = $('preview-content').querySelector('.preview-image-stage');
    if (stage) {
      stage.style.width = `${state.previewZoom * 100}%`;
      stage.style.height = `${state.previewZoom * 100}%`;
    }
    $('preview-zoom-level').textContent = state.previewZoom === 1 ? 'Fit' : `${state.previewZoom.toFixed(2)}×`;
    $('preview-zoom-out').disabled = state.previewZoom <= 1;
    $('preview-zoom-in').disabled = state.previewZoom >= 3;
    $('preview-zoom-fit').disabled = state.previewZoom === 1;
  }

  function updatePreviewPosition() {
    const index = state.previewIDs.indexOf(state.previewPhotoId);
    $('preview-previous').disabled = index <= 0;
    $('preview-next').disabled = index < 0 || index + 1 >= state.previewIDs.length;
    $('preview-position').textContent = state.previewFromPage && index >= 0
      ? `${number.format(index + 1)} of ${number.format(state.previewIDs.length)} on this page`
      : 'Single photograph';
  }

  function resetSavedCrop() {
    state.previewCropVersion += 1;
    state.previewCropController?.abort(); state.previewCropController = null;
    if (state.previewCropURL) URL.revokeObjectURL(state.previewCropURL);
    state.previewCropURL = null; state.previewCropId = null;
    $('preview-back').hidden = true;
  }

  function returnToFullPreview() {
    if (!state.previewCropId) return;
    resetSavedCrop();
    $('preview-title').textContent = state.previewPhotoName || 'Photo preview';
    $('preview-content').replaceChildren(state.previewURL && state.previewMainStage
      ? state.previewMainStage : element('p', '', 'The saved full-photo preview is unavailable. Its saved details remain available.'));
    setPreviewZoom(1);
  }

  async function openSavedCrop(photo, crop) {
    if (!$('preview-dialog').open || state.previewPhotoId !== photo.id || typeof crop?.id !== 'string'
      || !crop.id || crop.id.length > 128 || crop.hasPreview !== true) return;
    resetSavedCrop(); state.previewCropId = crop.id;
    const version = ++state.previewCropVersion;
    const controller = new AbortController(); state.previewCropController = controller;
    $('preview-back').hidden = false;
    $('preview-title').textContent = `${state.previewPhotoName} · ${text(crop.label) || 'Saved closer detail'}`;
    $('preview-content').replaceChildren(element('p', '', 'Loading saved closer detail…'));
    setPreviewZoom(1);
    try {
      const blob = await fetchPreview(photo.id, controller.signal, { params: { photoId: photo.id, cropId: crop.id } });
      if (controller.signal.aborted || version !== state.previewCropVersion || state.previewPhotoId !== photo.id
        || state.previewCropId !== crop.id || !$('preview-dialog').open) return;
      const url = URL.createObjectURL(blob); state.previewCropURL = url;
      const image = element('img'); image.src = url; image.alt = text(crop.label) || 'Saved closer detail';
      image.addEventListener('error', () => {
        if (version !== state.previewCropVersion) return;
        URL.revokeObjectURL(url); state.previewCropURL = null;
        $('preview-content').replaceChildren(element('p', '', 'This saved detail crop could not be displayed.'));
      }, { once: true });
      const stage = element('div', 'preview-image-stage'); stage.append(image);
      $('preview-content').replaceChildren(stage); setPreviewZoom(state.previewZoom);
    } catch (error) {
      if (isAbort(error) || error instanceof SessionError || version !== state.previewCropVersion) return;
      $('preview-content').replaceChildren(element('p', '', error.message));
    }
  }

  function previewAction(label, callback) {
    const control = element('button', 'secondary-button', label); control.type = 'button';
    control.addEventListener('click', async () => {
      control.disabled = true;
      try { await callback(); }
      catch (error) { if (!isAbort(error) && !(error instanceof SessionError)) showMessage('global-error', error.message); }
      finally { control.disabled = false; }
    });
    return control;
  }

  function renderPreviewDetails(photo) {
    const details = $('preview-details');
    const fragment = document.createDocumentFragment();
    fragment.append(photoDetailHeader(photo));
    if (photo.error || photo.improvementIssue) {
      const issue = element('div'); issue.append(element('p', '', text(photo.error) || text(photo.improvementIssue)));
      if (photo.error) issue.append(previewAction('View tagging diagnostic…', () => notifyEvent('tagging-diagnostic', { photoId: photo.id })));
      fragment.append(detailSection('Needs attention', issue));
    }
    appendPhotoContext(fragment, photo);
    fragment.append(detailSection('Saved description', element('p', '', text(photo.caption) || 'No saved description yet.')));
    const tags = strings(photo.tags);
    if (tags.length) { const list = element('div', 'tag-list'); tags.forEach((tag) => list.append(element('span', 'tag', tag))); fragment.append(detailSection(`Tags · ${number.format(tags.length)}`, list)); }
    for (const section of Array.isArray(photo.detailSections) ? photo.detailSections : []) {
      const items = strings(section?.items); if (!items.length) continue;
      const list = element('ul'); items.forEach((item) => list.append(element('li', '', item)));
      fragment.append(detailSection(text(section.title) || text(section.id) || 'Saved details', list));
    }
    appendPhotoWorkflows(fragment, photo);
    const people = strings(photo.personNames);
    if (people.length) {
      const list = element('div', 'button-row');
      people.forEach((name, index) => {
        const personId = photo.personIds?.[index];
        list.append(personId ? previewAction(name, () => { closePreview(); window.PhotoFindryPanels?.show('people', { personId }); }) : element('span', 'tag', name));
      });
      fragment.append(detailSection('People · saved names', list));
    }
    const crops = Array.isArray(photo.detailCrops) ? photo.detailCrops.filter((crop) =>
      typeof crop?.id === 'string' && crop.id.length > 0 && crop.id.length <= 128 && typeof crop.label === 'string').slice(0, 128) : [];
    if (crops.length) {
      const list = element('div', 'preview-crop-list');
      crops.forEach((crop) => {
        const control = previewAction(`Open saved crop · ${crop.label}`, () => openSavedCrop(photo, crop));
        control.disabled = crop.hasPreview !== true;
        list.append(control);
      });
      fragment.append(detailSection(`Saved closer details · ${number.format(crops.length)}`, list));
    }
    if (photo.detailCropError) fragment.append(detailSection('Closer detail issue', element('p', '', text(photo.detailCropError))));
    if (photo.source) {
      const source = element('div');
      source.append(element('p', 'inspector-source-name', text(photo.source.label) || 'Saved source'),
        element('p', 'inspector-context', photoSourceStatus(photo) || (photo.source.status === 'available' ? 'Original available' : 'Not checked yet')));
      if (photo.source.relativePath) source.append(element('p', 'photo-original-path', text(photo.source.relativePath)));
      fragment.append(detailSection('Original & source', source));
    }
    const actions = element('div', 'button-row');
    const tagged = photo.status === 'tagged';
    actions.append(previewAction(tagged ? 'Retag photograph…' : 'Tag photograph', async () => {
      if (tagged) {
        const confirm = element('div', 'preview-confirm');
        confirm.append(element('p', '', 'Replace tags for this photograph? Existing good tags are kept if replacement fails; other photos are unchanged.'),
          previewAction('Confirm retag', async () => { await action('enqueueJob', { kind: 'tagging', params: { photoIds: [photo.id], force: true } }); confirm.remove(); }),
          previewAction('Cancel', () => confirm.remove()));
        details.append(confirm);
      } else await action('enqueueJob', { kind: 'tagging', params: { photoIds: [photo.id] } });
    }));
    if (photo.source?.id) actions.append(previewAction('View source', () => { closePreview(); window.PhotoFindryPanels?.show('sources', { sourceId: photo.source.id }); }));
    if (photo.source?.accessOriginals !== false && ['available', 'onlineOnly', 'unchecked'].includes(photo.source?.status)) {
      actions.append(previewAction('Show in Finder', () => action('revealPhoto', { photoId: photo.id })),
        previewAction('Open original', () => action('openOriginal', { photoId: photo.id })));
    }
    fragment.append(actions);
    const focused = details.contains(document.activeElement) ? document.activeElement?.dataset.detailAction : null;
    const scrollTop = details.scrollTop;
    details.replaceChildren(fragment);
    if (focused) [...details.querySelectorAll('[data-detail-action]')].find(node => node.dataset.detailAction === focused)?.focus({ preventScroll: true });
    details.scrollTop = scrollTop;
  }

  async function openPreview(photo, { ids, fromPage, loaded = false } = {}) {
    if (!state.token) return;
    const candidates = ids || state.photos.map((item) => item.id);
    const pageMember = candidates.includes(photo.id);
    const sequence = pageMember ? [...candidates] : [photo.id];
    clearPreview();
    state.previewIDs = sequence;
    state.previewFromPage = fromPage ?? pageMember;
    state.previewPhotoId = photo.id;
    state.previewIdentity = previewIdentity(photo);
    const controller = new AbortController();
    state.previewController = controller;
    const version = ++state.previewVersion;
    state.previewPhotoName = text(photo.name) || 'Photo preview';
    $('preview-title').textContent = state.previewPhotoName;
    $('preview-details').replaceChildren(element('p', 'inspector-loading', 'Loading complete saved details…'));
    updatePreviewPosition(); setPreviewZoom(1);
    $('preview-content').replaceChildren(element('p', '', photo.hasPreview ? 'Loading saved preview…' : 'There is no saved preview for this photo. Its saved details remain available.'));
    if (!$('preview-dialog').open) $('preview-dialog').showModal();
    const details = (loaded ? Promise.resolve(photo) : api('/api/photo', { photoId: photo.id }, { signal: controller.signal }))
      .then((full) => {
        if (controller.signal.aborted || version !== state.previewVersion || !$('preview-dialog').open) return;
        if (full?.id !== photo.id) throw new Error('The companion returned details for a different photograph.');
        state.previewIdentity = previewIdentity(full);
        state.previewPhotoName = text(full.name) || state.previewPhotoName;
        if (!state.previewCropId) $('preview-title').textContent = state.previewPhotoName;
        const merged = { ...photo, ...full };
        state.previewPhoto = merged;
        renderPreviewDetails(merged);
      }).catch((error) => {
        if (isAbort(error) || error instanceof SessionError || version !== state.previewVersion) return;
        $('preview-details').replaceChildren(element('p', 'inspector-error', error.message));
      });
    if (!photo.hasPreview) {
      const unavailable = element('div', 'preview-error');
      const message = element('p', '', 'No retained preview is available. You can prepare a new copy from an available original without changing that original.');
      unavailable.append(message);
      if (photo.source?.accessOriginals !== false && !['checking', 'changed', 'needsReview', 'missing', 'permission', 'offline', 'unavailable'].includes(photo.source?.status)) {
        const prepare = element('button', 'secondary-button', 'Prepare this photo’s preview'); prepare.type = 'button';
        prepare.addEventListener('click', async () => {
          prepare.disabled = true; message.textContent = 'Preparing a local preview…';
          try {
            const result = await action('ensurePreview', { photoId: photo.id });
            if (controller.signal.aborted || version !== state.previewVersion || !$('preview-dialog').open) return;
            const updated = await api('/api/photo', { photoId: photo.id }, { signal: controller.signal });
            if (controller.signal.aborted || version !== state.previewVersion) return;
            if (updated.hasPreview) await openPreview(updated);
            else message.textContent = text(result.reason) || 'A preview could not be prepared. Check this photo’s source and try again.';
          } catch (error) { if (!isAbort(error) && version === state.previewVersion) message.textContent = error.message; }
          finally { prepare.disabled = false; }
        }); unavailable.append(prepare);
      }
      if (photo.source?.id) {
        const recover = element('button', 'secondary-button', 'View original source'); recover.type = 'button';
        recover.addEventListener('click', () => { closePreview(); window.PhotoFindryPanels?.show('sources', { sourceId: photo.source.id }); }); unavailable.append(recover);
      }
      $('preview-content').replaceChildren(unavailable); return details;
    }
    try {
      const blob = await fetchPreview(photo.id, controller.signal);
      if (controller.signal.aborted || version !== state.previewVersion || !state.token || !$('preview-dialog').open) return;
      const url = URL.createObjectURL(blob);
      state.previewURL = url;
      const image = element('img');
      image.alt = text(photo.caption) || text(photo.name) || 'Saved photo preview';
      image.src = url;
      image.addEventListener('error', () => {
        if (version !== state.previewVersion) return;
        if (!state.previewCropId) $('preview-content').replaceChildren(element('p', '', 'This saved preview could not be displayed.'));
        URL.revokeObjectURL(url);
        state.previewURL = null;
        state.previewMainStage = null;
      }, { once: true });
      const stage = element('div', 'preview-image-stage'); stage.append(image);
      state.previewMainStage = stage;
      if (!state.previewCropId) { $('preview-content').replaceChildren(stage); setPreviewZoom(state.previewZoom); }
    } catch (error) {
      if (isAbort(error) || error instanceof SessionError || version !== state.previewVersion) return;
      const message = element('div');
      message.append(element('p', '', error.message));
      const retry = element('button', 'secondary-button', 'Try preview again');
      retry.type = 'button';
      retry.addEventListener('click', () => openPreview(photo));
      message.className = 'preview-error';
      message.append(retry);
      if (photo.source?.accessOriginals !== false && !['checking', 'changed', 'needsReview', 'missing', 'permission', 'offline', 'unavailable'].includes(photo.source?.status)) {
        const rebuild = element('button', 'secondary-button', 'Prepare a new preview'); rebuild.type = 'button';
        rebuild.addEventListener('click', async () => {
          rebuild.disabled = true;
          try {
            await action('ensurePreview', { photoId: photo.id });
            if (controller.signal.aborted || version !== state.previewVersion) return;
            const updated = await api('/api/photo', { photoId: photo.id }, { signal: controller.signal });
            if (!controller.signal.aborted && version === state.previewVersion) await openPreview(updated);
          } catch (failure) {
            if (!isAbort(failure) && version === state.previewVersion) message.querySelector('p').textContent = failure.message;
          } finally { rebuild.disabled = false; }
        }); message.append(rebuild);
      }
      if (photo.source?.id) {
        const source = element('button', 'secondary-button', 'View original source'); source.type = 'button';
        source.addEventListener('click', () => { closePreview(); window.PhotoFindryPanels?.show('sources', { sourceId: photo.source.id }); }); message.append(source);
      }
      $('preview-content').replaceChildren(message);
    }
    return details;
  }

  function clearPreview() {
    state.previewVersion += 1;
    state.previewController?.abort();
    state.previewController = null;
    resetSavedCrop();
    $('preview-content').replaceChildren();
    $('preview-title').textContent = 'Photo preview';
    if (state.previewURL) URL.revokeObjectURL(state.previewURL);
    state.previewURL = null;
    state.previewMainStage = null;
    state.previewPhotoId = null;
    state.previewIdentity = null;
    state.previewPhotoName = '';
    state.previewPhoto = null;
    state.previewIDs = [];
    state.previewFromPage = false;
    $('preview-details').replaceChildren();
    updatePreviewPosition(); setPreviewZoom(1);
  }

  function closePreview() {
    if ($('preview-dialog').open) $('preview-dialog').close();
    clearPreview();
  }

  function navigatePreview(direction) {
    if (!$('preview-dialog').open) return;
    const index = state.previewIDs.indexOf(state.previewPhotoId);
    const id = state.previewIDs[index + direction];
    if (!id) return;
    const ids = [...state.previewIDs], fromPage = state.previewFromPage;
    const summary = state.photos.find((photo) => photo.id === id) || { id, name: 'Photograph', hasPreview: true };
    openPreview(summary, { ids, fromPage });
  }

  // A queued close event from the previous preview must not clear a newly opened one.
  $('preview-dialog').addEventListener('close', () => { if (!$('preview-dialog').open) clearPreview(); });
  $('close-preview').addEventListener('click', closePreview);
  $('preview-previous').addEventListener('click', () => navigatePreview(-1));
  $('preview-next').addEventListener('click', () => navigatePreview(1));
  $('preview-back').addEventListener('click', returnToFullPreview);
  $('preview-zoom-out').addEventListener('click', () => setPreviewZoom(state.previewZoom - 0.25));
  $('preview-zoom-fit').addEventListener('click', () => setPreviewZoom(1));
  $('preview-zoom-in').addEventListener('click', () => setPreviewZoom(state.previewZoom + 0.25));
  $('preview-dialog').addEventListener('keydown', (event) => {
    if (!$('preview-dialog').open || ['INPUT', 'TEXTAREA', 'SELECT'].includes(event.target?.tagName)) return;
    if (event.key === 'Backspace' && state.previewCropId) returnToFullPreview();
    else if (event.key === 'ArrowLeft') navigatePreview(-1);
    else if (event.key === 'ArrowRight') navigatePreview(1);
    else if (event.key === '+' || event.key === '=') setPreviewZoom(state.previewZoom + 0.25);
    else if (event.key === '-') setPreviewZoom(state.previewZoom - 0.25);
    else if (event.key === '0') setPreviewZoom(1);
    else if (event.key === 'Escape') closePreview();
    else return;
    event.preventDefault();
  });
  $('clear-selection').addEventListener('click', clearSelection);
  $('retry-library').addEventListener('click', () => loadLibrary(state.offset));
  $('refresh').addEventListener('click', () => loadLibrary(state.offset));
  $('previous-page').addEventListener('click', () => loadLibrary(Math.max(0, state.offset - state.limit)));
  $('next-page').addEventListener('click', () => loadLibrary(state.offset + state.limit));
  function showAllPhotos() {
    state.aiJobId = null;
    $('search').value = '';
    state.scope = { ...state.scope, sourceId: '', scopeId: '', status: 'all', metadataFilters: {}, locationOptions: {} };
    notifyEvent('scope', getScope());
    loadLibrary(0);
  }
  $('clear-search').addEventListener('click', () => {
    if (state.total) return loadLibrary(0);
    state.aiJobId = null; $('search').value = ''; loadLibrary(0);
  });
  $('clear-filters').addEventListener('click', () => {
    state.aiJobId = null;
    state.scope = { ...state.scope, sourceId: '', scopeId: '', status: 'all', metadataFilters: {}, locationOptions: {} };
    notifyEvent('scope', getScope()); loadLibrary(0);
  });
  $('empty-add-folder').addEventListener('click', () => $('add-folder').click());
  $('empty-restore').addEventListener('click', () => notifyEvent('guide-destination', { panel: 'portability' }));
  $('empty-sources').addEventListener('click', () => notifyEvent('guide-destination', { panel: 'sources' }));
  $('library-nav').addEventListener('click', showAllPhotos);
  $('search-form').addEventListener('submit', (event) => { event.preventDefault(); state.aiJobId = null; loadLibrary(0); });
  $('search').addEventListener('input', () => {
    state.aiJobId = null;
    clearTimeout(state.queryTimer);
    // Cancel immediately so a superseded response cannot publish during the debounce.
    clearPage();
    $('empty-state').hidden = true;
    $('library-error').hidden = true;
    $('query-notice').hidden = true;
    $('loading-state').hidden = false;
    $('result-count').textContent = 'Updating search…';
    state.loading = true;
    updatePagination();
    state.queryTimer = setTimeout(() => loadLibrary(0), 250);
  });

  $('gallery').addEventListener('keydown', (event) => {
    if (event.target !== $('gallery') || !state.photos.length) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'a') {
      event.preventDefault();
      state.selected = new Set(state.photos.map((photo) => photo.id));
      state.activeId ||= state.photos[0].id;
      state.anchorId ||= state.activeId;
      updateSelection();
      loadDetails(state.activeId);
      return;
    }
    if (event.key === 'Escape') { event.preventDefault(); clearSelection(); return; }
    if (event.key === ' ' || event.key === 'Enter') {
      const photo = state.photos.find((item) => item.id === state.activeId);
      if (photo) { event.preventDefault(); openPreview(photo); }
      return;
    }
    const current = state.photos.findIndex((photo) => photo.id === state.activeId);
    const columns = Math.max(1, getComputedStyle($('gallery')).gridTemplateColumns.split(' ').length);
    const changes = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns };
    let index;
    if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = state.photos.length - 1;
    else if (Object.hasOwn(changes, event.key)) index = current < 0 ? 0 : Math.max(0, Math.min(state.photos.length - 1, current + changes[event.key]));
    else return;
    event.preventDefault();
    selectPhoto(state.photos[index].id, { extend: event.shiftKey });
    $('gallery').children[index]?.scrollIntoView({ block: 'nearest' });
  });

  window.addEventListener('pagehide', () => {
    state.pageSuspended = true;
    resetSession('', { preserveCredential: true });
  });
  window.addEventListener('pageshow', () => { state.pageSuspended = false; void restoreSession(); });
  updatePagination();
  void restoreSession();
})();
