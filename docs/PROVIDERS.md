# Provider configuration

Study Harness uses a native OpenAI adapter for planning, task generation, tutoring, and transcription. Configure your key in **Settings → Tutor**. Saving a key does not make a provider request. You can keep it in memory for the launch or explicitly save it in macOS Keychain.

Model and reasoning-effort choices are available in Settings and beside the composer. The application registry is defined in `src-tauri/src/registry.rs`; Sol with low effort is the default tutor selection. Plans and tasks use at least medium effort. Each request keeps the model and effort it was prepared with, even if the preference changes before completion. Actual account access and live output quality require verification with the intended API account.

## Requests and usage

Structured responses are parsed and validated by Rust before they can change the learning record. The task, rubric and bounded context accompany the saved response. New goal plans and task-writing requests also carry relevant plan information and selected source passages. The interface can cancel a request or retry deliberately; failures never fall back to simulated responses.

Default limits are $1 per session and $10 per UTC calendar month, scoped to a local profile. Pending requests reserve an estimated cost. Rejected, incomplete, cancelled or stale responses can still consume usage, so recorded usage is kept independently of whether the content was accepted. The versioned rate registry is an estimate, not an account billing guarantee. Other profiles and other uses of the same key are outside these limits.

Requests use `store:false`. This is a request setting, not a claim of zero provider retention. See [privacy](PRIVACY.md) for what leaves the device and how credentials and recordings are handled.

## Without a key

The authored sample unit's tasks, hints, examples, visuals and local saving remain usable. Planning new topics, live feedback, and transcription require a configured key.

The separate offline development build supplies explicitly labelled fixture responses through the real validation and storage pipeline. It does not access credentials or make provider calls, and its data cannot be restored as real evidence in a production profile.
