'use strict';
(async () => {
  const parameters = new URL(location.href).searchParams;
  const surface = parameters.get('surface');
  const theme = parameters.get('theme') === 'dark' ? 'dark' : 'light';
  const allowed = ['gallery', 'files', 'settings', 'jobs', 'preview'];
  if (!allowed.includes(surface)) return;
  const settle = async () => { await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); };
  const failures = [], interactionChecks = [];
  const assert = (value, message) => { if (!value) failures.push(message); };
  try {
    await window.layoutFixture.show(surface); await settle();
    const active = document.querySelector('dialog[open]') || [...document.querySelectorAll('.pfp-shell')].find(node => !node.hidden) || document.getElementById('app');
    const visible = node => {
      if (node.closest('[hidden]')) return false;
      const style = getComputedStyle(node), rect = node.getBoundingClientRect();
      if (style.display === 'none' || style.visibility === 'hidden' || !rect.width || !rect.height) return false;
      for (let parent = node.parentElement; parent; parent = parent.parentElement) if (getComputedStyle(parent).visibility === 'hidden' || getComputedStyle(parent).display === 'none') return false;
      return true;
    };
    const outside = [...active.querySelectorAll('*')].filter(visible).filter(node => {
      const rect = node.getBoundingClientRect();
      if (rect.left >= -1 && rect.right <= innerWidth + 1) return false;
      // Content inside an explicitly scrollable or clipped viewport may be wider
      // by design, e.g. horizontal workflow tabs and ellipsized photo filenames.
      for (let parent = node.parentElement; parent && parent !== active; parent = parent.parentElement) {
        if (['auto', 'scroll', 'hidden', 'clip'].includes(getComputedStyle(parent).overflowX)) return false;
      }
      return true;
    }).map(node => `${node.tagName.toLowerCase()}.${node.className || ''}: ${Math.round(node.getBoundingClientRect().left)}..${Math.round(node.getBoundingClientRect().right)}`).slice(0, 15);
    const minimum = innerWidth <= 767 || matchMedia('(pointer: coarse)').matches ? 44 : 36;
    const selectors = 'button,select,.workspace-input:not([type=checkbox]),.pfp-input';
    const controls = [...active.querySelectorAll(selectors)].filter(visible).filter(node => !node.closest('[inert]'));
    const short = controls.filter(node => node.getBoundingClientRect().height < minimum - 1).map(node => `${node.textContent.trim().slice(0, 45) || node.tagName}: ${node.getBoundingClientRect().height.toFixed(1)}px`);
    const contentOverflow = controls.filter(node => node.tagName === 'BUTTON' && parseFloat(getComputedStyle(node).fontSize) > 0).flatMap(node => {
      const range = document.createRange(); range.selectNodeContents(node);
      const content = range.getBoundingClientRect(), box = node.getBoundingClientRect();
      const label = node.querySelector('.pf-nav-label');
      const clippedLabel = label && getComputedStyle(label).overflow === 'hidden' && getComputedStyle(label).textOverflow === 'ellipsis';
      const childOutside = [...node.children].filter(visible).some(child => { const rect = child.getBoundingClientRect(); return rect.left < box.left - 1 || rect.right > box.right + 1; });
      // Range bounds include the invisible tail of deliberately ellipsized
      // labels. Their actual boxes, icons and counts must still fit the button.
      return content.top < box.top - 1 || content.bottom > box.bottom + 1 || childOutside || (!clippedLabel && (content.left < box.left - 1 || content.right > box.right + 1))
        ? [`${node.textContent.trim().slice(0, 45)} text exceeds its button (${node.className}; range ${[content.left,content.top,content.right,content.bottom].map(Math.round).join(',')} vs box ${[box.left,box.top,box.right,box.bottom].map(Math.round).join(',')}; childOutside ${childOutside})`] : [];
    });
    // Navigation and model cards deliberately use compact hierarchy text; task
    // actions retain one type scale and every visible control retains its target height.
    const standardButtons = controls.filter(node => node.matches('.primary-button,.secondary-button,.pfp-button,.search-submit,.ai-search-button') && !node.matches('.pf-sidebar-toggle,.pf-inspector-toggle') && !node.closest('.pf-sidebar'));
    const expectedFont = innerWidth <= 767 ? 14 : 13;
    const inconsistentFonts = standardButtons.filter(node => Math.abs(parseFloat(getComputedStyle(node).fontSize) - expectedFont) > .1)
      .map(node => `${node.textContent.trim().slice(0, 45)}: ${getComputedStyle(node).fontSize}`);
    const tallActions = standardButtons.filter(node => node.getBoundingClientRect().height > minimum * 1.5).map(node => `${node.textContent.trim().slice(0, 45)}: ${node.getBoundingClientRect().height.toFixed(1)}px`);
    assert(document.documentElement.scrollWidth <= innerWidth + 1, `Document overflows horizontally: ${document.documentElement.scrollWidth} > ${innerWidth}`);
    assert(outside.length === 0, `Uncontained horizontal overflow: ${outside.join('; ')}`);
    assert(short.length === 0, `Controls below ${minimum}px: ${short.join('; ')}`);
    assert(inconsistentFonts.length === 0, `Inconsistent action button type: ${inconsistentFonts.join('; ')}`);
    assert(contentOverflow.length === 0, `Button content overflow: ${contentOverflow.join('; ')}`);
    assert(tallActions.length === 0, `Action button wrapping exceeds two lines: ${tallActions.join('; ')}`);
    if (surface === 'gallery' && innerWidth <= 767) for (const id of ['sidebar-toggle', 'inspector-toggle']) {
      const label = document.getElementById(id).querySelector('span');
      assert(!label || getComputedStyle(label).display === 'none', `${id} still displays its long text label`);
    }
    if (surface === 'gallery') {
      const library = document.querySelector('.library'), gallery = document.getElementById('gallery'), tile = gallery.firstElementChild;
      const beforeTile = tile, beforeGallery = gallery;
      assert(library.scrollHeight > library.clientHeight, 'Large gallery does not have its own vertical scroll region');
      assert(gallery.getBoundingClientRect().width <= library.clientWidth + 1, 'Gallery exceeds library width');
      assert(Math.abs(tile.querySelector('.photo-image').getBoundingClientRect().width / tile.querySelector('.photo-image').getBoundingClientRect().height - 1.28) < .02, 'Photo aspect ratio changed');
      if (innerWidth <= 1023) {
        document.getElementById('sidebar-toggle').click(); await settle();
        assert(window.PhotoFindryShell.getState().navigationOpen, 'Menu failed to open');
        assert(document.getElementById('sidebar-navigation').contains(document.activeElement), 'Menu did not receive focus');
        assert(document.querySelector('.library').inert, 'Menu did not isolate library input');
        document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); await settle();
        assert(!window.PhotoFindryShell.getState().navigationOpen, 'Menu Escape failed');
        assert(document.activeElement === document.getElementById('sidebar-toggle'), 'Menu did not restore focus');
        interactionChecks.push('Menu focus, inert, Escape and return focus');
      }
      if (innerWidth <= 1279) {
        document.getElementById('inspector-toggle').click(); await settle();
        assert(window.PhotoFindryShell.getState().inspectorOpen, 'Photo details failed to open');
        assert(document.getElementById('inspector').contains(document.activeElement), 'Photo details did not receive focus');
        document.getElementById('inspector-backdrop').click(); await settle();
        assert(!window.PhotoFindryShell.getState().inspectorOpen, 'Photo details backdrop failed');
        interactionChecks.push('Photo details focus and backdrop dismissal');
      }
      assert(document.querySelectorAll('.pf-photo-summary').length === 60, 'Compiled photo summaries were not rendered for this page');
      assert(document.querySelector('.pf-nav-count')?.textContent === new Intl.NumberFormat().format(93806), 'Library navigation count did not update');
      assert(document.querySelectorAll('.pf-folder-row').length === 1, 'Duplicate imported folder aliases were not collapsed into one visible row');
      assert(document.querySelector('.pf-folder-row .pf-nav-count')?.textContent === new Intl.NumberFormat().format(60978), 'Root folder count did not use the authoritative source count');
      assert(document.querySelector('.pf-library-summary')?.textContent === `${new Intl.NumberFormat().format(93806)} photographs · ${new Intl.NumberFormat().format(65127)} tagged`, 'Rich header counts stayed on the fallback after duplicate folder aliases');
      interactionChecks.push('Imported duplicate aliases deduplicated; authoritative source and header counts');
      const appearance = document.querySelector('.pf-theme-menu'), trigger = appearance.querySelector('summary');
      trigger.click(); await settle();
      assert(appearance.open, 'Appearance menu did not open');
      const options = appearance.querySelector('.pf-theme-options').getBoundingClientRect();
      assert(options.left >= -1 && options.right <= innerWidth + 1, 'Appearance menu is outside the viewport');
      for (const choice of appearance.querySelectorAll('.pf-theme-choice')) assert(choice.getBoundingClientRect().height >= minimum - 1, 'Appearance choice is below the control-height minimum');
      const alternate = theme === 'light' ? 'dark' : 'light';
      appearance.querySelector(`[data-appearance-choice="${alternate}"]`).click(); await settle();
      assert(document.documentElement.dataset.theme === alternate, 'Appearance did not update immediately');
      assert(window.layoutFixture.getAppearance() === alternate && window.layoutFixture.appearanceWrites.includes(alternate), 'Appearance did not reach the mock persistence callback');
      appearance.querySelector('[data-appearance-choice="system"]').click(); await settle();
      assert(document.documentElement.dataset.appearance === 'system' && document.documentElement.dataset.theme === (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'), 'Follow system did not use the browser preference');
      appearance.querySelector(`[data-appearance-choice="${theme}"]`).click(); await settle();
      assert((await window.PhotoFindry.api('/api/appearance')).mode === theme, 'Saved appearance was not readable after the change');
      trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); await settle();
      assert(!appearance.open && document.activeElement === trigger, 'Appearance Escape did not dismiss and restore focus');
      interactionChecks.push('Light/Dark/System UI and mock persistence, Escape and focus');
      const filterToggle = document.getElementById('filter-toggle'); filterToggle.click(); await settle();
      const filters = document.getElementById('library-filters');
      assert(!filters.hidden && filterToggle.getAttribute('aria-expanded') === 'true', 'Filters failed to expand');
      assert(filters.scrollWidth <= filters.clientWidth + 1, 'Expanded filters overflow their panel');
      for (const control of filters.querySelectorAll('button, select')) if (visible(control)) assert(control.getBoundingClientRect().height >= minimum - 1, 'Expanded filter control is too short');
      filterToggle.click(); await settle(); assert(filters.hidden, 'Filters failed to collapse');
      const large = document.querySelector('.pf-density-control [aria-label="Large thumbnails"]'); large.click(); await settle();
      assert(document.getElementById('app').dataset.density === 'large', 'Large density did not update');
      assert(gallery.scrollWidth <= gallery.clientWidth + 1, 'Large photo density overflows the gallery');
      document.querySelector('.pf-density-control [aria-label="Comfortable thumbnails"]').click(); await settle();
      interactionChecks.push('Filters expand/collapse and photo density without content replacement');
      const reviewAction = document.querySelector('.photo-review-badge.photo-summary-action'); reviewAction.click(); await settle();
      assert(window.layoutFixture.previewRequests.includes('synthetic-0'), 'Photo review action did not delegate its photo identity');
      interactionChecks.push('Rich photo review delegates the selected photo');
      assert(beforeTile === document.querySelector('.photo-tile') && beforeGallery === document.getElementById('gallery'), 'Drawer recreated gallery content');
      interactionChecks.push('Photo and gallery identity preserved');
    }
    if (surface === 'settings') {
      const dialog = document.querySelector('dialog[open]'); dialog.scrollTop = dialog.scrollHeight; await settle();
      const footer = dialog.querySelector('.settings-footer');
      assert(Boolean(footer), 'Settings footer missing');
      if (footer) { const rect = footer.getBoundingClientRect(); assert(rect.bottom <= innerHeight + 1 && rect.top >= 0, 'Settings footer is outside viewport'); }
      dialog.scrollTop = 0;
    }
    if (surface === 'preview' && innerWidth <= 767) {
      const close = document.getElementById('close-preview').getBoundingClientRect(), title = document.getElementById('preview-title').getBoundingClientRect();
      assert(Math.abs((close.top + close.height / 2) - (title.top + title.height / 2)) < 3, 'Preview close is detached from its title row');
    }
    assert(document.documentElement.dataset.theme === theme, `Expected ${theme} appearance`);
    let surfaceColor = getComputedStyle(active).backgroundColor, surfaceStyleSettles = 0;
    // WebKit may expose the pre-paint transparent value when a fixed panel
    // first becomes visible. Wait at most two rendered frames, then require
    // an opaque surface; a persistent transparent dark panel must not pass.
    while (surfaceColor === 'rgba(0, 0, 0, 0)' && surfaceStyleSettles < 2) { await settle(); surfaceStyleSettles += 1; surfaceColor = getComputedStyle(active).backgroundColor; }
    const rgba = surfaceColor.match(/[\d.]+/g)?.map(Number) || [];
    const channels = rgba.slice(0, 3), opaque = rgba.length === 3 || rgba[3] > .99;
    assert(opaque && channels.length === 3 && (theme === 'dark' ? channels.reduce((a, b) => a + b, 0) < 330 : channels.reduce((a, b) => a + b, 0) > 600), `${theme} appearance did not reach the ${surface} surface (${active.tagName}.${active.className}): ${surfaceColor}`);
    assert(window.layoutFixture.errors.length === 0, `Runtime errors: ${window.layoutFixture.errors.join('; ')}`);
    const report = { width: innerWidth, height: innerHeight, surface, theme, passed: failures.length === 0, controls: controls.length,
      minimumControlPx: minimum, surfaceColor, surfaceStyleSettles, surfaceDiagnostics: { connected: active.isConnected, hidden: active.hidden, currentPanel: active === document.getElementById('advanced-panel'), display: getComputedStyle(active).display, token: getComputedStyle(active).getPropertyValue('--pf-bg'), inline: active.getAttribute('style'), parent: active.parentElement?.id || null, rootTheme: document.documentElement.dataset.theme }, surfaceClass: `${active.tagName}.${active.className}`, documentWidth: document.documentElement.scrollWidth, overflow: outside, shortControls: short, interactionChecks, failures,
      standardButtonFontPx: expectedFont, inconsistentFonts, contentOverflow, tallActions,
      runtimeErrors: window.layoutFixture.errors.slice() };
    window.parent.postMessage({ type: 'photofindry-layout-report', report }, location.origin);
  } catch (error) {
    window.parent.postMessage({ type: 'photofindry-layout-report', report: { width: innerWidth, surface, theme, passed: false, failures: [error.stack || String(error)] } }, location.origin);
  }
})();
