# Architecture

Study Harness is a Tauri 2 desktop application with a React/TypeScript interface and a Rust/SQLite core. Rust owns the durable learning record and validates every state transition; the interface renders projections of that record.

## Main components

| Component | Responsibility |
| --- | --- |
| `src/App.tsx`, `src/components/` | Goals, plans, study conversation, source and visual panels, settings |
| `src-tauri/src/goals.rs` | Goal ownership, immutable plan revisions, capability sessions and progress |
| `src-tauri/src/generation.rs` | Structured plan/task proposals, validation and frozen task material |
| `src-tauri/src/learning.rs` | Learning stages, assistance, assessment reconciliation and spaced reviews |
| `src-tauri/src/checks.rs` | Bounded expression parsing, dimensional analysis and answer checks |
| `src-tauri/src/db.rs`, `ownership.rs` | Transactions, budgets, migrations, archives, restore and deletion |
| `src-tauri/src/providers.rs`, `registry.rs` | Native provider calls, request context, model selection and usage estimates |
| `src-tauri/src/sources.rs` | Original source storage, extraction records, search, selected passages and citations |
| `src-tauri/src/visualizer.rs`, `src/visualizer/host.ts` | Explorable Models host protocol, grants, checkpoints and interaction summaries |
| `src-tauri/src/evaluation.rs` | Separate personal-comparison records and held-out assessments |

## Goals and learning

A goal produces a plan of concrete capabilities. Plan revisions are appended rather than overwriting earlier plans. Unchanged capabilities retain their identity; capabilities with recorded work remain available if a revision removes them from the active plan. Each capability has its own session and evidence.

The authored sample unit and generated tasks use the same learning engine. Tasks, reference solutions, rubrics and submission conditions are fixed before assessment. Attempts are saved before provider requests; assistance is recorded before display. Questions are discussion and never graded. Corrections retain originals and invalidate dependent evidence. Unresolved disputes cannot strengthen progress.

The core reconciles tutor assessments with supported deterministic checks. Conflicting numerical results hold a proposed success. Symbolic comparison samples a stated domain; it is not a proof system. Reviews use a 1/3/7/14-day spacing heuristic. Supported work, independent work, and later demonstrations are recorded separately.

## Native boundaries

Only explicitly allowed Tauri commands are exposed to the local application window. API credentials remain in Rust memory or macOS Keychain; no command retrieves a saved key into the renderer. Provider requests freeze model, prompt, content and policy identities, reserve estimated cost, and retain usage for rejected, cancelled or stale responses. Requests do not retry automatically.

SQLite migrations through schema 9 are transactional. Existing profiles receive a pre-migration backup. A profile lock prevents two application instances from writing the same store. Generated TypeScript IPC types and versioned JSON schemas live in `src/generated/` and `contracts/`.

## Visuals and sources

The actual Explorable Models player runs locally, using pinned package tarballs from `vendor/visualizer/`. Its native validators are extracted unchanged from the SDK. The host governs reveal permissions, assistance records, exact scientific state and saved artifacts. Camera state is separate from scientific state. Independent checks withhold earlier teaching and visuals.

Source imports retain original bytes, hashes and extraction anchors. PDF extraction uses a local PDF.js worker; OCR is not included. Captured passages accompany each relevant request, and citations must quote supplied text. Imported source instructions cannot grant application permissions. Source deletion also removes the sessions and shared plans derived from an attached source.

## Development and testing

`offline-provider` is a development-only feature with visibly simulated responses and separate data. Release builds reject that feature. The offline browser harness forwards UI commands to a separate process running the real Rust store; it does not reimplement learning or persistence in JavaScript.

The desktop application needs no development server once packaged. See [verification](VERIFICATION.md), [visualizer integration](VISUALIZER.md), [privacy](PRIVACY.md), and [recovery](RECOVERY.md).
