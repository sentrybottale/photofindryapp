import { build } from 'esbuild';
import { compile, VERSION } from 'svelte/compiler';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const destination = path.dirname(directory);
const checking = process.argv.includes('--check');
if (process.argv.slice(2).some(argument => argument !== '--check')) throw new Error('Use npm run build or npm run check.');
const sha256 = value => createHash('sha256').update(value).digest('hex');
const css = new Map();
const result = await build({
  absWorkingDir: directory,
  entryPoints: ['src/main.js'],
  outfile: 'ui-framework.js',
  bundle: true,
  write: false,
  format: 'iife',
  platform: 'browser',
  target: ['safari16', 'chrome110', 'firefox115'],
  conditions: ['browser', 'production'],
  define: { 'process.env.NODE_ENV': '"production"', __PF_SVELTE_VERSION__: JSON.stringify(VERSION) },
  minify: true,
  sourcemap: false,
  legalComments: 'inline',
  metafile: true,
  plugins: [{
    name: 'local-svelte-shell',
    setup(builder) {
      builder.onLoad({ filter: /\.svelte$/ }, async ({ path: filename }) => {
        const relative = path.relative(directory, filename).split(path.sep).join('/');
        const compiled = compile(await readFile(filename, 'utf8'), {
          filename: relative, generate: 'client', css: 'external', runes: true,
          dev: false, hmr: false, discloseVersion: false,
        });
        if (compiled.warnings.length) throw new Error(compiled.warnings.map(warning => `${relative}: ${warning.code}: ${warning.message}`).join('\n'));
        if (compiled.css?.code) css.set(relative, compiled.css.code);
        return { contents: compiled.js.code, loader: 'js', resolveDir: path.dirname(filename) };
      });
    },
  }],
});
if (result.outputFiles.length !== 1) throw new Error('The local framework must be one self-contained script.');
const script = result.outputFiles[0].contents;
const stylesheet = Buffer.from([...css.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, source]) => source).join('\n') + '\n');
if (script.length > 96 * 1024 || stylesheet.length > 8 * 1024) throw new Error('The local shell exceeded its reviewed asset budget.');

const runtimePackages = new Set();
for (const input of Object.keys(result.metafile.inputs)) {
  const match = /(?:^|\/)node_modules\/((?:@[^/]+\/)?[^/]+)/.exec(input);
  if (match) runtimePackages.add(match[1]);
}
const licenceFiles = { svelte: 'LICENSE.md', 'esm-env': 'LICENSE', clsx: 'license' };
const runtime = [];
let notices = 'PhotoFindry browser shell: bundled third-party runtime notices\n\nFirst-party browser client code is MIT licensed; proprietary engines have separate terms.\n\n';
for (const name of [...runtimePackages].sort()) {
  if (!licenceFiles[name]) throw new Error(`Review the new browser runtime dependency before bundling: ${name}`);
  const base = path.join(directory, 'node_modules', name);
  const metadata = JSON.parse(await readFile(path.join(base, 'package.json'), 'utf8'));
  const licence = await readFile(path.join(base, licenceFiles[name]), 'utf8');
  runtime.push({ name, version: metadata.version, license: metadata.license });
  notices += `=== ${name} ${metadata.version} (${metadata.license}) ===\n${licence.trim()}\n\n`;
}
const assets = {
  'ui-framework.js': script,
  'ui-framework.css': stylesheet,
  'ui-framework.NOTICES.txt': Buffer.from(notices),
};
const sources = ['package.json', 'package-lock.json', 'scripts/build.mjs', 'src/AppShell.svelte', 'src/Appearance.svelte', 'src/Icon.svelte', 'src/Navigation.svelte', 'src/PhotoSummary.svelte', 'src/main.js'];
const sourceHash = createHash('sha256');
for (const source of sources) { sourceHash.update(`${source}\0`); sourceHash.update(await readFile(path.join(directory, source))); }
const manifest = {
  schemaVersion: 1,
  framework: { name: 'svelte', version: VERSION },
  sourceSha256: sourceHash.digest('hex'),
  runtimeDependencies: runtime,
  assets: Object.fromEntries(Object.entries(assets).map(([name, bytes]) => [name, { bytes: bytes.length, sha256: sha256(bytes) }])),
};
assets['ui-framework.manifest.json'] = Buffer.from(JSON.stringify(manifest, null, 2) + '\n');
for (const [name, bytes] of Object.entries(assets)) {
  const filename = path.join(destination, name);
  if (checking) {
    let current;
    try { current = await readFile(filename); } catch { throw new Error(`${name} is missing. Run npm run build in web-companion/frontend.`); }
    if (!current.equals(bytes)) throw new Error(`${name} is stale. Run npm run build in web-companion/frontend.`);
  } else await writeFile(filename, bytes);
}
process.stdout.write(`${checking ? 'Verified' : 'Built'} Svelte ${VERSION} local shell: ${script.length} JS bytes; ${stylesheet.length} CSS bytes; no runtime imports.\n`);
