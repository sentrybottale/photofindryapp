'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {harness}=require('./helpers/companion-panel-harness');

test('advanced pages isolate only the app underlay and restore its prior inert state and focus', async () => {
  const ui = harness();
  const opener = ui.document.createElement('button'); const blocked = ui.document.createElement('section'); blocked.inert = true;
  ui.app.append(opener, blocked); opener.focus();
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const panel = ui.all().find((node) => node.id === 'advanced-panel');
    assert.equal(opener.inert, true); assert.equal(blocked.inert, true);
    assert.equal(panel.inert, false); assert.equal(ui.app.inert, false);
    const nested = ui.document.createElement('dialog'); ui.document.body.append(nested); nested.showModal();
    assert.equal(nested.inert, false, 'Body-level native dialogs remain usable');
    await ui.window.PhotoFindryPanels.show('people');
    await ui.findButton('Back to photos').click();
    assert.equal(opener.inert, false); assert.equal(blocked.inert, true);
    assert.equal(ui.document.activeElement, opener);
    await ui.window.PhotoFindryPanels.show('sources');
    ui.emit('photofindry:session', { connected: false });
    assert.equal(opener.inert, false); assert.equal(blocked.inert, true);
    assert.equal(ui.document.activeElement, opener);
  } finally { await ui.close(); }
});

