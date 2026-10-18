<script>
  import { onDestroy } from 'svelte';
  import Icon from './Icon.svelte';
  let { data, scope, navigate, closeNavigation } = $props();
  let children = $state({});
  let expanded = $state([]);
  let loading = $state('');
  let error = $state(null);
  let controller = null;
  let identity = '';
  const number = new Intl.NumberFormat();
  const format = value => Number.isFinite(value) && value >= 0 ? number.format(value) : '';
  $effect(() => {
    if (identity !== data.identity) {
      identity = data.identity; controller?.abort(); children = {}; expanded = []; loading = ''; error = null;
    }
  });
  onDestroy(() => controller?.abort());
  async function select(folder) {
    try {
      window.PhotoFindryPanels?.close?.();
      window.PhotoFindryWorkspace?.rememberFolders?.([folder]);
      await window.PhotoFindry.setScope({ sourceId: folder.sourceId, scopeId: folder.scopeId, status: 'all' });
      await closeNavigation(false);
      document.getElementById('gallery')?.focus({ preventScroll: true });
    } catch (failure) { window.PhotoFindry?.showError(failure.message || 'This folder could not open. Try selecting it again.'); }
  }
  async function toggle(folder, retry = false) {
    if (!retry && expanded.includes(folder.scopeId)) { expanded = expanded.filter(id => id !== folder.scopeId); return; }
    if (children[folder.scopeId] && !retry) { expanded = [...expanded, folder.scopeId].slice(-8); return; }
    controller?.abort(); const request = new AbortController(); controller = request;
    const expectedIdentity = identity; loading = folder.scopeId; error = null;
    try {
      const result = await window.PhotoFindry.api('/api/action', { action: 'folderChildren', params: { scopeId: folder.scopeId } }, { signal: request.signal });
      if (request.signal.aborted || expectedIdentity !== identity) return;
      const unique = [...new Map((Array.isArray(result.children) ? result.children : [])
        .filter(item => typeof item.scopeId === 'string' && item.scopeId)
        .map(item => [item.scopeId, item])).values()];
      const items = unique.slice(0, 80).map(item => ({ scopeId: item.scopeId, sourceId: folder.sourceId, label: item.name || item.label || item.pathLabel || 'Folder', count: item.count }));
      const next = { ...children, [folder.scopeId]: { items, more: unique.length > 80, message: result.message || '' } };
      window.PhotoFindryWorkspace?.rememberFolders?.(items);
      while (Object.keys(next).length > 8) { const oldest = Object.keys(next)[0]; delete next[oldest]; expanded = expanded.filter(id => id !== oldest); }
      children = next; expanded = [...expanded.filter(id => id !== folder.scopeId), folder.scopeId].slice(-8);
    } catch (failure) { if (!request.signal.aborted) error = { id: folder.scopeId, message: failure.message || 'Subfolders could not load.' }; }
    finally { if (controller === request) loading = ''; }
  }
</script>

<section class="pf-nav-group" aria-label="Library">
  <h2 class="pf-nav-heading">Library</h2>
  <button id="library-nav" class="nav-item pf-nav-item" class:active={!scope.sourceId && !scope.scopeId} type="button" aria-current={!scope.sourceId && !scope.scopeId ? 'page' : undefined}><Icon name="grid"/><span class="pf-nav-label">All photographs</span><span class="pf-nav-count">{format(data.total)}</span></button>
  <button class="pf-nav-item" type="button" onclick={() => navigate('People')}><Icon name="people"/><span class="pf-nav-label">People</span><span class="pf-nav-count">{format(data.people)}</span></button>
  <button class="pf-nav-item" type="button" onclick={() => navigate('Files')}><Icon name="folder"/><span class="pf-nav-label">Files</span><span class="pf-nav-count">{format(data.sources)}</span></button>
</section>
<section class="pf-nav-group" aria-label="Photo folders">
  <h2 class="pf-nav-heading"><span>Folders</span><button type="button" aria-label="Add photo folder" title="Add photo folder" onclick={() => document.getElementById('add-folder')?.click()}><Icon name="plus"/></button></h2>
  <div class="pf-folder-tree">
    {#snippet rows(items, depth)}
      {#each items as folder (folder.scopeId)}
        <div class={'pf-folder-row depth-' + depth}>
          <button type="button" aria-label={'Subfolders of ' + folder.label} aria-expanded={expanded.includes(folder.scopeId)} disabled={loading === folder.scopeId} onclick={() => toggle(folder)}><Icon name={expanded.includes(folder.scopeId) ? 'down' : 'right'}/></button>
          <button type="button" title={folder.label} aria-current={scope.scopeId === folder.scopeId ? 'page' : undefined} onclick={() => select(folder)}><Icon name="folder"/><span class="pf-nav-label">{folder.label}</span><span class="pf-nav-count">{format(folder.count)}</span></button>
        </div>
        {#if loading === folder.scopeId}<p class="pf-folder-notice" role="status">Reading saved subfolders…</p>{/if}
        {#if error?.id === folder.scopeId}<div class="pf-folder-notice" role="alert"><p>{error.message}</p><button type="button" onclick={() => toggle(folder, true)}>Retry subfolders</button></div>{/if}
        {#if expanded.includes(folder.scopeId) && children[folder.scopeId]}
          {#if depth < 6}{@render rows(children[folder.scopeId].items, depth + 1)}{/if}
          {#if !children[folder.scopeId].items.length}<p class="pf-folder-notice">No saved subfolders.</p>{/if}
          {#if children[folder.scopeId].more || depth === 6}<button class="pf-nav-item" type="button" onclick={() => navigate('Browse folders')}>Browse more folders…</button>{/if}
        {/if}
      {/each}
    {/snippet}
    {@render rows(data.folders, 0)}
    {#if !data.folders.length}<p class="pf-folder-empty">Your photo folders appear here.</p>{/if}
    {#if data.moreFolders}<button class="pf-nav-item" type="button" onclick={() => navigate('Browse folders')}>Browse all folders…</button>{/if}
  </div>
</section>
<section class="pf-nav-group" aria-label="Organize and improve">
  <h2 class="pf-nav-heading">Workspace</h2>
  <button class="pf-nav-item" type="button" onclick={() => navigate('Photo improvement')}><Icon name="sparkles"/><span class="pf-nav-label">Photo improvement</span></button>
  <button class="pf-nav-item" type="button" onclick={() => navigate('Jobs')}><Icon name="jobs"/><span class="pf-nav-label">Jobs & activity</span><span class="pf-nav-count">{format(data.jobs)}</span></button>
  <button class="pf-nav-item" type="button" onclick={() => navigate('Backups & archives')}><Icon name="archive"/><span class="pf-nav-label">Backups & archives</span></button>
  <button class="pf-nav-item" type="button" onclick={() => navigate('Removed')}><Icon name="trash"/><span class="pf-nav-label">Removed</span></button>
</section>
<section class="pf-nav-group pf-nav-utilities" aria-label="Application">
  <h2 class="pf-nav-heading">PhotoFindry</h2>
  {#each [['Settings','settings'],['Getting started','book'],['This library','image'],['Help & feedback','help'],['About','info']] as item}
    <button class="pf-nav-item" type="button" onclick={() => navigate(item[0])}><Icon name={item[1]}/><span class="pf-nav-label">{item[0]}</span></button>
  {/each}
  <button class="pf-nav-item support-link" type="button" onclick={() => navigate('Support PhotoFindry')}><Icon name="coffee"/><span class="pf-nav-label">Support PhotoFindry</span></button>
</section>
