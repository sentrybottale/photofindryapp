import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { fileURLToPath } from 'node:url';

test('responsive evidence rejects foreign origins, rebinding and malformed reports, then recovers', { timeout: 15000 }, async t => {
  const output = await mkdtemp(path.join(tmpdir(), 'pf-layout-report-'));
  const child = spawn(process.execPath, [fileURLToPath(new URL('../scripts/serve-responsive-fixture.mjs', import.meta.url)), output], { stdio: ['ignore', 'pipe', 'pipe'] });
  t.after(async () => { if (child.exitCode === null && child.signalCode === null) { const ended = once(child, 'exit'); child.kill('SIGTERM'); await ended; } await rm(output, { recursive: true, force: true }); });
  const launchURL = await new Promise((resolve, reject) => {
    let text = ''; child.on('error', reject); child.once('exit', () => reject(Error('Fixture server exited')));
    child.stdout.on('data', chunk => { text += chunk; if (text.includes('\n')) { try { resolve(new URL(JSON.parse(text.split('\n')[0]).url)); } catch (error) { reject(error); } } });
  });
  const origin = launchURL.origin, capability = new URLSearchParams(launchURL.hash.slice(1)).get('fixture');
  const report = { cases: ['light', 'dark'].flatMap(theme => [320,390,768,1024,1440].flatMap(width => ['gallery','files','settings','jobs','preview'].map(surface => ({ theme, width, surface, passed: true })))) };
  async function request(body, headers = {}) {
    return new Promise((resolve, reject) => {
      const req = http.request(origin+'/report', { method: 'POST', headers: { Origin: origin, 'X-PhotoFindry-Fixture': capability, 'Content-Type': 'application/json', ...headers } }, res => { res.resume(); res.on('end', () => resolve(res.statusCode)); });
      req.on('error', reject); req.end(body);
    });
  }
  const bytes = JSON.stringify(report);
  assert.equal(await request(bytes, { Origin: 'https://attacker.invalid', 'Content-Type': 'text/plain' }), 403);
  assert.equal(await request(bytes, { Origin: '' }), 403);
  assert.equal(await request(bytes, { 'X-PhotoFindry-Fixture': '' }), 403);
  assert.equal(await request(bytes, { 'X-PhotoFindry-Fixture': '0'.repeat(64) }), 403);
  assert.equal(await request(bytes, { Host: 'attacker.invalid' }), 403);
  assert.equal(await request(bytes, { 'Content-Type': 'text/plain' }), 415);
  assert.equal(await request(bytes, { 'Sec-Fetch-Site': 'cross-site' }), 403);
  await assert.rejects(readFile(path.join(output, 'responsive-report.json')), { code: 'ENOENT' });
  assert.equal(await request('{'), 400);
  assert.equal(await request(JSON.stringify({ cases: Array(50).fill(report.cases[0]) })), 400);
  assert.equal(await request(bytes), 200);
  const saved = await readFile(path.join(output, 'responsive-report.json'), 'utf8');
  assert.deepEqual(JSON.parse(saved), report);
  assert.equal(await request(JSON.stringify({ cases: Array(50).fill({ passed: true }) }), { Origin: 'https://attacker.invalid' }), 403);
  assert.equal(await readFile(path.join(output, 'responsive-report.json'), 'utf8'), saved);
  const page = await fetch(origin);
  assert.match(page.headers.get('content-security-policy'), /frame-ancestors 'self'/);
  assert.equal(page.status, 200);
});
