<script>
  // Only the current page's display strings live here. The gallery controller
  // retains image URLs, requests, photo identity, selection and full evidence.
  let { data } = $props();
  export function update(next) { data = next; }
  function preview(event) { event.stopPropagation(); data.onPreview?.(); }
</script>

{#snippet icon(path)}
  <svg class="pf-photo-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={path}></path></svg>
{/snippet}

<div class="pf-photo-summary">
  <p class="photo-name" title={data.name}>{data.name}</p>
  {#if data.sourceLabel}
    <p class="photo-source" title={[data.sourceLabel, data.sourceStatus].filter(Boolean).join(' · ')}>
      {@render icon('M3 7V5a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z')}
      <span>{[data.sourceLabel, data.sourceStatus].filter(Boolean).join(' · ')}</span>
    </p>
  {/if}
  <p class="photo-state" data-tone={data.statusTone}>
    {#if data.statusTone === 'success'}
      {@render icon('M20 11v1a8 8 0 1 1-4.7-7.3M8 11l4 4L21 6')}
    {:else if data.statusTone === 'warning'}
      {@render icon('M12 8v5m0 3h.01M10.3 3.9 2.5 17.4A2 2 0 0 0 4.2 20h15.6a2 2 0 0 0 1.7-2.6L13.7 3.9a2 2 0 0 0-3.4 0Z')}
    {:else}
      {@render icon('M12 4v1m5-0.7-.5.8M20 8l-.8.5M20 12h-1m1 4-.8-.5M17 19l-.5-.8M12 20v-1m-5 .7.5-.8M4 16l.8-.5M4 12h1M4 8l.8.5M7 5l.5.8')}
    {/if}
    <span>{data.statusLabel}</span>
  </p>
  {#if data.caption}<p class="photo-caption" title={data.caption}>{data.caption}</p>{/if}
  {#if data.reviewLabel}
    <button type="button" class="photo-review-badge photo-summary-action" data-tone={data.reviewTone} title={data.reviewHint} aria-label={`${data.reviewLabel}. Open review for ${data.name}`} onclick={preview} ondblclick={event => event.stopPropagation()} disabled={typeof data.onPreview !== 'function'}>
      {@render icon('M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Zm2 5h10M7 13h6')}
      <span>{data.reviewLabel}</span>
    </button>
  {/if}
  {#if data.locationEvidence}<p class="photo-location">{@render icon('M19 10c0 5-7 11-7 11S5 15 5 10a7 7 0 1 1 14 0ZM12 7a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z')}<span>{data.locationEvidence}</span></p>{/if}
  {#if data.matchLabel || data.matchText}
    <button type="button" class="photo-match-block photo-summary-action" data-tone={data.matchTone} aria-label={`Open ${data.matchLabel || 'matching details'} for ${data.name}`} onclick={preview} ondblclick={event => event.stopPropagation()} disabled={typeof data.onPreview !== 'function'}>
      {#if data.matchLabel}<span class="photo-match" title={data.matchHint}>{@render icon('M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15ZM16 16l5 5')}<span>{data.matchLabel}</span></span>{/if}
      {#if data.matchText}<span class="photo-match-evidence">{data.matchText}</span>{/if}
    </button>
  {/if}
</div>
