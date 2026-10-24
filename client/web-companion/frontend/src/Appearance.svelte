<script>
  import { onMount } from 'svelte';

  const modes = [
    { value: 'light', label: 'Light', path: 'M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6 7 7m10 10 1.4 1.4M5.6 18.4 7 17M17 7l1.4-1.4M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z' },
    { value: 'dark', label: 'Dark', path: 'M20.5 13.3A8.6 8.6 0 0 1 10.7 3.5 8.6 8.6 0 1 0 20.5 13.3Z' },
    { value: 'system', label: 'Follow system', path: 'M4 4h16a1 1 0 0 1 1 1v11H3V5a1 1 0 0 1 1-1Zm5 16h6m-3-4v4' },
  ];
  const valid = value => modes.some(item => item.value === value);
  let mode = $state('system');
  let status = $state('');
  let error = $state('');
  let saving = $state(false);
  let connected = $state(false);
  let retryKind = $state('');
  let menu;
  let trigger;
  let media;
  let disposed = false;
  let epoch = 0;
  let revision = 0;
  let pending = null;
  let reading = null;
  let writing = false;

  function apply(value) {
    mode = valid(value) ? value : 'system';
    document.documentElement.dataset.appearance = mode;
    document.documentElement.dataset.theme = mode === 'system' ? (media?.matches ? 'dark' : 'light') : mode;
  }

  function dismiss() {
    if (!menu?.open) return;
    menu.open = false;
    trigger?.focus();
  }

  async function load() {
    reading?.abort();
    if (!connected || disposed || pending !== null) return;
    const controller = new AbortController();
    reading = controller;
    const ownEpoch = epoch, ownRevision = revision;
    try {
      const result = await window.PhotoFindry.api('/api/appearance', {}, { signal: controller.signal });
      if (disposed || ownEpoch !== epoch || ownRevision !== revision || pending !== null) return;
      if (!valid(result?.mode)) throw new Error('The saved appearance could not be read.');
      apply(result.mode); status = ''; error = ''; retryKind = '';
    } catch (failure) {
      if (disposed || controller.signal.aborted || ownEpoch !== epoch || ownRevision !== revision || pending !== null) return;
      error = failure?.message || 'The saved appearance could not be read.';
      status = 'Choose a mode or check again.'; retryKind = 'load';
    } finally {
      if (reading === controller) reading = null;
    }
  }

  // At most one write is accepted at a time. A newer choice replaces the queued
  // choice, and is saved after an older accepted request finishes. Revocation
  // invalidates its UI result without assuming an aborted write was undone.
  async function save() {
    if (writing || !connected || disposed || pending === null) return;
    writing = true; saving = true;
    try {
      while (!disposed && connected && pending !== null) {
        const ownEpoch = epoch, ownRevision = revision, chosen = pending;
        status = 'Saving appearance…'; error = ''; retryKind = '';
        try {
          const result = await window.PhotoFindry.api('/api/appearance/update', { mode: chosen });
          if (disposed) return;
          if (ownEpoch !== epoch) continue;
          if (result?.mode !== chosen) throw new Error('The selected appearance was not saved.');
          if (ownRevision === revision) {
            pending = null; status = /^(linux|windows)$/.test(document.documentElement.dataset.platform) ? 'Saved on this computer.' : 'Saved on this Mac.';
          }
        } catch (failure) {
          if (disposed) return;
          if (ownEpoch !== epoch || ownRevision !== revision) continue;
          error = failure?.message || 'Appearance could not be saved.';
          status = 'Your choice stays visible. Retry saving to keep it for next time.';
          retryKind = 'save'; break;
        }
      }
    } finally {
      writing = false;
      if (!disposed) saving = false;
    }
  }

  function choose(value) {
    if (!valid(value)) return;
    revision += 1; pending = value; reading?.abort();
    apply(value); error = ''; retryKind = '';
    if (connected) void save();
    else status = 'Open your local library to save this choice.';
  }

  function retry() {
    if (!connected || saving) return;
    if (retryKind === 'load') void load();
    else void save();
  }

  onMount(() => {
    media = window.matchMedia('(prefers-color-scheme: dark)');
    apply(document.documentElement.dataset.appearance);
    const systemChanged = () => { if (mode === 'system') apply(mode); };
    const session = event => {
      epoch += 1; reading?.abort();
      connected = event.detail?.connected === true;
      error = ''; retryKind = '';
      if (!connected) {
        dismiss();
        status = pending === null ? '' : 'Open your local library to save this choice.';
      } else if (pending !== null) void save();
      else void load();
    };
    const outside = event => { if (menu?.open && !menu.contains(event.target)) dismiss(); };
    const keydown = event => {
      if (event.key !== 'Escape' || !menu?.open || document.querySelector('dialog[open]')) return;
      event.preventDefault(); event.stopPropagation(); dismiss();
    };
    media.addEventListener('change', systemChanged);
    window.addEventListener('photofindry:session', session);
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', keydown, true);
    if (window.PhotoFindry?.isConnected?.()) session({ detail: { connected: true } });
    return () => {
      disposed = true; epoch += 1; reading?.abort();
      media.removeEventListener('change', systemChanged);
      window.removeEventListener('photofindry:session', session);
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', keydown, true);
    };
  });
</script>

{#snippet icon(path)}
  <svg class="pf-theme-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={path}></path></svg>
{/snippet}

<details class="pf-appearance-control pf-theme-menu" bind:this={menu}>
  <summary class="pf-theme-trigger" aria-label="Appearance" bind:this={trigger}>
    {@render icon(modes.find(item => item.value === mode).path)}<span>Appearance</span>
  </summary>
  <div class="pf-theme-options" role="group" aria-label="Color theme">
    {#each modes as item}
      <button type="button" class="pf-theme-choice" data-appearance-choice={item.value} aria-pressed={mode === item.value} onclick={() => choose(item.value)}>
        {@render icon(item.path)}<span>{item.label}</span>
      </button>
    {/each}
    {#if status}<p class="pf-theme-status" role="status">{status}</p>{/if}
    {#if error}<p class="pf-theme-error" role="alert">{error}</p>{/if}
    {#if retryKind}<button type="button" class="pf-theme-retry" disabled={!connected || saving} onclick={retry}>{retryKind === 'load' ? 'Check again' : 'Retry saving'}</button>{/if}
  </div>
</details>
