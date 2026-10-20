'use strict';

(() => {
  const P = window.PhotoFindry;
  const $ = (id) => document.getElementById(id);
  const fmt = new Intl.NumberFormat();
  const str = (value) => typeof value === 'string' ? value : '';
  const list = (value) => Array.isArray(value) ? value : [];
  const num = (value) => Number.isFinite(value) ? value : 0;
  const terminal = (job) => ['completed', 'cancelled', 'failed'].includes(job.state);
  let snapshot = {};
  let jobsDialog = null;
  let settingsDialog = null;
  let jobsSignature = '';
  let activeJobId = null;
  let activityIdentity = '';
  const dismissedActivity = new Set();
  let activityController = null;
  const activityURLs = new Set();
  let storyController = null;
  let storyIdentity = '';
  let storyFrames = null;
  let storyFrame = 0;
  let storyTimer = null;
  let storyCanvas = null;
  let aiRefreshTimer = null;
  let aiProgressSignature = '';
  let aiControlsSignature = '';
  let aiPendingProbeTimer = null;
  let aiPendingProbeJobId = null;
  let aiPendingProbeUsed = false;
  let feedbackDialog = null;
  let feedbackDraft = null;
  let feedbackSubmission = null;
  let feedbackRender = null;
  let uninstallDialog = null;
  let uninstallHandoff = null;
  let previousJobs = new Map();
  let sourceRefreshTimer = null;
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const dialogs = new Set();
  const pendingSubmissions = new Set();
  const el = (tag, cls, value) => {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (value != null) node.textContent = value;
    return node;
  };
  const run = async (operation) => {
    try { return await operation(); }
    catch (error) { if (error?.name !== 'AbortError') P.showError(error.message || 'The operation could not be completed.'); }
  };
  function button(label, operation, primary = false) {
    const node = el('button', primary ? 'primary-button' : 'secondary-button', label);
    node.type = 'button';
    node.addEventListener('click', async () => {
      if (node.disabled || !node.isConnected || node.closest?.('dialog')?.open === false) return;
      node.disabled = true;
      try { await run(operation); } finally { node.disabled = node.dataset.locked === 'true'; }
    });
    return node;
  }
  function field(label, input) {
    const wrapper = el('label', 'form-field'); wrapper.append(el('span', '', label), input); return wrapper;
  }
  function input(type, value = '', placeholder = '') {
    const node = el('input', 'workspace-input'); node.type = type; node.value = value; node.placeholder = placeholder; return node;
  }
  function select(options, value) {
    const node = el('select');
    for (const [key, label] of options) { const choice = el('option', '', label); choice.value = key; node.append(choice); }
    node.value = value; return node;
  }
  function checkbox(label, value) {
    const node = input('checkbox'); node.checked = Boolean(value);
    const wrapper = el('label', 'check-field'); wrapper.append(node, el('span', '', label)); return { node, wrapper };
  }
  function section(title, description) {
    const node = el('section', 'settings-section'); node.append(el('h3', '', title));
    if (description) node.append(el('p', '', description)); return node;
  }
  function externalLink(label, url) {
    const link = el('a', 'external-link', label); link.href = url; link.target = '_blank'; link.rel = 'noopener noreferrer'; link.referrerPolicy = 'no-referrer'; return link;
  }
  function modal(title, intro = '') {
    const dialog = el('dialog', 'workspace-dialog');
    const header = el('div', 'dialog-heading');
    const close = button('×', () => dialog.close()); close.className = 'icon-button'; close.setAttribute('aria-label', 'Close');
    header.append(el('h2', '', title), close);
    const body = el('div', 'dialog-body'); if (intro) body.append(el('p', 'dialog-intro', intro));
    dialog.append(header, body); document.body.append(dialog); dialogs.add(dialog);
    dialog.addEventListener('close', () => { dialogs.delete(dialog); dialog.remove(); }, { once: true });
    dialog.showModal(); return { dialog, body };
  }
  function confirmAction(title, description, label, operation) {
    const { dialog, body } = modal(title, description);
    const controls = el('div', 'button-row');
    let confirm;
    confirm = button(label, async () => {
      confirm.textContent = label === 'Stop all remaining' ? 'Stopping…' : label;
      try { await operation(); dialog.close(); }
      finally { if (dialog.open) confirm.textContent = label; }
    }, true);
    controls.append(button('Cancel', () => dialog.close()), confirm);
    body.append(controls);
  }
  const duration = (seconds) => {
    if (!Number.isFinite(seconds) || seconds < 0) return 'Not measured';
    if (seconds < 60) return `${Math.floor(seconds)} s`;
    if (seconds < 3600) return `${Math.ceil(seconds / 60)} min`;
    return `${Math.floor(seconds / 3600)} hr ${Math.ceil((seconds % 3600) / 60)} min`;
  };
  const bytes = (value) => Number.isFinite(value) ? `${(value / 1024 ** 3).toFixed(2)} GB` : 'Not measured';
  function scopedParams() {
    const { sourceId, scopeId } = P.getScope();
    return { ...(sourceId ? { sourceId } : {}), ...(scopeId ? { scopeId } : {}) };
  }
  function modelConfigured() { return snapshot.settings?.modelConfigured && snapshot.settings?.projectorConfigured; }
  function requireModel() {
    if (modelConfigured()) return true;
    openSettings(); P.showNotice('Choose your model and vision projector. Your search and selection are kept.'); return false;
  }
  async function enqueue(kind, params, schedule) {
    if (['tagging', 'improve', 'aiSearch'].includes(kind) && !requireModel()) return;
    const identity = JSON.stringify([kind, { ...params, ...(params.photoIds ? { photoIds: [...new Set(params.photoIds)].sort() } : {}) }, schedule]);
    if (pendingSubmissions.has(identity)) return;
    pendingSubmissions.add(identity);
    try {
      const result = await P.action('enqueueJob', { kind, params, ...(schedule ? { schedule } : {}) });
      P.showNotice('Added to Jobs. Saved progress and source availability are checked by your local engine.');
      return result;
    } finally { pendingSubmissions.delete(identity); }
  }
  async function addFolder() {
    const chosen = await P.pick('folder');
    if (chosen.cancelled || !chosen.grantId) return;
    await P.action('addFolders', { folderGrants: [chosen.grantId] });
    await P.refreshLibrary();
    P.showNotice('Folder added. Check Files and Jobs for discovery progress.');
  }
  $('add-folder').addEventListener('click', () => run(addFolder));
  $('tag-pending').addEventListener('click', () => run(() => enqueue('tagging', scopedParams())));
  $('ai-search').addEventListener('click', () => run(async () => {
    const scope = P.getScope();
    if (!scope.query.trim()) { $('search').focus(); P.showNotice('Enter a search before starting Local AI Search.'); return; }
    if (!requireModel()) return;
    if (!snapshot.settings?.rerankerModelConfigured) { openSettings(); P.showNotice('Choose a search reranker before starting Local AI Search. Your query is kept.'); return; }
    const requestId = crypto.randomUUID();
    const job = await enqueue('aiSearch', { ...scopedParams(), requestId, query: scope.query, metadataFilters: scope.metadataFilters, locationOptions: scope.locationOptions, broaden: false, sort: 'relevance', candidateView: 'best' });
    if (job?.id) await openAIJob(job.id, requestId);
  }));

  function openAdvanced(name) {
    if (window.PhotoFindryPanels) window.PhotoFindryPanels.show(name);
    else P.showError('The local panel could not be loaded. Reopen the companion interface.');
  }
  const navigation = [
    ['Library', () => { window.PhotoFindryPanels?.hide?.(); $('library-title').scrollIntoView({ block: 'start' }); }],
    ['Files', () => openAdvanced('sources')], ['People', () => openAdvanced('people')],
    ['Photo improvement', () => openAdvanced('improvement')],
    ['Jobs', () => openJobs()], ['Removed', () => openAdvanced('removed')],
    ['Backups & archives', () => openAdvanced('portability')], ['Settings', openSettings], ['About', openAbout],
    ['Help & feedback', () => window.dispatchEvent(new CustomEvent('photofindry:feedback'))],
  ];
  if (!window.PhotoFindryShell?.ownsNavigation) {
    for (const [label, operation] of navigation) { const node = button(label, operation); node.className = 'nav-item'; $('workspace-navigation').append(node); }
    const support = button('☕ Support PhotoFindry', () => window.PhotoFindrySupport?.open()); support.classList.add('support-link'); $('workspace-navigation').append(support);
  }
  function navigate(destination) {
    if (destination === 'Getting started') return window.PhotoFindryOnboarding?.open();
    if (destination === 'This library') return window.PhotoFindryOnboarding?.openLibraryInfo();
    if (destination === 'Support PhotoFindry') return window.PhotoFindrySupport?.open();
    if (destination === 'Browse folders') return openFolderChooser();
    if (destination === 'Improve library') return enqueue('improve', { ...scopedParams(), force: false });
    const operation = navigation.find(([label]) => label === destination)?.[1];
    if (!operation) throw new Error('This view is unavailable. Reopen PhotoFindry and try again.');
    return operation();
  }
  function openAbout() {
    const { body } = modal('About PhotoFindry', 'Find the photograph you remember. Free to use, with optional support.');
    body.append(el('p', 'dialog-intro', 'This browser workspace is bundled into the installed macOS companion. The website cannot replace its code or receive its local session. Library databases, original photographs and model files stay on this Mac.'), el('p', 'dialog-intro', 'The background companion starts at login and checks for signed updates. Manage startup, updates and recovery below. The one-time migration application is no longer needed. Updates preserve your library and chosen model paths. Community betas are not Apple-notarized.'), el('p', 'dialog-intro', 'PhotoFindry is free to use, with an MIT open-source client and proprietary local engines. Third-party licences and notices are retained with the installed runtime. Your photos, human labels and saved details remain portable through backups and source archives.'), button('Privacy & distribution', openPrivacy), button('Optional Buy Me a Coffee support', () => window.PhotoFindrySupport?.open()), button('Uninstall companion…', openUninstall), lifecycleControls());
  }
  function lifecycleControls() {
    const panel = section('Background companion', 'PhotoFindry runs locally without keeping the migration application open. Save pending edits before installing or switching versions.');
    const message = el('p', 'settings-live', 'Checking this installation…');
    message.setAttribute('role', 'status');
    const controls = el('div', 'button-row');
    const reopen = el('a', 'secondary-button', 'Open PhotoFindry'); reopen.href = 'photofindry-local://open';
    const refresh = button('Refresh status', load);
    let pending = false;
    const controller = new AbortController();
    const visible = () => panel.isConnected && panel.closest('dialog')?.open !== false;
    queueMicrotask(() => panel.closest('dialog')?.addEventListener('close', () => controller.abort(), { once: true }));
    async function act(action) {
      if (pending) return;
      pending = true;
      const restarting = ['update', 'rollback', 'recover'].includes(action);
      try {
        await P.api('/api/lifecycle/action', { action }, { signal: controller.signal });
        message.textContent = restarting
          ? 'Finishing saved work and restarting the companion. Your library stays in place. The new workspace opens automatically after an update; Open PhotoFindry below is also available.'
          : 'Working… This panel will show the result automatically.';
        const deadline = Date.now() + 660000;
        while (visible() && Date.now() < deadline) {
          let value;
          try { value = await P.api('/api/lifecycle/status', {}, { signal: controller.signal }); }
          catch (error) {
            if (error?.name === 'AbortError' || !visible()) return;
            if (!restarting) throw error;
            message.textContent = 'The companion connection closed for the change. Use Open PhotoFindry below to return; its local helper offers recovery if needed. Your saved library is kept.';
            return;
          }
          if (!visible()) return;
          message.textContent = str(value.status?.message) || 'Waiting for the companion operation to finish…';
          if (['ready', 'available', 'error', 'uninstalled'].includes(value.status?.phase)) { await load(); return; }
          await new Promise(resolve => setTimeout(resolve, 1500));
        }
        if (visible()) message.textContent = 'The operation is taking longer than expected. Choose Refresh status to check it, or Open PhotoFindry for local recovery. Your library is kept.';
      } finally { pending = false; }
    }
    async function load() {
      const value = await P.api('/api/lifecycle/status', {}, { signal: controller.signal });
      if (!visible()) return;
      controls.replaceChildren();
      if (!value?.installed) { message.textContent = str(value?.message) || 'Open the latest verified installer once to enable background startup and signed CLI updates.'; return; }
      message.textContent = `Version ${str(value.version)} · build ${num(value.build)}. ${str(value.status?.message)}${value.openUpdate?.message ? ' '+str(value.openUpdate.message) : ''}`;
      const actions = [
        [value.autostart ? 'Turn off login startup' : 'Start at login', value.autostart ? 'disable-autostart' : 'enable-autostart'],
        [value.automaticUpdates ? 'Turn off background update checks' : 'Check for updates in the background', value.automaticUpdates ? 'disable-update-checks' : 'enable-update-checks'],
        ['Check for updates', 'check'],
      ];
      if (num(value.available?.build) > num(value.build)) actions.push(['Install signed update', 'update']);
      if (value.previousBuild) actions.push(['Use previous companion version', 'rollback']);
      if (value.recoveryPending || value.status?.phase === 'error') actions.push(['Recover companion', 'recover']);
      for (const [title, action] of actions) controls.append(button(title, () => act(action), action === 'update'));
    }
    panel.append(message, controls, refresh, reopen);
    queueMicrotask(() => { if (panel.isConnected) run(load); });
    return panel;
  }
  async function openUninstall() {
    if (uninstallDialog?.open) { uninstallDialog.focus(); return; }
    const opened = modal('Uninstall companion', 'Remove this installed companion while keeping your saved work. macOS will ask you to confirm before anything is removed.');
    uninstallDialog = opened.dialog;
    const controller = new AbortController();
    const review = el('div', 'uninstall-review');
    const status = el('p', 'settings-live', 'Checking this installation…'); status.setAttribute('role', 'status');
    const controls = el('div', 'button-row');
    const continueButton = button('Continue in macOS…', async () => {
      if (uninstallHandoff || !opened.dialog.open) return;
      const attempt = { opening: true, status: 'Opening the macOS confirmation… No uninstall result has been received.' };
      uninstallHandoff = attempt; status.textContent = attempt.status;
      continueButton.dataset.locked = 'true'; checkStatus.hidden = true; closeReview.textContent = 'Close review';
      try {
        const result = await P.api('/api/uninstall/open', {});
        if (uninstallHandoff !== attempt) return;
        attempt.status = result?.opened === true
          ? 'Continue in the macOS confirmation to cancel or uninstall. Closing this review does not cancel that confirmation. This page cannot confirm the outcome. If you uninstall, the local connection will close.'
          : 'The macOS confirmation could not be verified. Check for its window before trying again.';
      } catch (error) {
        if (uninstallHandoff !== attempt) return;
        attempt.status = `The confirmation handoff could not be verified. ${str(error?.message)} Check for the macOS window before trying again.`;
      }
      attempt.opening = false;
      if (uninstallHandoff === attempt) uninstallDialog?.updateHandoff?.();
    }, true);
    continueButton.hidden = true; continueButton.disabled = true; continueButton.dataset.locked = 'true';
    const checkStatus = button('Check uninstall status', () => loadPlan()); checkStatus.hidden = true;
    const closeReview = button('Cancel', () => opened.dialog.close());
    opened.dialog.updateHandoff = () => {
      if (!opened.dialog.open || !uninstallHandoff) return;
      status.textContent = uninstallHandoff.status;
      continueButton.hidden = true; continueButton.disabled = true; continueButton.dataset.locked = 'true';
      checkStatus.hidden = uninstallHandoff.opening; closeReview.textContent = 'Close review';
    };
    controls.append(closeReview, continueButton, checkStatus);
    opened.body.append(review, status, controls);
    opened.dialog.addEventListener('close', () => { controller.abort(); if (uninstallDialog === opened.dialog) uninstallDialog = null; }, { once: true });
    async function loadPlan() {
      continueButton.disabled = true; continueButton.dataset.locked = 'true';
      try {
        const plan = await P.api('/api/uninstall/plan', {}, { signal: controller.signal });
        if (!opened.dialog.open || controller.signal.aborted) return;
        if (plan?.available !== true) {
          continueButton.hidden = true;
          status.textContent = str(plan?.reason) || 'This installation cannot be uninstalled here. Reopen PhotoFindry from its installed companion package and try again.';
          return;
        }
        const checkedHandoff = Boolean(uninstallHandoff);
        if (plan.pending === true) uninstallHandoff ||= { opening: false, status: 'A macOS uninstall confirmation is already open. Continue there to cancel or uninstall, then check its result.' };
        else if (!uninstallHandoff?.opening) uninstallHandoff = null;
        review.replaceChildren(section('Remove from this Mac', `${str(plan.packageLabel) || 'This companion package'} will be moved to Trash. Its launcher registration will be removed. Running jobs will stop safely; other installations are left alone.`));
        const kept = section('Keep your library and files', `${str(plan.libraryLabel) || 'Your saved local library'} stays where it is. Original photographs, model files, saved tags, previews and saved settings are preserved.`);
        const keptList = el('ul');
        for (const item of list(plan.keeps).filter(value => typeof value === 'string').slice(0, 12)) keptList.append(el('li', '', item));
        if (keptList.children.length) kept.append(keptList);
        kept.append(el('p', '', 'Unsaved edits in this browser are not saved by uninstalling.'));
        review.append(kept);
        status.textContent = uninstallHandoff?.status || str(plan.lastError) || (checkedHandoff ? 'The companion is still installed and no uninstall confirmation is running. You can continue again when ready.' : 'Review the installation above, then continue to the macOS confirmation.');
        continueButton.hidden = Boolean(uninstallHandoff);
        continueButton.disabled = Boolean(uninstallHandoff);
        continueButton.dataset.locked = uninstallHandoff ? 'true' : 'false';
        checkStatus.hidden = !uninstallHandoff || uninstallHandoff.opening;
        closeReview.textContent = uninstallHandoff ? 'Close review' : 'Cancel';
      } catch (error) {
        if (opened.dialog.open && !controller.signal.aborted) status.textContent = `This installation could not be checked. ${str(error?.message)} Close this review and try again from the installed companion.`;
      }
    }
    await loadPlan();
  }
  function openPrivacy() {
    const { body } = modal('Privacy & distribution', 'Your photographs stay yours.');
    for (const [title, description] of [
      ['Processing on this Mac', 'Tagging, search inference and face processing run locally. Originals stay in their folders. Library records, names, previews and model files remain on this Mac. Backups and source archives keep your saved work portable.'],
      ['Public entry and local workspace', 'The public site opens the installed launcher. This trusted workspace is bundled with the companion and served from loopback. The website receives no library session or local API authority. No mandatory account, advertising analytics or background feedback reporting is added.'],
      ['Connections you choose', 'Model downloads contact their pinned hosts only when requested. Feedback sends the report, selected screenshots and optional diagnostic attachment you review. Support and website links open external pages only when selected. Those hosts receive normal connection information such as IP addresses.'],
      ['Working offline', 'Local photo workflows can run without internet when the required models and originals are available. Cloud-only originals may need your cloud provider. This build does not enforce an operating-system network sandbox.'],
      ['Installation and updates', 'Installed companion updates are authenticated by the pinned publisher key. Use Background companion in Settings to check, install or recover an update; automatic checks and login startup can be turned off there. The migration application closes after installing the background CLI. Updates preserve libraries, originals, previews and chosen model locations. Community betas are not Apple Developer ID signed or notarized.'],
      ['Feedback you share', 'Reports go by HTTPS to photofindry.com/bugs for private review and follow-up. A receipt confirms storage. No photo library or raw logs are attached automatically. Prepared screenshots remove file metadata, but visible private content stays visible. Optional diagnostics contain only the exact allowlisted states and counters shown in the report preview. Reports remain until removed by the team.'],
      ['Distribution and ownership', 'PhotoFindry is free to use, with an MIT open-source client and proprietary local engines. The client includes the browser interface, launcher and local bridge. Engine implementation remains private. Published release signatures establish publisher approval and file integrity; they do not certify absence of defects. Third-party software retains its licences and notices.'],
    ]) body.append(section(title, description));
  }
  window.addEventListener('photofindry:feedback', openFeedback);
  function openFeedback() {
    if (feedbackDialog?.open) { feedbackDialog.focus(); return; }
    feedbackDraft ||= { kind: 'Bug report', area: 'General', summary: '', details: '', steps: '', expected: '', contact: '', includeEnvironment: true, submissionId: crypto.randomUUID(), receipt: null, screenshots: [], diagnostics: null };
    const draft = feedbackDraft;
    const delivery = feedbackSubmission?.draft === draft ? feedbackSubmission : (feedbackSubmission = { draft, sending: false, controller: null, status: '' });
    const screenshotURLs = new Set(); const loadedScreenshots = new Set();
    let preparingScreenshot = false; let diagnosticLoading = false; let environmentValue = null; let environmentLoading = true;
    const opened = modal('Help & feedback', 'Send a report only when you choose to. The companion sends the fields you review to PhotoFindry support. Library records, photographs, model output and logs are not attached automatically.');
    feedbackDialog = opened.dialog;
    const fields = {}; const body = opened.body;
    const kind = select(['Bug report', 'Suggestion', 'Other feedback'].map((value) => [value, value]), draft.kind);
    const area = select(['General', 'Library', 'Tagging', 'Search', 'People', 'Sources', 'Jobs', 'Model setup', 'Backups', 'Updates', 'Other'].map((value) => [value, value === 'Sources' ? 'Files' : value]), draft.area);
    fields.kind = kind; fields.area = area;
    const header = el('div', 'settings-grid'); header.append(field('Kind', kind), field('Area', area)); body.append(header);
    const limits = { summary: 120, details: 8000, steps: 4000, expected: 2000, contact: 254 };
    for (const [key, label, type] of [['summary', 'Short summary', 'text'], ['details', 'What happened or what would help?', 'textarea'], ['steps', 'Steps to reproduce · optional', 'textarea'], ['expected', 'Expected result · optional', 'textarea'], ['contact', 'Email for follow-up', 'email']]) {
      const control = type === 'textarea' ? el('textarea', 'workspace-input') : input(type);
      control.value = draft[key]; control.maxLength = limits[key];
      if (type === 'textarea') control.rows = key === 'details' ? 5 : 3;
      if (['summary', 'details', 'contact'].includes(key)) control.required = true;
      fields[key] = control; body.append(field(label, control));
    }
    const environment = checkbox('Include companion version, macOS version and architecture', draft.includeEnvironment);
    const consent = checkbox('Send these fields to PhotoFindry support', false);
    const diagnostics = checkbox('Include diagnostic log with this report', Boolean(draft.diagnostics));
    const diagnosticDescription = el('p', 'dialog-intro', 'Optional: up to 200 operational events from the last 30 minutes. Only allowlisted job states and numeric progress, token and memory counters. No names, filenames, paths, searches, model names, raw errors or output. The exact captured log is shown below.');
    const diagnosticPreview = el('pre', 'diagnostic-preview diagnostic-raw');
    const refreshDiagnostics = button('Refresh diagnostic log', async () => captureDiagnostics(true));
    async function captureDiagnostics(include) {
      if (delivery.sending || diagnosticLoading) return;
      diagnosticLoading = true; updateSendState();
      try {
        const value = include ? await P.api('/api/action', { action: 'feedbackDiagnostics', params: {} }) : null;
        if (!opened.dialog.open || feedbackDraft !== draft) return;
        if (value && (value.schema !== 1 || value.consent !== true || !Array.isArray(value.events) || value.events.length > 200 || JSON.stringify(value).length > 128000)) throw new Error('The bounded diagnostic log could not be read.');
        draft.diagnostics = value; diagnostics.node.checked = Boolean(value);
        draft.submissionId = crypto.randomUUID(); draft.receipt = null; consent.node.checked = false;
        renderDiagnosticPreview(); status.textContent = value ? 'Diagnostic log captured. Review it before sending; it stays frozen until refreshed.' : 'Diagnostic log excluded.';
      } finally { diagnosticLoading = false; updateSendState(); }
    }
    function renderDiagnosticPreview() {
      diagnosticPreview.textContent = draft.diagnostics ? JSON.stringify(draft.diagnostics, null, 2) : '';
      diagnosticPreview.hidden = !draft.diagnostics; refreshDiagnostics.hidden = !draft.diagnostics;
    }
    const screenshotSection = section('Optional screenshots', 'Choose up to three images through the native file chooser. The companion prepares fresh PNG pixels without metadata. Visible private information remains visible: review every image before sending.');
    const screenshotList = el('div', 'feedback-screenshots');
    const reviewedScreenshots = checkbox('I reviewed these screenshots and want to include them', false);
    const addScreenshot = button('Choose screenshot…', async () => {
      if (delivery.sending || preparingScreenshot) return;
      if (draft.screenshots.length >= 3) throw new Error('You can attach up to three screenshots.');
      preparingScreenshot = true; updateSendState();
      try {
        const chosen = await P.pick('screenshot');
        if (chosen.cancelled || !chosen.grantId || !opened.dialog.open || feedbackDraft !== draft) return;
        const prepared = await P.action('prepareFeedbackScreenshot', { grantId: chosen.grantId });
        if (!opened.dialog.open || feedbackDraft !== draft) return;
        if (typeof prepared.pngBase64 !== 'string' || prepared.pngBase64.length > 7 * 1024 * 1024 || !Number.isInteger(prepared.width) || !Number.isInteger(prepared.height) || prepared.width < 1 || prepared.height < 1 || prepared.width * prepared.height > 48000000 || !/^[a-f0-9]{64}$/i.test(prepared.sha256 || '')) throw new Error('The prepared screenshot is invalid. Choose it again.');
        const raw = atob(prepared.pngBase64);
        if (raw.length !== prepared.bytes || raw.length > 5 * 1024 * 1024 || ![137, 80, 78, 71, 13, 10, 26, 10].every((value, index) => raw.charCodeAt(index) === value)) throw new Error('The prepared screenshot is not a bounded PNG.');
        const pixels = Uint8Array.from(raw, (character) => character.charCodeAt(0));
        draft.screenshots.push({ grantId: chosen.grantId, blob: new Blob([pixels], { type: 'image/png' }), width: prepared.width, height: prepared.height, bytes: prepared.bytes });
        attachmentChanged(); renderScreenshots();
      } finally { preparingScreenshot = false; updateSendState(); }
    });
    screenshotSection.append(addScreenshot, screenshotList, reviewedScreenshots.wrapper);
    const status = el('p', 'settings-live'); status.setAttribute('role', 'status');
    body.append(environment.wrapper, diagnostics.wrapper, diagnosticDescription, refreshDiagnostics, diagnosticPreview, screenshotSection, el('p', 'dialog-intro', 'Review the text for private information before sending. Reports go to the fixed PhotoFindry support endpoint; a response cannot change this interface or run code.'), consent.wrapper, status);
    const controls = el('div', 'button-row');
    const reportText = () => [`${draft.kind}: ${draft.summary}`, `Area: ${draft.area}`, `Contact: ${draft.contact}`, '', draft.details, draft.steps ? `\nSteps to reproduce\n${draft.steps}` : '', draft.expected ? `\nExpected result\n${draft.expected}` : '', `\nScreenshots: ${draft.screenshots.length} (image pixels are not copied as text)`, draft.includeEnvironment ? `\nEnvironment\n${environmentValue ? JSON.stringify(environmentValue, null, 2) : 'Reading companion environment…'}` : '', draft.diagnostics ? `\nOptional diagnostic log (consented)\n${JSON.stringify(draft.diagnostics, null, 2)}` : '', draft.receipt ? `\nReceipt: ${draft.receipt.reportID}\n${draft.receipt.receivedAt}` : ''].filter(Boolean).join('\n');
    const reviewText = el('pre', 'feedback-report-preview diagnostic-raw');
    const reviewContainer = el('details'); reviewContainer.append(el('summary', '', 'Review report text and diagnostic attachment'), reviewText); body.append(reviewContainer);
    const updateReportPreview = () => { reviewText.textContent = reportText(); };
    void P.api('/api/action', { action: 'feedbackEnvironment', params: {} }).then(value => {
      if (opened.dialog.open && feedbackDraft === draft) {
        if (!value || !['appVersion', 'systemVersion', 'architecture'].every(key => typeof value[key] === 'string' && value[key].length <= 128)) throw new Error('Invalid companion environment.');
        environmentValue = value; updateReportPreview();
      }
    }).catch(() => { environmentValue = null; status.textContent = 'The companion environment could not be previewed. Try reopening feedback, or exclude the environment fields.'; })
      .finally(() => { environmentLoading = false; updateSendState(); });
    controls.append(button('Clear draft…', () => {
      if (delivery.sending || preparingScreenshot || diagnosticLoading) return;
      confirmAction('Clear this feedback draft?', 'This removes only this unsent draft, selected attachments and captured diagnostics. Previously sent reports are unchanged.', 'Clear draft', () => {
        feedbackDraft = null; feedbackSubmission = null; feedbackRender = null; opened.dialog.close(); openFeedback();
      });
    }));
    const newReport = button('New report', () => { if (delivery.sending) return; feedbackDraft = null; feedbackSubmission = null; feedbackRender = null; opened.dialog.close(); openFeedback(); }); newReport.hidden = !draft.receipt;
    const cancelSend = button('Cancel sending', async () => {
      if (!delivery.sending) return;
      await P.api('/api/action', { action: 'cancelFeedback', params: { submissionId: draft.submissionId } });
      if (!delivery.sending && draft.receipt) return;
      delivery.controller?.abort(); delivery.status = 'Sending stopped. The server may already have received it; retry this unchanged draft to avoid a duplicate.'; feedbackRender?.();
    }); cancelSend.hidden = true;
    controls.append(newReport, cancelSend, button('Copy report', async () => {
      try { await navigator.clipboard.writeText(reportText()); status.textContent = 'Report copied. Nothing was sent.'; }
      catch {
        const copy = el('textarea', 'workspace-input'); copy.value = reportText(); copy.readOnly = true; copy.rows = 10;
        copy.setAttribute('aria-label', 'Report text to copy'); body.append(copy); copy.focus(); copy.select();
        status.textContent = 'Select and copy this report text. Nothing was sent.';
      }
    }));
    const send = button('Send feedback', async () => {
      if (delivery.sending) return;
      if (draft.includeEnvironment && !environmentValue) throw new Error('Wait for the environment preview, or exclude the environment fields.');
      if (!consent.node.checked) throw new Error('Review and acknowledge the report before sending.');
      if (preparingScreenshot || draft.screenshots.length > 3 || (draft.screenshots.length && (!reviewedScreenshots.node.checked || draft.screenshots.some((image) => !loadedScreenshots.has(image.grantId))))) throw new Error('Review the prepared screenshot previews before sending.');
      if (!draft.summary.trim() || !draft.details.trim()) { status.textContent = 'Add a summary and details before sending.'; return; }
      if (!/^[^\s@]{1,64}@[^\s@]+\.[^\s@]+$/.test(draft.contact) || draft.contact.length > 254) { status.textContent = 'Add a valid email address for follow-up.'; fields.contact.focus(); return; }
      for (const [key, limit] of Object.entries(limits)) if ([...draft[key]].length > limit) { status.textContent = `${key} is too long (maximum ${limit} characters).`; return; }
      delivery.sending = true; delivery.controller = new AbortController(); cancelSend.hidden = false; addScreenshot.disabled = true;
      Object.values(fields).forEach((control) => { control.disabled = true; }); environment.node.disabled = true; consent.node.disabled = true; reviewedScreenshots.node.disabled = true;
      delivery.status = 'Sending your reviewed report…'; feedbackRender?.();
      try {
        const { receipt, screenshots, ...payload } = draft;
        const result = await P.api('/api/action', { action: 'sendFeedback', params: { ...payload, screenshotGrants: screenshots.map((image) => image.grantId), confirmSend: true } }, { signal: delivery.controller.signal, timeoutMs: 100000 });
        if (draft.submissionId !== payload.submissionId) return;
        draft.receipt = result;
        delivery.status = `Received as ${str(result.reportID)}${result.receivedAt ? ` · ${new Date(result.receivedAt).toLocaleString()}` : ''}. Keep this receipt for follow-up.`;
        consent.node.checked = false;
      } catch (error) {
        delivery.status = error.name === 'AbortError' ? 'Sending stopped. The server may already have received it; retry this unchanged draft to avoid a duplicate.' : `${error.message} Your draft is kept. Retry unchanged to check the same submission.`;
      } finally {
        delivery.sending = false; delivery.controller = null; cancelSend.hidden = true; newReport.hidden = !draft.receipt; updateReportPreview(); addScreenshot.disabled = draft.screenshots.length >= 3;
        Object.values(fields).forEach((control) => { control.disabled = false; }); environment.node.disabled = false; consent.node.disabled = false;
        reviewedScreenshots.node.disabled = draft.screenshots.some((image) => !loadedScreenshots.has(image.grantId)); updateSendState();
        if (feedbackSubmission === delivery) feedbackRender?.();
      }
    }, true);
    send.disabled = true; send.dataset.locked = 'true';
    controls.append(send); body.append(controls);
    if (draft.receipt) status.textContent = `Already received as ${str(draft.receipt.reportID)}. Edit the report to create a new submission.`;
    function updateSendState() {
      send.disabled = delivery.sending || preparingScreenshot || diagnosticLoading || (draft.includeEnvironment && (environmentLoading || !environmentValue)) || !consent.node.checked || Boolean(draft.receipt) || (draft.screenshots.length > 0 && (!reviewedScreenshots.node.checked || draft.screenshots.some((image) => !loadedScreenshots.has(image.grantId))));
      send.dataset.locked = String(send.disabled);
      addScreenshot.dataset.locked = String(delivery.sending || draft.screenshots.length >= 3);
      diagnostics.node.disabled = delivery.sending || diagnosticLoading; refreshDiagnostics.disabled = delivery.sending || diagnosticLoading;
      refreshDiagnostics.dataset.locked = String(delivery.sending || diagnosticLoading);
      updateReportPreview();
    }
    function attachmentChanged() {
      draft.submissionId = crypto.randomUUID(); draft.receipt = null; consent.node.checked = false; reviewedScreenshots.node.checked = false;
      status.textContent = 'Screenshots changed. Review the prepared previews and report before sending.'; updateSendState();
    }
    function renderScreenshots() {
      for (const url of screenshotURLs) URL.revokeObjectURL(url); screenshotURLs.clear(); loadedScreenshots.clear(); screenshotList.replaceChildren();
      reviewedScreenshots.wrapper.hidden = !draft.screenshots.length; reviewedScreenshots.node.disabled = Boolean(draft.screenshots.length);
      draft.screenshots.forEach((image, index) => {
        const card = el('div', 'feedback-screenshot'); const preview = el('img'); preview.alt = `Prepared screenshot ${index + 1}`;
        const url = URL.createObjectURL(image.blob); screenshotURLs.add(url); preview.src = url;
        preview.addEventListener('load', () => {
          if (!opened.dialog.open || !draft.screenshots.includes(image)) return;
          loadedScreenshots.add(image.grantId); reviewedScreenshots.node.disabled = delivery.sending || draft.screenshots.some((item) => !loadedScreenshots.has(item.grantId)); updateSendState();
        }, { once: true });
        preview.addEventListener('error', () => { status.textContent = 'A screenshot preview could not be displayed. Remove it and choose the image again.'; loadedScreenshots.delete(image.grantId); updateSendState(); }, { once: true });
        card.append(preview, el('p', '', `${image.width} × ${image.height} · ${(image.bytes / 1024).toFixed(0)} KB`), button(`Remove screenshot ${index + 1}`, () => {
          if (delivery.sending) return;
          draft.screenshots = draft.screenshots.filter((item) => item !== image); attachmentChanged(); renderScreenshots();
        })); screenshotList.append(card);
      });
      addScreenshot.disabled = draft.screenshots.length >= 3 || delivery.sending; updateSendState();
    }
    const changed = () => {
      const values = Object.fromEntries(Object.entries(fields).map(([key, control]) => [key, control.value])); values.includeEnvironment = environment.node.checked;
      if (!Object.entries(values).some(([key, value]) => draft[key] !== value)) return;
      Object.assign(draft, values, { submissionId: crypto.randomUUID(), receipt: null }); consent.node.checked = false; updateSendState(); status.textContent = 'Draft updated. Review these fields before sending.';
    };
    Object.values(fields).forEach((control) => { control.addEventListener('input', changed); control.addEventListener('change', changed); }); environment.node.addEventListener('change', changed);
    consent.node.addEventListener('change', updateSendState); reviewedScreenshots.node.addEventListener('change', updateSendState);
    diagnostics.node.addEventListener('change', () => run(() => captureDiagnostics(diagnostics.node.checked)));
    feedbackRender = () => {
      if (feedbackDialog !== opened.dialog || feedbackDraft !== draft) return;
      Object.values(fields).forEach(control => { control.disabled = delivery.sending; });
      environment.node.disabled = consent.node.disabled = delivery.sending;
      reviewedScreenshots.node.disabled = delivery.sending || draft.screenshots.some(image => !loadedScreenshots.has(image.grantId));
      cancelSend.hidden = !delivery.sending; newReport.hidden = !draft.receipt;
      if (delivery.status) status.textContent = delivery.status;
      updateSendState();
    };
    renderDiagnosticPreview(); renderScreenshots(); updateReportPreview(); feedbackRender();
    opened.dialog.addEventListener('close', () => { for (const url of screenshotURLs) URL.revokeObjectURL(url); screenshotURLs.clear(); if (feedbackDialog === opened.dialog) feedbackDialog = null; }, { once: true });
  }

  const sourceChoice = select([['', 'All sources']], ''); sourceChoice.setAttribute('aria-label', 'Source');
  const folderChoice = select([['', 'All folders']], ''); folderChoice.setAttribute('aria-label', 'Folder');
  const statusChoice = select([['all', 'All statuses'], ['pending', 'Pending'], ['tagged', 'Tagged'], ['needsAttention', 'Needs attention']], 'all'); statusChoice.setAttribute('aria-label', 'Photo status');
  const sortChoice = select([['library', 'Library order'], ['filename', 'Filename'], ['recentlyTagged', 'Recently tagged'], ['recentlyImproved', 'Recently improved'], ['dateTaken', 'Date taken'], ['relevance', 'Relevance']], 'library'); sortChoice.setAttribute('aria-label', 'Sort photos');
  const relatedChoice = checkbox('Related', true);
  const metadataButton = button('Camera, place & time', openMetadataFilters);
  statusChoice.classList.add('pf-legacy-status');
  $('library-filters').append(sourceChoice, folderChoice, button('Subfolders', openFolderChooser), statusChoice);
  ($('sort-controls') || $('library-filters')).append(sortChoice);
  $('library-filters').append(metadataButton, relatedChoice.wrapper);
  sourceChoice.addEventListener('change', () => run(() => P.setScope({ sourceId: sourceChoice.value, scopeId: '' })));
  folderChoice.addEventListener('change', () => run(() => P.setScope({ scopeId: folderChoice.value })));
  statusChoice.addEventListener('change', () => run(() => P.setScope({ status: statusChoice.value })));
  sortChoice.addEventListener('change', () => run(() => P.setScope({ sort: sortChoice.value })));
  relatedChoice.node.addEventListener('change', () => run(() => P.setScope({ includeRelated: relatedChoice.node.checked })));
  let scopeSignature = '';
  const knownFolders = new Map();
  function rememberFolders(folders) {
    for (const folder of list(folders)) if (typeof folder.scopeId === 'string' && folder.scopeId) {
      knownFolders.delete(folder.scopeId); knownFolders.set(folder.scopeId, folder);
    }
    while (knownFolders.size > 512) {
      const oldest = [...knownFolders.keys()].find(id => id !== P.getScope().scopeId);
      if (!oldest) break;
      knownFolders.delete(oldest);
    }
  }
  function updateScopeChoices() {
    const scope = P.getScope();
    const sources = list(snapshot.sources);
    const page = P.getPage();
    if (page?.aiScopeId && !knownFolders.has(page.aiScopeId)) knownFolders.set(page.aiScopeId, { scopeId: page.aiScopeId, sourceId: page.aiSourceId, name: page.aiFolderLabel || 'Saved folder scope' });
    rememberFolders(snapshot.folders);
    const folders = [...knownFolders.values()].filter((folder) => !scope.sourceId || folder.sourceId === scope.sourceId);
    const signature = JSON.stringify([sources.map((source) => [source.id, source.name, source.label]), folders.map((folder) => [folder.scopeId, folder.label, folder.name, folder.count]), scope.sourceId, scope.scopeId]);
    if (signature !== scopeSignature) {
      scopeSignature = signature;
      const sourceOptions = [['', 'All sources'], ...sources.map((source) => [source.id || source.sourceId, source.name || source.label || 'Source'])];
      if (scope.sourceId && !sourceOptions.some(([id]) => id === scope.sourceId)) sourceOptions.push([scope.sourceId, 'Saved source · unavailable']);
      sourceChoice.replaceChildren(...sourceOptions.map(([id, label]) => { const item = el('option', '', label); item.value = id; return item; }));
      sourceChoice.value = scope.sourceId;
      folderChoice.replaceChildren(...[['', 'All folders'], ...folders.filter((folder) => folder.scopeId).map((folder) => [folder.scopeId, `${folder.label || folder.name || folder.pathLabel || 'Folder'}${Number.isFinite(folder.count) ? ` · ${fmt.format(folder.count)}` : ''}`])].map(([id, label]) => { const item = el('option', '', label); item.value = id; return item; }));
      folderChoice.value = scope.scopeId;
    }
    statusChoice.value = scope.status; relatedChoice.node.checked = scope.includeRelated;
    const filterCount = Object.values(scope.metadataFilters).filter((value) => String(value).trim()).length;
    metadataButton.textContent = `Camera, place & time${filterCount ? ` · ${filterCount}` : ''}`;
    sortChoice.value = P.getAIJobId() ? P.getPage()?.sort || 'relevance' : scope.sort;
  }

  function openFolderChooser() {
    const { dialog, body } = modal('Browse folders', 'A folder is a subtree of its existing source. Choosing it changes browse, search and tagging scope; it does not register a new source. Saved subfolders remain usable with originals offline.');
    const content = el('div'); body.append(content); let controller = null;
    const choose = async (folder) => {
      const sourceId = folder.sourceId || P.getScope().sourceId;
      await P.setScope({ sourceId, scopeId: folder.scopeId }); dialog.close();
    };
    const row = (folder) => {
      const card = el('div', 'folder-row');
      card.append(el('div', '', `${folder.name || folder.pathLabel || folder.label || 'Folder'} · ${fmt.format(num(folder.count))} photos`));
      const actions = el('div', 'button-row');
      const targets = { scopeId: folder.scopeId, ...(folder.sourceId ? { sourceId: folder.sourceId } : {}) };
      actions.append(button('Use this folder', () => choose(folder)), button('Open subfolders', () => load(folder)),
        button('Tag pending in this folder', () => enqueue('tagging', { ...targets })),
        button('Improve this folder', () => enqueue('improve', { ...targets, force: false })),
        button('Improve overnight', () => enqueue('improve', { ...targets, force: false }, { mode: 'overnight', start: snapshot.improvementPreferences?.start || '22:00', end: snapshot.improvementPreferences?.end || '07:00' })),
        button('Screen saved tags', () => enqueue('reviewScreen', { ...targets })),
        button('Retag this folder…', () => confirmAction('Retag this whole folder?',
          'This includes every tagged or imported photo in this folder and its subfolders, independently of the current page, search and status filters. Other folders and confirmed names are preserved; existing good tags remain if replacement fails.',
          'Retag whole folder', () => enqueue('tagging', { ...targets, force: true }))));
      card.append(actions); return card;
    };
    const roots = () => {
      controller?.abort(); content.replaceChildren();
      const folders = list(snapshot.folders).filter((folder) => !P.getScope().sourceId || folder.sourceId === P.getScope().sourceId);
      if (!folders.length) content.append(el('p', 'dialog-intro', 'No source folders in this scope. Add a folder or choose another source.'));
      folders.forEach((folder) => content.append(row(folder)));
    };
    const load = async (folder) => {
      if (!folder.scopeId) return;
      controller?.abort(); const request = new AbortController(); controller = request;
      content.replaceChildren(el('p', 'dialog-intro', 'Reading subfolders…'));
      try {
        const result = await P.api('/api/action', { action: 'folderChildren', params: { scopeId: folder.scopeId } }, { signal: request.signal });
        if (request.signal.aborted || !dialog.open) return;
        const sourceId = folder.sourceId || P.getScope().sourceId;
        const current = { ...result.folder, sourceId };
        const children = list(result.children).map((child) => ({ ...child, sourceId }));
        rememberFolders([current, ...children]);
        updateScopeChoices(); content.replaceChildren();
        const trail = el('div', 'button-row'); trail.append(button('Source folders', roots));
        for (const ancestor of list(result.ancestors)) trail.append(button(ancestor.name || ancestor.pathLabel || 'Folder', () => load({ ...ancestor, sourceId })));
        content.append(trail, row(current));
        if (result.message) content.append(el('p', 'dialog-intro', result.message));
        if (result.needsRefresh) content.append(el('p', 'dialog-intro', 'New files may be available. Discovering locations adds pending records without altering accepted details.'));
        content.append(button('Discover files in this folder', async () => { await P.action('discoverFolders', { scopeId: current.scopeId, recursive: true }); P.showNotice('Folder discovery started. Its progress is available in Jobs.'); }));
        if (!children.length) content.append(el('p', 'dialog-intro', 'No subfolders are recorded here.'));
        children.forEach((child) => content.append(row(child)));
      } catch (error) {
        if (request.signal.aborted) return;
        content.replaceChildren(el('p', 'error-text', error.message), button('Try again', () => load(folder)), button('Source folders', roots));
      }
    };
    dialog.addEventListener('close', () => controller?.abort(), { once: true });
    const selected = knownFolders.get(P.getScope().scopeId);
    if (selected) run(() => load(selected)); else roots();
  }

  function openMetadataFilters() {
    const { dialog, body } = modal('Camera, place & time', 'These filters use recorded metadata and apply before AI ranking. Unknown fields remain unknown. Focal lengths use actual millimetres.');
    const values = P.getScope().metadataFilters;
    const controls = {};
    const grid = el('div', 'settings-grid');
    const definitions = [['camera', 'Camera', 'text'], ['lens', 'Lens model', 'text'], ['place', 'Recorded place', 'text'], ['dateFrom', 'Taken from', 'date'], ['dateTo', 'Through', 'date'], ['focalMin', 'Minimum focal length · mm', 'number'], ['focalMax', 'Maximum focal length · mm', 'number'], ['latitude', 'Latitude', 'number'], ['longitude', 'Longitude', 'number'], ['radiusKm', 'Within · km', 'number']];
    for (const [key, label, type] of definitions) {
      const control = input(type, values[key] ?? ''); if (type === 'number') control.step = 'any';
      const preference = key.startsWith('date') ? 'date' : key.startsWith('focal') ? 'focal' : ['place', 'latitude', 'longitude', 'radiusKm'].includes(key) ? 'location' : key;
      control.disabled = snapshot.metadataPreferences?.[preference] === false;
      controls[key] = control; grid.append(field(label, control));
      if (['camera', 'lens', 'place'].includes(key)) attachSuggestions(control, key, controls);
    }
    const actions = el('div', 'button-row');
    actions.append(button('Clear filters', async () => { await P.setScope({ metadataFilters: {} }); dialog.close(); }), button('Apply filters', async () => {
      const filters = Object.fromEntries(Object.entries(controls).map(([key, control]) => [key, control.value]));
      if (filters.dateFrom && filters.dateTo && filters.dateFrom > filters.dateTo) throw new Error('Choose an end date on or after the start date.');
      if (filters.radiusKm && Number(filters.radiusKm) <= 0) throw new Error('Choose a positive GPS radius.');
      await P.setScope({ metadataFilters: filters }); dialog.close();
    }, true));
    body.append(grid, el('p', 'dialog-intro', 'Place labels are approximate offline GeoNames data. Metadata preferences in Settings control which fields are available.'), actions);
  }
  function attachSuggestions(control, key, controls) {
    const datalist = el('datalist'); datalist.id = `metadata-suggestions-${key}`; control.setAttribute('list', datalist.id); control.after(datalist);
    // The datalist travels with the input when its label is placed into the dialog.
    queueMicrotask(() => control.parentNode?.append(datalist));
    let controller; let timer; let suggestions = [];
    queueMicrotask(() => control.closest('dialog')?.addEventListener('close', () => { clearTimeout(timer); controller?.abort(); }, { once: true }));
    control.addEventListener('change', () => {
      const chosen = suggestions.find((item) => item.kind === 'focal' && (item.label === control.value || item.value === control.value));
      if (key === 'lens' && chosen) { control.value = ''; controls.focalMin.value = chosen.value; controls.focalMax.value = chosen.value; }
    });
    control.addEventListener('input', () => {
      clearTimeout(timer); controller?.abort();
      timer = setTimeout(async () => {
        controller = new AbortController(); const request = controller; const query = control.value;
        try {
          const filters = Object.fromEntries(Object.entries(controls).map(([name, node]) => [name, name === key ? '' : node.value]));
          const result = await P.api('/api/action', { action: 'metadataSuggestions', params: { field: key, query, ...scopedParams(), statusFilter: P.getScope().status, metadataFilters: filters, limit: 8 } }, { signal: request.signal });
          if (!control.isConnected || request.signal.aborted || control.value !== query) return;
          suggestions = list(result.items);
          datalist.replaceChildren(...suggestions.map((item) => { const option = el('option'); option.value = item.kind === 'focal' ? str(item.label) : str(item.value); option.label = `${str(item.label)} · ${fmt.format(num(item.count))}`; return option; }));
        } catch { /* Free text remains usable when suggestions are unavailable. */ }
      }, 200);
    });
  }

  window.addEventListener('photofindry:selection', () => {
    const ids = P.selectedIDs(); const bar = $('selection-actions'); bar.hidden = !ids.length;
    bar.replaceChildren(el('span', 'selection-summary', `${fmt.format(ids.length)} selected`));
    if (!ids.length) return;
    const selectedJob = (kind, force = false) => async () => {
      const frozen = P.selectedIDs(); if (!frozen.length) throw new Error('Select at least one photograph.');
      await enqueue(kind, { photoIds: frozen, force });
    };
    bar.append(button('Tag', selectedJob('tagging')), button('Retag', () => {
      const frozen = P.selectedIDs(); if (!frozen.length) return;
      confirmAction('Retag selected photos?', `Replace tags for exactly ${fmt.format(frozen.length)} selected photos. Existing good tags are kept if replacement fails. Unselected photos are unchanged.`, 'Retag selected', () => enqueue('tagging', { photoIds: frozen, force: true }));
    }), button('Improve', selectedJob('improve')), button('Metadata', selectedJob('metadata')), button('Prepare previews', selectedJob('previews')), button('Review details', () => openAdvanced('improvement')));
  });

  function stopPendingProbe() {
    clearTimeout(aiPendingProbeTimer); aiPendingProbeTimer = null;
  }
  function pendingProbe(id) {
    if (aiPendingProbeJobId !== id) {
      stopPendingProbe(); aiPendingProbeJobId = id; aiPendingProbeUsed = false;
    }
    if (aiPendingProbeUsed || aiPendingProbeTimer) return;
    // A crash can commit the candidate header before its queue readiness marker.
    // One delayed probe recovers that gap without polling every preparation phase.
    aiPendingProbeTimer = setTimeout(() => {
      aiPendingProbeTimer = null; aiPendingProbeUsed = true;
      const page = P.getPage();
      if (P.getAIJobId() !== id || (page?.aiJobId === id && page.aiResultsReady === true)) return;
      run(() => P.refreshCurrentPage());
    }, 2000);
  }
  async function openAIJob(id, requestId) {
    clearTimeout(aiRefreshTimer); aiProgressSignature = '';
    const opening = P.openAIResults(id, requestId);
    renderAIResults(P.getPage());
    await opening;
  }
  function returnToLibrary() {
    clearTimeout(aiRefreshTimer); aiProgressSignature = '';
    stopPendingProbe(); aiPendingProbeJobId = null;
    const loading = P.returnToLibrary();
    renderAIResults(null);
    return loading;
  }
  window.addEventListener('photofindry:page', (event) => renderAIResults(event.detail));
  function renderAIResults(page) {
    const target = $('ai-results-controls'); const id = P.getAIJobId(); target.hidden = !id;
    $('library-title').textContent = id ? 'AI search candidates' : 'Your photographs';
    $('search-help').textContent = id ? 'Local AI Search ranks saved evidence using your local models. Typing starts ordinary saved-evidence search.' : 'Searches saved library evidence. No model is started.';
    if (!id) {
      if (aiControlsSignature) target.replaceChildren();
      aiControlsSignature = ''; stopPendingProbe(); aiPendingProbeJobId = null;
      updateScopeChoices(); return;
    }
    const job = list(snapshot.jobs?.jobs).find((item) => item.id === id);
    const ownsPage = page?.aiJobId === id;
    const terminalWithoutResults = ownsPage && page.aiResultsPending === false && page.aiResultsReady === false;
    const ready = ownsPage && page.aiResultsPending !== true && page.aiResultsReady !== false;
    const pending = !ready && !terminalWithoutResults;
    const jobState = job?.state || (ownsPage ? page.aiJobState : '');
    const pool = ready ? num(page.aiCandidateCount ?? page.aiRetrievedCount ?? page.totalCount) : 0;
    const refined = ready ? num(page.aiRefinedCount) : 0;
    const view = ready ? page.aiCandidateView || 'best' : 'best';
    const viewCount = ready ? num(page.aiViewTotalCount ?? page.totalCount) : 0;
    const summary = pending ? 'Preparing saved AI search candidates…'
      : terminalWithoutResults ? 'No candidates were saved for this search.'
      : `${view === 'best' ? 'Best so far' : view === 'refined' ? 'Refined' : 'All candidates'} · ${fmt.format(viewCount)} of ${fmt.format(pool)} candidates · ${fmt.format(refined)} refined. Ranking does not confirm a match.`;
    const status = pending
      ? str(job?.message || (ownsPage ? page.aiError : '')) || 'The local search is preparing its first saved candidate page.'
      : terminalWithoutResults ? str(page.aiError || job?.message) || (page.aiStopped ? 'Search stopped before candidates were saved.' : 'See Jobs for the search outcome.')
      : [page.aiStale ? 'Saved results are older than the current library evidence.' : '', page.aiStopped ? 'Stopped. Reviewed candidates are kept; this search cannot resume.' : '', page.aiPartial ? 'Results are partial.' : '', page.semanticLimited === true ? 'Related evidence indexing is incomplete; other photos may become searchable later. Every retrieved candidate is retained.' : '', page.aiRankingNotice || page.aiError || ''].filter(Boolean).join(' ');
    const signature = JSON.stringify([id, pending, terminalWithoutResults, summary, status, view, jobState]);
    if (signature === aiControlsSignature) return;
    aiControlsSignature = signature;
    if (pending && job?.aiResultsReady !== true && (!job || !terminal(job))) pendingProbe(id);
    else stopPendingProbe();
    target.replaceChildren(el('p', '', summary)); if (status) target.append(el('p', '', status));
    const tabs = el('div', 'candidate-tabs');
    if (ready) for (const [key, label] of [['best', 'Best so far · up to 60'], ['refined', 'Refined'], ['all', 'All candidates']]) {
      const tab = button(label, () => P.setCandidateView(key)); tab.setAttribute('aria-pressed', String(view === key)); tabs.append(tab);
    }
    tabs.append(button('Ordinary search', returnToLibrary));
    if (job && !terminal(job)) {
      tabs.append(button(job.state === 'paused' ? 'Resume search' : 'Pause search', () => jobAction(job.state === 'paused' ? 'resume' : 'pause', job.id)), button('Stop search', () => stopJob(job)));
    }
    if (!pending && (!job || terminal(job))) tabs.append(button('Run AI Search again', async () => {
      if (!requireModel()) return;
      const requestId = crypto.randomUUID(); const scope = P.getScope();
      const next = await enqueue('aiSearch', { ...scopedParams(), requestId, query: scope.query, metadataFilters: scope.metadataFilters, locationOptions: scope.locationOptions, broaden: false, sort: 'relevance', candidateView: 'best' });
      if (next?.id) await openAIJob(next.id, requestId);
    }));
    target.append(tabs); updateScopeChoices();
  }

  async function jobAction(action, id, schedule) {
    const result = await P.action('jobAction', { action, ...(id ? { id } : {}), ...(schedule ? { schedule } : {}) });
    if (P.getAIJobId() === id) await P.refreshLibrary();
    return result;
  }
  async function stopAllRemaining() {
    // The confirmed queue-wide stop is idempotent. A lost local HTTP reply may
    // be retried for this same library without selecting any new job or photo.
    const libraryId = snapshot.libraryId;
    for (let attempt = 0; attempt < 4; attempt += 1) {
      try {
        const jobs = await P.api('/api/action', { action: 'jobAction', params: { action: 'cancelAll' } }, { timeoutMs: 10000 });
        if (snapshot.libraryId === libraryId) { snapshot = { ...snapshot, jobs }; renderJobs(true); }
        P.refreshState().catch(() => { /* The next normal status poll also refreshes Jobs. */ });
        return jobs;
      } catch (error) {
        if (error?.name === 'AbortError' || error?.status && error.status < 500 || snapshot.libraryId !== libraryId || attempt === 3) throw error;
        await new Promise((resolve) => setTimeout(resolve, 750 * (attempt + 1)));
      }
    }
  }
  function stopJob(job) {
    confirmAction('Stop remaining work?', 'Completed photos, accepted tags and reviewed search candidates are kept. Remaining targets are cancelled. A stopped AI search cannot resume.', 'Stop remaining work', () => jobAction('cancel', job.id));
  }
  function editSchedule(job) {
    const { dialog, body } = modal('When to run', job.title || 'Scheduled work');
    const mode = select([['now', 'Run anytime'], ['overnight', 'Overnight']], job.schedule?.mode || 'now');
    const start = input('time', job.schedule?.start || '22:00'); const end = input('time', job.schedule?.end || '07:00');
    const grid = el('div', 'settings-grid'); grid.append(field('Schedule', mode), field('From', start), field('Until', end));
    const change = () => { start.disabled = end.disabled = mode.value !== 'overnight'; }; mode.addEventListener('change', change); change();
    body.append(grid, el('p', 'dialog-intro', 'Uses this Mac’s local clock. Keep the companion running and the Mac awake. Closing the lid or choosing Sleep still pauses work.'), button('Save schedule', async () => {
      if (mode.value === 'overnight' && (!start.value || !end.value || start.value === end.value)) throw new Error('Choose different start and end times.');
      await jobAction('schedule', job.id, { mode: mode.value, start: start.value, end: end.value }); dialog.close();
    }, true));
  }
  // These presentations mirror the native durable-queue rules. Source waiting is
  // not a generic user pause, and unavailable originals are not tagging failures.
  function jobFacts(job) {
    const settled = job.state === 'paused' && job.pauseReason === 'source-missing' && job.phase === 'available-finished';
    const sourceWait = job.state === 'paused' && ['safety-source', 'source-capsule', 'source-missing', 'source-waiting'].includes(job.pauseReason);
    const automaticWait = sourceWait && !settled && job.sourceLifecycleVersion === 1 && job.waitingForAvailability === true;
    const sourceWork = ['tagging', 'metadata', 'faces', 'previews', 'improve'].includes(job.kind);
    const total = Math.max(0, num(job.total)), failed = Math.max(0, num(job.failed));
    const completed = Math.max(0, num(job.completed)), skipped = Math.max(0, num(job.skippedOriginalCount));
    const waiting = Math.max(0, num(job.waitingOriginalCount ?? job.deferredOriginalCount)), missing = Math.max(0, num(job.missingOriginalCount));
    const recover = sourceWork && (sourceWait && Boolean(job.sourceNextAction || job.pauseReason === 'safety-source')
      || !['completed', 'cancelled', 'paused'].includes(job.state) && waiting + missing > 0 && Boolean(job.sourceNextAction));
    const sourceMessage = recover ? ({
      reviewIdentity: 'Some originals need verification; folder confirmation may be needed after a move or drive remount. Review originals in Files; other usable photos can continue.',
      allowAccess: 'Open Files, select this source, and choose its originals folder again to allow access. Saved details are kept.',
    })[job.sourceNextAction?.kind] || '' : '';
    const status = settled ? 'Originals missing' : sourceWait ? 'Waiting for originals' : job.state === 'pausing' ? job.pauseReason === 'cancel' ? 'Stopping…' : 'Pausing…' : job.state === 'queued' && job.waitingForSchedule ? 'Scheduled' : ({ running: 'Running', queued: 'Queued', paused: 'Paused', completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled' })[job.state] || str(job.state);
    let message = str(job.message);
    if (settled) message = 'Finished available photos. Saved tags and retained previews are kept. Verified returning originals continue automatically; review missing originals or check recovered photos now.';
    else if (sourceWait && job.sourceNextAction?.kind === 'reviewIdentity') message = 'Progress is saved. Review these originals in Files before they can continue.';
    else if (automaticWait) message = snapshot.jobs?.paused ? 'Progress is saved. Resume the queue when you are ready; originals are checked before this work continues.' : 'Progress is saved. Available photos can continue when their folder returns. Other jobs can keep working.';
    else if (sourceWait) message = `${message.replace(/resume the queue/ig, 'resume this job').replace(/resume queue/ig, 'resume this job') || 'Connect this source in Files, then resume this job.'} Other jobs can continue while these originals are unavailable.`;
    else if (job.state === 'queued' && snapshot.jobs?.paused) message = 'Waiting for Resume queue. This job’s schedule still applies.';
    else if (job.state === 'queued' && job.waitingForSchedule) message = 'Waiting for its scheduled window. Choose Run now to start sooner.';
    else if (job.state === 'paused' && job.pauseReason === 'user' && (!message || /^paused[. ]*$/i.test(message))) message = 'Paused individually. Resume this job to continue.';
    const parts = job.sourceLifecycleVersion === 1
      ? [`${fmt.format(Math.max(0, completed - skipped - (job.kind === 'tagging' ? failed : 0)))} ${job.kind === 'tagging' ? 'tagged' : 'processed'}`]
      : [`${fmt.format(completed)} of ${fmt.format(total)} ${job.kind === 'aiSearch' ? 'reviewed' : 'processed'}`];
    let meterCompleted = completed;
    if (job.aiRanking) {
      const refining = job.aiRankingStage === 'refining';
      meterCompleted = Math.max(0, num(refining ? job.aiRefineProcessedCount ?? job.aiRefinedCount : job.aiPreviewProcessedCount ?? job.aiPreviewedCount));
      parts[0] = `${fmt.format(meterCompleted)} of ${fmt.format(total)} ${refining ? 'refinement checks' : 'preview checks'} · ${fmt.format(num(job.aiRefinedCount))} refined`;
    }
    if (missing) parts.push(`${fmt.format(missing)} originals missing`);
    if (waiting) parts.push(`${fmt.format(waiting)} originals waiting`);
    if (failed) parts.push(`${fmt.format(failed)} need attention`);
    if (skipped) parts.push(`${fmt.format(skipped)} removed from library`);
    return { settled, sourceWait, automaticWait, recover, total, completed, failed, waiting, status, message, sourceMessage, count: parts.join(' · '), meterCompleted };
  }
  function sourceProgress(job) {
    const explicit = ['prepare', 'checking', 'verify', 'connect', 'publish'].includes(job.phase) ? job.phase : null;
    const saved = job.progressDetails || {};
    const detail = explicit && saved.phase !== explicit ? {} : saved;
    const phase = explicit || detail.phase || job.phase;
    const scanned = Math.max(0, num(detail.scanned)), directories = Math.max(0, num(detail.directories));
    const checked = Math.max(0, num(detail.checked)), total = Math.max(0, num(detail.total));
    const running = ['running', 'pausing'].includes(job.state);
    if (terminal(job) && explicit) return { title: job.state === 'completed' ? job.sourceVerifyContents ? 'Content comparison complete' : 'Folder check complete' : job.state === 'cancelled' ? 'Folder check stopped' : 'Folder check needs attention', message: job.state === 'failed' ? job.message : 'Saved photographs and tags are kept.', meter: false };
    switch (phase) {
      case 'prepare': case 'checking': return { title: job.sourceVerifyContents ? 'Preparing content comparison' : 'Checking folder access', message: job.sourceVerifyContents ? 'Finding files with saved checksums. Reading their contents can take time.' : 'Finding the saved locations to check. Photo records and originals are kept.', meter: running };
      case 'verify': case 'connect': return { title: phase === 'verify' ? 'Comparing original file contents' : 'Connecting confirmed originals', message: total ? `${fmt.format(checked)} of ${fmt.format(total)} ${phase === 'verify' ? 'files' : 'locations'} checked` : phase === 'verify' ? 'Reading originals for the optional checksum comparison.' : 'Checking confirmed locations without reading photo contents.', meter: running, completed: checked, total };
      case 'publish': return { title: 'Saving checked locations', message: 'Publishing the folder check. Saved photographs and tags are kept.', meter: running };
      case 'inventory': return { title: 'Finding file locations', message: `${fmt.format(scanned)} files found · ${fmt.format(directories)} folders found`, meter: running };
      case 'validate': return { title: 'Verifying the folder inventory', message: `${fmt.format(scanned)} files found · ${fmt.format(Math.min(directories, Math.max(0, num(detail.validatedDirectories))))} of ${fmt.format(directories)} folders verified`, meter: running };
      case 'match': return { title: 'Checking original locations', message: job.sourceAction === 'check' && job.state === 'completed' ? `${fmt.format(num(job.completed))} of ${fmt.format(num(job.total))} source folders checked` : `${fmt.format(checked)}${total ? ` of ${fmt.format(total)}` : ''} originals checked`, meter: running && total > 0, completed: checked, total };
      case 'unavailable': return { title: 'Source unavailable', message: checked ? `${fmt.format(checked)} originals checked; progress saved` : 'Saved library details are still available.', meter: false };
      default: return { title: terminal(job) ? 'Source check finished' : 'Preparing source check', message: checked ? `${fmt.format(checked)} originals checked` : scanned ? `${fmt.format(scanned)} file locations saved` : str(job.message), meter: running };
    }
  }
  function recoveryAction(job) {
    jobsDialog?.close(); window.PhotoFindryPanels?.show('sources', { jobId: job.id,
      sourceId: job.sourceNextAction?.sourceId || job.sourceId, ...(job.sourceNextAction?.rootId ? { rootId: job.sourceNextAction.rootId } : {}) });
  }
  function jobActions(job, compact = false) {
    const facts = jobFacts(job), actions = [];
    const add = (key, label, action, primary = false) => {
      const control = button(label, action, primary);
      control.dataset.jobsAction = key; control.dataset.jobsFocus = JSON.stringify(['job', job.id, key]); actions.push(control);
    };
    if (facts.automaticWait || !terminal(job) && !['paused', 'pausing'].includes(job.state)) {
      if (compact && job.state === 'queued' && snapshot.jobs?.paused) add('resume', 'Resume', () => jobAction('resume', job.id), true);
      else if (compact && job.kind === 'improve' && job.state === 'queued' && job.waitingForSchedule) add('runNow', 'Run now', () => jobAction('runNow', job.id), true);
      else add('pause', 'Pause', () => jobAction('pause', job.id));
    }
    if (!terminal(job) && !facts.settled) {
      add('cancel', 'Stop', () => stopJob(job));
      if (!compact) {
        add('schedule', 'Schedule', () => editSchedule(job));
        if (!['running', 'pausing'].includes(job.state)) {
          add('runNext', 'Run next', () => jobAction('runNext', job.id));
          if (job.waitingForSchedule || job.schedule?.mode === 'overnight') add('runNow', 'Run now', () => jobAction('runNow', job.id));
        }
        if (facts.recover && job.sourceLifecycleVersion !== 1) add('continueAvailable', 'Continue available photos', () => jobAction('continueAvailable', job.id));
      }
    }
    if (facts.settled) add('retryMissing', 'Tag recovered photos', () => jobAction('retryMissing', job.id));
    if (facts.recover) add('recoverOriginals', job.sourceNextAction?.kind === 'reviewIdentity' ? 'Review originals' : job.sourceNextAction?.kind === 'allowAccess' ? 'Open Files' : job.sourceNextAction?.title || 'Find originals', () => recoveryAction(job), true);
    else if (job.state === 'paused' && !facts.automaticWait && !facts.settled) add('resume', 'Resume', () => jobAction('resume', job.id), true);
    else if (compact && facts.sourceWait && !facts.recover) add('recoverOriginals', 'Open Files', () => recoveryAction(job));
    if (job.state === 'paused' && job.sourceNextAction?.kind === 'allowAccess') add('retryAccess', 'Retry access', () => jobAction('resume', job.id));
    if (job.state === 'failed') {
      add('retry', 'Retry', () => jobAction('retry', job.id), true);
      if (!compact && job.dismissedFromQueue !== true) add('dismissJob', 'Dismiss from queue', () => jobAction('dismissJob', job.id));
    }
    if (job.kind === 'aiSearch' && (job.state === 'completed' || num(job.completed) > 0 || job.aiRanking && num(job.total) > 0 || num(job.aiPreviewedCount) > 0 || num(job.aiRefinedCount) > 0)) add('viewResults', 'View results', async () => { jobsDialog?.close(); await openAIJob(job.id); }, true);
    return actions;
  }
  function jobCard(job) {
    const facts = jobFacts(job), card = el('article', 'job-card');
    const heading = el('div', 'job-card-heading'); heading.append(el('h3', '', job.title || job.kind || 'Local job'), el('span', `job-badge ${job.state === 'failed' ? 'failed' : ''}`, facts.status)); card.append(heading);
    if (facts.message) card.append(el('p', '', facts.message));
    if (facts.sourceMessage) card.append(el('p', '', facts.sourceMessage));
    const failures = list(job.failureSummary).slice(0, 3);
    const invalidReview = job.kind === 'improve' && failures.some(value => /Invalid evidence review:|invalid answer after one automatic correction/.test(str(value)));
    if (invalidReview) {
      card.append(el('p', 'job-failure', 'The review model returned an invalid answer. Saved tags are kept. Retry to use automatic correction; if it repeats, choose another photo review model in Settings.'));
      const details = el('details'); details.append(el('summary', '', 'Validation details'));
      failures.forEach(failure => details.append(el('p', '', str(failure).slice(0, 1000)))); card.append(details);
    } else for (const failure of failures) card.append(el('p', 'job-failure', str(failure).slice(0, 1000)));
    if (job.dismissedFromQueue) card.append(el('p', '', 'Dismissed from queue · saved progress kept. Retry is still available here.'));
    if (job.modelName) { const name = el('p', 'job-model', `Model saved with job: ${str(job.modelName)}`); name.title = 'This job keeps the model selected when it was queued. Settings changes apply to new jobs.'; card.append(name); }
    const source = job.kind === 'sourceRecovery' ? sourceProgress(job) : null;
    const cleanupIndexing = job.kind === 'libraryCleanup' && job.phase === 'indexing' && !terminal(job);
    if (source) card.append(el('p', '', source.title), el('p', '', source.message));
    else if (cleanupIndexing) card.append(el('p', '', 'Checking shared saved files before deletion'));
    if (source?.meter || cleanupIndexing || !source && !cleanupIndexing && facts.total > 0 && !terminal(job) && !facts.settled) {
      const meter = el('progress');
      if (source?.total > 0) { meter.max = source.total; meter.value = Math.min(source.completed, source.total); }
      else if (!source && !cleanupIndexing) { meter.max = facts.total; meter.value = Math.min(facts.meterCompleted, facts.total); }
      meter.setAttribute('aria-label', source?.title || 'Processed targets'); card.append(meter);
    }
    if (!source && !cleanupIndexing) card.append(el('p', '', facts.count));
    if (job.schedule?.mode === 'overnight' && !terminal(job) && !facts.settled) card.append(el('p', '', `Overnight ${job.schedule.start}–${job.schedule.end}${job.runNow ? ' · Run now override' : ''}${job.waitingForSchedule && job.nextRunAt ? ` · Next ${new Date(job.nextRunAt).toLocaleString()}` : ''}`));
    if (job.finishedAt) card.append(el('p', '', new Date(job.finishedAt).toLocaleString()));
    const actions = el('div', 'button-row'); actions.append(...jobActions(job)); card.append(actions); return card;
  }
  function openJobs(activity = false) {
    if (jobsDialog?.open) { jobsDialog.focus(); return; }
    const opened = modal('Jobs & activity', 'Your library keeps working while you browse.'); jobsDialog = opened.dialog;
    const tabs = el('div', 'jobs-tabs'); tabs.setAttribute('role', 'tablist'); tabs.setAttribute('aria-label', 'Jobs and activity');
    const content = el('div', 'jobs-pane'); content.id = 'jobs-pane'; content.tabIndex = -1; content.setAttribute('role', 'tabpanel');
    const chooseTab = (value) => { jobsDialog.activity = value; renderJobs(true); };
    const queueTab = button('Queue', () => chooseTab(false)); queueTab.id = 'jobs-queue-tab';
    const activityTab = button('Activity', () => chooseTab(true)); activityTab.id = 'jobs-activity-tab';
    for (const [index, tab] of [queueTab, activityTab].entries()) {
      tab.setAttribute('role', 'tab'); tab.setAttribute('aria-controls', content.id);
      tab.addEventListener('keydown', (event) => {
        if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault(); const next = event.key === 'Home' ? 0 : event.key === 'End' ? 1 : 1 - index;
        chooseTab(next === 1); [queueTab, activityTab][next].focus();
      });
    }
    tabs.append(queueTab, activityTab); opened.body.append(tabs, content);
    jobsDialog.content = content; jobsDialog.activity = activity; jobsDialog.tabs = [queueTab, activityTab]; jobsDialog.historyOpen = true;
    jobsDialog.addEventListener('close', () => { if (jobsDialog === opened.dialog) { jobsDialog = null; jobsSignature = ''; } }, { once: true });
    renderJobs(true);
  }
  function renderJobs(force = false) {
    if (!jobsDialog?.open) return;
    const queue = snapshot.jobs || { jobs: [], notifications: [] };
    const signature = JSON.stringify([queue, snapshot.powerPreferences, jobsDialog.activity]);
    if (!force && signature === jobsSignature) return; jobsSignature = signature;
    const all = list(queue.jobs); const pending = all.filter((job) => !terminal(job) || (job.state === 'failed' && !job.dismissedFromQueue));
    const sourceSlots = pending.map((job, index) => job.kind === 'sourceRecovery' && job.background === true ? index : -1).filter(index => index >= 0);
    const sourceJobs = sourceSlots.map(index => pending[index]).sort((a, b) => str(a.createdAt).localeCompare(str(b.createdAt)) || str(a.id).localeCompare(str(b.id)));
    sourceSlots.forEach((slot, index) => { pending[slot] = sourceJobs[index]; });
    const unread = list(queue.notifications).filter(notice => !notice.read).length;
    const activeCount = queue.counts ? ['queued', 'running', 'pausing', 'paused'].reduce((sum, state) => sum + Math.max(0, num(queue.counts[state])), 0) : all.filter(job => !terminal(job)).length;
    const queueCount = Number.isSafeInteger(queue.queueCount) ? queue.queueCount : queue.counts ? activeCount + Math.max(0, num(queue.counts.attention)) : pending.length;
    const recoveries = list(queue.purgeRecovery);
    const unreadCount = Number.isSafeInteger(queue.unreadCount) ? queue.unreadCount : unread;
    const [queueTab, activityTab] = jobsDialog.tabs;
    queueTab.textContent = queueCount ? `Queue (${fmt.format(queueCount)})` : 'Queue';
    activityTab.textContent = unreadCount ? `Activity (${fmt.format(unreadCount)} unread)` : 'Activity';
    for (const [index, tab] of jobsDialog.tabs.entries()) {
      const selected = jobsDialog.activity === (index === 1); tab.setAttribute('aria-selected', String(selected)); tab.tabIndex = selected ? 0 : -1;
    }
    jobsDialog.content.setAttribute('aria-labelledby', jobsDialog.activity ? activityTab.id : queueTab.id);
    const content = jobsDialog.content; const fragment = document.createDocumentFragment();
    const previousFocus = document.activeElement;
    const ownedFocus = content.contains(previousFocus);
    const focusKey = ownedFocus ? previousFocus?.dataset.jobsFocus : null;
    const focusedChoice = ownedFocus && previousFocus.tagName === 'SELECT' ? previousFocus.value : null;
    const scrollTop = content.parentNode?.scrollTop;
    const keyed = (control, ...key) => { control.dataset.jobsFocus = JSON.stringify(key); return control; };
    if (recoveries.length) {
      const recoveryPanel = section('Finish interrupted deletion', 'Finish cleanup only for entries whose deletion already started. Untouched entries stay in Removed. Original photographs are never deleted.');
      for (const recovery of recoveries) {
        const entry = el('div', 'job-recovery');
        entry.append(el('p', '', `${fmt.format(num(recovery.startedCount))} entries started · ${fmt.format(num(recovery.untouchedCount))} untouched entries kept`));
        const busy = ['running', 'pausing'].includes(recovery.state);
        const finish = keyed(button(busy ? 'Finishing…' : 'Finish cleanup', () => P.action('jobAction', { action: 'finishDeletion', deletionId: recovery.id }), true), 'recovery', recovery.id, 'finishDeletion');
        finish.disabled = busy; finish.dataset.locked = String(busy); entry.append(finish); recoveryPanel.append(entry);
      }
      fragment.append(recoveryPanel);
    }
    const toolbar = el('div', 'jobs-toolbar');
    const queueToggle = keyed(button(queue.paused ? 'Resume queue' : 'Pause queue', () => jobAction(queue.paused ? 'resume' : 'pause'), true), 'queue', queue.paused ? 'resume' : 'pause');
    queueToggle.disabled = queue.paused ? recoveries.length > 0 : activeCount === 0;
    if (recoveries.length) queueToggle.title = 'Finish interrupted cleanup above before resuming other work.';
    const cancellingAll = queue.pauseReason === 'cancelled' && all.some(job => ['running', 'pausing'].includes(job.state));
    const stopAll = keyed(button(cancellingAll ? 'Stopping…' : 'Stop all remaining', () => confirmAction('Stop all remaining jobs?', 'This stops running work and cancels every waiting, paused and scheduled job, including jobs outside this page. Failed jobs move to Activity. Saved photos and reviewed matches are kept. The queue stays paused.', 'Stop all remaining', stopAllRemaining)), 'queue', 'cancelAll');
    stopAll.disabled = queueCount === 0 || cancellingAll; toolbar.append(queueToggle, stopAll);
    const power = keyed(select([['off', 'Keep awake: off'], ['processing', 'Awake while processing'], ['queue', 'Awake until queue finishes']], snapshot.powerPreferences?.mode || 'processing'), 'queue', 'keepAwake');
    power.setAttribute('aria-label', 'Keep Mac awake'); power.addEventListener('change', () => run(() => P.action('configurePower', { mode: power.value }))); toolbar.append(power);
    if (!jobsDialog.activity) fragment.append(toolbar, el('p', 'jobs-guidance', queue.pauseMessage || queue.error || (queue.paused ? 'Queue paused. Resume continues jobs paused with the queue. Individually paused jobs, resource holds and schedules keep their own controls.' : 'One local processing lane. Every new action runs first. Scheduled jobs need this companion running and your Mac awake. Display sleep is allowed.')));
    if (!jobsDialog.activity && snapshot.powerPreferences?.error) fragment.append(el('p', 'error-text', snapshot.powerPreferences.error));
    const queueList = el('div', 'job-list'); pending.forEach((job) => queueList.append(jobCard(job)));
    if (!jobsDialog.activity && pending.length < queueCount) fragment.append(el('p', 'jobs-guidance', `Showing next ${fmt.format(pending.length)} of ${fmt.format(queueCount)} jobs. More appear as jobs finish.`));
    if (!pending.length) queueList.append(el('p', 'dialog-intro', 'No unfinished jobs. Start work from the library, Files or Settings.'));
    if (!jobsDialog.activity) fragment.append(queueList);
    const activity = section('Recent activity', 'Dismissing a notification does not delete its job, checkpoint or saved results.');
    const notificationActions = el('div', 'button-row');
    const markAll = keyed(button('Mark all read', () => jobAction('markAllRead')), 'notices', 'markAllRead'); markAll.disabled = unreadCount === 0;
    const dismissAll = keyed(button('Dismiss all notifications', () => jobAction('dismissAll')), 'notices', 'dismissAll'); dismissAll.disabled = !list(queue.notifications).length;
    dismissAll.title = 'Remove notifications only. Job history, checkpoints and saved results are kept.';
    notificationActions.append(markAll, dismissAll); activity.append(notificationActions);
    for (const notice of [...list(queue.notifications)].sort((a, b) => str(b.createdAt).localeCompare(str(a.createdAt)))) {
      const row = el('article', 'notification-card'); row.append(el('h3', '', `${notice.read ? '' : '● '}${str(notice.title)}`));
      if (notice.message) row.append(el('p', '', notice.message));
      if (notice.createdAt) row.append(el('p', '', new Date(notice.createdAt).toLocaleString()));
      const actions = el('div', 'button-row');
      const job = all.find(job => job.id === notice.jobId);
      if (job) actions.append(...jobActions(job).filter(action => ['retry', 'dismissJob', 'viewResults'].includes(action.dataset.jobsAction)).map(action => keyed(action, 'notice', notice.id, job.id, action.dataset.jobsAction)));
      if (!notice.read) actions.append(keyed(button('Mark read', () => jobAction('markRead', notice.id)), 'notice', notice.id, 'markRead'));
      actions.append(keyed(button('Dismiss', () => jobAction('dismiss', notice.id)), 'notice', notice.id, 'dismiss')); row.append(actions); activity.append(row);
    }
    if (!list(queue.notifications).length) activity.append(el('p', '', 'No notifications.'));
    if (jobsDialog.activity) fragment.append(activity);
    const history = el('details', 'jobs-history'); history.open = jobsDialog.historyOpen;
    history.addEventListener('toggle', () => { if (history.isConnected && jobsDialog?.content === content) jobsDialog.historyOpen = history.open; });
    const finished = all.filter(terminal).sort((a, b) => str(b.finishedAt || b.updatedAt).localeCompare(str(a.finishedAt || a.updatedAt)));
    history.append(keyed(el('summary', '', `Recent job history · ${fmt.format(finished.length)} shown${queue.totalCount > all.length ? ` of ${fmt.format(queue.totalCount)} total jobs` : ''}`), 'history', 'toggle'));
    const historyList = el('div', 'job-list'); finished.forEach((job) => historyList.append(jobCard(job))); history.append(historyList); if (jobsDialog.activity) fragment.append(history);
    content.replaceChildren(fragment);
    if (ownedFocus) {
      const replacement = focusKey && [...content.querySelectorAll('[data-jobs-focus]')].find(control => control.dataset.jobsFocus === focusKey && !control.disabled && !control.hidden);
      if (replacement && focusedChoice !== null && replacement.tagName === 'SELECT'
        && [...replacement.children].some(option => option.value === focusedChoice)) replacement.value = focusedChoice;
      (replacement || content).focus({ preventScroll: true });
      if (Number.isFinite(scrollTop) && content.parentNode) content.parentNode.scrollTop = scrollTop;
    }
  }

  async function openTaggingDiagnostic(photoId) {
    if (typeof photoId !== 'string' || !photoId || photoId.length > 128) throw new Error('Choose a photograph with a saved diagnostic.');
    const { dialog, body } = modal('Tagging diagnostic', 'Read the saved final attempt and validation details for this photograph. Model output is private and stays on this Mac.');
    const controller = new AbortController();
    dialog.addEventListener('close', () => controller.abort(), { once: true });
    const content = el('div', 'diagnostic-content'); content.append(el('p', 'settings-live', 'Reading saved diagnostic…')); body.append(content);
    try {
      const report = await P.api('/api/action', { action: 'taggingDiagnostic', params: { photoId } }, { signal: controller.signal });
      if (controller.signal.aborted || !dialog.open) return;
      if (report?.photoId !== photoId) throw new Error('The companion returned a diagnostic for a different photograph.');
      const rows = [el('h3', '', str(report.photoName).slice(0, 180) || 'Saved photograph'),
        el('p', 'settings-live', str(report.summary).slice(0, 1000) || 'No diagnostic summary was saved.')];
      if (report.model) rows.push(el('p', 'settings-live', `Model: ${str(report.model).slice(0, 180)}`));
      if (report.recordedAt) rows.push(el('p', 'settings-live', `Recorded: ${str(report.recordedAt).slice(0, 80)}`));
      const attempts = list(report.attempts).slice(-8);
      if (list(report.attempts).length > attempts.length) rows.push(el('p', 'settings-live', `Showing the final ${attempts.length} saved attempts.`));
      attempts.forEach((attempt) => {
        const card = section(`Attempt ${Number.isSafeInteger(attempt.number) ? attempt.number : '—'} · ${str(attempt.finishReason).slice(0, 80) || 'Unknown finish'}`);
        const usage = [Number.isFinite(attempt.outputTokens) ? `${fmt.format(attempt.outputTokens)} output tokens` : '',
          Number.isFinite(attempt.tokenLimit) ? `${fmt.format(attempt.tokenLimit)} token limit` : '', attempt.truncated === true ? 'Saved response preview' : ''].filter(Boolean).join(' · ');
        if (usage) card.append(el('p', 'settings-live', usage));
        const raw = str(attempt.raw);
        if (raw) { const output = el('pre', 'diagnostic-raw', raw.slice(0, 12000));
          card.append(output); if (raw.length > 12000) card.append(el('p', 'settings-live', 'Display limited to the first 12,000 characters of this saved attempt.')); }
        rows.push(card);
      });
      const findings = list(report.findings).slice(0, 32);
      if (findings.length) {
        const group = section(`Validation details · ${fmt.format(list(report.findings).length)}`);
        findings.forEach((finding) => group.append(el('p', 'settings-live', [str(finding.path).slice(0, 160), str(finding.message).slice(0, 400)].filter(Boolean).join(' · '))));
        if (list(report.findings).length > findings.length) group.append(el('p', 'settings-live', `Showing the first ${findings.length} findings.`));
        rows.push(group);
      }
      content.replaceChildren(...rows);
    } catch (error) {
      if (!controller.signal.aborted && dialog.open) content.replaceChildren(el('p', 'error-text', error.message || 'The saved diagnostic could not be read.'));
    }
  }
  window.addEventListener('photofindry:tagging-diagnostic', (event) => run(() => openTaggingDiagnostic(event.detail?.photoId)));

  function openSettings() {
    if (settingsDialog?.open) { settingsDialog.focus(); return; }
    const opened = modal('Settings', 'Model files may live in any folder. Choosing or saving files does not start inference. A configured model restarts automatically when you begin tagging.');
    settingsDialog = opened.dialog;
    const draft = { grants: {}, dirty: false, modelDirty: false };
    const body = opened.body; const settings = snapshot.settings || {};
    const models = section('Local models');
    const modelLabels = {}, modelCheckRows = {};
    const checkStatus = el('p', 'model-check-summary', 'File checks read only small local headers or template text. They never start a model.');
    checkStatus.setAttribute('role', 'status');
    let checkGeneration = 0, checkController = null;
    const advanced = el('details', 'model-advanced');
    advanced.open = Boolean(settings.templateConfigured);
    advanced.append(el('summary', '', 'Advanced: chat formatting and optional template'));
    const help = {
      model: 'Required · The main .gguf weights that describe your photos. Example: Qwen3.5-4B-…-Q4_K_M.gguf. Do not choose the mmproj file here.',
      projector: 'Required · The vision adapter lets the main model read images. Choose the separate mmproj-….gguf supplied for that exact model, normally from the same download page. It is not a chat template.',
      reranker: 'For Local AI Search · A separate model that ranks saved photo descriptions. Example: qwen3-reranker-0.6b-q8_0.gguf. It is not needed for ordinary search or your first tags.',
      template: 'Usually leave empty · An optional small .jinja or .txt file from the model author that changes how prompts are formatted. Never select a .gguf here. Jinja is the template format, not another model to download.',
    };
    for (const [kind, title, prefix] of [['model', 'Vision model', 'model'], ['projector', 'Vision adapter (mmproj)', 'projector'], ['reranker', 'Search reranker', 'rerankerModel'], ['template', 'Custom chat template', 'template']]) {
      const row = el('div', 'model-file-row'); const label = el('span', 'model-file-label', settings[`${prefix}Label`] || (kind === 'template' ? 'No override — normally leave empty' : 'Not selected')); modelLabels[prefix] = label;
      const fileActions = el('div', 'button-row');
      const description = el('p', 'model-file-help', help[kind]); description.id = `model-file-help-${kind}`;
      const choose = button('Choose…', async () => {
        const chosen = await P.pick(kind); if (!opened.dialog.open || chosen.cancelled || !chosen.grantId) return;
        draft.grants[`${prefix}Grant`] = chosen.grantId; label.textContent = chosen.label || 'Selected file'; draft.dirty = draft.modelDirty = true;
        if (['template', 'reranker'].includes(kind)) delete draft.grants[kind === 'reranker' ? 'clearReranker' : 'clearTemplate'];
        await checkModels();
      });
      choose.setAttribute('aria-label', `Choose ${title.toLowerCase()}`); choose.setAttribute('aria-describedby', description.id); fileActions.append(choose);
      if (['template', 'reranker'].includes(kind)) fileActions.append(button(kind === 'template' ? 'Use built-in' : 'Clear', async () => {
        delete draft.grants[`${prefix}Grant`]; draft.grants[kind === 'reranker' ? 'clearReranker' : 'clearTemplate'] = true;
        label.textContent = kind === 'template' ? 'No override — normally leave empty' : 'Not selected'; draft.dirty = draft.modelDirty = true;
        if (kind === 'template') jinja.node.checked = true;
        await checkModels();
      }));
      const check = el('p', 'model-file-check'); check.setAttribute('role', 'status'); modelCheckRows[kind] = check;
      row.append(el('span', '', title), label, fileActions, description, check); (kind === 'template' ? advanced : models).append(row);
    }
    const reviewerController = new AbortController();
    const reviewerEditor = window.PhotoFindryPanels?.mountReviewSettings?.(models, {
      signal: reviewerController.signal, isCurrent: () => opened.dialog.open,
    });
    if (!reviewerEditor) models.append(el('p', 'error-text', 'Photo detail reviewer controls could not be loaded. Reopen the local workspace.'));
    opened.dialog.addEventListener('close', () => reviewerController.abort(), { once: true });
    const jinja = checkbox('Use the model’s chat formatting (recommended)', settings.jinja !== false);
    jinja.node.addEventListener('change', () => { draft.dirty = draft.modelDirty = true; void checkModels(); });
    advanced.append(jinja.wrapper, el('p', '', 'Enabled: use the model’s built-in chat format, or your custom override above. Keep enabled unless the model author explicitly says otherwise.'));
    models.append(advanced);
    async function checkModels() {
      const generation = ++checkGeneration; checkController?.abort(); checkController = new AbortController();
      const changes = { ...draft.grants, jinja: jinja.node.checked };
      checkStatus.textContent = 'Checking local files… No model is being started.';
      try {
        const report = await P.api('/api/model-files/check', changes, { signal: checkController.signal });
        if (generation !== checkGeneration || !opened.dialog.open) return null;
        if (typeof report?.canSave !== 'boolean' || !Array.isArray(report.files)) throw Error('The companion could not return file checks. Try Check files again.');
        for (const node of Object.values(modelCheckRows)) node.textContent = '';
        for (const file of report.files) {
          const node = modelCheckRows[file.role]; if (!node) continue;
          node.textContent = file.message; node.dataset.status = file.status;
        }
        checkStatus.textContent = report.summary;
        if (!report.canSave) advanced.open = advanced.open || report.files.some(file => file.role === 'template' && file.status === 'error');
        return { report, generation, changes };
      } catch (error) {
        if (generation === checkGeneration && opened.dialog.open) checkStatus.textContent = error.name === 'AbortError' ? 'File check cancelled. Check files again before saving.' : error.message;
        return null;
      }
    }
    const smartCrops = checkbox('Inspect small details during improvement', settings.smartCrops === true);
    smartCrops.node.addEventListener('change', () => { draft.dirty = draft.modelDirty = true; });
    models.append(smartCrops.wrapper, el('p', '', 'Tagging starts with one whole-photo overview. Optional improvement may inspect bounded saved detail crops; existing tags remain available.'));
    models.append(checkStatus);
    const saveModels = async () => {
      if (!draft.modelDirty) return;
      const checked = await checkModels();
      if (!checked || checked.generation !== checkGeneration || !opened.dialog.open) throw Error('Files changed or could not be checked. Check files again before saving.');
      if (!checked.report.canSave) throw Error('Fix the highlighted model-file choices before saving. Your saved settings are unchanged.');
      await P.action('configure', { ...checked.changes, smartCrops: smartCrops.node.checked });
      draft.grants = {}; draft.modelDirty = false;
    };
    const modelActions = el('div', 'button-row');
    const checkPerformance = button('Check performance', async () => { await saveModels(); await P.action('optimizeModel'); });
    const stopPerformance = button('Stop performance check', () => P.action('cancelModelOptimization'));
    modelActions.append(button('Check files', () => checkModels()), button('Start model', async () => { await saveModels(); await P.action('startModel'); }), button('Unload model', () => P.action('stopModel')), checkPerformance, stopPerformance);
    models.append(modelActions); const modelStatus = el('p', 'settings-live'); models.append(modelStatus);
    const optimization = section('Automatic performance', 'The first eligible tagging job can compare a small local sample. Sample checks are separate from photographs processed; retained details and memory are checked before a faster profile is chosen.');
    const optimizationStatus = el('p', 'settings-live');
    const optimizationProgress = el('progress'); optimizationProgress.max = 1; optimizationProgress.setAttribute('aria-label', 'Performance sample checks');
    const optimizationSummary = el('p', 'settings-live'); const optimizationReports = el('div', 'settings-optimization-reports');
    optimization.append(optimizationStatus, optimizationProgress, optimizationSummary, optimizationReports); models.append(optimization);
    let optimizationReportKey = '';
    const downloads = section('Verified model downloads', 'Downloads run only when requested, use pinned sources and hashes, and keep files in the folder chosen through the native dialog. Choosing a download never changes your saved model settings automatically.');
    const downloadControls = el('div', 'button-row');
    downloadControls.append(button('Download search reranker · 639 MB', () => P.action('startDownload', { kind: 'reranker' })), button('Download CLEF Flash pair · 10.9 GB', () => P.action('startDownload', { kind: 'clef' })), button('Cancel download', () => P.action('cancelDownload')));
    const downloadLive = el('div'); downloads.append(downloadControls, downloadLive, el('p', '', 'The search reranker is Qwen3-Reranker 0.6B Q8_0 (Apache 2.0). CLEF Flash is a separate photo detail reviewer; choose its provider in Photo improvement.'), externalLink('Reranker source & licence', 'https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF'), document.createTextNode(' · '), externalLink('CLEF Flash source & licence', 'https://ollama.com/library/clef-flash/9b-q8_0'));
    const recommendations = section('Vision model starting points', 'Choose a vision-capable GGUF and its matching mmproj GGUF from the same repository. Keep space for macOS, the projector and runtime. Keep model chat formatting enabled; a separate template file is usually unnecessary.');
    const memoryTier = el('p', 'settings-live'); recommendations.append(memoryTier);
    for (const [label, url, detail] of [
      ['8–12 GB · Qwen3.5 4B Q4_K_M', 'https://huggingface.co/HauhauCS/Qwen3.5-4B-Uncensored-HauhauCS-Aggressive', 'Compact starting point. About 3.4 GB for the model and matching BF16 projector.'],
      ['16–24 GB · Qwen3.5 9B', 'https://huggingface.co/HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive', 'Q4_K_M at 16 GB; Q6_K at 24 GB. Keep room for the matching projector and runtime.'],
      ['32–36 GB · Qwen3.6 35B-A3B Q4_K_M', 'https://huggingface.co/HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive', 'About 21 GB of model weights, plus the matching projector and runtime.'],
      ['48 GB and above · Qwen3.6 35B-A3B', 'https://huggingface.co/HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive', 'Q6_K_P at 48 GB (31 GB of weights); Q8_K_P at 64 GB or more (44 GB). Add the matching F16 projector.'],
      ['16 GB and above · Gemma 4 E4B alternative', 'https://huggingface.co/HauhauCS/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive', 'Works with detail crops. About 6.6 GiB model-process RAM in our test; too tight for the default 8 GB recommendation.'],
    ]) { const row = el('p'); row.append(externalLink(label, url), el('span', '', ` · ${detail}`)); recommendations.append(row); }
    recommendations.append(el('p', '', 'These preserve the native app’s starting recommendations. Qwen 4B and Gemma E4B were tested on four photos on a 32 GB Mac; other pairs and memory tiers remain estimates awaiting qualification. Try a few selected photos and inspect Model activity first. Links open external model pages; downloads from those pages are separate from the verified companion downloads above.'));
    recommendations.append(el('p', '', 'Choose a vision / image-text-to-text GGUF supported by llama.cpp on Apple silicon. Apple Metal acceleration comes from the bundled runtime; no separate Metal model is needed. MLX, safetensors and text-only models do not meet these tagging requirements.'));
    const metadata = section('Photo metadata', 'Read locally without rewriting originals. Changes control both display and derived search indexing.');
    const metadataGrid = el('div', 'settings-grid'); const metadataNodes = {};
    for (const [key, label] of [['location', 'Location'], ['camera', 'Camera'], ['lens', 'Lens'], ['focal', 'Focal length'], ['date', 'Capture date & time'], ['exposure', 'Exposure, aperture, shutter & ISO']]) {
      const choice = checkbox(label, snapshot.metadataPreferences?.[key] ?? key !== 'exposure'); metadataNodes[key] = choice.node; metadataGrid.append(choice.wrapper); choice.node.addEventListener('change', () => { draft.dirty = true; });
    }
    metadata.append(metadataGrid, el('p', '', 'Approximate offline city labels: GeoNames · CC BY 4.0. Unknown capture zones remain unknown.'), button('Scan metadata in current scope', () => enqueue('metadata', { ...scopedParams(), force: true })));
    const storage = section('Saved previews', 'Retained previews are rebuildable copies, not originals or backups. Existing previews are never silently evicted.');
    const keepPreviews = checkbox('Keep a preview when browsing', snapshot.storagePreferences?.keepOnBrowse !== false);
    const dimension = select([['800', '800 px · smaller files'], ['1024', '1024 px']], String(snapshot.storagePreferences?.dimension || 1024));
    const savedBudgetBytes = snapshot.storagePreferences?.budgetBytes ?? 5 * 1024 ** 3;
    const budget = input('number', String(savedBudgetBytes / 1024 ** 3)); budget.min = '0.0625'; budget.max = '1024'; budget.step = 'any';
    const storageGrid = el('div', 'settings-grid'); storageGrid.append(field('Longest edge', dimension), field('Preview storage budget · GiB', budget));
    const storageStatus = el('p');
    storage.append(keepPreviews.wrapper, storageGrid, storageStatus, button('Prepare previews in current scope', () => enqueue('previews', scopedParams())));
    for (const node of [keepPreviews.node, dimension, budget]) node.addEventListener('change', () => { draft.dirty = true; });
    const search = section('Search & activity', 'Background indexing reads saved evidence and yields to foreground work. Paused indexing stays paused.');
    const indexEnabled = checkbox('Background semantic indexing', snapshot.semanticIndex?.enabled !== false);
    const stories = checkbox('Findrydal local emoji stories', snapshot.searchAppearance?.progressArtEnabled !== false);
    indexEnabled.node.addEventListener('change', () => { draft.dirty = true; });
    stories.node.addEventListener('change', () => {
      const previous = snapshot.searchAppearance?.progressArtEnabled !== false;
      snapshot.searchAppearance = { ...snapshot.searchAppearance, progressArtEnabled: stories.node.checked }; renderActivity();
      run(async () => {
        try { await P.action('configureSearchAppearance', { progressArtEnabled: stories.node.checked }); }
        catch (error) { snapshot.searchAppearance = { ...snapshot.searchAppearance, progressArtEnabled: previous }; stories.node.checked = previous; renderActivity(); throw error; }
      });
    });
    search.append(indexEnabled.wrapper, stories.wrapper);
    const searchActions = el('div', 'button-row'); searchActions.append(button('Retry search indexing', () => P.action('configureSearchIndex', { enabled: true, retry: true })), button('Queue search indexing', () => enqueue('semantic', scopedParams()))); search.append(searchActions);
    const indexStatus = el('p', 'settings-live'); search.append(indexStatus);
    const power = section('Keep Mac awake', 'Display sleep and system power preferences remain unchanged. Lid closure and manual sleep still stop processing.');
    const awake = select([['off', 'Off'], ['processing', 'While processing'], ['queue', 'Until queue finishes']], snapshot.powerPreferences?.mode || 'processing'); awake.addEventListener('change', () => { draft.dirty = true; }); power.append(field('Prevent idle system sleep', awake));
    const uninstall = section('Uninstall companion', 'Remove this companion through a macOS confirmation. Your saved library, original photographs, model files and saved settings are kept.');
    uninstall.append(button('Review uninstall…', openUninstall));
    body.append(models, downloads, recommendations, metadata, storage, search, power, lifecycleControls(), uninstall);
    const footer = el('div', 'settings-footer'); footer.append(button('Close', () => settingsDialog?.close()), button('Save settings', async () => {
      const budgetValue = Number(budget.value); if (!Number.isFinite(budgetValue) || budgetValue < 0.0625 || budgetValue > 1024) throw new Error('Choose a preview budget from 0.0625 to 1024 GiB.');
      await saveModels();
      await P.action('configureMetadata', Object.fromEntries(Object.entries(metadataNodes).map(([key, node]) => [key, node.checked])));
      const filters = { ...P.getScope().metadataFilters };
      for (const [preference, keys] of Object.entries({ camera: ['camera'], lens: ['lens'], focal: ['focalMin', 'focalMax'], date: ['dateFrom', 'dateTo'], location: ['place', 'latitude', 'longitude', 'radiusKm'] })) {
        if (!metadataNodes[preference].checked) for (const key of keys) delete filters[key];
      }
      if (JSON.stringify(filters) !== JSON.stringify(P.getScope().metadataFilters)) await P.setScope({ metadataFilters: filters });
      await P.action('configureStorage', { keepOnBrowse: keepPreviews.node.checked, dimension: Number(dimension.value), budgetBytes: Math.round(budgetValue * 1024 ** 3) });
      await P.action('configureSearchAppearance', { progressArtEnabled: stories.node.checked });
      await P.action('configureSearchIndex', { enabled: indexEnabled.node.checked });
      await P.action('configurePower', { mode: awake.value });
      draft.dirty = false; P.showNotice('Settings saved on this Mac.');
    }, true)); settingsDialog.append(footer);
    settingsDialog.update = () => {
      const physicalMemory = snapshot.monitor?.resources?.physicalMemoryBytes;
      const gib = Number.isFinite(physicalMemory) && physicalMemory > 0 ? Math.floor(physicalMemory / 1024 ** 3) : null;
      const suggested = gib === null ? '' : gib < 16 ? 'Qwen3.5 4B · Q4_K_M' : gib < 24 ? 'Qwen3.5 9B · Q4_K_M' : gib < 32 ? 'Qwen3.5 9B · Q6_K' : gib < 48 ? 'Qwen3.6 35B-A3B · Q4_K_M' : gib < 64 ? 'Qwen3.6 35B-A3B · Q6_K_P' : 'Qwen3.6 35B-A3B · Q8_K_P';
      memoryTier.textContent = gib === null ? 'Mac memory has not been reported yet. Choose a tier for your actual Mac memory and test a small sample.' : `This Mac · ${gib} GiB memory. Suggested starting point: ${suggested}. Exact setup still needs a small local sample check.`;
      const runtimeMessage = modelConfigured() && snapshot.runtime?.status === 'stopped' ? 'Configured; loads automatically when needed.' : snapshot.runtime?.message;
      modelStatus.textContent = runtimeMessage || '';
      const status = snapshot.modelOptimization || {}, result = status.result || {};
      optimizationStatus.textContent = [str(status.modelName).slice(0, 120), str(status.message).slice(0, 300), str(status.error).slice(0, 300)].filter(Boolean).join(' · ') || 'Performance will be checked automatically when eligible photos are ready.';
      const sampleTotal = Number.isSafeInteger(status.total) && status.total > 0 ? status.total : 0;
      const sampleCompleted = Number.isSafeInteger(status.completed) ? Math.max(0, Math.min(status.completed, sampleTotal)) : 0;
      optimizationProgress.hidden = status.running !== true || sampleTotal === 0;
      if (!optimizationProgress.hidden) optimizationProgress.value = sampleCompleted / sampleTotal;
      const hasResult = status.result && typeof status.result === 'object' && Object.keys(result).length > 0;
      checkPerformance.textContent = hasResult ? 'Recheck performance' : 'Check performance';
      checkPerformance.dataset.locked = status.running === true ? 'true' : 'false';
      checkPerformance.disabled = status.running === true;
      stopPerformance.hidden = status.running !== true;
      stopPerformance.dataset.locked = String(status.phase === 'saving'); stopPerformance.disabled = status.phase === 'saving';
      optimizationSummary.textContent = status.running === true
        ? sampleTotal ? `${fmt.format(sampleCompleted)} of ${fmt.format(sampleTotal)} sample checks` : 'Preparing a small local sample…'
        : hasResult ? [
          Number.isSafeInteger(result.sampleCount) ? `${fmt.format(result.sampleCount)} sample photos checked` : '',
          Number.isSafeInteger(result.testedProfiles) ? `${fmt.format(result.testedProfiles)} profiles compared` : '',
          Number.isFinite(result.baselineSecondsPerPhoto) && result.baselineSecondsPerPhoto > 0 ? `${result.baselineSecondsPerPhoto.toFixed(1)} s/photo baseline` : '',
          Number.isFinite(result.secondsPerPhoto) && result.secondsPerPhoto > 0 ? `${result.secondsPerPhoto.toFixed(1)} s/photo sample estimate` : '',
          Number.isFinite(result.speedup) && result.speedup > 0 ? `${result.speedup.toFixed(2)}× measured speed` : '',
          result.improved === true ? 'Faster checked profile selected' : 'Baseline retained',
          result.qualityChecked === true ? 'Saved detail retention checked' : 'Detail retention not fully measured',
          result.memoryMeasured === true ? `Peak ${bytes(result.peakMemoryBytes)}` : 'Peak memory not measured',
        ].filter(Boolean).join(' · ') : 'No completed performance comparison yet.';
      const reports = Array.isArray(result.reports) ? result.reports.slice(0, 8) : [];
      const reportKey = JSON.stringify([reports, result.note, result.checkedAt]);
      if (reportKey !== optimizationReportKey) {
        optimizationReportKey = reportKey;
        const rows = reports.map((trial) => {
          const line = el('p', 'settings-live');
          line.textContent = [str(trial.label).slice(0, 100) || 'Settings check', trial.accepted === true ? 'Accepted' : trial.accepted === false ? 'Not selected' : '',
            Number.isFinite(trial.secondsPerPhoto) && trial.secondsPerPhoto > 0 ? `${trial.secondsPerPhoto.toFixed(1)} s/photo` : '',
            Number.isFinite(trial.peakMemoryBytes) && trial.peakMemoryBytes >= 0 ? `peak ${bytes(trial.peakMemoryBytes)}` : '',
            Number.isFinite(trial.retention) && trial.retention >= 0 && trial.retention <= 1 ? `${Math.round(trial.retention * 100)}% retained details` : '',
            trial.cacheMeasured === false ? 'cache unmeasured' : '',
            str(trial.reason).slice(0, 240)].filter(Boolean).join(' · ');
          return line;
        });
        if (result.note) rows.push(el('p', 'settings-live', str(result.note).slice(0, 300)));
        optimizationReports.replaceChildren(...rows);
      }
      storageStatus.textContent = Number.isFinite(snapshot.storageSummary?.count)
        ? `${fmt.format(snapshot.storageSummary.count)} saved previews · ${bytes(snapshot.storageSummary.bytes)}`
        : 'Saved preview count not measured · Storage size not measured';
      const index = snapshot.semanticIndex || {};
      const photoCoverage = Number.isFinite(index.completedPhotos) && Number.isFinite(index.totalPhotos) ? `${fmt.format(index.completedPhotos)} of ${fmt.format(index.totalPhotos)} photos` : 'Photo coverage not measured';
      const detailCoverage = Number.isFinite(index.completedChunks) && Number.isFinite(index.totalChunks) ? `${fmt.format(index.completedChunks)} of ${fmt.format(index.totalChunks)} details` : 'Detail coverage not measured';
      indexStatus.textContent = [index.message, index.phase, photoCoverage, detailCoverage, index.model ? `Model: ${index.model}` : '', index.error].filter(Boolean).join(' · ');
      if (!draft.modelDirty) for (const [prefix, node] of Object.entries(modelLabels)) node.textContent = snapshot.settings?.[`${prefix}Label`] || (prefix === 'template' ? 'No override — normally leave empty' : 'Not selected');
      const download = snapshot.download || {};
      downloadLive.replaceChildren(el('p', 'settings-live', download.message || 'No download in progress.'));
      if (download.running) { const progress = el('progress'); progress.max = 1; progress.value = Math.max(0, Math.min(1, num(download.progress))); progress.setAttribute('aria-label', 'Model download'); downloadLive.append(progress); }
      if (!download.running && download.rerankerGrant) downloadLive.append(button('Use downloaded reranker', async () => {
        await P.action('configure', { rerankerModelGrant: download.rerankerGrant }); delete draft.grants.rerankerModelGrant; delete draft.grants.clearReranker;
        modelLabels.rerankerModel.textContent = snapshot.settings?.rerankerModelLabel || 'Downloaded reranker'; P.showNotice('Downloaded reranker selected.');
      }, true));
      if (!download.running && download.modelGrant && download.projectorGrant) downloadLive.append(button('Use downloaded CLEF Flash pair', async () => {
        await P.action('configurePhotoReview', { provider: 'clef-flash', modelGrant: download.modelGrant, projectorGrant: download.projectorGrant }); P.showNotice('CLEF Flash selected for photo detail review. Tagging and search model settings are unchanged.');
      }, true));
    };
    settingsDialog.update(); void checkModels(); settingsDialog.addEventListener('close', () => { checkGeneration += 1; checkController?.abort(); if (settingsDialog === opened.dialog) settingsDialog = null; }, { once: true });
  }

  function clearActivity() {
    activityController?.abort(); activityController = null; activityIdentity = '';
    for (const url of activityURLs) URL.revokeObjectURL(url); activityURLs.clear();
    clearStory(); $('job-strip').replaceChildren(); $('job-strip').hidden = true;
  }
  function clearStory() {
    storyController?.abort(); storyController = null; clearInterval(storyTimer); storyTimer = null;
    storyIdentity = ''; storyFrames = null; storyFrame = 0; storyCanvas = null;
  }
  function activityPhoto(label, photoId, current, jobId, signal, kind = 'tagging') {
    const wrapper = el('div', 'activity-photo'); wrapper.append(el('span', '', label));
    const target = el(current ? 'div' : 'button', 'activity-image');
    if (!current) { target.type = 'button'; target.disabled = !photoId; target.setAttribute('aria-label', `Open ${label.toLowerCase()} photo`); target.addEventListener('click', () => { if (photoId) run(() => P.showPhoto(photoId)); }); }
    target.append(el('span', '', photoId ? 'Loading…' : current ? 'Preparing…' : 'Waiting')); wrapper.append(target);
    if (!photoId) return wrapper;
    const request = current ? P.activityPreviewBlob(jobId, photoId, signal, kind) : P.previewBlob(photoId, signal);
    request.then((blob) => {
      if (signal.aborted || activeJobId !== jobId) return;
      const url = URL.createObjectURL(blob); activityURLs.add(url); const image = el('img'); image.alt = ''; image.src = url; target.replaceChildren(image);
    }).catch(() => { if (!signal.aborted) target.replaceChildren(el('span', '', current ? 'Preparing…' : 'No preview')); });
    return wrapper;
  }
  function activityJob() {
    const jobs = list(snapshot.jobs?.jobs).filter(job => job.dismissedFromQueue !== true);
    const active = jobs.find(job => job.id === snapshot.jobs?.activeJobId && !terminal(job));
    if (active) return active;
    if (snapshot.modelOptimization?.running) return { id: 'manual-performance-check', kind: 'performance', title: 'Model performance', state: 'running' };
    const candidates = jobs.filter(job => ['tagging', 'improve', 'aiSearch'].includes(job.kind));
    const newest = values => [...values].sort((a, b) => str(b.updatedAt || b.createdAt).localeCompare(str(a.updatedAt || a.createdAt)))[0];
    const running = candidates.find(job => ['running', 'pausing'].includes(job.state));
    const unfinished = newest(candidates.filter(job => !terminal(job)));
    const selected = running || unfinished || newest(candidates.filter(terminal));
    // Dismissing the latest summary must not uncover every older history entry.
    return selected && (running || !dismissedActivity.has(selected.id)) ? selected : null;
  }
  function activityStage(job, facts, checking) {
    if (checking) return 'Checking model performance';
    if (job.state === 'pausing') return job.pauseReason === 'cancel' ? 'Stopping & saving…' : 'Finishing the current request…';
    if (facts.settled) return 'Finished available photos';
    if (facts.sourceWait) return 'Waiting for originals';
    if (job.state === 'queued') return job.waitingForSchedule ? 'Scheduled' : snapshot.jobs?.paused ? 'Queue paused' : 'Waiting to continue';
    if (job.state === 'paused') return 'Paused · saved progress kept';
    if (job.state === 'failed') return `${job.kind === 'improve' ? 'Improvement' : job.kind === 'aiSearch' ? 'Search' : job.kind === 'tagging' ? 'Tagging' : 'Task'} needs attention`;
    if (job.state === 'cancelled') return 'Stopped · saved results kept';
    if (job.state === 'completed') return facts.failed ? 'Finished with errors' : job.kind === 'improve' ? 'Photo review complete' : job.kind === 'tagging' ? 'Tagging complete' : 'Search complete';
    if (job.kind === 'sourceRecovery') return sourceProgress(job).title;
    const ownWorker = snapshot.job?.jobId === job.id;
    const phase = job.phase || (ownWorker ? snapshot.job.phase : '');
    return ({ 'loading model': 'Loading your model', 'checking originals': 'Checking original photographs', 'preparing-photos': 'Checking original photographs', tagging: 'Reading the current photograph…', checking: 'Reviewing saved observations…', inspecting: 'Checking details in the photograph…', details: 'Checking details in the photograph…', 'repairing tags': 'Checking photo descriptions', 'building search': 'Saving details for search', indexing: 'Saving details for search', saving: 'Saving the photo review', 'saving review': 'Saving the photo review' })[phase] || job.message || (job.kind === 'improve' ? 'Preparing photo improvement' : 'Preparing photographs');
  }
  function reviewChangeSummary(review) {
    if (!review) return '';
    if (review.outcome === 'undone' || review.summaryState === 'undone') return 'Review undone · original observations restored.';
    if (review.summaryState === 'reused') return 'Saved review reused.';
    const decision = ['clef-flash', 'intern-decision-4b'].includes(review.decisionProvider) || review.summaryState === 'suggestions';
    const changes = list(review.changes);
    const change = changes[0];
    if (change) return `${({ added: 'Added detail', uncertain: 'Marked uncertain', conflicting: 'Conflicting detail', suggestion: 'Review suggestion' })[change.kind] || 'Saved change'}: ${str(change.text)}`;
    if (decision) return 'Photo detail review saved · accepted details kept.';
    return review.summaryState === 'unchanged' || ['no_change', 'reviewed', 'supported'].includes(review.outcome) ? 'Reviewed with no changes to accepted details.' : 'Open the photograph to see its saved review.';
  }
  function renderActivity() {
    const job = activityJob();
    if (!job) { activeJobId = null; clearActivity(); return; }
    activeJobId = job.id;
    const facts = jobFacts(job), checking = (job.kind === 'tagging' || job.kind === 'performance') && job.state === 'running' && snapshot.modelOptimization?.running === true;
    const rawStory = job.kind === 'aiSearch' ? snapshot.aiStory : job.kind === 'improve' ? snapshot.improvementStory : job.kind === 'tagging' ? snapshot.taggingStory : null;
    const story = rawStory && (rawStory.jobID === job.id || rawStory.jobId === job.id) ? rawStory : null;
    const running = job.state === 'running';
    const currentId = running && story?.running ? story.currentPhoto?.photoID || story.currentPhoto?.photoId : null;
    const lastId = story?.lastCompleted?.photoID || story?.lastCompleted?.photoId;
    const photos = ['tagging', 'improve'].includes(job.kind) && !checking;
    const leftId = running ? currentId : lastId;
    const identity = JSON.stringify([job.id, leftId, lastId, running, photos]);
    if (activityIdentity !== identity) {
      const frozenStory = storyFrames && storyIdentity === `${job.id}:${story?.sequence || 0}`
        ? { frames: storyFrames, frame: storyFrame, identity: storyIdentity } : null;
      clearActivity(); activityIdentity = identity; activityController = new AbortController();
      if (frozenStory) { storyFrames = frozenStory.frames; storyFrame = frozenStory.frame; storyIdentity = frozenStory.identity; }
      const strip = $('job-strip'); strip.hidden = false;
      if (photos && (!running || leftId)) strip.append(activityPhoto(running ? job.kind === 'improve' ? 'Reviewing now' : 'Reading now' : job.kind === 'improve' ? 'Last reviewed' : 'Last read', leftId, running, job.id, activityController.signal, job.kind === 'improve' ? 'improvement' : 'tagging'));
      const main = el('div', 'activity-main'); main.id = 'activity-main'; strip.append(main);
      const heading = el('h2'); heading.id = 'activity-heading';
      const stage = el('p', 'activity-stage'); stage.id = 'activity-stage';
      const meter = el('progress'); meter.id = 'activity-meter';
      const pixelMeter = el('canvas', 'activity-pixel-meter'); pixelMeter.id = 'activity-pixel-meter'; pixelMeter.height = 8;
      pixelMeter.setAttribute('role', 'progressbar'); pixelMeter.setAttribute('aria-valuemin', '0');
      const count = el('p'); count.id = 'activity-count';
      const detail = el('p'); detail.id = 'activity-detail';
      const timing = el('p'); timing.id = 'activity-timing';
      const review = el('button', 'activity-review'); review.type = 'button'; review.id = 'activity-review';
      review.addEventListener('click', () => { if (lastId) run(() => P.showPhoto(lastId)); });
      main.append(heading, stage, meter, pixelMeter, count, detail, timing, review);
      const controls = el('div', 'activity-controls'); controls.id = 'activity-controls'; strip.append(controls);
      if (photos) strip.append(activityPhoto(job.kind === 'improve' ? 'Last reviewed · saved details' : 'Last tagged · saved story', lastId, false, job.id, activityController.signal));
    }
    const set = (id, value) => { const node = $(id); if (node.textContent !== value) node.textContent = value; };
    set('activity-heading', job.title || job.kind); set('activity-stage', activityStage(job, facts, checking));
    const meter = $('activity-meter');
    const sample = snapshot.modelOptimization || {};
    const total = checking ? Math.max(0, num(sample.total)) : facts.total;
    const completed = checking ? Math.max(0, num(sample.completed)) : facts.meterCompleted;
    meter.hidden = terminal(job) || facts.settled || total === 0;
    if (!meter.hidden) { meter.max = total; meter.value = Math.min(completed, total); }
    meter.setAttribute('aria-label', checking ? 'Performance sample checks' : 'Actual processed targets');
    const pixelMeter = $('activity-pixel-meter');
    pixelMeter.hidden = meter.hidden || snapshot.searchAppearance?.progressArtEnabled === false;
    if (!pixelMeter.hidden) {
      const width = Math.max(4, Math.min(4096, Math.floor($('activity-main').getBoundingClientRect?.().width || 240)));
      const signature = `${width}:${completed}:${total}`;
      if (pixelMeter.dataset.signature !== signature) {
        pixelMeter.dataset.signature = signature; pixelMeter.width = width;
        const context = pixelMeter.getContext('2d');
        if (context) {
          const cells = Math.floor(width / 4), filled = cells * 4 * Math.min(1, Math.max(0, completed / total));
          for (let cell = 0; cell < cells; cell += 1) for (let row = 0; row < 2; row += 1) {
            context.fillStyle = '#e1e6dc'; context.fillRect(cell * 4, row * 4, 3, 3);
            const extent = Math.min(3, Math.max(0, filled - cell * 4));
            if (extent > 0) { context.fillStyle = '#b57b39'; context.fillRect(cell * 4, row * 4, extent, 3); }
          }
        }
      }
      pixelMeter.setAttribute('aria-label', checking ? 'Performance sample checks' : 'Actual processed targets');
      pixelMeter.setAttribute('aria-valuemax', String(total)); pixelMeter.setAttribute('aria-valuenow', String(Math.min(completed, total)));
      meter.hidden = true;
    }
    set('activity-count', checking ? `${fmt.format(completed)} of ${fmt.format(total)} sample checks${job.kind === 'tagging' ? ` · ${facts.count}` : ''}` : facts.count);
    $('activity-count').hidden = total === 0;
    set('activity-detail', checking ? job.kind === 'tagging' ? 'Tagging starts automatically when this check finishes. Sample checks do not count as tagged photographs.' : 'Finding suitable settings for this Mac. Saved photo details stay unchanged.' : [facts.message, facts.sourceMessage].filter(Boolean).join(' '));
    const timing = snapshot.monitor?.jobTiming;
    let estimate = '';
    if (!checking && job.kind === 'tagging' && timing && snapshot.job?.jobId === job.id && !facts.settled) {
      estimate = timing.estimateState === 'adjusting' ? 'Updating estimate…' : timing.estimateState === 'finishing' ? 'Finalizing search & saving…' : timing.estimateState === 'stopping' ? 'Stopping & saving…' : Number.isFinite(timing.remainingSeconds) && timing.running ? `About ${duration(timing.remainingSeconds)} left${timing.estimateState === 'early' ? ' · early estimate' : ''}` : timing.running ? 'Learning this batch’s pace…' : '';
      if (timing.running && Number.isFinite(timing.averagePhotoSeconds) && timing.averagePhotoSeconds > 0) estimate += ` · ${(60 / timing.averagePhotoSeconds).toFixed(1)} photos/min`;
    }
    set('activity-timing', estimate);
    const reviewText = job.kind === 'improve' ? reviewChangeSummary(story?.lastCompleted?.improvement) : '';
    set('activity-review', reviewText); $('activity-review').hidden = !lastId || !reviewText;
    const controls = $('activity-controls');
    const controlsIdentity = JSON.stringify([job.id, job.state, snapshot.jobs?.paused, job.waitingForSchedule, facts.automaticWait, facts.settled, facts.recover, job.sourceNextAction, sample.phase]);
    if (controls.dataset.identity !== controlsIdentity) {
      controls.dataset.identity = controlsIdentity;
      if (job.kind === 'performance') { const stop = button('Stop check', () => P.action('cancelModelOptimization')); stop.disabled = sample.phase === 'saving'; stop.dataset.locked = String(stop.disabled); controls.replaceChildren(stop); }
      else controls.replaceChildren(...jobActions(job, true));
      controls.append(button('Jobs', openJobs));
      if (!['running', 'pausing'].includes(job.state)) controls.append(button('Dismiss summary', () => { dismissedActivity.add(job.id); renderActivity(); }));
    }
    updateStory(job, story);
  }
  function paintStory() {
    if (!storyCanvas) return;
    const context = storyCanvas.getContext('2d'); if (!context) return;
    context.fillStyle = '#e7ecdc'; context.fillRect(0, 0, 64, 64);
    const cells = storyFrames?.[storyFrame] || [{ column: 7, row: 7, red: 85, green: 104, blue: 65 }, { column: 9, row: 7, red: 85, green: 104, blue: 65 }];
    for (const cell of cells) { context.fillStyle = `rgb(${cell.red},${cell.green},${cell.blue})`; context.fillRect(cell.column * 4, cell.row * 4, 3, 3); }
  }
  function updateStory(job, story) {
    if (snapshot.searchAppearance?.progressArtEnabled === false || !['tagging', 'improve', 'aiSearch'].includes(job.kind)) { clearStory(); $('activity-story')?.remove(); return; }
    if (!storyCanvas || !storyCanvas.isConnected) {
      storyCanvas = el('canvas', 'activity-story'); storyCanvas.id = 'activity-story'; storyCanvas.width = storyCanvas.height = 64;
      storyCanvas.setAttribute('role', 'img'); storyCanvas.setAttribute('aria-label', 'Findrydal decorative story from saved details'); $('job-strip').append(storyCanvas); paintStory();
    }
    const sequence = story?.sequence;
    const identity = `${job.id}:${sequence || 0}`;
    if (sequence && identity !== storyIdentity) {
      storyController?.abort(); storyController = new AbortController(); const controller = storyController; storyIdentity = identity;
      P.api('/api/story', { kind: job.kind === 'aiSearch' ? 'ai' : job.kind === 'improve' ? 'improvement' : 'tagging', jobId: job.id, sequence }, { signal: controller.signal }).then((result) => {
        if (controller.signal.aborted || activeJobId !== job.id || storyIdentity !== identity) return;
        if (result.side !== 16 || !Array.isArray(result.beats) || result.beats.length !== 3 || result.beats.some((beat) => !Array.isArray(beat) || beat.length > 256 || beat.some((cell) => !Number.isInteger(cell.column) || !Number.isInteger(cell.row) || cell.column < 0 || cell.column > 15 || cell.row < 0 || cell.row > 15 || ['red', 'green', 'blue'].some((key) => !Number.isInteger(cell[key]) || cell[key] < 0 || cell[key] > 255)))) return;
        storyFrames = result.beats; storyFrame = 0; paintStory();
      }).catch(() => {});
    }
    const animate = job.state === 'running' && !snapshot.jobs?.paused && !reducedMotion.matches && story?.running !== false;
    if (animate && !storyTimer) storyTimer = setInterval(() => { storyFrame = (storyFrame + 1) % 3; paintStory(); }, 1400);
    if (!animate) { clearInterval(storyTimer); storyTimer = null; }
  }
  reducedMotion.addEventListener('change', () => renderActivity());
  window.addEventListener('resize', () => { if (activeJobId) renderActivity(); });

  window.addEventListener('photofindry:guide-destination', (event) => {
    const panel = event.detail?.panel;
    if (panel === 'settings') return openSettings();
    if (panel === 'jobs') return openJobs();
    if (['sources', 'people', 'portability'].includes(panel)) return openAdvanced(panel);
    if (panel === 'library') { window.PhotoFindryPanels?.close?.(); $('library-title').scrollIntoView({ block: 'start' }); }
  });
  window.addEventListener('photofindry:state', (event) => {
    snapshot = event.detail || {};
    const jobs = list(snapshot.jobs?.jobs);
    const sourceKinds = new Set(['tagging', 'improve', 'metadata', 'cleanup', 'sourceRecovery', 'reviewScreen']);
    const committedJobEnded = jobs.some((job) => sourceKinds.has(job.kind) && terminal(job) && previousJobs.has(job.id) && !previousJobs.get(job.id).terminal);
    previousJobs = new Map(jobs.map((job) => [job.id, { terminal: terminal(job) }]));
    if (committedJobEnded && !P.getAIJobId() && P.getScope().query.trim()) {
      clearTimeout(sourceRefreshTimer);
      sourceRefreshTimer = setTimeout(() => { if (!P.getAIJobId() && P.getScope().query.trim()) run(() => P.refreshCurrentPage()); }, 350);
    }
    updateScopeChoices(); renderJobs(); settingsDialog?.update?.(); renderActivity();
    if (P.getAIJobId()) renderAIResults(P.getPage());
    const aiId = P.getAIJobId();
    const aiJob = list(snapshot.jobs?.jobs).find((job) => job.id === aiId);
    const page = P.getPage();
    const pageReady = page?.aiJobId === aiId && page.aiResultsPending !== true && page.aiResultsReady !== false;
    const resultsReady = aiJob?.aiResultsReady === true || pageReady;
    if (resultsReady || (aiJob && terminal(aiJob))) stopPendingProbe();
    const signature = JSON.stringify(aiJob ? [aiId, aiJob.state, aiJob.phase, aiJob.completed, aiJob.aiPreviewedCount, aiJob.aiRefinedCount, aiJob.aiPreviewProcessedCount, aiJob.aiRefineProcessedCount, aiJob.aiResultsReady] : [aiId, 'missing']);
    if (aiId && (resultsReady || (aiJob && terminal(aiJob))) && signature !== aiProgressSignature) {
      clearTimeout(aiRefreshTimer);
      aiRefreshTimer = setTimeout(() => { if (P.getAIJobId() === aiId) run(() => P.refreshCurrentPage()); }, 450);
    }
    aiProgressSignature = signature;
  });
  window.addEventListener('photofindry:scope', updateScopeChoices);
  window.addEventListener('photofindry:library-restored', () => {
    // Library-bound drafts and confirmations cannot outlive the library they
    // described. Feedback is independent user-authored operational text.
    for (const dialog of [...dialogs]) if (dialog !== feedbackDialog) dialog.close();
    pendingSubmissions.clear(); jobsDialog = null; settingsDialog = null; jobsSignature = '';
    knownFolders.clear(); scopeSignature = ''; dismissedActivity.clear(); clearActivity();
    clearTimeout(aiRefreshTimer); aiProgressSignature = ''; aiControlsSignature = ''; stopPendingProbe(); aiPendingProbeJobId = null; aiPendingProbeUsed = false;
    clearTimeout(sourceRefreshTimer); previousJobs.clear();
  });
  window.addEventListener('photofindry:session', (event) => {
    if (event.detail.connected) return;
    if (uninstallHandoff) P.showNotice('The local connection closed. Check the macOS uninstall window for the outcome; a closed connection alone does not confirm removal.');
    uninstallHandoff = null;
    for (const dialog of [...dialogs]) dialog.close(); snapshot = {}; scopeSignature = ''; knownFolders.clear(); dismissedActivity.clear(); clearActivity(); feedbackSubmission?.controller?.abort(); feedbackDraft = null; feedbackSubmission = null; feedbackRender = null;
    clearTimeout(aiRefreshTimer); aiProgressSignature = ''; aiControlsSignature = ''; stopPendingProbe(); aiPendingProbeJobId = null; aiPendingProbeUsed = false; pendingSubmissions.clear();
    clearTimeout(sourceRefreshTimer); previousJobs.clear();
    $('ai-results-controls').replaceChildren(); $('ai-results-controls').hidden = true;
    $('onboarding').replaceChildren(); $('onboarding').hidden = true;
  });
  window.PhotoFindryJobs = Object.freeze({ card: jobCard, facts: jobFacts });
  window.PhotoFindryWorkspace = Object.freeze({ modal, button, field, input, element: el, section, run, openSettings, openJobs, navigate, rememberFolders,
    folderLabel: scopeId => { const folder = knownFolders.get(scopeId); return folder?.name || folder?.label || folder?.pathLabel || ''; } });
})();
