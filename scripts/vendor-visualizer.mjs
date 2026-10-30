// Import an explicitly built local package set. No source aliases, dist links, or lifecycle scripts.
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile, copyFile, mkdtemp } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import os from 'node:os';

const source = process.argv[2];
if (!source)
  throw new Error(
    'Usage: node scripts/vendor-visualizer.mjs /path/to/visualizer/release/study-harness/packages',
  );
const destination = path.resolve('vendor/visualizer');
const manifest = JSON.parse(await readFile(path.join(source, 'manifest.json'), 'utf8'));
if (!manifest.complete || manifest.source.dirty)
  throw new Error('Use a complete pack from a clean local source checkpoint.');
await mkdir(path.join(destination, 'packages'), { recursive: true });
for (const entry of manifest.packages) {
  if (!/^[a-z0-9.-]+\.tgz$/.test(entry.file)) throw new Error('Unexpected package filename');
  const file = path.join(source, entry.file);
  const hash = createHash('sha256')
    .update(await readFile(file))
    .digest('hex');
  if (hash !== entry.sha256) throw new Error(`Hash mismatch: ${entry.name}`);
  await copyFile(file, path.join(destination, 'packages', entry.file));
}
await writeFile(path.join(destination, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
const run = (cmd, args) => {
  const result = spawnSync(cmd, args, { stdio: 'inherit' });
  if (result.status !== 0) throw new Error(`${cmd} failed`);
};
const activity = manifest.packages.find((p) => p.name === '@explorable-models/activity');
await mkdir(path.join(destination, 'activity'), { recursive: true });
run('tar', [
  '-xzf',
  path.join(destination, 'packages', activity.file),
  '-C',
  path.join(destination, 'activity'),
  '--strip-components=1',
  'package/rust',
  'package/schema',
  'package/LICENSE',
  'package/fixtures',
]);
const cache = await mkdtemp(path.join(os.tmpdir(), 'study-harness-npm-'));
run('npm', [
  'install',
  '--ignore-scripts',
  '--cache',
  cache,
  ...manifest.packages.map((p) => `./vendor/visualizer/packages/${p.file}`),
  'three@0.186.1',
]);
console.log(
  `Installed six packed packages from ${manifest.source.commit}; native sources extracted unchanged from the activity tarball.`,
);
