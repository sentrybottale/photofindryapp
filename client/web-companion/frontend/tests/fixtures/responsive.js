'use strict';
// Synthetic, browser-only data. There is no native bridge, filesystem grant,
// authentication token, model endpoint or customer library in this fixture.
const fixturePath = '/Users/example/External photo archive/Long but ordinary folder names/2026/Travel photographs/Originals and saved photographs';
const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480"><rect width="640" height="480" fill="#e8eddf"/><path d="M0 350L170 160L320 320L470 100L640 340V480H0Z" fill="#a8b89a"/><rect x="270" y="60" width="60" height="60" fill="#d8cba8"/></svg>';
const image = 'data:image/svg+xml,' + encodeURIComponent(svg);
const state = {
  libraryId: 'synthetic-layout-library', libraryGeneration: 1, workerGeneration: 1, photoCount: 93806,
  library: { total: 93806, tagged: 65127, scopeCount: 93806, scopeTagged: 65127 },
  people: { people: [{ id: 'person-one', name: 'Alex' }, { id: 'person-two', name: 'Morgan' }, { id: 'person-three', name: 'Sam' }] },
  settings: { modelConfigured: true, projectorConfigured: true, rerankerModelConfigured: true,
    modelLabel: 'Qwen3.5-4B-local-vision-model-with-a-deliberately-long-label-Q4_K_M.gguf',
    projectorLabel: 'mmproj-Qwen3.5-4B-vision-adapter-with-a-deliberately-long-label-BF16.gguf',
    rerankerModelLabel: 'qwen3-reranker-0.6b-q8_0.gguf', useJinja: true },
  sources: [{ id: 'fixture-source', label: 'Original photographs and travel archive', mode: 'metadata', accessOriginals: false, status: 'offline',
    statusLabel: 'Saved details available · originals folder needs confirmation', photoCount: 60978, available: 0, missing: 122, needsReview: 60380,
    roots: [{ id: 'fixture-root', scopeId: 'fixture-scope', label: 'Original photographs', locationLabel: fixturePath }] }],
  // Imported libraries may expose current and legacy folder aliases with the
  // same opaque scope identity. The source's current count is authoritative.
  folders: [
    { id: 'fixture-folder', scopeId: 'fixture-scope', sourceId: 'fixture-source', name: 'Original photographs and travel archive', count: 58240 },
    { id: 'fixture-folder-legacy', scopeId: 'fixture-scope', sourceId: 'fixture-source', name: 'Legacy path alias for the same originals', count: 58240 },
  ],
  jobs: { paused: false, totalCount: 2, counts: { running: 1, paused: 1, queued: 0 }, jobs: [
    { id: 'fixture-tag-job', kind: 'tagging', title: 'Tag photographs', state: 'paused', phase: 'paused', completed: 1234, total: 60380, message: 'Paused. Saved work and the remaining photographs are kept.', sourceId: 'fixture-source' },
    { id: 'fixture-search-job', kind: 'semanticIndex', title: 'Build semantic search', state: 'running', phase: 'checking', completed: 67328, total: 93806, message: 'Checking saved evidence. Original photographs are not read.' }
  ], notifications: [{ id: 'fixture-notice', title: 'Original folder needs confirmation', message: 'Open Files and choose the matching originals folder on this Mac.', read: false }] },
  improvementPreferences: { enabled: true, interleave: true, mode: 'overnight', start: '22:00', end: '07:00' },
  reviewPreferences: { provider: 'current' }, searchAppearance: { progressArtEnabled: false },
  searchIndexPreferences: { enabled: true }, job: { running: false }, runtime: { status: 'stopped', phase: 'stopped', modelLoaded: false },
  monitor: { modelActivityVersion: 1, modelActivityRevision: 1, phase: 'idle', activeModel: null, loadedModels: [], appTotalTokens: 449715, appInputTokens: 312000, appOutputTokens: 137715, appRequests: 423, appUnmeasuredRequests: 0, resources: { currentBytes: 194000000, peakBytes: 220000000, budgetBytes: 24000000000, complete: true, pressureLevel: 1, processes: [] } }
};
const scope = { sourceId: '', scopeId: '', query: '', status: 'all', sort: 'library', metadataFilters: {}, locationOptions: {}, includeRelated: true };
const errors = [], requests = [];
let appearanceMode = document.documentElement.dataset.appearance === 'dark' ? 'dark' : 'light';
const appearanceWrites = [];
window.addEventListener('error', event => errors.push(String(event.message)));
window.addEventListener('unhandledrejection', event => errors.push(String(event.reason?.message || event.reason)));

const action = async (name, params) => {
  requests.push({ name, params });
  if (name === 'folderChildren') return { folder: state.folders[0], children: [{ scopeId: 'child-one', name: 'Mountains and distant places', count: 8534 }, { scopeId: 'child-two', name: 'Everyday photographs', count: 12481 }], ancestors: [state.folders[0]], available: true };
  if (name === 'sourceDetails') return { candidates: [], canUndo: false, totalCount: 0 };
  if (name === 'browsePeople') return { people: { faces: [] }, peoplePage: { photoIds: [], total: 0 }, peoplePhotos: [] };
  if (name === 'prepareSourceConnection') return { planId: 'fixture-plan', sourceId: 'fixture-source', rootId: 'fixture-root', count: 60380, unresolved: 122, previousRootLocationLabel: fixturePath, rootLocationLabel: fixturePath.replace('/Users/example/', '/Users/current/') };
  throw new Error(`The isolated layout fixture does not perform ${name}.`);
};
window.PhotoFindry = {
  isConnected: () => true, getState: () => state, getScope: () => scope, getPage: () => ({ items: [], total: 60, offset: 0, limit: 240 }), getAIJobId: () => null,
  selectedIDs: () => [], setScope: async updates => { Object.assign(scope, updates); window.dispatchEvent(new CustomEvent('photofindry:scope', { detail: scope })); }, setCandidateView: async () => {}, returnToLibrary: async () => {},
  refreshLibrary: async () => {}, refreshCurrentPage: async () => {}, openAIResults: async () => {}, showPhoto: async () => {},
  action, pick: async () => ({ cancelled: true }),
  api: async (route, body) => {
    if (route === '/api/appearance') return { mode: appearanceMode };
    if (route === '/api/appearance/update') {
      if (!['light', 'dark', 'system'].includes(body?.mode)) throw new Error('Fixture appearance mode is invalid.');
      appearanceMode = body.mode; appearanceWrites.push(body.mode); return { mode: appearanceMode };
    }
    if (route === '/api/monitor') return { workerGeneration: state.workerGeneration, monitor: state.monitor, runtime: state.runtime };
    if (route === '/api/action') return action(body.action, body.params);
    if (route === '/api/model-files/check') return { canSave: true, files: [], summary: 'Synthetic model headers checked. No inference is available in this fixture.' };
    if (route === '/api/review-files/check') return { canSave: true, files: [], summary: 'Synthetic reviewer files checked.', physicalMemoryBytes: 32 * 1024 ** 3 };
    if (route === '/api/downloads') return { items: [] };
    return {};
  },
  previewBlob: async () => new Blob([svg], { type: 'image/svg+xml' }),
  activityPreviewBlob: async () => new Blob([svg], { type: 'image/svg+xml' }),
  showError: message => errors.push(String(message)), showNotice: () => {}
};
window.layoutFixture = { state, errors, requests, image, fixturePath, appearanceWrites, previewRequests: [], getAppearance: () => appearanceMode };
window.PhotoFindryShell.mount();
document.getElementById('pair-screen').hidden = true;
document.getElementById('app').hidden = false;