test('panel navigation stays enabled across repeated clicks, closing and reopening', async () => {
  const ui = harness();
  const opener = ui.document.createElement('button'); ui.app.append(opener); opener.focus();
  const labels = ['Files', 'People', 'Library cleanup', 'Backups & archives', 'Photo improvement'];
  try {
    for (let round = 0; round < 2; round += 1) {
      await ui.window.PhotoFindryPanels.show('sources');
      for (const label of [...labels, ...labels.toReversed()]) {
        await ui.findButton(label).click();
        assert.equal(ui.findButton(label).attributes['aria-current'], 'page', label);
        for (const navigation of [...labels, 'Back to photos']) {
          assert.equal(ui.findButton(navigation).disabled, false, `${navigation} remains usable after ${label}`);
        }
      }
      await ui.findButton('Back to photos').click();
      assert.equal(ui.all().find(node => node.id === 'advanced-panel').hidden, true);
      assert.equal(opener.inert, false);
      assert.equal(ui.document.activeElement, opener);
      assert.equal(ui.findButton('Back to photos').disabled, false);
    }
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('navigation can leave a pending People read and ignores its stale completion or failure', async () => {
  for (const fails of [false, true]) {
    let settle;
    const pending = new Promise((resolve, reject) => { settle = fails ? reject : resolve; });
    const ui = harness({}, { browsePeople: () => pending });
    try {
      await ui.window.PhotoFindryPanels.show('sources');
      const opening = ui.findButton('People').click();
      assert.equal(ui.findButton('People').disabled, false, 'a slow People read cannot lock navigation');
      await ui.findButton('Backups & archives').click();
      assert.equal(ui.findButton('Backups & archives').attributes['aria-current'], 'page');
      const content = ui.all().find(node => node.className === 'pfp-body');
      const before = content.textContent;
      settle(fails ? new Error('Old People request failed') : { peoplePage: { photoIds: [], total: 0 }, peoplePhotos: [] });
      await opening;
      assert.equal(content.textContent, before, 'old People response must not repaint the active panel');
      assert.deepEqual(ui.errors, []);
      await ui.findButton('Back to photos').click();
      await ui.window.PhotoFindryPanels.show('portability');
      await ui.findButton('Back to photos').click();
      assert.equal(ui.all().find(node => node.id === 'advanced-panel').hidden, true);
    } finally { settle(fails ? new Error('Cancelled fixture') : {}); await ui.close(); }
  }
});

test('a failed People read remains navigable and can recover using Refresh people', async () => {
  let attempts = 0;
  const ui = harness({}, { browsePeople: () => {
    if (++attempts === 1) throw new Error('People read unavailable');
    return { peoplePage: { photoIds: [], total: 0 }, peoplePhotos: [] };
  } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    await ui.findButton('People').click();
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /People read unavailable/);
    assert.equal(ui.findButton('People').disabled, false);
    await ui.findButton('Refresh people').click();
    assert.equal(attempts, 2);
    assert.equal(ui.all().some(node => node.className === 'pfp-problem'), false);
    await ui.findButton('Files').click();
    await ui.findButton('People').click();
    await ui.findButton('Back to photos').click();
    assert.equal(ui.all().find(node => node.id === 'advanced-panel').hidden, true);
  } finally { await ui.close(); }
});

test('automatic improvement saves native scheduling fields without starting a job', async () => {
  const ui = harness({ improvementPreferences: { enabled: true, interleave: true, mode: 'overnight', start: '22:00', end: '07:00' } });
  try {
    await ui.window.PhotoFindryPanels.show('improvement');
    ui.field('From').value = '23:15'; ui.field('Until').value = '06:45';
    await ui.findButton('Save automatic improvement').click();
    assert.deepEqual(ui.calls.filter(call => !call.action.startsWith('/api/')), [{ action: 'configureImprovement', params: { enabled: true, interleave: true, mode: 'overnight', start: '23:15', end: '06:45' } }]);
    assert(!ui.calls.some(call => ['enqueueJob', 'startModel'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('source name draft survives a terminal job snapshot after focus moves to Save', async () => {
  const ui = harness({ jobs: { jobs: [{ id: 'job', state: 'running', kind: 'tagging' }] } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    ui.window.PhotoFindryPanels.onState(structuredClone(ui.state));
    const name = ui.field('Source name'); name.value = 'My unsaved source'; await name.fire('input');
    ui.findButton('Save name').focus();
    ui.state.jobs.jobs[0].state = 'completed'; ui.window.PhotoFindryPanels.onState(structuredClone(ui.state));
    await new Promise((resolve) => setTimeout(resolve, 250));
    assert.equal(ui.field('Source name').value, 'My unsaved source');
    await ui.findButton('Save name').click();
    assert(ui.calls.some((call) => call.action === 'renameSource' && call.params.label === 'My unsaved source'));
  } finally { await ui.close(); }
});

test('named-person routing uses the requested person ID, and archive labels cannot become URLs', async () => {
  const ui = harness({ people: { people: [{ id: 'person-a', name: 'Alex', count: 0 }], faces: [] } });
  try {
    await ui.window.PhotoFindryPanels.show('people', { personId: 'person-a' });
    assert(ui.calls.some((call) => call.action === 'browsePeople' && call.params.personId === 'person-a'));
    await ui.window.PhotoFindryPanels.show('sources');
    ui.field('Open a saved archive label').value = 'https://photofindry.com/anything';
    const before = ui.calls.length; await ui.findButton('Open saved source').click();
    assert.equal(ui.calls.length, before);
    assert.match(ui.errors.at(-1), /complete archive UUID/);
  } finally { await ui.close(); }
});

test('People keeps a saved multi-folder face scope until an explicit confirmed replacement', async () => {
  const ui = harness({ facePreferences: { enabled: true, scope: 'folders', scopeIds: ['scope-a', 'scope-b'] },
    folders: [{ scopeId: 'scope-a', name: 'Family' }, { scopeId: 'scope-b', name: 'Trips' }] });
  try {
    await ui.window.PhotoFindryPanels.show('people');
    const choice = ui.field('Automatic discovery scope');
    assert.equal(choice.value, 'saved-folders');
    assert.match(choice.textContent, /Current saved folder scope · 2 folders/);
    assert.equal(ui.findButton('Apply discovery scope').disabled, true);
    assert(!ui.calls.some((call) => call.action === 'configureFaceScan'));
    choice.value = 'new'; await choice.fire('change');
    assert.equal(ui.findButton('Apply discovery scope').disabled, false);
    const applying = ui.findButton('Apply discovery scope').click();
    await new Promise((resolve) => setImmediate(resolve));
    assert(!ui.calls.some((call) => call.action === 'configureFaceScan'), 'choosing another scope must still wait for the review');
    await ui.findButton('Apply scope').click();
    await applying;
    assert.deepEqual(ui.calls.filter((call) => call.action === 'configureFaceScan'), [
      { action: 'configureFaceScan', params: { enabled: true, scope: 'new', scopeIds: [] } },
    ]);
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('People shows an existing single-folder scan scope instead of silently selecting new photos', async () => {
  const ui = harness({ facePreferences: { enabled: true, scope: 'folders', scopeIds: ['scope-a'] },
    folders: [{ scopeId: 'scope-a', name: 'Family' }] });
  try {
    await ui.window.PhotoFindryPanels.show('people');
    assert.equal(ui.field('Automatic discovery scope').value, 'scope-a');
    assert.equal(ui.findButton('Apply discovery scope').disabled, true);
    assert(!ui.calls.some((call) => call.action === 'configureFaceScan'));
  } finally { await ui.close(); }
});

test('a restored library drops prior source drafts and confirmation context', async () => {
  const ui = harness();
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const name = ui.field('Source name'); name.value = 'Uncommitted old-library name'; await name.fire('input');
    ui.emit('photofindry:library-restored');
    ui.state.sources[0].label = 'Restored source name';
    await ui.window.PhotoFindryPanels.show('sources');
    assert.equal(ui.field('Source name').value, 'Restored source name');
    assert(!ui.calls.some((call) => call.action === 'renameSource'));
  } finally { await ui.close(); }
});

test('same-filename recovery candidates show distinct locations and confirm only the chosen native ID', async () => {
  const ui = harness({}, { sourceDetails: {
    canUndo: false, offset: 0, totalCount: 1,
    candidates: [{ filename: 'photo.jpg', photoId: 'saved-photo', candidates: [
      { id: 'candidate-a', pathLabel: 'photo.jpg', locationLabel: '/Disk A/Photos/photo.jpg', reason: 'Possible original' },
      { id: 'candidate-b', pathLabel: 'photo.jpg', locationLabel: '/Disk B/Photos/photo.jpg', reason: 'Possible original' },
    ] }],
  } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const labels = ui.all().map((node) => node.textContent).join('\n');
    assert.match(labels, /\/Disk A\/Photos\/photo.jpg/); assert.match(labels, /\/Disk B\/Photos\/photo.jpg/);
    const pending = ui.findButton('Confirm this original').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const dialog = ui.all().find((node) => node.tagName === 'DIALOG');
    assert.match(dialog.textContent, /\/Disk A\/Photos\/photo.jpg/);
    assert(!ui.calls.some((call) => call.action === 'confirmSourceMatch'));
    await ui.findButton('Confirm original').click(); await pending;
    assert.deepEqual(ui.calls.find((call) => call.action === 'confirmSourceMatch').params, {
      sourceId: ui.state.sources[0].id, photoId: 'saved-photo', candidateId: 'candidate-a',
    });
  } finally { await ui.close(); }
});

test('permanent deletion needs a frozen native preparation and a separate acknowledged confirmation', async () => {
  const ui = harness({}, {
    cleanupRemoved: { items: [{ photo: { id: 'removed-a', name: 'Saved photo' }, eligible: true }], totalCount: 1, offset: 0 },
    prepareRemovedDeletion: { id: 'frozen-deletion-1', count: 1, expiresAt: '2099-01-01T00:00:00Z' },
  });
  try {
    await ui.window.PhotoFindryPanels.show('removed');
    const entry = ui.all().find((node) => node.tagName === 'LABEL' && node.textContent === 'Select entry').children[0];
    entry.checked = true; await entry.click();
    const pending = ui.findButton('Delete selected permanently…').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert(ui.calls.some((call) => call.action === 'prepareRemovedDeletion' && JSON.stringify(call.params.photoIds) === '["removed-a"]'));
    assert(!ui.calls.some((call) => call.action === 'deleteRemovedEntries'));
    const approval = ui.findButton('Delete permanently'); assert.equal(approval.disabled, true);
    const acknowledgement = ui.all().find((node) => node.tagName === 'LABEL' && node.textContent.includes('I understand this permanently deletes')).children[0];
    acknowledgement.checked = true; await acknowledgement.fire('change');
    await approval.click(); await pending;
    const deletion = ui.calls.find((call) => call.action === 'deleteRemovedEntries');
    assert.deepEqual(deletion.params, { deletionId: 'frozen-deletion-1', confirmed: true });
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('photo-context People edits only the opened off-page face, not gallery selection', async () => {
  const face={id:'face-offpage',photoId:'offpage',personId:'person-a',assignmentSource:'manual',ageBand:'adult',fingerprint:'pixels',model:'face-v1',box:[0,0,1,1]};
  const ui=harness({people:{people:[{id:'person-a',name:'Alex'},{id:'person-b',name:'Sam'}],faces:[],canUndo:true}}, {'/api/photo':()=>({id:'offpage',name:'Outside current page.jpg',faces:[face],personNames:['Alex']}),selectedIDs:()=>['unrelated-gallery-photo']});
  try {
    await ui.window.PhotoFindryPanels.show('people',{photoId:'offpage'});
    const naming=ui.findButton('Change this face’s name or age…').click();
    ui.field('Existing person').value='person-b';ui.field('Your age label (optional)').value='child';
    await ui.findButton('Save confirmed name').click();await naming;
    assert.deepEqual(ui.calls.find(c=>c.action==='assignPerson').params,{faceIds:['face-offpage'],name:'',personId:'person-b',ageBand:'child'});
    assert(!('photoId' in ui.calls.find(c=>c.action==='assignPerson').params), 'Naming a face must not create an independent photograph-level label.');
    await ui.findButton('Scan faces in this photograph').click();assert.deepEqual(ui.calls.find(c=>c.action==='enqueueJob').params,{kind:'faces',params:{photoIds:['offpage']}});
    assert(!ui.calls.some(c=>c.action==='browsePeople'));assert.deepEqual(ui.errors,[]);
  } finally {await ui.close();}
});

test('a changed face assignment cannot be overwritten by an old confirmation', async () => {
  let owner='person-a';const face=()=>({id:'f',photoId:'p',personId:owner,fingerprint:'pixels',model:'face-v1',box:[0,0,1,1]});
  const ui=harness({people:{people:[{id:'person-a',name:'Alex'},{id:'person-b',name:'Sam'}]}},{'/api/photo':()=>({id:'p',faces:[face()]})});
  try {
    await ui.window.PhotoFindryPanels.show('people',{photoId:'p'});const naming=ui.findButton('Change this face’s name or age…').click();owner='person-b';await ui.findButton('Save confirmed name').click();await naming;
    assert(!ui.calls.some(c=>c.action==='assignPerson'));assert(ui.errors.some(e=>e.includes('face changed')));
  } finally {await ui.close();}
});

test('archived sources expose attachment instead of active mutations and metadata-only sources hide image actions',async()=>{
  const archived=harness({sources:[{id:'source-a',label:'Archived',mode:'archived',roots:[{id:'root-a',label:'Old folder'}]}]});
  try{await archived.window.PhotoFindryPanels.show('sources');assert(archived.findButton('Inspect & attach archive…'));assert(!archived.all().some(n=>n.tagName==='BUTTON'&&['Save name','Read camera & location metadata','Show folder in Finder','Confirm this original'].includes(n.textContent)));}finally{await archived.close();}
  const details={candidates:[{photoId:'p',filename:'one.jpg',candidates:[{id:'c',locationLabel:'Saved path'}]}]};
  const metadata=harness({sources:[{id:'source-a',label:'Metadata only',mode:'metadata',accessOriginals:false,roots:[]}]},{sourceDetails:details});
  try{await metadata.window.PhotoFindryPanels.show('sources');assert(!metadata.all().some(n=>n.tagName==='BUTTON'&&['Preview candidate','Show candidate in Finder','Read camera & location metadata'].includes(n.textContent)));}finally{await metadata.close();}
});

test('candidate viewing and source metadata actions use explicit native IDs, and discovery is not tagging',async()=>{
  const ui=harness({}, {sourceDetails:{candidates:[{photoId:'p',filename:'one.jpg',candidates:[{id:'candidate-a',locationLabel:'Chosen candidate'}]}]},'/api/source-candidate-preview':()=>new Blob(['safe prepared fixture'],{type:'image/jpeg'})});
  try{await ui.window.PhotoFindryPanels.show('sources');await ui.findButton('Preview candidate').click();await ui.findButton('Show candidate in Finder').click();
    const sourceId=ui.state.sources[0].id;assert.deepEqual(ui.calls.find(c=>c.action==='/api/source-candidate-preview').params,{sourceId,photoId:'p',candidateId:'candidate-a'});assert.deepEqual(ui.calls.find(c=>c.action==='revealSourceCandidate').params,{sourceId,photoId:'p',candidateId:'candidate-a'});
    await ui.findButton('Read camera & location metadata').click();assert.deepEqual(ui.calls.find(c=>c.action==='enqueueJob').params,{kind:'metadata',params:{sourceId,force:true}});
    await ui.findButton('Find new photographs').click();assert.deepEqual(ui.calls.find(c=>c.action==='discoverFolders').params,{recursive:true});assert(!ui.calls.some(c=>c.action==='enqueueJob'&&c.params.kind==='tagging'));
  }finally{await ui.close();}
});

test('source image permission changes refresh Files in place and withdraw stale candidate pixels',async()=>{
  const ui=harness({}, {sourceDetails:{candidates:[{photoId:'p',filename:'one.jpg',candidates:[{id:'c'}]}]},'/api/source-candidate-preview':()=>new Blob(['safe'],{type:'image/jpeg'})});
  try{ui.emit('photofindry:state',structuredClone(ui.state));await ui.window.PhotoFindryPanels.show('sources');await ui.findButton('Preview candidate').click();assert(ui.all().some(n=>n.className==='pfp-candidate-preview'));ui.state.sources[0].mode='metadata';ui.state.sources[0].accessOriginals=false;ui.emit('photofindry:state',structuredClone(ui.state));await new Promise(resolve=>setImmediate(resolve));assert.equal(ui.all().find(n=>n.id==='advanced-panel').hidden,false);assert(!ui.all().some(n=>n.className==='pfp-candidate-preview'));assert(!ui.notices.some(s=>s.includes('Reopen the panel')));assert(!visiblePanelButtons(ui).includes('Preview candidate'));}finally{await ui.close();}
});

test('source navigation clears the previous job recovery context',async()=>{
 const ui=harness({sources:[{id:'a',label:'A',roots:[]},{id:'b',label:'B',roots:[]}]});
 try{await ui.window.PhotoFindryPanels.show('sources',{sourceId:'a',jobId:'job-a'});assert(ui.findButton('Recheck originals for the selected job'));ui.field('Source').value='b';await ui.field('Source').fire('change');await new Promise(r=>setImmediate(r));assert(!ui.all().some(n=>n.tagName==='BUTTON'&&n.textContent==='Recheck originals for the selected job'));await ui.window.PhotoFindryPanels.show('sources',{sourceId:'a',jobId:'job-a'});await ui.window.PhotoFindryPanels.show('sources');assert(!ui.all().some(n=>n.tagName==='BUTTON'&&n.textContent==='Recheck originals for the selected job'));assert(!ui.calls.some(c=>c.action==='recoverJobOriginals'));}finally{await ui.close();}
});

test('photograph-level human names retain the optional human age label without inventing a face reference',async()=>{
 const ui=harness({}, {'/api/photo':()=>({id:'p',name:'Saved photo',faces:[]})});
 try{await ui.window.PhotoFindryPanels.show('people',{photoId:'p'});const naming=ui.findButton('Add person label to this photograph…').click();await new Promise(r=>setImmediate(r));ui.field('New name').value='Alex';ui.field('Your age label (optional)').value='adult';await ui.findButton('Save person label').click();await naming;assert.deepEqual(ui.calls.find(c=>c.action==='assignPerson').params,{photoId:'p',name:'Alex',personId:null,ageBand:'adult'});assert.deepEqual(ui.errors,[]);}finally{await ui.close();}
});

test('completed export Finder controls use only server receipt identities and never displayed paths', async () => {
  const ui=harness({transferResults:[{kind:'library',id:'a'.repeat(64),label:'Saved library.photofindrybackup'},{kind:'source',id:'b'.repeat(64),label:'Saved source.photofindrysource.zip'}]});
  try {
    await ui.window.PhotoFindryPanels.show('portability');
    await ui.findButton('Show backup in Finder').click();await ui.findButton('Show archive in Finder').click();
    assert.deepEqual(ui.calls.filter(call=>call.action==='revealTransferResult'),[
      {action:'revealTransferResult',params:{kind:'library',id:'a'.repeat(64)}},
      {action:'revealTransferResult',params:{kind:'source',id:'b'.repeat(64)}},
    ]);
    ui.state.transferResults=[];ui.emit('photofindry:state',ui.state);
    assert(!ui.all().some(node=>node.tagName==='BUTTON'&&/Show (backup|archive) in Finder/.test(node.textContent)));
  } finally {await ui.close();}
});

test('an accepted or failed export cannot invent a Finder receipt from its progress label', async () => {
  const ui=harness({storageJob:{kind:'export',running:true,phase:'preparing',pathLabel:'Not finished.photofindrybackup'},sourceTransfer:{kind:'export',running:false,phase:'error',pathLabel:'Failed.zip'}});
  try {await ui.window.PhotoFindryPanels.show('portability');assert(!ui.all().some(node=>node.tagName==='BUTTON'&&/Show (backup|archive) in Finder/.test(node.textContent)));}
  finally {await ui.close();}
});

test('Files and Backups both import whole-library folders through the backup grant and inspect before replacement', async () => {
  for (const panel of ['sources', 'portability']) {
    const ui=harness({}, { inspectBackup: { photos: 93806, people: 5, bytes: 6400000000, includePreviews: true } });
    const kinds=[];ui.window.PhotoFindry.pick=async kind=>{kinds.push(kind);return{grantId:'backup-grant',label:'PhotoFindry.photofindrybackup'};};
    try {
      await ui.window.PhotoFindryPanels.show(panel);
      const labels=ui.all().map(node=>node.textContent).join('\n');
      assert.match(labels,/whole \.photofindrybackup folder/);assert.match(labels,/\.photofindrysource\.zip/);
      if(panel==='sources')assert(ui.all().some(node=>node.tagName==='H2'&&node.textContent==='Files'));
      const importing=ui.findButton('Import library backup…').click();await new Promise(resolve=>setImmediate(resolve));
      assert.deepEqual(kinds,['backupOpen']);
      assert.deepEqual(ui.calls.filter(call=>call.action==='inspectBackup'),[{action:'inspectBackup',params:{sourceGrant:'backup-grant'}}]);
      assert(!ui.calls.some(call=>['restoreLibrary','attachSourceArchive','exportSourceArchive'].includes(call.action)));
      assert.equal(ui.findButton('Restore verified backup').disabled,true);
      assert.match(ui.all().find(node=>node.tagName==='DIALOG'&&node.open).textContent,/current library is backed up for recovery/);
      await ui.findButton('Cancel').click();await importing;
      assert(!ui.calls.some(call=>call.action==='restoreLibrary'));assert.deepEqual(ui.errors,[]);
    } finally {await ui.close();}
  }
});

test('easy source removal requires a frozen archive review, acknowledgement and native destination', async () => {
  const ui=harness({}, {prepareSourceArchive:{planId:'frozen-source-plan',photos:1,removedPhotos:0,people:1,faces:1,previews:1,bytes:100,includeImageCaches:true,missingImages:0}});
  const kinds=[];ui.window.PhotoFindry.pick=async kind=>{kinds.push(kind);return{grantId:'archive-destination-grant'};};
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const removing=ui.findButton('Remove source from gallery…').click();await new Promise(resolve=>setImmediate(resolve));
    assert.deepEqual(ui.calls.filter(call=>call.action==='prepareSourceArchive'),[{action:'prepareSourceArchive',params:{sourceId:ui.state.sources[0].id,mode:'detach',includeImageCaches:true}}]);
    const dialog=ui.all().find(node=>node.tagName==='DIALOG'&&node.open);
    assert.match(dialog.textContent,/Test source/);assert.match(dialog.textContent,/restorable source archive/);assert.match(dialog.textContent,/Other sources and original files are unchanged/);
    const approve=ui.findButton('Save archive & remove source');assert.equal(approve.disabled,true);
    assert.deepEqual(kinds,[]);assert(!ui.calls.some(call=>call.action==='exportSourceArchive'));
    const acknowledgement=dialog.children.find(node=>node.tagName==='LABEL'&&node.textContent.includes('I understand'));acknowledgement.children[0].checked=true;await acknowledgement.children[0].fire('change');
    await approve.click();await removing;
    assert.deepEqual(kinds,['archiveSave']);
    assert.deepEqual(ui.calls.filter(call=>call.action==='exportSourceArchive'),[{action:'exportSourceArchive',params:{planId:'frozen-source-plan',destinationGrant:'archive-destination-grant',disconnect:true,confirmClearImages:true}}]);
    assert(!ui.calls.some(call=>['removeFolder','purgeSource','restoreLibrary','enqueueJob','startModel'].includes(call.action)));
    assert.deepEqual(ui.errors,[]);
  } finally {await ui.close();}
});

test('cancelled source removal leaves the gallery intact and does not choose or write an archive', async () => {
  const ui=harness({}, {prepareSourceArchive:{planId:'review-only',photos:1}});
  ui.window.PhotoFindry.pick=async()=>assert.fail('Cancelling the review must not open a destination chooser.');
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const removing=ui.findButton('Remove source from gallery…').click();await new Promise(resolve=>setImmediate(resolve));
    await ui.findButton('Cancel').click();await removing;
    assert(!ui.calls.some(call=>call.action==='exportSourceArchive'));assert.equal(ui.state.sources.length,1);
    assert.deepEqual(ui.errors,[]);
  } finally {await ui.close();}
});

test('source removal chooses only active sources and never silently removes the other gallery sources', async () => {
  const ui=harness({sources:[{id:'a',label:'Archive already removed',mode:'archived'},{id:'b',label:'Current source',photoCount:2},{id:'c',label:'Other source',photoCount:3}]},{prepareSourceArchive:{planId:'chosen-c',photos:3}});
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const removing=ui.findButton('Remove source from gallery…').click();await new Promise(resolve=>setImmediate(resolve));
    const chooser=ui.field('Source to remove');assert.equal(chooser.value,'b');assert.equal(chooser.children.length,2);assert(!chooser.children.some(node=>node.value==='a'));
    assert(!ui.calls.some(call=>call.action==='prepareSourceArchive'));
    chooser.value='c';await ui.findButton('Review source removal').click();await new Promise(resolve=>setImmediate(resolve));
    assert.deepEqual(ui.calls.filter(call=>call.action==='prepareSourceArchive'),[{action:'prepareSourceArchive',params:{sourceId:'c',mode:'detach',includeImageCaches:true}}]);
    await ui.findButton('Cancel').click();await removing;
    assert(!ui.calls.some(call=>call.action==='exportSourceArchive'));assert.deepEqual(ui.errors,[]);
  } finally {await ui.close();}
});

test('a library replacement discards an old source removal preparation and destination grant', async () => {
  for(const phase of ['preparation','destination']) {
    let release;const waiting=new Promise(resolve=>{release=resolve;});
    const plan={planId:'old-plan',photos:1};
    const ui=harness({}, {prepareSourceArchive:phase==='preparation'?()=>waiting:plan});
    ui.window.PhotoFindry.pick=async()=>waiting;
    try {
      await ui.window.PhotoFindryPanels.show('sources');
      const removing=ui.findButton('Remove source from gallery…').click();await new Promise(resolve=>setImmediate(resolve));
      if(phase==='destination') {
        const dialog=ui.all().find(node=>node.tagName==='DIALOG'&&node.open);
        const acknowledgement=dialog.children.find(node=>node.tagName==='LABEL'&&node.textContent.includes('I understand'));acknowledgement.children[0].checked=true;await acknowledgement.children[0].fire('change');
        void ui.findButton('Save archive & remove source').click();await new Promise(resolve=>setImmediate(resolve));
      }
      ui.emit('photofindry:library-restored');release(phase==='preparation'?plan:{grantId:'old-destination'});await removing;
      assert(!ui.all().some(node=>node.tagName==='DIALOG'&&node.open));assert(!ui.calls.some(call=>call.action==='exportSourceArchive'));
      assert.deepEqual(ui.errors,[]);
    } finally {await ui.close();}
  }
});

test('archived source export reports the native attach-first requirement instead of silently doing nothing', async () => {
  const ui=harness({sources:[{id:'old-source',label:'Already archived',mode:'archived'}]});
  try {
    await ui.window.PhotoFindryPanels.show('portability');
    ui.field('Source to export').value='old-source';await ui.findButton('Export source…').click();
    assert.match(ui.errors.at(-1),/Attach this source archive before exporting it again/);
    assert(!ui.calls.some(call=>['prepareSourceArchive','exportSourceArchive'].includes(call.action)));
  } finally {await ui.close();}
});

test('a failed replacement folder check restores the primary chooser instead of leaving a dead review', async () => {
  const source={id:'source-a',label:'Imported folder',mode:'metadata',accessOriginals:false,roots:[{id:'root-a',locationLabel:'/Users/old/pCloud Drive/Photos'}]};
  const ui=harness({sources:[source]},{prepareSourceConnection:{planId:'old-plan',sourceId:source.id,rootId:'root-a',count:2},locateSource:()=>{throw Error('The selected folder is unavailable.');}});
  ui.window.PhotoFindry.pick=async()=>({grantId:'new-folder',locationLabel:'/Users/current/pCloud Drive/Photos'});
  try {
    await ui.window.PhotoFindryPanels.show('sources');await ui.findButton('Review originals').click();
    await ui.findButton('Choose another folder…').click();
    assert.match(ui.errors.at(-1),/selected folder is unavailable/);
    assert(visiblePanelButtons(ui).includes('Choose this Mac’s originals folder…'));
    assert(!ui.calls.some(call=>call.action==='confirmSourceConnection'));
  } finally {await ui.close();}
});

test('a delayed This library removal entry cannot open a review after navigation supersedes it', async () => {
  let release;const waiting=new Promise(resolve=>{release=resolve;});
  const ui=harness({}, {sourceDetails:()=>waiting});
  try {
    const opening=ui.window.PhotoFindryPanels.removeSource();await new Promise(resolve=>setImmediate(resolve));
    await ui.window.PhotoFindryPanels.show('people');release({candidates:[],totalCount:0});await opening;
    assert(!ui.calls.some(call=>call.action==='prepareSourceArchive'));assert(!ui.all().some(node=>node.tagName==='DIALOG'&&node.open));assert.deepEqual(ui.errors,[]);
  } finally {await ui.close();}
});

test('backup and source import discard native chooser or inspection results after navigation and library replacement', async () => {
  const flows=[{button:'Import library backup…',kind:'backupOpen',inspection:'inspectBackup',command:'restoreLibrary'},
    {button:'Attach source archive…',kind:'archiveOpen',inspection:'inspectSourceArchive',command:'attachSourceArchive'}];
  for(const flow of flows)for(const phase of ['chooser','inspection'])for(const interruption of ['navigation','replacement']) {
    let release;const waiting=new Promise(resolve=>{release=resolve;});
    const details={photos:2,people:1,faces:2,bytes:100};
    const ui=harness({}, {[flow.inspection]:phase==='inspection'?()=>waiting:details});
    const kinds=[];ui.window.PhotoFindry.pick=async kind=>{kinds.push(kind);return phase==='chooser'?waiting:{grantId:'old-library-grant'};};
    try {
      await ui.window.PhotoFindryPanels.show('sources');
      const importing=ui.findButton(flow.button).click();await new Promise(resolve=>setImmediate(resolve));
      if(interruption==='navigation')await ui.window.PhotoFindryPanels.show('people');
      else ui.emit('photofindry:library-restored');
      release(phase==='chooser'?{grantId:'old-library-grant'}:details);await importing;
      assert.deepEqual(kinds,[flow.kind]);
      assert.equal(ui.calls.filter(call=>call.action===flow.inspection).length,phase==='chooser'?0:1);
      assert(!ui.calls.some(call=>call.action===flow.command));assert(!ui.all().some(node=>node.tagName==='DIALOG'&&node.open));
      assert.deepEqual(ui.errors,[]);
    } finally {await ui.close();}
  }
});

test('an old accepted import confirmation cannot restore or attach after another panel supersedes it', async () => {
  for(const flow of [{button:'Import library backup…',inspection:'inspectBackup',approval:'Restore verified backup',command:'restoreLibrary'},
    {button:'Attach source archive…',inspection:'inspectSourceArchive',approval:'Attach verified source',command:'attachSourceArchive'}]) {
    const ui=harness({}, {[flow.inspection]:{photos:2,people:1,faces:2,bytes:100}});
    ui.window.PhotoFindry.pick=async()=>({grantId:'old-library-grant'});
    try {
      await ui.window.PhotoFindryPanels.show('sources');
      const importing=ui.findButton(flow.button).click();await new Promise(resolve=>setImmediate(resolve));
      const dialog=ui.all().find(node=>node.tagName==='DIALOG'&&node.open);
      const acknowledgement=dialog.children.find(node=>node.tagName==='LABEL'&&node.textContent.includes('I want'));
      if(acknowledgement){acknowledgement.children[0].checked=true;await acknowledgement.children[0].fire('change');}
      await ui.window.PhotoFindryPanels.show('people');await ui.findButton(flow.approval).click();await importing;
      assert(!ui.calls.some(call=>call.action===flow.command));assert.deepEqual(ui.errors,[]);
    } finally {await ui.close();}
  }
});

test('current inspected imports still commit only the approved native grant', async () => {
  for(const flow of [{button:'Import library backup…',inspection:'inspectBackup',approval:'Restore verified backup',command:'restoreLibrary',params:{sourceGrant:'chosen-native-grant',confirmReplace:true}},
    {button:'Attach source archive…',inspection:'inspectSourceArchive',approval:'Attach verified source',command:'attachSourceArchive',params:{sourceGrant:'chosen-native-grant',confirm:true}}]) {
    const ui=harness({}, {[flow.inspection]:{photos:2,people:1,faces:2,bytes:100}});
    ui.window.PhotoFindry.pick=async()=>({grantId:'chosen-native-grant'});
    try {
      await ui.window.PhotoFindryPanels.show('sources');
      const importing=ui.findButton(flow.button).click();await new Promise(resolve=>setImmediate(resolve));
      const dialog=ui.all().find(node=>node.tagName==='DIALOG'&&node.open);
      const acknowledgement=dialog.children.find(node=>node.tagName==='LABEL'&&node.textContent.includes('I want'));
      if(acknowledgement){acknowledgement.children[0].checked=true;await acknowledgement.children[0].fire('change');}
      await ui.findButton(flow.approval).click();await importing;
      assert.deepEqual(ui.calls.filter(call=>call.action===flow.command),[{action:flow.command,params:flow.params}]);assert.deepEqual(ui.errors,[]);
    } finally {await ui.close();}
  }
});

function visiblePanelButtons(ui) {
  return ui.all().filter(node => node.tagName === 'BUTTON' && !node.hidden && (() => {
    for (let parent = node.parentNode; parent; parent = parent.parentNode) {
      if (parent.hidden || parent.tagName === 'DETAILS' && !parent.open || parent.tagName === 'DIALOG' && !parent.open) return false;
    }
    return true;
  })()).map(node => node.textContent);
}

function importedSource(roots = [{ id: 'root-a', label: 'Automatic Upload', locationLabel: '/Users/other-mac/pCloud Drive/Automatic Upload' }]) {
  return { id: 'imported-source', label: 'Automatic Upload', mode: 'metadata', accessOriginals: false, status: 'offline',
    photoCount: 32827, available: 0, needsReview: 26959, roots };
}

test('imported Files prioritizes choosing this Mac’s original folder, shows the old path, and collapses secondary work', async () => {
  const ui = harness({ sources: [importedSource()] });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /\/Users\/other-mac\/pCloud Drive\/Automatic Upload/);
    const visible = visiblePanelButtons(ui);
    assert(visible.includes('Choose this Mac’s originals folder…'));
    for (const label of ['Save name', 'Review originals', 'Import library backup…', 'Attach source archive…', 'Remove source from gallery…', 'Read QR image…', 'Scan with camera…']) assert(!visible.includes(label), label);
    assert(ui.all().filter(node => node.tagName === 'DETAILS').every(node => !node.open));
    assert(!ui.calls.some(call => ['prepareSourceConnection', 'locateSource', 'checkSources', 'enqueueJob', 'ensurePreview'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('reconnecting multiple saved roots requires explicit root selection and uses only the matching source and native grant', async () => {
  const ui = harness({ sources: [importedSource([
    { id: 'root-a', locationLabel: '/Users/other/Photos/A' }, { id: 'root-b', locationLabel: '/Users/other/Photos/B' },
  ])] }, { locateSource: { submittedJob: { id: 'check-b', sourceId: 'imported-source', sourceRootId: 'root-b', kind: 'sourceRecovery', state: 'queued' } } });
  const kinds = []; ui.window.PhotoFindry.pick = async kind => { kinds.push(kind); return { grantId: 'chosen-folder', locationLabel: '/Users/this/Photos/B' }; };
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    assert.equal(ui.field('Saved originals folder').value, '');
    assert.equal(ui.findButton('Choose this Mac’s originals folder…').disabled, true);
    await ui.findButton('Choose this Mac’s originals folder…').click(); assert.deepEqual(kinds, []);
    ui.field('Saved originals folder').value = 'root-b'; await ui.field('Saved originals folder').fire('change'); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Choose this Mac’s originals folder…').click();
    assert.deepEqual(kinds, ['sourceFolder']);
    assert.deepEqual(ui.calls.filter(call => call.action === 'locateSource'), [{ action: 'locateSource', params: { sourceId: 'imported-source', rootId: 'root-b', folderGrant: 'chosen-folder' } }]);
    assert(!ui.calls.some(call => ['addFolders', 'confirmSourceConnection', 'startModel', 'enqueueJob'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a chosen folder waits for its own completed check, then reviews old and new paths before acknowledged confirmation', async () => {
  const source = importedSource();
  const check = { id: 'my-check', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', state: 'queued' };
  const plan = { planId: 'review-plan', sourceId: source.id, rootId: 'root-a', previousRootLocationLabel: source.roots[0].locationLabel,
    rootLocationLabel: '/Users/this-mac/pCloud Drive/Automatic Upload', count: 12, unresolved: 26947, examples: [{ relativePath: '2020/photo.jpg' }] };
  const ui = harness({ sources: [source], jobs: { jobs: [] } }, {
    locateSource: () => { ui.state.jobs.jobs.push(check); return { submittedJob: check }; }, prepareSourceConnection: plan,
    confirmSourceConnection: { submittedJob: { id: 'confirm-exact', sourceId: source.id, kind: 'sourceRecovery', sourceAction: 'confirmLocations', state: 'queued' } },
  });
  ui.window.PhotoFindry.pick = async kind => ({ grantId: 'originals-grant', locationLabel: plan.rootLocationLabel });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); ui.window.PhotoFindryPanels.onState(structuredClone(ui.state));
    await ui.findButton('Choose this Mac’s originals folder…').click();
    assert(!ui.calls.some(call => call.action === 'prepareSourceConnection'));
    assert.equal(ui.findButton('Choose this Mac’s originals folder…').disabled, true);
    ui.state.jobs.jobs.push({ id: 'other-check', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', state: 'running' });
    ui.window.PhotoFindryPanels.onState(structuredClone(ui.state)); ui.state.jobs.jobs[1].state = 'completed'; ui.window.PhotoFindryPanels.onState(structuredClone(ui.state));
    await new Promise(resolve => setTimeout(resolve, 240)); assert(!ui.calls.some(call => call.action === 'prepareSourceConnection'));
    check.state = 'completed'; ui.window.PhotoFindryPanels.onState(structuredClone(ui.state));
    await new Promise(resolve => setTimeout(resolve, 240));
    assert.deepEqual(ui.calls.filter(call => call.action === 'prepareSourceConnection'), [{ action: 'prepareSourceConnection', params: { sourceId: source.id, rootId: 'root-a' } }]);
    const review = ui.all().find(node => node.className.split(' ').includes('pfp-originals-review'));
    assert.match(review.textContent, /\/Users\/other-mac\/pCloud Drive\/Automatic Upload/); assert.match(review.textContent, /\/Users\/this-mac\/pCloud Drive\/Automatic Upload/);
    assert.match(review.textContent, /Other locations in this source still unresolved/);
    assert(!ui.calls.some(call => call.action === 'confirmSourceConnection'));
    const confirming = ui.findButton('Confirm matching locations').click(); await new Promise(resolve => setImmediate(resolve));
    const approval = ui.findButton('Confirm 12 locations'); assert.equal(approval.disabled, true);
    const acknowledgement = ui.all().find(node => node.tagName === 'LABEL' && node.textContent === 'I confirm this is the same originals folder.').children[0];
    acknowledgement.checked = true; await acknowledgement.fire('change'); await approval.click(); await confirming;
    assert.deepEqual(ui.calls.filter(call => call.action === 'confirmSourceConnection'), [{ action: 'confirmSourceConnection', params: { sourceId: source.id, planId: 'review-plan', confirmed: true } }]);
    assert(!ui.calls.some(call => ['addFolders', 'startModel', 'enqueueJob'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a zero-match folder offers choose folder and a disabled confirmation without a misleading failure', async () => {
  const ui = harness({ sources: [importedSource()] }, { prepareSourceConnection: { planId: 'no-matches', sourceId: 'imported-source', rootId: 'root-a', count: 0, unresolved: 26959,
    previousRootLocationLabel: '/Users/old/pCloud Drive/Automatic Upload', rootLocationLabel: '/Users/current/Wrong folder' } });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    assert.equal(ui.findButton('Confirm matching locations').disabled, true);
    const buttons = visiblePanelButtons(ui); assert.equal(buttons.filter(label => label === 'Choose this Mac’s originals folder…').length, 1);
    assert.match(ui.all().find(node => node.className.split(' ').includes('pfp-originals-review')).textContent, /No matching originals in this folder/);
    await ui.findButton('Confirm matching locations').click(); assert(!ui.calls.some(call => call.action === 'confirmSourceConnection'));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('folder chooser and match review results cannot mutate a superseded source or library', async () => {
  for (const interruption of ['source', 'library', 'navigation']) {
    let release; const chosen = new Promise(resolve => { release = resolve; });
    const ui = harness({ sources: [importedSource(), { id: 'other-source', label: 'Other', roots: [] }] });
    ui.window.PhotoFindry.pick = async () => chosen;
    try {
      await ui.window.PhotoFindryPanels.show('sources'); const locating = ui.findButton('Choose this Mac’s originals folder…').click(); await new Promise(resolve => setImmediate(resolve));
      if (interruption === 'source') { ui.field('Source').value = 'other-source'; await ui.field('Source').fire('change'); }
      else if (interruption === 'library') ui.emit('photofindry:library-restored');
      else await ui.window.PhotoFindryPanels.show('people');
      release({ grantId: 'stale-folder-grant' }); await locating;
      assert(!ui.calls.some(call => call.action === 'locateSource')); assert.deepEqual(ui.errors, []);
    } finally { await ui.close(); }
  }
});

test('reopening Files keeps a completed chosen-folder review visible without rescanning the old import path', async () => {
  const source = importedSource();
  const ui = harness({ sources: [source], jobs: { jobs: [
    { id: 'completed-locate', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', state: 'completed', updatedAt: '2026-10-16T20:00:00Z' },
  ] } }, { prepareSourceConnection: { planId: 'retained-review', sourceId: source.id, rootId: 'root-a', count: 7 } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    assert(visiblePanelButtons(ui).includes('Review checked folder…'));
    assert(!ui.calls.some(call => call.action === 'prepareSourceConnection'));
    await ui.findButton('Back to photos').click(); await ui.window.PhotoFindryPanels.show('sources');
    assert(visiblePanelButtons(ui).includes('Review checked folder…'));
    await ui.findButton('Review checked folder…').click();
    assert.deepEqual(ui.calls.filter(call => call.action === 'prepareSourceConnection'), [{ action: 'prepareSourceConnection', params: { sourceId: source.id, rootId: 'root-a' } }]);
    assert(!ui.calls.some(call => ['locateSource', 'confirmSourceConnection', 'checkSources'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a newer confirmed root or another root’s completed check cannot offer an unrelated reconnect review', async () => {
  const source = importedSource();
  for (const job of [
    { id: 'other-root', sourceRootId: 'root-b', sourceAction: 'locate', state: 'completed', updatedAt: '2026-10-16T21:00:00Z' },
    { id: 'confirmed', sourceRootId: 'root-a', sourceAction: 'confirmLocations', state: 'completed', updatedAt: '2026-10-16T21:00:00Z' },
    { id: 'stopped', sourceRootId: 'root-a', sourceAction: 'locate', state: 'cancelled', updatedAt: '2026-10-16T21:00:00Z' },
  ]) {
    const prior = job.sourceRootId === 'root-a' ? [{ id: 'old-check', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', state: 'completed', updatedAt: '2026-10-16T20:00:00Z' }] : [];
    const ui = harness({ sources: [source], jobs: { jobs: [...prior, { ...job, sourceId: source.id, kind: 'sourceRecovery' }] } });
    try { await ui.window.PhotoFindryPanels.show('sources'); assert(!visiblePanelButtons(ui).includes('Review checked folder…')); assert(!ui.calls.some(call => call.action === 'prepareSourceConnection')); }
    finally { await ui.close(); }
  }
});

test('a delayed folder-match plan cannot open a review after changing source, library or panel', async () => {
  for (const interruption of ['source', 'library', 'navigation']) {
    let release; const waiting = new Promise(resolve => { release = resolve; });
    const ui = harness({ sources: [importedSource(), { id: 'other-source', label: 'Other', roots: [] }] }, { prepareSourceConnection: () => waiting });
    try {
      await ui.window.PhotoFindryPanels.show('sources'); const reviewing = ui.findButton('Review originals').click(); await new Promise(resolve => setImmediate(resolve));
      if (interruption === 'source') { ui.field('Source').value = 'other-source'; await ui.field('Source').fire('change'); }
      else if (interruption === 'library') ui.emit('photofindry:library-restored');
      else await ui.window.PhotoFindryPanels.show('people');
      release({ planId: 'stale-plan', sourceId: 'imported-source', count: 4 }); await reviewing;
      assert(!ui.all().some(node => node.className.split(' ').includes('pfp-originals-review')));
      assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
    } finally { await ui.close(); }
  }
});

test('accepting an old folder confirmation cannot reconnect after navigation supersedes it', async () => {
  const ui = harness({ sources: [importedSource()] }, { prepareSourceConnection: { planId: 'old-plan', sourceId: 'imported-source', count: 4 } });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    const confirming = ui.findButton('Confirm matching locations').click(); await new Promise(resolve => setImmediate(resolve));
    await ui.window.PhotoFindryPanels.show('people');
    const acknowledgement = ui.all().find(node => node.tagName === 'LABEL' && node.textContent === 'I confirm this is the same originals folder.').children[0];
    acknowledgement.checked = true; await acknowledgement.fire('change'); await ui.findButton('Confirm 4 locations').click(); await confirming;
    assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a newer native confirmLocations job without a root ID suppresses an older root-specific locate review', async () => {
  const source = importedSource();
  const ui = harness({ sources: [source], jobs: { jobs: [
    { id: 'old-locate', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', state: 'completed', updatedAt: '2026-10-16T20:00:00Z' },
    // Actual worker shape: confirm_source_connection stores sourceId + planId,
    // so its public job has no sourceRootId even after a partial confirmation.
    { id: 'native-confirm', sourceId: source.id, kind: 'sourceRecovery', sourceAction: 'confirmLocations', state: 'completed', updatedAt: '2026-10-16T21:00:00Z' },
  ] } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    assert(source.needsReview > 0); assert(!visiblePanelButtons(ui).includes('Review checked folder…'));
    assert(visiblePanelButtons(ui).includes('Choose this Mac’s originals folder…'));
    assert(!ui.calls.some(call => call.action === 'prepareSourceConnection'));
  } finally { await ui.close(); }
});

test('optional full-file comparison progress clearly states that it reads complete original contents', async () => {
  const source = importedSource(); const check = { id: 'compare-files', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', sourceVerifyContents: true, state: 'queued' };
  const ui = harness({ sources: [source], jobs: { jobs: [] } }, {
    prepareSourceConnection: { planId: 'comparison-plan', sourceId: source.id, rootId: 'root-a', count: 7, verificationCount: 7 },
    locateSource: () => { ui.state.jobs.jobs = [check]; return { submittedJob: check }; },
  });
  ui.window.PhotoFindry.pick = async () => ({ grantId: 'comparison-folder', locationLabel: '/Users/current/Photos' });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    const comparing = ui.findButton('Optional full-file comparison…').click(); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Choose folder & compare').click(); await comparing;
    assert.deepEqual(ui.calls.find(call => call.action === 'locateSource').params, { sourceId: source.id, rootId: 'root-a', folderGrant: 'comparison-folder', verifyContents: true });
    const copy = ui.all().find(node => node.className === 'pfp-body').textContent;
    assert.match(copy, /Comparing complete original file contents/); assert.match(copy, /reads the photographs/);
    assert(!copy.includes('Photo contents are not read.')); assert.deepEqual(ui.errors, []);
    await ui.findButton('Back to photos').click(); await ui.window.PhotoFindryPanels.show('sources');
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /reads the photographs/);
  } finally { await ui.close(); }
});

test('starting another folder choice invalidates an old acknowledgement while the new picker or queue request waits', async () => {
  for (const phase of ['picker', 'queue']) {
    let release; const waiting = new Promise(resolve => { release = resolve; });
    const source = importedSource();
    const ui = harness({ sources: [source] }, { prepareSourceConnection: { planId: 'old-review', sourceId: source.id, rootId: 'root-a', count: 7 },
      locateSource: phase === 'queue' ? () => waiting : { submittedJob: { id: 'new-check' } } });
    ui.window.PhotoFindry.pick = async () => phase === 'picker' ? waiting : { grantId: 'new-folder' };
    try {
      await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
      const oldApproval = ui.findButton('Confirm matching locations').click(); await new Promise(resolve => setImmediate(resolve));
      const accept = ui.findButton('Confirm 7 locations');
      const acknowledgement = ui.all().find(node => node.tagName === 'LABEL' && node.textContent === 'I confirm this is the same originals folder.').children[0];
      acknowledgement.checked = true; await acknowledgement.fire('change');
      const relocating = ui.findButton('Choose another folder…').click();
      // The detached old acknowledgement is deliberately delivered before the
      // newly chosen folder has even reached the worker's completed check.
      await accept.click(); await oldApproval; await new Promise(resolve => setImmediate(resolve));
      assert(!ui.calls.some(call => call.action === 'confirmSourceConnection'));
      release(phase === 'picker' ? { grantId: 'new-folder' } : { submittedJob: { id: 'new-check' } }); await relocating;
      assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
    } finally { await ui.close(); }
  }
});

test('a busy previous folder review recovers automatically without confirming or starting another check', async () => {
  let attempts = 0;
  const source = importedSource();
  const ui = harness({ sources: [source] }, { prepareSourceConnection: () => {
    if (++attempts === 1) throw Error('A folder review is still finishing. Cancel that review or wait a moment, then try again.');
    return { planId: 'recovered-review', sourceId: source.id, rootId: 'root-a', count: 8 };
  } });
  try {
    await ui.window.PhotoFindryPanels.show('sources');
    const reviewing = ui.findButton('Review originals').click(); await new Promise(resolve => setImmediate(resolve));
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /continue automatically/);
    assert(visiblePanelButtons(ui).includes('Cancel review'));
    await reviewing;
    assert.equal(attempts, 2); assert(visiblePanelButtons(ui).includes('Confirm matching locations'));
    assert(!ui.calls.some(call => ['confirmSourceConnection', 'locateSource', 'checkSources'].includes(call.action)));
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a folder preparation failure offers a working retry that reaches its confirmation review', async () => {
  let attempts = 0; const source = importedSource();
  const ui = harness({ sources: [source] }, { prepareSourceConnection: () => {
    if (++attempts === 1) throw Error('The original folder is temporarily unavailable.');
    return { planId: 'available-again', sourceId: source.id, rootId: 'root-a', count: 3 };
  } });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    assert(visiblePanelButtons(ui).includes('Retry folder review')); assert(visiblePanelButtons(ui).includes('Cancel review'));
    await ui.findButton('Retry folder review').click();
    assert.equal(attempts, 2); assert.equal(ui.findButton('Confirm matching locations').disabled, false);
    assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('Cancel review stops an in-flight native review and discards its late result', async () => {
  let finish; const waiting = new Promise(resolve => { finish = resolve; }); const source = importedSource();
  const ui = harness({ sources: [source] }, { prepareSourceConnection: () => waiting });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); const reviewing = ui.findButton('Review originals').click(); await new Promise(resolve => setImmediate(resolve));
    await ui.findButton('Cancel review').click();
    assert.deepEqual(ui.calls.filter(call => call.action === 'cancelSourceConnectionReview'), [{ action: 'cancelSourceConnectionReview', params: {} }]);
    finish({ planId: 'cancelled-plan', sourceId: source.id, rootId: 'root-a', count: 8 }); await reviewing;
    assert(!visiblePanelButtons(ui).includes('Confirm matching locations'));
    assert(visiblePanelButtons(ui).includes('Choose this Mac’s originals folder…'));
    assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('an inherited source review exposes cancellation even before a new folder action', async () => {
  const ui = harness({ sources: [importedSource()], storageJob: { kind: 'sourceReview', phase: 'preparing', running: true, message: 'Checking possible original locations…' } });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); assert(visiblePanelButtons(ui).includes('Cancel folder review'));
    await ui.findButton('Cancel folder review').click();
    assert.deepEqual(ui.calls.filter(call => call.action === 'cancelSourceConnectionReview'), [{ action: 'cancelSourceConnectionReview', params: {} }]);
    assert(!ui.calls.some(call => ['cancelTransfer', 'cancelSourceTransfer', 'confirmSourceConnection'].includes(call.action)));
  } finally { await ui.close(); }
});

test('confirmation follows its exact rootless job through policy changes and refreshes Files to the new path', async () => {
  const source = importedSource(); const confirmation = { id: 'own-confirm', sourceId: source.id, kind: 'sourceRecovery', sourceAction: 'confirmLocations', state: 'running', message: 'Connecting originals…' };
  const ui = harness({ sources: [source], jobs: { jobs: [{ id: 'own-locate', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', state: 'completed' }] } }, {
    prepareSourceConnection: { planId: 'review-own', sourceId: source.id, rootId: 'root-a', count: 13, rootLocationLabel: '/Users/current/Photos' },
    confirmSourceConnection: () => {
      ui.state.jobs.jobs.push(confirmation); source.mode = 'online'; source.accessOriginals = true;
      ui.emit('photofindry:state', structuredClone(ui.state)); return { submittedJob: confirmation };
    },
  });
  try {
    ui.emit('photofindry:state', structuredClone(ui.state)); await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review checked folder…').click();
    const confirming = ui.findButton('Confirm matching locations').click(); await new Promise(resolve => setImmediate(resolve));
    const acknowledgement = ui.all().find(node => node.tagName === 'LABEL' && node.textContent === 'I confirm this is the same originals folder.').children[0];
    acknowledgement.checked = true; await acknowledgement.fire('change'); await ui.findButton('Confirm 13 locations').click(); await confirming;
    assert.equal(ui.all().find(node => node.id === 'advanced-panel').hidden, false);
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /Connecting originals folder/);
    assert(!visiblePanelButtons(ui).includes('Review checked folder…')); assert(!visiblePanelButtons(ui).includes('Confirm matching locations'));
    ui.state.jobs.jobs.push({ id: 'other-confirm', sourceId: source.id, kind: 'sourceRecovery', sourceAction: 'confirmLocations', state: 'completed' });
    ui.emit('photofindry:state', structuredClone(ui.state)); await new Promise(resolve => setTimeout(resolve, 240));
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /Connecting originals folder/);
    confirmation.state = 'completed'; source.available = 13; source.status = 'available';
    ui.emit('photofindry:state', structuredClone(ui.state)); await new Promise(resolve => setTimeout(resolve, 240));
    // The final sources snapshot may follow the terminal Jobs event. Its later
    // path update must still reach the open Files panel without another click.
    source.roots[0].locationLabel = '/Users/current/Photos';
    ui.emit('photofindry:state', structuredClone(ui.state)); await new Promise(resolve => setTimeout(resolve, 240));
    const body = ui.all().find(node => node.className === 'pfp-body').textContent;
    assert.match(body, /Folder confirmation finished/); assert.match(body, /\/Users\/current\/Photos/);
    assert(!body.includes('/Users/other-mac/')); assert(!visiblePanelButtons(ui).includes('Review checked folder…'));
    assert.equal(ui.calls.filter(call => call.action === 'prepareSourceConnection').length, 1);
    assert.equal(ui.calls.filter(call => call.action === 'confirmSourceConnection').length, 1);
    assert.deepEqual(ui.errors, []); assert(!ui.notices.some(message => message.includes('Reopen the panel')));
  } finally { await ui.close(); }
});

test('source job progress updates controls without rebuilding folder choices', async () => {
  const source = importedSource(), job = { id: 'progress-check', sourceId: source.id, sourceRootId: 'root-a', kind: 'sourceRecovery', sourceAction: 'locate', state: 'running', message: 'Checking 1 of 100', completed: 1, total: 100 };
  const ui = harness({ sources: [source], jobs: { jobs: [job] } });
  try {
    ui.emit('photofindry:state', structuredClone(ui.state)); await ui.window.PhotoFindryPanels.show('sources'); const chooser = ui.field('Source');
    job.message = 'Checking 50 of 100'; job.completed = 50; ui.emit('photofindry:state', structuredClone(ui.state));
    assert.equal(ui.field('Source'), chooser); assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /Checking 50 of 100/);
    assert(visiblePanelButtons(ui).includes('Stop')); assert(!ui.calls.some(call => call.action === 'prepareSourceConnection'));
  } finally { await ui.close(); }
});

test('busy folder review retries are bounded and its visible retry can recover afterwards', async () => {
  let available = false, attempts = 0; const source = importedSource();
  const ui = harness({ sources: [source] }, { prepareSourceConnection: () => {
    attempts += 1; if (!available) throw Error('A folder review is still finishing. Cancel that review or wait a moment, then try again.');
    return { planId: 'after-drain', sourceId: source.id, rootId: 'root-a', count: 2 };
  } });
  try {
    await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    assert.equal(attempts, 6); assert(visiblePanelButtons(ui).includes('Retry folder review')); assert(visiblePanelButtons(ui).includes('Cancel review'));
    available = true; await ui.findButton('Retry folder review').click();
    assert.equal(attempts, 7); assert(visiblePanelButtons(ui).includes('Confirm matching locations'));
    assert(!ui.calls.some(call => call.action === 'confirmSourceConnection')); assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});

test('a failed connection stays recoverable through its exact Retry job and completes without reconfirming', async () => {
  const source = importedSource(), failed = { id: 'failed-confirm', sourceId: source.id, kind: 'sourceRecovery', sourceAction: 'confirmLocations', state: 'failed', message: 'The drive became unavailable.' };
  const ui = harness({ sources: [source], jobs: { jobs: [] } }, {
    prepareSourceConnection: { planId: 'retry-proof', sourceId: source.id, rootId: 'root-a', count: 6 },
    confirmSourceConnection: () => { ui.state.jobs.jobs.push(failed); return { submittedJob: failed }; },
    jobAction: params => { assert.equal(params.id, failed.id); assert.equal(params.action, 'retry'); failed.state = 'running'; ui.emit('photofindry:state', structuredClone(ui.state)); return {}; },
  });
  try {
    ui.emit('photofindry:state', structuredClone(ui.state)); await ui.window.PhotoFindryPanels.show('sources'); await ui.findButton('Review originals').click();
    const confirming = ui.findButton('Confirm matching locations').click(); await new Promise(resolve => setImmediate(resolve));
    const acknowledgement = ui.all().find(node => node.tagName === 'LABEL' && node.textContent === 'I confirm this is the same originals folder.').children[0];
    acknowledgement.checked = true; await acknowledgement.fire('change'); await ui.findButton('Confirm 6 locations').click(); await confirming;
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /Use Retry or Resume below/);
    await ui.findButton('Retry').click(); await new Promise(resolve => setTimeout(resolve, 240));
    failed.state = 'completed'; source.mode = 'online'; source.accessOriginals = true; source.available = 6;
    ui.emit('photofindry:state', structuredClone(ui.state)); await new Promise(resolve => setImmediate(resolve));
    assert.match(ui.all().find(node => node.className === 'pfp-body').textContent, /Folder confirmation finished/);
    assert.equal(ui.calls.filter(call => call.action === 'confirmSourceConnection').length, 1);
    assert.deepEqual(ui.errors, []);
  } finally { await ui.close(); }
});
