'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { harness } = require('./helpers/companion-panel-harness');
const transferStatus = ui => ui.all().find(node => node.className === 'pfp-status');

test('asynchronous backup completion and failure remain visible without claiming a failed export succeeded', async () => {
  const ui = harness({ storageJob: { running: true, kind: 'export', phase: 'assets', completed: 1, total: 5, message: 'Copying saved evidence…' } });
  try {
    await ui.window.PhotoFindryPanels.show('portability');
    const status = transferStatus(ui); assert.equal(status.hidden, false); assert.match(status.textContent, /Copying saved evidence/);
    ui.state.storageJob = { running: false, kind: 'export', phase: 'complete', message: 'Library backup verified: 5 photographs across every folder.', pathLabel: 'Saved.photofindrybackup' };
    ui.emit('photofindry:state', ui.state);
    assert.equal(status.hidden, false); assert.match(status.textContent, /Library backup verified.*Saved as Saved\.photofindrybackup/s);
    assert(!status.children.some(node => /Cancel/.test(node.textContent)));
    const stable = status.children[0]; ui.state.monitor = { generatedTokens: 50 }; ui.emit('photofindry:state', ui.state); assert.equal(status.children[0], stable);
    ui.state.storageJob = { running: true, kind: 'export', phase: 'verifying', message: 'Verifying the backup…' }; ui.emit('photofindry:state', ui.state);
    ui.window.PhotoFindryPanels.close();
    ui.state.storageJob = { running: false, kind: 'export', phase: 'error', message: 'Backup verification failed. The destination was not published.' }; ui.emit('photofindry:state', ui.state);
    assert.deepEqual(ui.errors, ['Backup verification failed. The destination was not published.']);
    ui.emit('photofindry:state', ui.state); assert.equal(ui.errors.length, 1, 'Metric snapshots must not repeat a terminal error.');
    await ui.window.PhotoFindryPanels.show('portability');
    assert.equal(status.hidden, false); assert.match(status.textContent, /verification failed/); assert.doesNotMatch(status.textContent, /Library backup verified|Saved as/);
    assert(!ui.calls.some(call => /export|reveal|open/i.test(call.action)));
  } finally { await ui.close(); }
});

test('interrupted cache cleanup remains retryable even when the selected source is archived', async () => {
  const sourceId = '12345678-1234-4234-8234-123456789abc';
  const ui = harness({ sources: [{ id: sourceId, label: 'Archived source', mode: 'archived', roots: [] }],
    sourceTransfer: { running: false, kind: 'cleanup', phase: 'cleanupPending', message: 'Reconnect the verified archive to finish cache cleanup.', error: 'Archive unavailable.' } });
  try {
    await ui.window.PhotoFindryPanels.show('sources', { sourceId });
    const status = transferStatus(ui); assert.equal(status.hidden, false); assert.match(status.textContent, /finish cache cleanup.*Archive unavailable/s);
    const retry = ui.findButton('Retry cleanup'); assert.equal(retry.disabled, false); await retry.click();
    assert.deepEqual(ui.calls.filter(call => call.action === 'retrySourceTransfer'), [{ action: 'retrySourceTransfer', params: {} }]);
    assert(!ui.calls.some(call => ['attachSourceArchive', 'exportSourceArchive', 'deleteRemovedEntries'].includes(call.action)));
    await ui.window.PhotoFindryPanels.show('portability'); assert(ui.findButton('Retry cleanup'));
    ui.state.sourceTransfer = { running: true, kind: 'cleanup', phase: 'verifying', message: 'Rechecking the archive…', cancellable: true }; ui.emit('photofindry:state', ui.state);
    assert.doesNotMatch(status.textContent, /Retry cleanup/); assert.match(status.textContent, /Rechecking the archive/);
    ui.state.sourceTransfer = { running: false, kind: 'cleanup', phase: 'complete', message: 'Source cache cleanup finished.', cleanup: { deletedFiles: 7, retainedFiles: 2 } }; ui.emit('photofindry:state', ui.state);
    assert.match(status.textContent, /7 image copies removed · 2 retained/); assert.doesNotMatch(status.textContent, /Retry cleanup|Archive unavailable/);
  } finally { await ui.close(); }
});

test('a stale cleanup retry cannot run after another transfer changes the native state', async () => {
  const ui = harness({ sourceTransfer: { running: false, kind: 'cleanup', phase: 'cleanupPending' } });
  try {
    await ui.window.PhotoFindryPanels.show('portability'); const retry = ui.findButton('Retry cleanup');
    ui.state.storageJob = { running: true, kind: 'restore', phase: 'restoring' };
    await retry.click(); assert(!ui.calls.some(call => call.action === 'retrySourceTransfer'));
    assert.match(ui.errors.at(-1), /transfer state changed/);
    ui.emit('photofindry:state', ui.state); assert(ui.findButton('Retry cleanup').disabled);
    assert(!transferStatus(ui).textContent.includes('Cancel'));
  } finally { await ui.close(); }
});
