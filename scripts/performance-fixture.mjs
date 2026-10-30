// Creates only synthetic, explicitly offline records through the real store. Never points at a live profile.
import { OfflineCore } from '../tests/core-server.mjs';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
const folder = await mkdtemp(path.join(os.tmpdir(), 'study-harness-performance-'));
const core = new OfflineCore(path.join(folder, 'offline.sqlite3'));
core.start();
try {
  const seed = await core.request('test.seedHistory');
  if (!seed.ok) throw new Error(JSON.stringify(seed.error));
  const exported = await core.request('test.export');
  if (!exported.ok) throw new Error(JSON.stringify(exported.error));
  await mkdir('output/performance', { recursive: true });
  await writeFile(
    'output/performance/synthetic-history.sharchive',
    Buffer.from(exported.result, 'base64'),
    { mode: 0o600 },
  );
  await writeFile(
    'output/performance/fixture.json',
    JSON.stringify(
      {
        kind: 'synthetic offline engineering fixture',
        sessions: seed.result.sessions.length,
        attempts: 1000,
        sources: 3,
        database: core.database,
        archive: path.resolve('output/performance/synthetic-history.sharchive'),
      },
      null,
      2,
    ),
  );
  console.log(
    'Created output/performance/synthetic-history.sharchive with 51 synthetic sessions and 1000 saved attempts.',
  );
} finally {
  core.close();
}
