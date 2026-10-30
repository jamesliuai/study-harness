# Verification

The public preview has automated coverage of its core and targeted browser checks. These checks establish software behavior with authored content and simulated provider responses; they do not establish live model accuracy, speech quality, or improved learning.

## Current checks

| Check | Latest local result |
| --- | --- |
| Production TypeScript/Vite build | Passed |
| Vitest | 9 tests passed |
| Rust core, normal and offline configurations | 72 tests passed in each |
| Clippy with warnings denied | Passed in normal and offline/dev-tools configurations |
| Prettier and rustfmt | Passed |
| Permission/schema/provider boundary audit | Passed |
| Packed visualizer SHA-256 identities | All six match the manifest |
| Current production UI with real isolated Rust/SQLite core | Targeted workflow passed |

The current UI check covers prediction-before-reveal, saving an answer, the actual visualization player, keyboard parameter changes, authored help, saved checkpoint restoration, and hiding earlier teaching and visuals for independent checks. Desktop and narrow layouts were inspected. A separate goal flow covered source attachment, plan revision, a generated numerical task, and a held verdict when the automatic check disagreed with simulated tutor success.

The source-deletion regression verifies that deleting an attached source removes every session sharing its plan, including an empty capability session, while retaining an unrelated session.

## Reproduce

```sh
npm run check
npm run format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --features offline-provider
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets \
  --features dev-tools,offline-provider -- -D warnings
node scripts/verify-boundaries.mjs
```

To capture and verify the README's actual application workflow, use the installed Playwright Chromium browser:

```sh
npm run build
cargo build --manifest-path src-tauri/Cargo.toml \
  --bin offline-harness --features dev-tools,offline-provider
node scripts/capture-readme.mjs
```

The capture script starts a temporary local store, renders the production frontend under the configured content security policy, captures a 3200 × 2000 image, checks restart and an independent check, and closes its browser/server. It does not use personal data or credentials.

## Remaining coverage

Some older specs in `tests/e2e/` still target the interface before the goal-based redesign. Their historical results are not a current full-suite pass; migrate those workflows before using `npx playwright test` as a release gate.

Live planning/grading/transcription quality, knowledgeable subject review, human assistive-technology testing, native dialogs and foreground performance, and signed/notarized clean-machine installation remain unverified. The [personal comparison protocol](EVALUATION-PROTOCOL.md) describes how learning outcomes can be collected separately from software checks.
