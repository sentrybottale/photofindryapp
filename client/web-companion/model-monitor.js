'use strict';

// Metric-only companion monitor. Polling this endpoint never republishes a
// library snapshot or reads a photograph, prompt, or model response.
(() => {
  const P = window.PhotoFindry;
  const localText = value => window.PhotoFindry?.localText?.(value) ?? value;
  const supports = feature => window.PhotoFindry?.supports?.(feature) !== false;
  const isPreview = () => (window.PhotoFindry?.isPreview?.() ?? window.PhotoFindry?.isLinux?.()) === true;
  const button = document.getElementById('open-model-monitor');
  const sidebarStatus = document.getElementById('sidebar-model-status');
  if (!P || !button || !sidebarStatus) return;

  const format = new Intl.NumberFormat();
  const activePhases = new Set(['processing', 'generating', 'scoring']);
  const roleNames = {
    native: 'Local companion', worker: 'Library engine', model: 'Vision model',
    face: 'Face matching', embedding: 'Search indexing', fastRanker: 'Fast search ranking',
    reranker: 'Search refinement', engine: 'Rust tagging engine',
    vectorEngine: 'Rust search engine', policyEngine: 'App policy engine',
  };
  const modelNames = {
    tagger: 'Tagging model', reranker: 'Search reranker', decision: 'Detail reviewer',
    fastRanker: 'Fast search ranking',
  };
  const string = value => typeof value === 'string' ? value : '';
  const finite = value => typeof value === 'number' && Number.isFinite(value) && value >= 0;
  const integer = value => finite(value) ? format.format(Math.floor(value)) : 'Not reported';
  const measured = (value, unit, digits = 1) => finite(value) ? `${value.toFixed(digits)} ${unit}` : 'Not reported';
  const bytes = value => {
    if (!finite(value)) return 'Not measured';
    if (value < 1024) return `${Math.floor(value)} B`;
    if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
    if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
    return `${(value / 1024 ** 3).toFixed(2)} GiB`;
  };
  const seconds = value => finite(value) ? `${(value / 1000).toFixed(2)} s` : 'Not measured';
  const duration = value => {
    if (!finite(value)) return 'Not measured';
    if (value < 60) return `${Math.floor(value)} s`;
    if (value < 3600) return `${Math.floor(value / 60)} min ${Math.floor(value % 60)} s`;
    return `${Math.floor(value / 3600)} hr ${Math.floor((value % 3600) / 60)} min`;
  };
  const el = (tag, className = '', content) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (content !== undefined) node.textContent = content;
    return node;
  };
  const set = (node, content) => {
    const next = String(content);
    if (node.textContent !== next) node.textContent = next;
  };
  const sparkline = values => {
    if (values.length < 2) return 'Not enough samples yet';
    const peak = Math.max(1, ...values);
    const blocks = '▁▂▃▄▅▆▇█';
    return values.map(value => blocks[Math.min(7, Math.floor(value / peak * 7))]).join('');
  };

  let connected = false;
  let healthy = false;
  let sessionGeneration = 0;
  let workerGeneration = null;
  let timer = null;
  let controller = null;
  let pending = null;
  let monitor = null;
  let runtime = {};
  let lastFullState = null;
  let speedHistory = [];
  let memoryHistory = [];
  let speedIdentity = '';
  let memorySample = null;
  let runtimeActionPending = false;

  const sidebarCard = button;
  const sidebarModel = el('span', 'pfm-sidebar-model');
  const sidebarTokens = el('strong', 'pfm-sidebar-tokens');
  sidebarTokens.title = 'Reported model tokens since the local engine started.';
  const sidebarMemory = el('span', 'pfm-sidebar-memory');
  sidebarCard.append(sidebarModel, sidebarTokens, sidebarMemory);
  const runtimeControls = el('div', 'pfm-runtime-controls');
  const startTagger = el('button', '', 'Start tagger'), unloadModel = el('button', '', 'Unload');
  startTagger.type = unloadModel.type = 'button';
  startTagger.title = 'Load the saved tagging model. Other models load when their task starts.';
  unloadModel.title = 'Pause the current model task and unload its model. Saved progress is kept.';
  startTagger.setAttribute('aria-label', 'Start tagging model'); unloadModel.setAttribute('aria-label', 'Unload active model');
  runtimeControls.append(startTagger, unloadModel); button.after(runtimeControls);
  const runtimeHelp = el('p', 'pfm-help');
  runtimeHelp.setAttribute('aria-live', 'polite');
  const recoverTagging = el('button', 'pfm-memory-logs', 'Review originals');
  recoverTagging.type = 'button'; recoverTagging.hidden = true;
  runtimeControls.after(runtimeHelp); runtimeHelp.after(recoverTagging);
  let blockedTagging = null;
  recoverTagging.addEventListener('click', () => {
    if (!blockedTagging || !connected) return;
    const next = blockedTagging.sourceNextAction;
    window.PhotoFindryPanels?.show('sources', { jobId: blockedTagging.id,
      sourceId: next.sourceId, ...(next.rootId ? { rootId: next.rootId } : {}) });
  });
  async function runtimeAction(name) {
    if (runtimeActionPending) return;
    runtimeActionPending = true; renderRuntimeControls();
    try { await P.action(name); }
    catch (error) { P.showError?.(error.message || 'The local model action could not finish.'); }
    finally { runtimeActionPending = false; renderRuntimeControls(); }
  }
  startTagger.addEventListener('click', () => { if (!startTagger.disabled) return runtimeAction('startModel'); });
  unloadModel.addEventListener('click', () => { if (!unloadModel.disabled) return runtimeAction('stopModel'); });
  function renderRuntimeControls() {
    const state = lastFullState || {}, queue = Array.isArray(state.jobs?.jobs) ? state.jobs.jobs : [];
    const running = queue.filter(job => ['running', 'pausing'].includes(job.state));
    const loaded = monitor?.modelActivityVersion === 1 ? Boolean(monitor.activeModel) : ['starting', 'ready'].includes(runtime.status);
    const preparing = state.modelOptimization?.running === true;
    const modelWork = running.some(job => ['tagging', 'improve', 'reviewScreen', 'aiSearch'].includes(job.kind));
    const transfer = state.sourceTransfer?.running === true || state.storageJob?.running === true;
    startTagger.disabled = !connected || runtimeActionPending || transfer || loaded || preparing || running.length > 0 || state.job?.running === true || state.aiSearch?.running === true || !state.settings?.modelConfigured || !state.settings?.projectorConfigured;
    unloadModel.disabled = !connected || runtimeActionPending || !(loaded || preparing || modelWork || state.job?.running === true);
    blockedTagging = queue.find(job => job.kind === 'tagging' && job.state === 'paused'
      && ['source-waiting', 'source-missing', 'safety-source'].includes(job.pauseReason)
      && ['reviewIdentity', 'allowAccess'].includes(job.sourceNextAction?.kind)
      && job.sourceNextAction?.sourceId) || null;
    recoverTagging.hidden = !connected || !blockedTagging;
    set(recoverTagging, blockedTagging?.sourceNextAction?.kind === 'allowAccess' ? 'Allow folder access' : 'Review originals');
    const configured = state.settings?.modelConfigured && state.settings?.projectorConfigured;
    set(runtimeHelp, !connected ? '' : blockedTagging
      ? 'Tagging is waiting for originals. Loading the model does not resolve this; open the affected folder below.'
      : runtime.status === 'error' ? `${string(runtime.message)} Retry Start tagger or check the model files in Settings.`
      : runtime.status === 'starting' ? 'Loading the saved tagging model…'
      : !configured ? 'Choose a vision model and matching projector in Settings.'
      : transfer ? 'Wait for the file operation to finish. Its progress and controls are in Files.'
      : running.length || modelWork ? 'A job is using the processing lane. Open Jobs for progress and Pause.'
      : preparing ? 'Checking model performance. Open Settings for progress and Stop.'
      : 'The saved model loads automatically for Tag photos. Start tagger only loads the model.');
    runtimeHelp.hidden = !connected;
  }

  const dialog = el('dialog', 'pfm-dialog');
  dialog.setAttribute('aria-labelledby', 'pfm-title');
  const header = el('div', 'pfm-header');
  const title = el('h2', '', 'Model activity'); title.id = 'pfm-title';
  const closeButton = el('button', 'pfm-close', '×'); closeButton.type = 'button';
  closeButton.setAttribute('aria-label', 'Close model activity');
  header.append(title, closeButton);
  const scroll = el('div', 'pfm-scroll');
  dialog.append(header, scroll);
  document.body.append(dialog);

  function section(heading, description) {
    const node = el('section', 'pfm-section');
    node.append(el('h3', '', heading));
    if (description) node.append(el('p', 'pfm-help', description));
    scroll.append(node);
    return node;
  }
  function row(parent, label) {
    const line = el('div', 'pfm-row');
    const name = el('span', 'pfm-row-label', label);
    const value = el('strong', 'pfm-row-value', 'Not measured');
    line.append(name, value); parent.append(line);
    return { line, name, value };
  }

  const statusLine = el('p', 'pfm-status', 'Companion unavailable');
  scroll.append(statusLine);
  const session = section('Local engine session', 'Reported language-model input and output from the current local engine session. A library restore or source transfer can restart the engine and reset these totals. Cached input is included when reported; face and embedding work has no token count.');
  const sessionRows = {
    total: row(session, 'Total tokens processed'), input: row(session, 'Input tokens'),
    output: row(session, 'Generated tokens'), requests: row(session, 'Requests'),
  };
  const measurementNotice = el('p', 'pfm-warning'); session.append(measurementNotice);

  const memory = section('Memory', isPreview() ? 'RSS covers owned local processes; browser and GPU memory are separate. Unavailable measurements are shown explicitly. Memory protection keeps saved progress.' : localText('Physical footprint includes compressed memory. RSS includes mapped files; neither is GPU allocation or total Mac RAM. These readings cover owned local processes; browser memory is separate. Critical pressure pauses processing and keeps saved progress.'));
  const memoryRows = {
    pressure: row(memory, 'Memory pressure'), current: row(memory, 'PhotoFindry footprint'),
    peak: row(memory, 'Peak footprint'), budget: row(memory, 'Safety budget'),
    swap: row(memory, localText('Mac swap · all apps')), appRSS: row(memory, 'Companion + worker RAM · RSS'),
  };
  const memoryNotice = el('p', 'pfm-warning'); memory.append(memoryNotice);
  const revealMemory = el('button', 'pfm-memory-logs', 'Show memory logs'); revealMemory.type = 'button';
  revealMemory.addEventListener('click', async () => {
    revealMemory.disabled = true;
    try { await P.action('revealMemoryLogs'); }
    catch (error) { P.showError?.(error.message || 'Memory logs could not be opened.'); }
    finally { revealMemory.disabled = false; }
  });
  revealMemory.hidden = !supports('openOriginal');
  memory.append(el('p', 'pfm-help', localText('Diagnostic logs stay on this Mac and contain memory metrics only. History is limited to 2 MiB.')), revealMemory);
  const memoryTrend = el('p', 'pfm-trend', 'Not enough samples yet');
  memoryTrend.setAttribute('aria-label', 'Recent PhotoFindry memory usage'); memory.append(memoryTrend);
  const processList = el('div', 'pfm-process-list'); memory.append(processList);
  const processRows = new Map();
  for (const [role, label] of Object.entries(roleNames)) {
    const entry = row(processList, label);
    processRows.set(role, entry);
  }

  const timing = section('Tagging job', 'Estimates learn from completed whole-photo attempts in this batch. Loading and unavailable files do not set the pace.');
  const timingHeadline = el('p', 'pfm-timing-headline'); timing.append(timingHeadline);
  const timingRows = {
    processed: row(timing, 'Photographs processed'), elapsed: row(timing, 'Elapsed'),
    pace: row(timing, 'Recent pace'), remaining: row(timing, 'Time remaining'),
  };

  const request = section('No requests yet', 'Current and last request measurements come from the owned local model process.');
  const requestTitle = request.querySelector('h3');
  const modelLine = el('p', 'pfm-model-name'); request.append(modelLine);
  const loadedLine = el('p', 'pfm-loaded'); request.append(loadedLine);
  const requestLabel = el('p', 'pfm-request-label'); request.append(requestLabel);
  const requestModel = el('p', 'pfm-request-model'); request.append(requestModel);
  const requestRows = {
    phase: row(request, 'Phase'), elapsed: row(request, 'Request time'),
    input: row(request, 'Input tokens'), output: row(request, 'Generated tokens'),
    context: row(request, 'Request tokens / context'), first: row(request, 'First token'),
    inputSpeed: row(request, 'Input speed'), outputSpeed: row(request, 'Output speed'),
    modelRSS: row(request, 'Model RAM · RSS'), peakModelRSS: row(request, 'Peak model RAM · RSS'),
    faceRSS: row(request, 'Face models RAM · RSS'),
  };
  const contextProgress = el('progress', 'pfm-context');
  contextProgress.setAttribute('aria-label', 'Request context used'); request.append(contextProgress);
  const requestNotice = el('p', 'pfm-warning'); request.append(requestNotice);
  const speedTrend = el('p', 'pfm-trend', 'Not enough samples yet');
  speedTrend.setAttribute('aria-label', 'Recent token generation speed'); request.append(speedTrend);
  const loadedSection = section('Loaded local models');
  const loadedSlots = Array.from({ length: 16 }, () => {
    const line = el('p', 'pfm-loaded-slot'); line.hidden = true; loadedSection.append(line); return line;
  });

  function isActive() {
    if (!monitor || !activePhases.has(monitor.phase)) return false;
    if (monitor.modelActivityVersion === 1) {
      return monitor.activeModel?.status === 'ready' && monitor.activeModel?.id === monitor.requestModelId;
    }
    return ['starting', 'ready'].includes(runtime.status);
  }
  function statusLabel() {
    if (!connected) return 'Companion unavailable';
    if (!healthy && workerGeneration !== null) return 'Metrics temporarily unavailable · last measured';
    if (!monitor) return 'Waiting for local metrics…';
    if (monitor.modelActivityVersion === 1) {
      const active = monitor.activeModel;
      if (!active && runtime.status === 'error') return 'Model needs attention';
      if (!active && runtime.status === 'starting') return 'Loading model…';
      if (!active) return 'No model loaded';
      if (active.status === 'starting') return 'Loading model…';
      if (isActive()) {
        if (active.kind === 'decision') return 'Checking photo details…';
        if (active.kind === 'reranker') return 'Ranking search evidence…';
        if (active.kind === 'fastRanker') return 'Ranking search candidates…';
        return monitor.phase === 'generating' ? `Generating · ${integer(monitor.generatedTokens)} tokens` : 'Processing input…';
      }
      return 'Model ready';
    }
    if (isActive()) return monitor.phase === 'generating' ? `Generating · ${integer(monitor.generatedTokens)} tokens` : 'Processing input…';
    if (runtime.status === 'ready') return 'Model ready';
    if (runtime.status === 'starting') return 'Model starting…';
    if (runtime.status === 'error') return 'Model needs attention';
    return 'Model stopped';
  }
  function requestHeading() {
    if (isActive()) return 'Current request';
    if (['error', 'failed'].includes(monitor?.phase)) return 'Last request · failed';
    if (monitor?.phase === 'cancelled') return 'Last request · cancelled';
    return (monitor?.requestId || (finite(monitor?.requests) && monitor.requests > 0)) ? 'Last request' : 'No requests yet';
  }
  function timingSummary(job) {
    if (!job || typeof job !== 'object') return '';
    if (!job.running) {
      if (job.phase === 'yielded') return 'Waiting to continue';
      if (job.phase === 'paused') return `Paused after ${duration(job.elapsedSeconds)}`;
      if (job.phase === 'error') return `Stopped after ${duration(job.elapsedSeconds)}`;
      return `Finished in ${duration(job.elapsedSeconds)}`;
    }
    if (job.estimateState === 'stopping') return 'Stopping and saving…';
    if (job.estimateState === 'finishing') return 'Finishing search and saving…';
    if (job.estimateState === 'adjusting') return 'Updating time estimate…';
    if (!finite(job.remainingSeconds) || job.remainingSeconds <= 0) return 'Estimating remaining time…';
    return job.remainingSeconds < 60 ? 'Less than a minute left' : `About ${duration(Math.ceil(job.remainingSeconds / 60) * 60)} left`;
  }
  function render() {
    const m = monitor || {};
    const resource = m.resources && typeof m.resources === 'object' ? m.resources : null;
    const processes = Array.isArray(resource?.processes) ? resource.processes : [];
    const hasProcessMeasurement = processes.some(item => item?.active === true && finite(item.footprintBytes));
    const currentMemory = resource && resource.complete === false && !hasProcessMeasurement
      ? 'Not measured' : bytes(resource?.currentBytes);
    const active = isActive();
    renderRuntimeControls();
    set(sidebarStatus, statusLabel());
    const loadedName = m.activeModel?.name || (Array.isArray(m.loadedModels) ? m.loadedModels.find(model => model?.status === 'ready')?.name : '') || '';
    set(sidebarModel, loadedName); sidebarModel.hidden = !loadedName;
    set(sidebarTokens, finite(m.appTotalTokens) ? `${integer(m.appTotalTokens)} tokens · this session` : 'Token total not reported');
    set(sidebarMemory, resource ? `RAM ${currentMemory}${resource.complete === false ? ' · partial' : ''}` : 'Memory not measured');
    set(statusLine, statusLabel());
    set(sessionRows.total.value, integer(m.appTotalTokens));
    set(sessionRows.input.value, integer(m.appInputTokens));
    set(sessionRows.output.value, integer(m.appOutputTokens));
    set(sessionRows.requests.value, integer(m.appRequests));
    const unmeasured = finite(m.appUnmeasuredRequests) ? m.appUnmeasuredRequests : 0;
    set(measurementNotice, unmeasured > 0 ? `${integer(unmeasured)} requests did not report complete usage. Totals include reported counts only.` : '');
    measurementNotice.hidden = unmeasured === 0;

    memory.hidden = !resource;
    if (resource) {
      const pressure = ({ 1: 'Normal', 2: 'Elevated', 4: 'Critical' })[resource.pressureLevel] || 'Unavailable';
      set(memoryRows.pressure.value, resource.paused ? `Processing paused · ${pressure}` : pressure);
      set(memoryRows.current.value, `${currentMemory}${resource.complete === false ? ' · partial' : ''}`);
      set(memoryRows.peak.value, hasProcessMeasurement || resource.complete === true ? bytes(resource.peakBytes) : 'Not measured');
      set(memoryRows.budget.value, bytes(resource.budgetBytes));
      set(memoryRows.swap.value, bytes(resource.swapUsedBytes));
      set(memoryRows.appRSS.value, bytes(m.appRSSBytes));
      set(memoryNotice, resource.complete === false ? 'Some process measurements are unavailable; the total is a partial reading.' : '');
      memoryNotice.hidden = resource.complete !== false;
      set(memoryTrend, sparkline(memoryHistory));
      for (const [role, entry] of processRows) {
        const process = processes.find(item => item?.role === role);
        const value = !process || process.active === false ? 'Stopped'
          : finite(process.footprintBytes) ? `${bytes(process.footprintBytes)} · RSS ${bytes(process.rssBytes)} · peak ${bytes(process.peakBytes)}`
            : 'Unavailable';
        set(entry.value, value);
      }
    } else {
      for (const entry of processRows.values()) set(entry.value, 'Not measured');
      for (const entry of Object.values(memoryRows)) set(entry.value, 'Not measured');
      set(memoryNotice, ''); set(memoryTrend, sparkline(memoryHistory));
    }

    const job = m.jobTiming && typeof m.jobTiming === 'object' ? m.jobTiming : null;
    timing.hidden = !job;
    if (job) {
      set(timingHeadline, timingSummary(job));
      set(timingRows.processed.value, `${integer(job.completed)} / ${integer(job.total)}`);
      set(timingRows.elapsed.value, duration(job.elapsedSeconds));
      set(timingRows.pace.value, finite(job.averagePhotoSeconds) ? `${duration(job.averagePhotoSeconds)} / photo` : 'Learning this batch’s pace');
      set(timingRows.remaining.value, job.running ? finite(job.remainingSeconds) ? duration(job.remainingSeconds) : 'Estimating' : '—');
    } else {
      set(timingHeadline, '');
      for (const entry of Object.values(timingRows)) set(entry.value, 'Not measured');
    }

    set(requestTitle, requestHeading());
    const model = m.activeModel && typeof m.activeModel === 'object' ? m.activeModel : null;
    set(modelLine, model ? `${modelNames[model.kind] || 'Local model'} · ${string(model.name)}` : string(m.requestModelName) || 'No active model');
    set(loadedLine, model ? `${string(model.status)} · ${integer(model.contextCapacity)} context` : '');
    set(requestLabel, string(m.label));
    set(requestModel, string(m.requestModelName)); requestModel.hidden = !m.requestModelName;
    set(requestRows.phase.value, string(m.phase) || 'Idle');
    set(requestRows.elapsed.value, seconds(m.elapsedMs));
    set(requestRows.input.value, finite(m.promptTokens) ? integer(m.promptTokens) : 'Not reported yet');
    set(requestRows.output.name, m.requestModelKind === 'decision' ? 'Review output tokens' : 'Generated tokens');
    set(requestRows.output.value, !m.requestId ? '—' : m.tokensMeasured === false && !active ? 'Not fully reported' : finite(m.generatedTokens) ? `${integer(m.generatedTokens)}${active && m.tokensMeasured !== true ? ' so far' : ''}` : 'Not reported');
    const capacity = finite(m.contextCapacity) && m.contextCapacity > 0 ? m.contextCapacity : null;
    const used = finite(m.contextUsed) ? m.contextUsed : null;
    set(requestRows.context.value, capacity && used !== null ? `${integer(used)} / ${integer(capacity)}` : 'Awaiting count');
    contextProgress.hidden = !(capacity && used !== null);
    if (!contextProgress.hidden) { contextProgress.max = capacity; contextProgress.value = Math.min(used, capacity); }
    set(requestRows.first.value, seconds(m.timeToFirstTokenMs));
    set(requestRows.inputSpeed.value, measured(m.promptTokensPerSecond, 'tok/s', 0));
    set(requestRows.outputSpeed.value, measured(m.tokensPerSecond, 'tok/s'));
    set(requestRows.modelRSS.value, bytes(m.modelRSSBytes));
    set(requestRows.peakModelRSS.value, finite(m.peakModelRSSBytes) && m.peakModelRSSBytes > 0 ? bytes(m.peakModelRSSBytes) : 'Not measured');
    set(requestRows.faceRSS.value, bytes(m.faceRSSBytes));
    set(requestNotice, m.requestId && m.tokensMeasured === false && !active ? 'This request did not report complete token usage.' : '');
    requestNotice.hidden = !(m.requestId && m.tokensMeasured === false && !active);
    set(speedTrend, sparkline(speedHistory));
    const loaded = Array.isArray(m.loadedModels) ? m.loadedModels.slice(0, 16) : [];
    loadedSection.hidden = !loaded.length;
    loadedSlots.forEach((slot, index) => {
      const item = loaded[index]; slot.hidden = !item;
      if (item) set(slot, `${modelNames[item.kind] || 'Local model'} · ${string(item.name)} · ${string(item.status)} · ${integer(item.contextCapacity)} context`);
      else set(slot, '');
    });
  }

  function recordHistories(next) {
    const requestId = string(next.requestId);
    if (requestId !== string(monitor?.requestId) || next.phase === 'processing' && monitor?.phase !== 'processing') {
      speedHistory = []; speedIdentity = '';
    }
    if (next.phase === 'generating' && finite(next.tokensPerSecond)) {
      const identity = JSON.stringify([requestId, next.generatedTokens, next.tokensPerSecond]);
      if (identity !== speedIdentity) {
        speedHistory = [...speedHistory, next.tokensPerSecond].slice(-50);
        speedIdentity = identity;
      }
    }
    const resource = next.resources;
    if (resource?.complete === true && finite(resource.sampledAtMs) && finite(resource.currentBytes) && (memorySample === null || resource.sampledAtMs > memorySample)) {
      memoryHistory = [...memoryHistory, resource.currentBytes].slice(-60);
      memorySample = resource.sampledAtMs;
    }
  }
  function epoch(value) {
    const number = typeof value === 'string' && /^\d+$/.test(value) ? Number(value) : value;
    return Number.isSafeInteger(number) && number >= 0 ? number : null;
  }
  function resetWorkerMetrics() {
    monitor = null; runtime = {};
    speedHistory = []; memoryHistory = []; speedIdentity = ''; memorySample = null;
  }
  function ingest(next, nextRuntime, nextGeneration = null) {
    if (!next || typeof next !== 'object' || Array.isArray(next)) return false;
    const incoming = epoch(nextGeneration);
    if (workerGeneration !== null && (incoming === null || incoming < workerGeneration)) return false;
    if (incoming !== null && incoming !== workerGeneration) {
      workerGeneration = incoming;
      resetWorkerMetrics();
    }
    if (monitor?.modelActivityVersion === 1 && next.modelActivityVersion !== 1) return false;
    if (finite(monitor?.modelActivityRevision) && finite(next.modelActivityRevision) && next.modelActivityRevision < monitor.modelActivityRevision) return false;
    recordHistories(next);
    monitor = next;
    runtime = nextRuntime && typeof nextRuntime === 'object' ? nextRuntime : runtime;
    render();
    return true;
  }
  function fallback(value) {
    const state = value && typeof value === 'object' ? value : P.getState?.();
    return state?.monitor ? ingest(state.monitor, state.runtime, state.workerGeneration) : false;
  }
  function delay() { return monitor && activePhases.has(monitor.phase) ? 250 : 1200; }
  function schedule(ms) {
    clearTimeout(timer); timer = null;
    if (!connected) return;
    timer = setTimeout(() => { timer = null; poll(); }, ms);
  }
  function poll() {
    if (!connected) return Promise.resolve();
    if (pending) return pending;
    const ownGeneration = sessionGeneration;
    const ownController = new AbortController(); controller = ownController;
    const task = Promise.resolve().then(() => P.api('/api/monitor', {}, { signal: ownController.signal }))
      .then(result => {
        if (ownGeneration !== sessionGeneration || ownController.signal.aborted) return;
        if (!result || !result.monitor || typeof result.monitor !== 'object' || Array.isArray(result.monitor)) throw Error('Invalid local monitor response.');
        healthy = true;
        if (!ingest(result.monitor, result.runtime, result.workerGeneration)) {
          healthy = false; render();
        }
      })
      .catch(error => {
        if (ownGeneration !== sessionGeneration || ownController.signal.aborted || error?.name === 'AbortError') return;
        healthy = false;
        if (!fallback(lastFullState)) render();
      })
      .finally(() => {
        if (pending !== task) return;
        pending = null; controller = null;
        if (connected && ownGeneration === sessionGeneration) schedule(healthy ? delay() : 2000);
      });
    pending = task;
    return task;
  }
  function refresh() { clearTimeout(timer); timer = null; return poll(); }
  function stop() {
    connected = false; healthy = false; sessionGeneration += 1; workerGeneration = null;
    clearTimeout(timer); timer = null;
    controller?.abort(); controller = null; pending = null;
    resetWorkerMetrics(); lastFullState = null;
    if (dialog.open) dialog.close();
    render();
  }
  function connect() {
    if (connected) return;
    connected = true; healthy = false; sessionGeneration += 1; workerGeneration = null;
    lastFullState = P.getState?.() || null;
    fallback(lastFullState);
    render(); schedule(0);
  }
  function positionDetails() {
    if (!button.getBoundingClientRect || !dialog.style) return;
    const anchor = button.getBoundingClientRect(), panel = dialog.getBoundingClientRect();
    const gutter = 12, viewportWidth = window.innerWidth, viewportHeight = window.innerHeight;
    const width = panel.width || Math.min(360, viewportWidth - 24), height = panel.height || Math.min(560, viewportHeight - 24);
    const left = anchor.right + gutter + width <= viewportWidth - gutter ? anchor.right + gutter
      : Math.max(gutter, Math.min(anchor.left, viewportWidth - width - gutter));
    const top = Math.max(gutter, Math.min(anchor.top, viewportHeight - height - gutter));
    dialog.style.left = `${left}px`; dialog.style.top = `${top}px`;
  }
  function open() {
    if (!connected && P.isConnected?.()) connect();
    positionDetails();
    if (!dialog.open) dialog.showModal();
    else closeButton.focus({ preventScroll: true });
    button.setAttribute('aria-expanded', 'true');
    positionDetails();
    if (connected) return refresh();
  }
  function close() { if (dialog.open) dialog.close(); }

  button.addEventListener('click', open);
  dialog.addEventListener('close', () => button.setAttribute('aria-expanded', 'false'));
  dialog.addEventListener('click', event => {
    if (event.target !== dialog) return;
    const bounds = dialog.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) close();
  });
  window.addEventListener('resize', () => { if (dialog.open) positionDetails(); });
  closeButton.addEventListener('click', close);
  window.addEventListener('photofindry:session', event => event.detail?.connected ? connect() : stop());
  window.addEventListener('photofindry:state', event => {
    lastFullState = event.detail || null;
    renderRuntimeControls();
    if (!connected) return;
    const incoming = epoch(lastFullState?.workerGeneration);
    if (incoming !== null && workerGeneration !== null && incoming > workerGeneration) {
      // A full-state event can announce replacement while an old metric fetch
      // is still in flight. Fence it before accepting the new engine session.
      sessionGeneration += 1;
      clearTimeout(timer); timer = null;
      controller?.abort(); controller = null; pending = null;
      healthy = false;
      if (!fallback(lastFullState)) {
        workerGeneration = incoming; resetWorkerMetrics(); render();
      }
      schedule(0);
    } else if (!healthy) fallback(lastFullState);
  });
  window.addEventListener('pagehide', stop);
  window.PhotoFindryModelMonitor = Object.freeze({ open, close, refresh });
  render();
  if (P.isConnected?.()) connect();
})();
