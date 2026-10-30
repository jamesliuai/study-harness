# Explorable Models integration

Study Harness embeds [Explorable Models](https://github.com/jamesliuai/Explorable-Models) as its actual interactive visualization engine. All computation is local. A separate source checkout or visualization server is not needed to build or run Study Harness.

## Packages and identity

Six `@explorable-models/*` packages are retained as exact tarballs in `vendor/visualizer/packages/`: core, activity, catalog, player, player-3d, and agent. [The manifest](../vendor/visualizer/manifest.json) records their versions, SHA-256 hashes, lock identity, and clean source commit. The current source identity is `ed6b56aed87f9016369a0e9b54d55bed655727c4`, with package version `0.3.0-preview.1`.

`node scripts/vendor-visualizer.mjs <pack-directory>` verifies a complete package manifest, copies the tarballs, and extracts the SDK's native validation/canonicalization code, schemas and fixtures unchanged. Make SDK changes in the Explorable Models repository, rebuild and pack there, then re-vendor here. Do not modify extracted vendor sources directly.

## Learner surface

`src/components/RealVisualPanel.tsx` mounts the player in its embedded dark theme. Installed models cover quadratic gradient descent, function/derivative, 2×2 linear transformation, 3D vector orientation, and quadratic surface gradient. The spatial renderer loads only when needed.

Visuals are attached to relevant capabilities and tasks. Parameters are fixed with each task and validated against the installed model's ranges and constraints. When the model cannot represent a task's exact parameters, the app labels the visual as a contrasting example. A saved answer is required before revealing a solution-bearing result; a question alone is not a prediction.

After an interaction, a bounded tutor-audience summary can accompany the next explicit tutor request. Rust validates its schema, task identity and size. Slider movement alone does not contact a provider. Independent checks hide earlier visuals and do not send their interaction summaries.

## Host and persistence

`src-tauri/src/visualizer.rs` implements the nine HostPort operations against the same SQLite store as the learning record: policy lookup, activity registration, stage transition, exposure intent, display acknowledgement, assistance recording, event append, journal cursor, and record retrieval. It also saves and restores canonical artifacts.

The host validates schema/catalog identity, revisions, parameters, references and digests. Reveals and assistance are durable before display. Interrupted exposure is recovered conservatively. Camera/presentation state stays separate from scientific state and assessment evidence. Hidden panels suspend work, and disposal releases the player and workers.

Saved activities use exact package/content identities. Unsupported or damaged checkpoints are not silently replaced by a new scene; supported historical content can use an archival display. These are provisional, reference-checked models, with independent subject review still pending.

See [architecture](ARCHITECTURE.md), [verification](VERIFICATION.md), and [backup and recovery](RECOVERY.md).
