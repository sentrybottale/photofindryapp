'use strict';
(() => {
  const fixture = window.layoutFixture;
  window.dispatchEvent(new CustomEvent('photofindry:state', { detail: fixture.state }));
  window.dispatchEvent(new CustomEvent('photofindry:session', { detail: { connected: true } }));
  const gallery = document.getElementById('gallery');
  for (let index = 0; index < 60; index += 1) {
    const tile = document.createElement('div'); tile.className = 'photo-tile'; tile.setAttribute('role', 'option'); tile.setAttribute('aria-selected', 'false'); tile.dataset.photoId = `synthetic-${index}`;
    const photo = document.createElement('div'); photo.className = 'photo-image'; const img = document.createElement('img'); img.src = fixture.image; img.alt = 'Synthetic geometric landscape'; photo.append(img);
    const metadata = document.createElement('div'); metadata.className = 'photo-metadata photo-summary-slot';
    tile.append(photo, metadata); gallery.append(tile);
    const pending = index % 5 === 4, attention = index % 7 === 3, reviewed = !pending && index % 3 === 0;
    window.PhotoFindryShell.mountPhotoSummary(metadata, {
      name: `2026-10-17 original photograph number ${index + 1}.jpeg`, onPreview: () => fixture.previewRequests.push(`synthetic-${index}`),
      sourceLabel: ['Travel photographs', 'Daily life', 'Saved camera archive'][index % 3],
      sourceStatus: index % 4 === 0 ? 'Originals disconnected' : '',
      statusTone: attention ? 'warning' : pending ? 'neutral' : 'success',
      statusLabel: attention ? 'Needs attention' : pending ? 'Photo not tagged yet' : `Tagged · ${24 + index} observations`,
      caption: pending ? '' : ['A quiet mountain landscape with trees and distant hills.', 'Objects on a table beside a bright window.', 'A group of people walking through an open square.'][index % 3],
      reviewLabel: reviewed ? 'Photo detail review' : '', reviewTone: 'review', reviewHint: 'Saved review; accepted observations kept.',
      locationEvidence: index % 9 === 0 ? 'Near a saved place · approximate' : '',
      matchLabel: index % 11 === 0 ? 'Saved description' : '', matchTone: 'direct', matchHint: 'Matched saved evidence',
      matchText: index % 11 === 0 ? 'Trees stand in front of a distant mountain range.' : '',
    });
  }
  function savedDetails(target) {
    target.replaceChildren();
    const header = document.createElement('div'); header.className = 'photo-detail-header';
    const eyebrow = document.createElement('p'); eyebrow.className = 'photo-detail-eyebrow'; eyebrow.textContent = 'Saved photograph';
    const filename = document.createElement('h3'); filename.className = 'inspector-filename'; filename.textContent = 'Mountains at the end of the afternoon.jpeg';
    const badges = document.createElement('div'); badges.className = 'photo-detail-badges';
    const tagged = document.createElement('span'); tagged.className = 'photo-status'; tagged.dataset.tone = 'success'; tagged.textContent = 'Tagged · 31 observations'; badges.append(tagged);
    const reviewed = document.createElement('span'); reviewed.className = 'photo-review-badge'; reviewed.dataset.tone = 'review'; reviewed.textContent = 'Photo detail review'; badges.append(reviewed);
    header.append(eyebrow, filename, badges); target.append(header);
    for (const [title, text] of [['Description', 'A mountain landscape with a narrow path crossing an open field. Trees stand near the hills.'], ['People · saved names', 'Alex · confirmed name'], ['Camera & location', 'Camera Example X1 · 35 mm · Capture time zone unknown'], ['Photo detail review', 'Accepted observations kept. A possible detail remains uncertain.'], ['Objects and setting', 'A path, several trees, open grass and a distant mountain range.'], ['Original & source', fixture.fixturePath + '/Mountains at the end of the afternoon.jpeg']]) {
      const section = document.createElement('section'); section.className = 'detail-section';
      const heading = document.createElement('h3'); heading.textContent = title;
      const copy = document.createElement('p'); copy.textContent = text; section.append(heading, copy); target.append(section);
    }
  }
  savedDetails(document.getElementById('inspector-content'));
  document.getElementById('result-count').textContent = '1–60 of 93,806 photographs'; document.getElementById('page-label').textContent = '1–60 of 93,806';
  fixture.show = async surface => {
    window.PhotoFindryShell.closeDrawers(); window.PhotoFindryPanels.close();
    for (const dialog of document.querySelectorAll('dialog[open]')) dialog.close();
    await new Promise(resolve => requestAnimationFrame(resolve));
    if (surface === 'files') await window.PhotoFindryPanels.show('sources');
    else if (surface === 'settings') window.PhotoFindryWorkspace.openSettings();
    else if (surface === 'jobs') window.PhotoFindryWorkspace.openJobs();
    else if (surface === 'preview') {
      const content = document.getElementById('preview-content'); const stage = document.createElement('div'); stage.className = 'preview-image-stage';
      const image = document.createElement('img'); image.src = fixture.image; image.alt = 'Synthetic preview'; stage.append(image); content.replaceChildren(stage);
      savedDetails(document.getElementById('preview-details'));
      document.getElementById('preview-title').textContent = 'A synthetic photograph with a long descriptive filename.jpeg'; document.getElementById('preview-dialog').showModal();
    }
    await new Promise(resolve => setTimeout(resolve, 100));
    return surface;
  };
  fixture.ready = true;
})();
