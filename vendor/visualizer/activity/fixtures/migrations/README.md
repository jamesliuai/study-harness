# Artifact migration fixtures

Inputs and expected outputs for `migrateArtifact` and `verifyArtifact` (`@explorable-models/activity`, report §14,
requirement A49). They are fixture data for compatibility tests, not teaching content and not real learner records.
`tests/activity/migrations.test.ts` checks that the deterministic files below are current and regenerates every file
with `UPDATE_MIGRATION_FIXTURES=1 npx vitest run tests/activity/migrations.test.ts`.

## Draft 0.9 format

`study-harness.activity-artifact` 0.9 is a pre-release draft that never shipped as a supported format. It is defined
here so the migration machinery is exercised end to end. It has the 1.0 shape except:

- exposure records are stored under `reveals` (1.0: `exposures`);
- there is no `pending` field: the draft kept only the committed checkpoint and could not record a pending preview;
- there is no `migratedFrom` field;
- the `schemas` entry for the artifact contract says `0.9` (nested records are already 1.0);
- the integrity manifest uses the 1.0 algorithm over the draft fields in this order: `contract`, `schemaVersion`,
  `artifactId`, `kind`, `schemas`, `runtime`, `template`, `model`, `policyRef`, `instance`, `scientific`,
  `presentation`, `reveals`, `journal`, `contentRefs`, `reviewRefs`.

The 0.9 → 1.0 migration is format-only (`semanticsChanged: false`). It renames `reveals`, records `pending` as none,
updates the schema entry, assigns the deterministic artifact ID `migrated-<first 24 hex digits of the original
digest>`, retains the original bytes digest and the change report in `migratedFrom`, and recomputes integrity.

## Files

| File | Provenance | Expected handling |
| --- | --- | --- |
| `draft-0.9-artifact.json` | Derived from the 1.0 contract exemplar `backupArtifact` (`fixtures-src/exemplars.ts`) by the inverse of the mapping above | Migrates to `draft-0.9-migrated-1.0.json` |
| `draft-0.9-migrated-1.0.json` | Assembled in the test from the exemplar and the draft bytes, independently of the migration code | Exact expected output, byte for byte |
| `draft-0.9-corrupted.json` | The draft with `scientific.params.eta` changed and the integrity manifest left as it was | Rejected: `INTEGRITY_FAILURE` at `$.scientific` |
| `draft-0.9-truncated.partial` | The first 60 % of the draft bytes (not valid JSON, hence the extension) | Rejected: `INTEGRITY_FAILURE`, not complete JSON |
| `draft-0.9-missing-blob.json` | The draft without its `scientific` content while the integrity manifest still references it | Rejected: `INTEGRITY_FAILURE`, missing content |
| `future-2.0.json` | The 1.0 exemplar relabelled as version 2.0 | Rejected: `UNSUPPORTED_ARTIFACT` with a recovery that keeps the original bytes |
| `legacy-document-2.0.json` | Exported by the schema 2.0 document session (`exportDocument`) from the `riemann-sums` preset | Routed to `document-session`; never converted |
| `legacy-snapshot-1.0.json` | Exported by the schema 1.0 runtime (`ExperimentRuntime.checkpoint`) from the test's motion definition | Routed to `source-runtime`; never converted |

The legacy exports carry run IDs and timestamps, so they are regenerated only on request; the test checks that their
own retained runtimes still read them.
