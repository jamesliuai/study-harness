// Resolved local manifests and original license texts. No network or credentials.
import fs from 'node:fs/promises';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const root = process.cwd(),
  out = path.join(root, 'licenses');
await fs.mkdir(out, { recursive: true });
const supplements = JSON.parse(
  await fs.readFile(path.join(out, 'upstream-supplements.json'), 'utf8'),
).supplements;
const records = [];
let text =
  'Study Harness dependency notices\n\nThis inventory includes resolved application, build, optional and platform dependencies for reproducibility. It does not imply every listed dependency is shipped on every platform. Original license text follows each entry when supplied by its package. Public packed visualizer source identity is in vendor/visualizer/manifest.json.\n\n';
async function entry(ecosystem, pkg, dir) {
  const candidates = (await fs.readdir(dir, { withFileTypes: true })).filter(
    (f) => f.isFile() && /^(license|licence|copying|notice|unlicense)([-_.].*)?$/i.test(f.name),
  );
  const originals = [];
  for (const f of candidates) {
    const bytes = await fs.readFile(path.join(dir, f.name));
    if (bytes.length <= 2_000_000)
      originals.push({
        file: f.name,
        sha256: createHash('sha256').update(bytes).digest('hex'),
        text: bytes.toString('utf8'),
      });
  }
  if (!originals.length) {
    for (const supplement of supplements.filter((s) =>
      s.packages.includes(`${pkg.name}@${pkg.version}`),
    ))
      for (const file of supplement.files) {
        originals.push({
          file: `${file.url} (published source commit ${supplement.commit})`,
          sha256: createHash('sha256').update(file.text).digest('hex'),
          text: file.text,
        });
      }
  }
  records.push({
    ecosystem,
    name: pkg.name,
    version: pkg.version,
    authors: pkg.authors ?? pkg.author ?? [],
    license: pkg.license ?? 'See original package',
    license_files: originals.map(({ file, sha256 }) => ({ file, sha256 })),
  });
  text += `===== ${ecosystem}: ${pkg.name} ${pkg.version} (${pkg.license ?? 'see package'}) =====\n${originals.map((o) => o.file + '\n' + o.text).join('\n\n') || 'License expression is recorded in the resolved package manifest; consult the original package for additional notices.'}\n\n`;
}
const seen = new Set();
async function npmPackage(name, parent) {
  let folder = parent;
  let dir;
  while (true) {
    const candidate = path.join(folder, 'node_modules', name);
    try {
      await fs.access(path.join(candidate, 'package.json'));
      dir = candidate;
      break;
    } catch {}
    const next = path.dirname(folder);
    if (next === folder) return;
    folder = next;
  }
  const pkg = JSON.parse(await fs.readFile(path.join(dir, 'package.json'), 'utf8'));
  const identity = `${pkg.name}@${pkg.version}`;
  if (seen.has(identity)) return;
  seen.add(identity);
  await entry('npm', pkg, dir);
  for (const dep of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies }))
    await npmPackage(dep, dir);
}
const manifest = JSON.parse(await fs.readFile('package.json', 'utf8'));
for (const dep of Object.keys(manifest.dependencies)) await npmPackage(dep, root);
const cargo = JSON.parse(
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
    { maxBuffer: 30_000_000, encoding: 'utf8' },
  ),
);
for (const pkg of cargo.packages
  .filter((p) => p.name !== 'study-harness')
  .sort((a, b) => a.name.localeCompare(b.name)))
  await entry('Cargo', pkg, path.dirname(pkg.manifest_path));
for (const folder of ['standard_fonts', 'cmaps'])
  for (const name of await fs.readdir(`node_modules/pdfjs-dist/${folder}`)) {
    if (/license|notice/i.test(name)) {
      const bytes = await fs.readFile(`node_modules/pdfjs-dist/${folder}/${name}`);
      text += `===== PDF.js ${folder}/${name} =====\n${bytes.toString('utf8')}\n\n`;
    }
  }
await fs.writeFile(path.join(out, 'THIRD-PARTY-NOTICES.txt'), text);
await fs.writeFile(
  path.join(out, 'inventory.json'),
  JSON.stringify(
    {
      source: 'local resolved lockfiles/package manifests; original licenses preserved',
      dependencies: records,
    },
    null,
    2,
  ) + '\n',
);
console.log(
  `Recorded ${records.length} dependencies and original available notices (${Buffer.byteLength(text)} bytes).`,
);
