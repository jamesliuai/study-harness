// Resolve exact installed content at build time. No model code is evaluated in Node.
import { createInstalledCatalog } from '@explorable-models/catalog';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
const catalog = createInstalledCatalog();
if (catalog.problems().length) throw new Error('The installed catalog has invalid entries.');
const templates = catalog.listTemplates().map((entry) => {
  const template = catalog.getTemplate(entry.template);
  const manifest = catalog.getModel(entry.model);
  const identity = catalog.identity(entry.template);
  if (!template || !manifest || identity?.kind !== 'template')
    throw new Error('Incomplete installed content');
  return {
    template: entry.template,
    model: entry.model,
    status: entry.status,
    initialStage: template.initialStage,
    title: template.title,
    groups: template.groups.map((g) => ({
      groupId: g.id,
      solutionBearing: g.solutionBearing,
      contentDigest: identity.groups.find((d) => d.groupId === g.id).contentDigest,
    })),
    stages: template.stages.map((s) => ({
      id: s.id,
      kind: s.kind,
      visible: Object.keys(s.visible),
      response: s.response?.kind ?? null,
    })),
    transitions: template.transitions,
    parameters: Object.fromEntries(
      template.controls.map((c) => [
        c.parameter,
        [manifest.parameters[c.parameter].min, manifest.parameters[c.parameter].max],
      ]),
    ),
    // Full exact content belongs to the portable backup, not to the tutoring retrieval index.
    content: { template, manifest },
  };
});
mkdirSync('src-tauri/visualizer', { recursive: true });
const text =
  JSON.stringify(
    { schema: 'study-harness.installed-visualizer/1', provisional: true, templates },
    null,
    2,
  ) + '\n';
writeFileSync('src-tauri/visualizer/catalog.json', text);
const packed = JSON.parse(readFileSync('vendor/visualizer/manifest.json', 'utf8'));
for (const p of packed.packages) {
  const hash = createHash('sha256')
    .update(readFileSync(`vendor/visualizer/packages/${p.file}`))
    .digest('hex');
  if (hash !== p.sha256) throw new Error(`Changed packed dependency: ${p.name}`);
}
console.log(
  `Resolved ${templates.length} provisional reference-checked visual templates from ${packed.source.commit}.`,
);
