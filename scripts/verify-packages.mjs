// Read-only bundle verification; writes only an ignored local result manifest.
import { readFile, readdir, lstat, mkdir, writeFile } from 'node:fs/promises';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import path from 'node:path';

const root = process.cwd();
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const execute = (command, args) => {
  const r = spawnSync(command, args, { encoding: 'utf8', cwd: root });
  if (r.status !== 0) throw new Error(`${command} failed: ${r.stderr}`);
  return `${r.stdout}${r.stderr}`.trim();
};
const walk = async (folder, base = folder) => {
  const files = [];
  for (const name of (await readdir(folder)).sort()) {
    const target = path.join(folder, name);
    const info = await lstat(target);
    if (info.isSymbolicLink()) throw new Error(`Unexpected bundle symlink: ${target}`);
    if (info.isDirectory()) files.push(...(await walk(target, base)));
    else
      files.push({
        path: path.relative(base, target),
        bytes: info.size,
        sha256: sha(await readFile(target)),
      });
  }
  return files.sort((a, b) => a.path.localeCompare(b.path, 'en'));
};
const results = [];
for (const [mode, relative, expectedName] of [
  ['offline', 'debug/bundle/macos/Study Harness Offline.app', 'Study Harness Offline'],
  ['normal', 'release/bundle/macos/Study Harness.app', 'Study Harness'],
]) {
  const bundle = path.join(root, 'src-tauri/target', relative);
  const binary = path.join(bundle, 'Contents/MacOS/study-harness');
  const raw = await readFile(binary);
  const source = [
    ...new Set([...raw.toString('latin1').matchAll(/[0-9a-f]{40} \(clean\)/g)].map((m) => m[0])),
  ];
  if (source.length !== 1) throw new Error(`Missing or ambiguous clean build source in ${mode}`);
  execFileSync('git', ['cat-file', '-e', `${source[0].slice(0, 40)}^{commit}`], { cwd: root });
  const changed = execute('git', ['diff', '--name-only', source[0].slice(0, 40)]).split('\n');
  if (
    changed.some((f) =>
      /^(src\/|src-tauri\/|content\/|contracts\/|vendor\/|package(?:-lock)?\.json$)/.test(f),
    )
  )
    throw new Error('Application sources changed since this bundle; rebuild it.');
  const info = JSON.parse(
    execute('plutil', ['-convert', 'json', '-o', '-', path.join(bundle, 'Contents/Info.plist')]),
  );
  if (info.CFBundleName !== expectedName || info.LSMinimumSystemVersion !== '14.0')
    throw new Error(`Wrong package identity/minimum OS in ${mode}`);
  const simulated = raw.includes(Buffer.from('SIMULATED RESPONSE'));
  if (simulated !== (mode === 'offline') || raw.includes(Buffer.from('--credential-smoke')))
    throw new Error(`Development provider/credential boundary mismatch in ${mode}`);
  execute('codesign', ['--verify', '--deep', '--strict', bundle]);
  const signature = execute('codesign', ['-dvv', bundle]);
  if (!signature.includes('adhoc,runtime'))
    throw new Error(`Expected local ad-hoc hardened-runtime signature in ${mode}`);
  const guides = ['PRIVACY.md', 'RECOVERY.md', 'SUPPORT.md', 'EVALUATION-PROTOCOL.md'];
  for (const guide of guides) {
    if (
      sha(await readFile(path.join(root, 'docs', guide))) !==
      sha(await readFile(path.join(bundle, 'Contents/Resources', guide)))
    )
      throw new Error(`Stale bundled guide: ${guide}`);
  }
  for (const entry of await walk(path.join(root, 'licenses'))) {
    if (
      sha(await readFile(path.join(bundle, 'Contents/Resources/licenses', entry.path))) !==
      entry.sha256
    )
      throw new Error(`Stale bundled license: ${entry.path}`);
  }
  const files = await walk(bundle);
  results.push({
    mode,
    path: bundle,
    build_source: source[0],
    total_bytes: files.reduce((sum, f) => sum + f.bytes, 0),
    executable_sha256: sha(raw),
    file_manifest_json_sha256: sha(JSON.stringify(files)),
    hash_method:
      'SHA-256 of UTF-8 JSON.stringify(file_manifest); files sorted by path with en locale',
    product_name: info.CFBundleName,
    bundle_identifier: info.CFBundleIdentifier,
    minimum_os: info.LSMinimumSystemVersion,
    architectures: execute('lipo', ['-archs', binary]),
    signature,
    entitlements: execute('codesign', ['-d', '--entitlements', '-', bundle]),
    fixture_provider_present: simulated,
    credential_smoke_flag_present: false,
    bundled_guides_and_licenses_match: true,
    file_manifest: files,
  });
}
await mkdir(path.join(root, 'output/release'), { recursive: true });
await writeFile(
  path.join(root, 'output/release/packages.json'),
  `${JSON.stringify(results, null, 2)}\n`,
);
for (const { mode, path: bundle, build_source, total_bytes, executable_sha256 } of results)
  console.log(JSON.stringify({ mode, path: bundle, build_source, total_bytes, executable_sha256 }));
