import fs from 'node:fs';
import assert from 'node:assert/strict';
const core = fs.readFileSync('src-tauri/src/lib.rs', 'utf8');
const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json'));
const cap = JSON.parse(fs.readFileSync('src-tauri/capabilities/main.json'));
const commands = [...core.matchAll(/#\[tauri::command\][\s\S]*?(?:async\s+)?fn\s+(\w+)/g)].map(
  (x) => x[1],
);
assert(!commands.some((x) => /(get|read|retrieve).*(key|credential|secret)/i.test(x)));
assert.deepEqual(cap.windows, ['main']);
assert.equal(cap.local, true);
assert(!cap.remote);
assert(cap.permissions.every((p) => p.startsWith('allow-') && !/shell|http|fs:/.test(p)));
assert.deepEqual(
  [...cap.permissions].sort(),
  commands.map((name) => `allow-${name.replaceAll('_', '-')}`).sort(),
);
assert(config.app.security.csp.includes("frame-src 'none'"));
assert(!config.app.security.csp.includes('https:'));
for (const name of fs.readdirSync('dist/assets').filter((x) => x.endsWith('.js'))) {
  const s = fs.readFileSync(`dist/assets/${name}`, 'utf8');
  assert(!s.includes('fixture-session'));
  assert(!s.includes('fixture-only'));
  assert(!s.includes('api.openai.com'));
  assert(!s.includes('not-an-api-key'));
}
const contracts = [
  'Session',
  'Activity',
  'Attempt',
  'Assistance',
  'Assessment',
  'Review',
  'Source',
  'Request',
  'Usage',
];
for (const name of contracts) {
  const schema = JSON.parse(
    fs.readFileSync(
      `contracts/${name}.v${['Assessment', 'Review', 'Usage'].includes(name) ? 2 : 1}.schema.json`,
    ),
  );
  assert(schema.properties.schema_version);
  assert(schema.required.includes('schema_version'));
  assert.equal(schema.additionalProperties, false);
}
const workspaceSchema = JSON.parse(fs.readFileSync('contracts/Workspace.v8.schema.json'));
assert(workspaceSchema.properties.pending_transcript);
assert(workspaceSchema.properties.offline);
assert(workspaceSchema.properties.learning);
for (const name of ['unit', 'sources', 'privacy']) assert(workspaceSchema.required.includes(name));
assert(!commands.includes('reveal_scene') && !commands.includes('save_scene'));
assert(config.app.security.csp.includes("worker-src 'self'"));
assert(!/unsafe-eval|unsafe-inline/.test(config.app.security.csp));
assert(core.includes('profile_lock.try_lock()'));
assert(/compile_error!\(\s*"offline-provider is development-only/.test(core));
assert(core.includes('provider: if offline'));
assert(core.includes('Live transcription is disabled in offline development'));
assert(core.includes('Credentials are disabled in offline development'));
assert.equal(workspaceSchema.additionalProperties, false);
console.log(
  JSON.stringify(
    {
      commands,
      credentialRetrievalCommand: false,
      rendererProviderEndpoint: false,
      browserMockInProduction: false,
      offlineProviderReleaseGuard: true,
      requiredVersionedSchemas: contracts.length,
      capability: 'local main only',
    },
    null,
    2,
  ),
);
