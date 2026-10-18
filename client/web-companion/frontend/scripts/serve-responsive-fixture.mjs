// Run with Node, then open its loopback URL in a browser through CUA. This server
// never starts or connects to a companion and serves only fixed test assets.
import http from 'node:http';
import { readFile, mkdir, writeFile, lstat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
const frontend = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const web = path.dirname(frontend), fixture = path.join(frontend, 'tests/fixtures');
const output = path.resolve(process.argv[2] || path.join(web, '../.private/responsive-qa-2026-10-17'));
const requestedPort = process.argv[3] === undefined ? 0 : Number(process.argv[3]);
if (!Number.isSafeInteger(requestedPort) || requestedPort < 0 || requestedPort > 65535) throw new Error('Choose an optional loopback port from 0 to 65535.');
await mkdir(output, { recursive: true, mode: 0o700 });
const outputInfo = await lstat(output);
if (!outputInfo.isDirectory() || outputInfo.isSymbolicLink() || outputInfo.uid !== process.getuid() || (outputInfo.mode & 0o077)) throw new Error('Choose an owner-only fixture output directory (mode 0700).');
const capability = randomBytes(32).toString('hex');
const validCapability = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value) && timingSafeEqual(Buffer.from(value), Buffer.from(capability));
const routes = new Map();
for (const name of ['style.css', 'workspace.css', 'panels.css', 'model-monitor.css', 'onboarding.css', 'ui-framework.css', 'design-system.css', 'photo-presentation.css', 'product-theme.css', 'ui-framework.js', 'workspace.js', 'panels.js', 'model-monitor.js']) routes.set('/' + name, await readFile(path.join(web, name)));
for (const [route, name] of [['/', 'responsive-harness.html'], ['/case', 'responsive.html'], ['/harness.css', 'responsive-harness.css'], ['/harness.js', 'responsive-harness.js'], ['/fixture.js', 'responsive.js'], ['/fixture-ready.js', 'responsive-ready.js'], ['/fixture-checks.js', 'responsive-checks.js']]) routes.set(route, await readFile(path.join(fixture, name)));
// A tiny same-origin placeholder avoids using or reading the owner's icon files.
routes.set('/brand.png', Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="38" height="38"><rect width="38" height="38" fill="#e8eddf"/><path d="M6 7H26V25H6ZM23 24L32 33" fill="none" stroke="#193148" stroke-width="4"/></svg>'));
await writeFile(path.join(output, 'fixture-assets.json'), JSON.stringify({ assets: Object.fromEntries([...routes].map(([route, bytes]) => [route, { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }])) }, null, 2) + '\n');
const expectedCases = new Set(['light', 'dark'].flatMap(theme => [320, 390, 768, 1024, 1440].flatMap(width => ['gallery', 'files', 'settings', 'jobs', 'preview'].map(surface => `${theme}:${width}:${surface}`))));
async function handle(request, response) {
  const host = `127.0.0.1:${server.address().port}`, origin = `http://${host}`;
  const singleton = name => request.headersDistinct[name]?.length === 1 ? request.headersDistinct[name][0] : undefined;
  if (singleton('host') !== host || (request.headers.origin !== undefined && singleton('origin') !== origin)
      || (request.headers['sec-fetch-site'] && !['same-origin', 'none'].includes(request.headers['sec-fetch-site']))) {
    response.writeHead(403).end(); return;
  }
  if (!request.url.startsWith('/') || request.url.startsWith('//')) { response.writeHead(400).end(); return; }
  const requested = new URL(request.url, 'http://127.0.0.1');
  const route = requested.pathname;
  if (request.method === 'POST' && route === '/report') {
    if (singleton('origin') !== origin || !validCapability(singleton('x-photofindry-fixture'))) { response.writeHead(403).end(); return; }
    if (singleton('content-type') !== 'application/json') { response.writeHead(415).end(); return; }
    let bytes = 0, chunks = [];
    for await (const chunk of request) { bytes += chunk.length; if (bytes > 256 * 1024) { response.writeHead(413).end(); return; } chunks.push(chunk); }
    const value = JSON.parse(Buffer.concat(chunks));
    if (!value || !Array.isArray(value.cases) || value.cases.length !== 50) { response.writeHead(400).end(); return; }
    const seen = new Set();
    for (const item of value.cases) {
      const key = item && `${item.theme}:${item.width}:${item.surface}`;
      if (!expectedCases.has(key) || seen.has(key) || typeof item.passed !== 'boolean') { response.writeHead(400).end(); return; }
      seen.add(key);
    }
    await writeFile(path.join(output, 'responsive-report.json'), JSON.stringify(value, null, 2) + '\n');
    console.log(JSON.stringify({ report: path.join(output, 'responsive-report.json'), passed: value.cases.filter(item => item.passed).length, total: value.cases.length }));
    response.writeHead(200, { 'Content-Type': 'application/json' }).end('{"saved":true}'); return;
  }
  if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405).end(); return; }
  let body = routes.get(route); if (!body) { response.writeHead(404).end('Fixture resource unavailable'); return; }
  if (route === '/case') {
    const theme = requested.searchParams.get('theme') === 'dark' ? 'dark' : 'light';
    body = Buffer.from(body.toString().replaceAll('__FIXTURE_APPEARANCE__', theme));
  }
  const type = route.endsWith('.css') ? 'text/css' : route.endsWith('.js') ? 'text/javascript' : route === '/brand.png' ? 'image/svg+xml' : 'text/html';
  response.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-store', 'Referrer-Policy': 'no-referrer', 'Content-Security-Policy': "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; frame-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'none'" }).end(request.method === 'HEAD' ? undefined : body);
}
const server = http.createServer((request, response) => {
  handle(request, response).catch(() => { if (!response.headersSent) response.writeHead(400); response.end(); });
});
server.requestTimeout = 10000; server.headersTimeout = 10000; server.maxConnections = 8;
server.setTimeout(10000, socket => socket.destroy());
server.listen(requestedPort, '127.0.0.1', async () => { const url = `http://127.0.0.1:${server.address().port}/#fixture=${capability}`; await writeFile(path.join(output, 'fixture-server.json'), JSON.stringify({ url, output, pid: process.pid }, null, 2) + '\n', { mode: 0o600 }); console.log(JSON.stringify({ url, output, assetsFrozenInMemory: true })); });
