<script module>
  const paths = {
    grid: 'M3 3h7v7H3Zm11 0h7v7h-7ZM3 14h7v7H3Zm11 0h7v7h-7Z',
    image: 'M4 3h16a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1ZM3 16l5-5 4 4 3-3 6 6M16 6h.01',
    people: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm13 18v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8',
    folder: 'M3 7V5a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z',
    folderPlus: 'M3 7V5a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1ZM12 10v7m-3-3.5h6',
    sparkles: 'm12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5ZM20 2v4m-2-2h4',
    jobs: 'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM7 7h2m3 0h5M7 12h2m3 0h5M7 17h2m3 0h5',
    archive: 'M3 3h18v5H3Zm2 5v13h14V8M9 12h6',
    trash: 'M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7',
    settings: 'M4 5h4m4 0h8M4 12h10m4 0h2M4 19h4m4 0h8M8 2v6m6 1v6m-6 1v6',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 7v7m0-10h.01',
    help: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM9.2 9a3 3 0 0 1 5.6 1c0 2-2.8 2-2.8 4m0 3h.01',
    coffee: 'M3 8h14v8a5 5 0 0 1-5 5H8a5 5 0 0 1-5-5Zm14 1h2a3 3 0 0 1 0 6h-2M6 2v3m4-3v3m4-3v3',
    book: 'M12 5C8 2 3 3 2 4v15c3-2 7-2 10 0 3-2 7-2 10 0V4c-3-2-7-2-10 1Zm0 0v14',
    chip: 'M6 6h12v12H6Zm3 3h6v6H9ZM9 2v4m6-4v4M9 18v4m6-4v4M2 9h4m-4 6h4M18 9h4m-4 6h4',
    right: 'm9 5 7 7-7 7', down: 'm5 9 7 7 7-7', plus: 'M12 4v16M4 12h16',
    search: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15ZM16 16l5 5',
    refresh: 'M20 7V2m0 5h-5M4 17v5m0-5h5M4.5 8a8 8 0 0 1 13-3L20 7M4 17l2.5 2a8 8 0 0 0 13-3',
    filter: 'M3 5h18M6 12h12m-8 7h4', menu: 'M4 6h16M4 12h16M4 18h16', close: 'm6 6 12 12M6 18 18 6',
    compact: 'M3 3h4v4H3Zm7 0h4v4h-4Zm7 0h4v4h-4ZM3 10h4v4H3Zm7 0h4v4h-4Zm7 0h4v4h-4ZM3 17h4v4H3Zm7 0h4v4h-4Zm7 0h4v4h-4Z',
    large: 'M4 4h16v16H4Z',
  };
</script>
<script>
  let { name = 'image' } = $props();
</script>

<svg class="pf-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d={paths[name] || paths.image}></path></svg>
