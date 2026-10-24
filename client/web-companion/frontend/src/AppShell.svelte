<script>
  import { onMount, tick } from 'svelte';
  import Icon from './Icon.svelte';
  import Navigation from './Navigation.svelte';
  import Appearance from './Appearance.svelte';

  // This component owns only the frame. Saved photos, requests, previews, jobs
  // and model metrics remain in their bounded controllers and content islands.
  const platform = document.documentElement.dataset.platform;
  const preview = /^(linux|windows)$/.test(platform);
  const hostName = preview ? 'computer' : 'Mac';
  const platformLabel = preview ? (platform === 'linux' ? 'Linux' : 'Windows') + ' preview' : 'macOS development branch';
  let navigationOpen = $state(false);
  let inspectorOpen = $state(false);
  let compactNavigation = $state(window.matchMedia('(max-width: 1023px)').matches);
  let compactInspector = $state(window.matchMedia('(max-width: 1279px)').matches);
  const drawerOpen = $derived((compactNavigation && navigationOpen) || (compactInspector && inspectorOpen));
  let shell;
  let navigation;
  let inspector;
  let navigationToggle;
  let inspectorToggle;
  let previousFocus = null;
  let navigationData = $state({ identity: '', total: null, tagged: null, people: null, sources: null, jobs: null, folders: [], moreFolders: false });
  let scope = $state({ sourceId: '', scopeId: '', status: 'all', metadataCount: 0, label: '' });
  let filtersOpen = $state(false);
  let density = $state('comfortable');
  let modelsConfigured = $state(false);
  let navigationSignature = '';
  const number = new Intl.NumberFormat();
  const format = value => Number.isFinite(value) ? number.format(value) : '—';
  const statuses = [['all', 'All photos'], ['tagged', 'Tagged'], ['pending', 'Untagged'], ['needsAttention', 'Needs attention']];
  async function navigate(destination) {
    try {
      if (!window.PhotoFindryWorkspace?.navigate) throw new Error('The interface is still loading. Try again in a moment.');
      await window.PhotoFindryWorkspace.navigate(destination);
      await closeNavigation(false);
    } catch (error) { window.PhotoFindry?.showError(error.message || 'This view could not open. Try again.'); }
  }
  function updateScope() {
    const current = window.PhotoFindry?.getScope?.() || {};
    scope = { sourceId: current.sourceId || '', scopeId: current.scopeId || '', status: current.status || 'all',
      label: window.PhotoFindryWorkspace?.folderLabel?.(current.scopeId) || '',
      metadataCount: Object.values(current.metadataFilters || {}).filter(value => String(value).trim()).length };
  }
  function updateNavigation(snapshot) {
    const sources = Array.isArray(snapshot.sources) ? snapshot.sources : [];
    // Imported folder aliases may project to the same current source scope.
    // That scope is one navigation destination, not duplicate keyed elements.
    const folders = [...new Map((Array.isArray(snapshot.folders) ? snapshot.folders : [])
      .filter(folder => typeof folder.scopeId === 'string' && folder.scopeId && sources.find(source => source.id === folder.sourceId)?.mode !== 'archived')
      .map(folder => [folder.scopeId, folder])).values()];
    const counts = snapshot.jobs?.counts || {};
    const next = {
      identity: JSON.stringify([snapshot.libraryId, snapshot.libraryGeneration, sources.map(source => [source.id, source.mode, source.accessOriginals, source.photoCount]), folders.map(folder => [folder.scopeId, folder.count])]),
      total: snapshot.library?.total, tagged: snapshot.library?.tagged,
      people: Array.isArray(snapshot.people?.people) ? snapshot.people.people.length : null,
      sources: sources.length, sourceLabels: sources.slice(0, 128).map(source => ({ id: source.id, label: source.name || source.label || 'Source' })),
      jobs: ['running', 'queued', 'paused', 'pausing', 'attention'].some(key => Number.isFinite(counts[key]))
        ? ['running', 'queued', 'paused', 'pausing', 'attention'].reduce((sum, key) => sum + (Number.isFinite(counts[key]) ? counts[key] : 0), 0) : null,
      folders: folders.slice(0, 128).map(folder => {
        const source = sources.find(source => source.id === folder.sourceId && source.roots?.length === 1 && source.roots[0].scopeId === folder.scopeId);
        return { scopeId: folder.scopeId, sourceId: folder.sourceId || '', label: folder.name || folder.label || folder.pathLabel || 'Folder', count: Number.isFinite(source?.photoCount) ? source.photoCount : folder.count };
      }),
      moreFolders: folders.length > 128,
    };
    const signature = JSON.stringify(next);
    if (signature !== navigationSignature) { navigationData = next; navigationSignature = signature; }
    modelsConfigured = snapshot.settings?.modelConfigured === true && snapshot.settings?.projectorConfigured === true;
  }
  async function filterStatus(status) {
    try { await window.PhotoFindry.setScope({ status }); }
    catch (error) { window.PhotoFindry?.showError(error.message || 'This filter could not finish. Select it to retry.'); }
  }

  function focusable(container) {
    return [...container.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')]
      .filter(node => !node.closest('[hidden], [inert]') && getComputedStyle(node).display !== 'none' && getComputedStyle(node).visibility !== 'hidden');
  }

  async function closeNavigation(restoreFocus = true) {
    navigationOpen = false;
    const target = previousFocus || navigationToggle;
    previousFocus = null;
    await tick();
    if (restoreFocus && !drawerOpen && target?.isConnected && !shell.hidden) target.focus();
  }

  async function closeInspector(restoreFocus = true) {
    inspectorOpen = false;
    const target = previousFocus || inspectorToggle;
    previousFocus = null;
    await tick();
    if (restoreFocus && !drawerOpen && target?.isConnected && !shell.hidden) target.focus();
  }

  async function toggleNavigation() {
    if (navigationOpen) return closeNavigation();
    // Safari pointer clicks do not necessarily focus the clicked button.
    previousFocus = navigationToggle;
    inspectorOpen = false; navigationOpen = true;
    await tick();
    focusable(navigation)[0]?.focus();
  }

  export async function showDetails(opener) {
    if (!compactInspector) { focusable(inspector)[0]?.focus(); return; }
    const candidate = opener || document.activeElement;
    previousFocus = candidate && candidate !== document.body && !navigation.contains(candidate)
      && !candidate.closest('[hidden], [inert]') ? candidate : inspectorToggle;
    navigationOpen = false; inspectorOpen = true;
    await tick();
    focusable(inspector)[0]?.focus();
  }

  export function closeDrawers() {
    previousFocus = null;
    navigationOpen = false; inspectorOpen = false;
  }

  export function getState() {
    return { navigationOpen, inspectorOpen, compactNavigation, compactInspector, density, filtersOpen };
  }

  async function sessionAction(id) {
    await closeNavigation(false);
    document.getElementById(id)?.click();
  }

  onMount(() => {
    const navigationMedia = window.matchMedia('(max-width: 1023px)');
    const inspectorMedia = window.matchMedia('(max-width: 1279px)');
    const updateNavigationMedia = async () => {
      compactNavigation = navigationMedia.matches;
      if (!compactNavigation && navigationOpen) {
        navigationOpen = false; previousFocus = null;
        await tick();
        if (shell?.isConnected && !shell.hidden && !document.querySelector('dialog[open]')) document.getElementById('library-nav')?.focus();
      }
    };
    const updateInspector = async () => {
      compactInspector = inspectorMedia.matches;
      if (!compactInspector && inspectorOpen) {
        inspectorOpen = false; previousFocus = null;
        await tick();
        if (shell?.isConnected && !shell.hidden && !document.querySelector('dialog[open]')) (focusable(inspector)[0] || document.getElementById('gallery'))?.focus();
      }
    };
    const keydown = event => {
      // Native dialogs own their own focus and Escape handling.
      if (!drawerOpen || document.querySelector('dialog[open]')) return;
      const current = compactNavigation && navigationOpen ? navigation : inspector;
      if (event.key === 'Escape') {
        event.preventDefault(); event.stopPropagation();
        void (current === navigation ? closeNavigation() : closeInspector());
      } else if (event.key === 'Tab') {
        const targets = focusable(current);
        if (!targets.length) { event.preventDefault(); return; }
        const first = targets[0], last = targets[targets.length - 1];
        if (event.shiftKey && (document.activeElement === first || !current.contains(document.activeElement))) {
          event.preventDefault(); last.focus();
        } else if (!event.shiftKey && (document.activeElement === last || !current.contains(document.activeElement))) {
          event.preventDefault(); first.focus();
        }
      }
    };
    const drawerNavigationClicked = event => {
      if (compactNavigation && navigationOpen && event.target instanceof Element
        && event.target.closest('#library-nav, #open-model-monitor')) void closeNavigation(false);
    };
    const session = event => { if (event.detail?.connected === false) { closeDrawers(); updateNavigation({}); updateScope(); } };
    const restored = () => { closeDrawers(); updateNavigation({}); updateScope(); };
    const stateChanged = event => { updateNavigation(event.detail || {}); updateScope(); };
    navigationMedia.addEventListener('change', updateNavigationMedia);
    inspectorMedia.addEventListener('change', updateInspector);
    document.addEventListener('keydown', keydown, true);
    document.addEventListener('click', drawerNavigationClicked);
    window.addEventListener('photofindry:session', session);
    window.addEventListener('photofindry:library-restored', restored);
    window.addEventListener('photofindry:state', stateChanged);
    window.addEventListener('photofindry:scope', updateScope);
    return () => {
      previousFocus = null;
      navigationMedia.removeEventListener('change', updateNavigationMedia);
      inspectorMedia.removeEventListener('change', updateInspector);
      document.removeEventListener('keydown', keydown, true);
      document.removeEventListener('click', drawerNavigationClicked);
      window.removeEventListener('photofindry:session', session);
      window.removeEventListener('photofindry:library-restored', restored);
      window.removeEventListener('photofindry:state', stateChanged);
      window.removeEventListener('photofindry:scope', updateScope);
    };
  });
</script>

<section id="pair-screen" class="pair-screen" aria-labelledby="pair-title">
      <div class="pair-card">
        <img class="pair-brand" src="/brand.png" alt="" width="80" height="80">
        <div class="eyebrow">PHOTOFINDRY / LOCAL COMPANION</div>
        <h1 id="pair-title">Your library.<br>Right here on your {hostName}.</h1>
        <p class="pair-intro">Open your installed PhotoFindry to return to this library.</p>
        <a class="primary-button local-open-link" hidden={preview} href="photofindry-local://open">Open PhotoFindry <span aria-hidden="true">→</span></a>
        <p class="field-help">Using a terminal launch? Connect below with its one-time code.</p>
        <form id="pair-form">
          <label for="pair-code">One-time pairing code</label>
          <input id="pair-code" name="pair-code" type="password" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="Paste the code from your terminal" required maxlength="256" aria-describedby="pair-help pair-error">
          <p id="pair-help" class="field-help">Need a fresh code? Type <code>pair</code> in the running companion’s terminal.</p>
          <p id="pair-error" class="error-text" role="alert" hidden></p>
          <button id="pair-submit" class="primary-button" type="submit">Open local library <span aria-hidden="true">→</span></button>
        </form>
        <div class="pair-note"><span class="local-dot" aria-hidden="true"></span><p>This interface is served by your installed companion. No website connection is needed.</p></div>
      </div>
      <p class="pair-footnote">{platformLabel} <span aria-hidden="true">·</span> Your local PhotoFindry engine</p>
      <noscript>This local interface needs JavaScript enabled in your browser.</noscript>
    </section>

    <div id="app" class="app pf-app-shell pf-rich-shell" data-density={density} class:pf-sidebar-open={navigationOpen} class:pf-inspector-open={inspectorOpen} bind:this={shell} hidden>
      <header class="app-header pf-shell-header" inert={drawerOpen}>
        <button id="sidebar-toggle" class="pf-sidebar-toggle secondary-button" type="button" aria-label="Open navigation" aria-controls="sidebar-navigation" aria-expanded={navigationOpen} bind:this={navigationToggle} onclick={toggleNavigation}><Icon name="menu"/><span>Menu</span></button>
        <div class="brand"><img src="/brand.png" alt="" width="38" height="38"><span>PhotoFindry</span><span class="experiment-label">LOCAL PREVIEW</span></div>
        <nav class="pf-toolbar-shortcuts" aria-label="Quick actions">
          {#each [['Jobs','jobs','Jobs & activity'],['People','people','People'],['Files','folder','Files'],['Settings','settings','Settings']] as item}
            <button class="icon-button" type="button" aria-label={item[2]} title={item[2]} onclick={() => navigate(item[0])}><Icon name={item[1]}/></button>
          {/each}
        </nav>
        <Appearance/>
        <button id="inspector-toggle" class="pf-inspector-toggle secondary-button" type="button" aria-label="Open photo details" aria-controls="inspector" aria-expanded={inspectorOpen} bind:this={inspectorToggle} onclick={() => inspectorOpen ? closeInspector() : showDetails(inspectorToggle)}><Icon name="info"/><span>Photo details</span></button>
        <div class="connection-actions"><span id="connection-status" class="connection-badge"><span class="local-dot" aria-hidden="true"></span>Connected to this {hostName}</span><button id="disconnect" class="text-button" type="button" title="Disconnect all browsers from this running companion">Disconnect browsers</button><button id="quit-companion" class="text-button" type="button">Quit PhotoFindry</button></div>
      </header>

      <div class="workspace pf-shell-content">
        <button id="sidebar-backdrop" class="pf-sidebar-backdrop" type="button" aria-label="Close navigation" tabindex="-1" hidden={!compactNavigation || !navigationOpen} onclick={() => closeNavigation()}></button>
        <button id="inspector-backdrop" class="pf-inspector-backdrop" type="button" aria-label="Close photo details" tabindex="-1" hidden={!compactInspector || !inspectorOpen} onclick={() => closeInspector()}></button>
        <aside id="sidebar-navigation" class="sidebar pf-sidebar" aria-label="Library navigation" role={compactNavigation && navigationOpen ? "dialog" : undefined} aria-modal={compactNavigation && navigationOpen ? "true" : undefined} aria-hidden={compactNavigation && !navigationOpen ? "true" : undefined} inert={(compactNavigation && !navigationOpen) || (compactInspector && inspectorOpen)} bind:this={navigation}>
          <button id="sidebar-close" class="pf-sidebar-close icon-button" type="button" aria-label="Close navigation" onclick={() => closeNavigation()}>×</button>
          <div class="pf-sidebar-brand"><img src="/brand.png" alt="" width="48" height="48"/><div><strong>PhotoFindry</strong><span>Your photographs,<br/>rediscovered.</span></div></div>
          <div id="workspace-navigation" class="workspace-navigation"><Navigation data={navigationData} {scope} {navigate} {closeNavigation}/></div>
          <div class="pf-sidebar-session" aria-label="Local session"><button id="sidebar-disconnect" class="text-button" type="button" onclick={() => sessionAction('disconnect')}>Disconnect browsers</button><button id="sidebar-quit" class="text-button" type="button" onclick={() => sessionAction('quit-companion')}>Quit PhotoFindry</button></div>
          <section class="sidebar-model-section" aria-label="Local model activity"><button id="open-local-models" class="pf-local-models" type="button" onclick={() => navigate('Settings')}><Icon name="chip"/><span><strong>Local models</strong><small>{modelsConfigured ? 'Load automatically when needed' : 'Choose your local models'}</small></span><Icon name="right"/></button><button id="open-model-monitor" class="pfm-sidebar-card" type="button" aria-label="Model activity" aria-haspopup="dialog" aria-expanded="false"><span class="pfm-sidebar-heading"><span class="pfm-bars" aria-hidden="true"><i></i><i></i><i></i></span><strong>Model activity</strong><span class="pfm-info" aria-hidden="true">ⓘ</span></span><span id="sidebar-model-status" class="pfm-sidebar-status">Reading model settings…</span></button><p class="sidebar-local-note">Models and photographs stay on this {hostName}.</p></section>
        </aside>

        <main class="library" aria-labelledby="library-title" inert={drawerOpen}>
          <div class="library-heading"><div><div class="eyebrow">A LITTLE LESS LOOKING. A LITTLE MORE FINDING.</div><h1 id="library-title">Your photographs</h1><p class="pf-library-summary">{#if Number.isFinite(navigationData.total)}{format(navigationData.total)} photographs · {format(navigationData.tagged)} tagged{:else}Your local photo library{/if}</p></div><div class="library-actions"><button id="add-folder" class="secondary-button" type="button"><Icon name="folderPlus"/><span>Add folder</span></button><button id="tag-pending" class="primary-button" type="button"><Icon name="image"/><span>Tag pending</span></button><button id="improve-library" class="secondary-button" type="button" onclick={() => navigate('Improve library')}><Icon name="sparkles"/><span>Improve library</span></button><button id="refresh" class="secondary-button" type="button" title="Reload saved library"><Icon name="refresh"/><span>Refresh</span></button></div></div>
          <section id="onboarding" class="onboarding" hidden></section>
          <section id="job-strip" class="job-strip" aria-label="Current activity" hidden></section>
          <form id="search-form" class="search-form" role="search">
            <span class="search-symbol" aria-hidden="true"></span>
            <label for="search" class="visually-hidden">Search saved photo details</label>
            <input id="search" type="search" autocomplete="off" spellcheck="false" placeholder="Search saved details or filenames…" maxlength="1000">
            <button class="search-submit" type="submit">Search</button>
            <button id="ai-search" class="ai-search-button" type="button">Local AI Search</button>
          </form>
          <p id="search-help" class="search-help">Searches saved library evidence. No model is started.</p>
          <div class="pf-filter-summary-row"><button id="filter-toggle" class="secondary-button" type="button" aria-expanded={filtersOpen} aria-controls="library-filters" onclick={() => filtersOpen = !filtersOpen}><Icon name="filter"/><span>Filters{scope.metadataCount ? ` · ${scope.metadataCount}` : ''}</span></button>{#if scope.sourceId || scope.scopeId}<span class="pf-scope-label"><Icon name="folder"/>{scope.scopeId ? (scope.label || navigationData.folders.find(folder => folder.scopeId === scope.scopeId)?.label || 'Selected subfolder') : (navigationData.sourceLabels?.find(source => source.id === scope.sourceId)?.label || 'Selected source')}</span>{/if}</div>
          <div id="library-filters" class="library-filters" hidden={!filtersOpen}></div>
          <div class="pf-view-toolbar"><div class="pf-status-tabs" role="group" aria-label="Photo status">{#each statuses as item}<button type="button" aria-pressed={scope.status === item[0]} onclick={() => filterStatus(item[0])}>{item[1]}</button>{/each}</div><div id="sort-controls"></div><div class="pf-density-control" role="group" aria-label="Photo size">{#each [['compact','compact','Compact thumbnails'],['comfortable','grid','Comfortable thumbnails'],['large','large','Large thumbnails']] as item}<button type="button" title={item[2]} aria-label={item[2]} aria-pressed={density === item[0]} onclick={() => density = item[0]}><Icon name={item[1]}/></button>{/each}</div></div>
          <div id="ai-results-controls" class="ai-results-controls" hidden></div>
          <div id="selection-actions" class="selection-actions" hidden></div>
          <div class="results-toolbar"><p id="result-count" class="result-count" role="status" aria-live="polite"></p><span id="selection-count" class="selection-count"></span></div>
          <div id="query-notice" class="notice" hidden></div>
          <div id="library-error" class="error-panel" role="alert" hidden><p id="library-error-text"></p><button id="retry-library" class="secondary-button" type="button">Try again</button></div>
          <div id="empty-state" class="empty-state" hidden><div class="empty-icon" aria-hidden="true">▦</div><h2 id="empty-title">No photos in this view</h2><p id="empty-description"></p><button id="clear-search" class="secondary-button" type="button" hidden>Clear search</button><button id="clear-filters" class="secondary-button" type="button" hidden>Clear filters</button><div id="empty-actions" class="button-row" hidden><button id="empty-add-folder" class="primary-button" type="button">Add photo folder</button><button id="empty-restore" class="secondary-button" type="button">Import library backup</button><button id="empty-sources" class="secondary-button" type="button">Check Files</button></div></div>
          <div id="loading-state" class="loading-state" hidden><span class="loading-dots" aria-hidden="true">▪ ▪ ▪</span><p>Reading your local library…</p></div>
          <div id="gallery" class="gallery" role="listbox" aria-label="Photos on this page" aria-multiselectable="true" tabindex="0"></div>
          <footer class="gallery-footer"><span class="keyboard-hint">Click to select <span aria-hidden="true">·</span> Space to preview</span><nav class="pagination" aria-label="Gallery pages"><button id="previous-page" class="page-button" type="button" aria-label="Previous page">←</button><span id="page-label"></span><button id="next-page" class="page-button" type="button" aria-label="Next page">→</button></nav></footer>
        </main>

        <aside id="inspector" class="inspector pf-inspector" aria-label="Saved photo details" role={compactInspector && inspectorOpen ? "dialog" : undefined} aria-modal={compactInspector && inspectorOpen ? "true" : undefined} aria-hidden={compactInspector && !inspectorOpen ? "true" : undefined} inert={(compactInspector && !inspectorOpen) || (compactNavigation && navigationOpen)} bind:this={inspector}>
          <div class="inspector-heading"><h2>Photo details</h2><button id="inspector-close" class="pf-inspector-close icon-button" type="button" aria-label="Close photo details" onclick={() => closeInspector()}>×</button><button id="clear-selection" class="icon-button" type="button" aria-label="Clear selection" hidden>×</button></div>
          <div id="inspector-content" class="inspector-content"><div class="inspector-empty"><span aria-hidden="true">⌗</span><h3>A closer look</h3><p>Select a photo to see its complete saved details.</p></div></div>
        </aside>
      </div>
      <footer class="app-footer" inert={drawerOpen}><span><span class="local-dot" aria-hidden="true"></span> Local interface · Local library</span><span>{platformLabel}</span></footer>
      <div id="global-notice" class="global-notice" role="status" hidden></div>
      <div id="global-error" class="global-error" role="alert" hidden></div>
    </div>

    <dialog id="preview-dialog" class="preview-dialog" aria-labelledby="preview-title">
      <div class="preview-toolbar">
        <div class="preview-navigation"><button id="preview-previous" type="button" aria-label="Previous photograph">←</button><button id="preview-next" type="button" aria-label="Next photograph">→</button><button id="preview-back" type="button" hidden>Full photo</button><span id="preview-position" aria-live="polite"></span></div>
        <h2 id="preview-title">Photo preview</h2>
        <div class="preview-zoom"><button id="preview-zoom-out" type="button" aria-label="Zoom out">−</button><button id="preview-zoom-fit" type="button">Fit</button><button id="preview-zoom-in" type="button" aria-label="Zoom in">+</button><span id="preview-zoom-level" aria-live="polite">Fit</span></div>
        <button id="close-preview" class="icon-button" type="button" aria-label="Close preview">×</button>
      </div>
      <div class="preview-body"><div id="preview-content" class="preview-content"></div><aside id="preview-details" class="preview-details" aria-label="Complete saved photograph details"></aside></div>
      <p class="preview-footnote">Saved preview · Your original is unchanged</p>
    </dialog>

<style>
  .pf-app-shell { isolation: isolate; }
</style>
