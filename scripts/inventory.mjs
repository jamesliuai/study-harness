import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
const root = new URL('../', import.meta.url);
const lock = JSON.parse(fs.readFileSync(new URL('package-lock.json', root)));
const npm = Object.entries(lock.packages)
  .filter(([p]) => p)
  .map(([path, p]) => ({
    name: p.name ?? path.split('node_modules/').at(-1),
    version: p.version,
    license: p.license ?? 'See package license',
    dev: !!p.dev,
    source: p.resolved,
  }));
const metadata = JSON.parse(
  execFileSync(
    'cargo',
    [
      'metadata',
      '--manifest-path',
      'src-tauri/Cargo.toml',
      '--format-version',
      '1',
      '--locked',
      '--offline',
    ],
    { cwd: root, maxBuffer: 20 * 1024 * 1024 },
  ),
);
const rust = metadata.packages
  .filter((p) => p.name !== 'study-harness')
  .map((p) => ({
    name: p.name,
    version: p.version,
    license: p.license ?? 'See crate license',
    source: p.source,
  }));
fs.writeFileSync(
  new URL('docs/dependencies.json', root),
  JSON.stringify(
    {
      generated: new Date().toISOString().slice(0, 10),
      note: 'Locked dependency inventory; includes build/dev and cross-platform transitive crates, not only shipped code. Public distribution needs license notice review.',
      npm,
      rust,
    },
    null,
    2,
  ) + '\n',
);
const lines = [
  '# Dependency inventory',
  '',
  'Exact source checksums/resolutions are in package-lock.json and src-tauri/Cargo.lock. Machine-readable full inventory: dependencies.json. Includes development/build and cross-platform dependencies; this is not a claim all listed crates ship in the macOS binary.',
  '',
  '| Ecosystem | Package | Version | Declared license |',
  '|---|---|---|---|',
];
for (const [eco, items] of [
  ['npm', npm],
  ['Cargo', rust],
])
  for (const x of items)
    lines.push(`| ${eco} | ${x.name} | ${x.version} | ${x.license.replaceAll('|', ' / ')} |`);
fs.writeFileSync(new URL('docs/DEPENDENCIES.md', root), lines.join('\n') + '\n');
console.log(`${npm.length} npm packages, ${rust.length} Rust crates inventoried`);
