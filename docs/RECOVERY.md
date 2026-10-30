# Backups, profiles and recovery

Use Settings → Your data and recovery. Save a portable archive for a complete unencrypted copy, or enter a passphrase of at least 12 characters and save a protected backup. Both include a coordinated SQLite snapshot, original source bytes, frozen tasks and assistance, exact native visual artifacts, the authored content, and all six packed visualizer dependencies with their identities/hashes. Credentials and raw recordings are excluded. An in-flight request remains pending in the snapshot and becomes interrupted with conservative estimated usage when restored.

Restore into a fresh profile of the same mode. Quit the app, then use its executable with a new profile name:

```sh
"src-tauri/target/debug/bundle/macos/Study Harness Offline.app/Contents/MacOS/study-harness" --profile restore-1
```

For the normal app, use `src-tauri/target/release/bundle/macos/Study Harness.app` instead. Profile names allow 1–64 letters, numbers, hyphens or underscores. The default production database remains in `~/Library/Application Support/dev.studyharness.desktop/`; offline development uses its `offline-development/` child. Named profiles use `profiles/<name>/` inside the appropriate mode directory. A profile with study work, drafts, sources, requests, transcripts or comparison records cannot be overwritten by restore. Blank initial scaffolding is allowed.

Choose Restore selected archive and enter the passphrase for a protected backup. The core bounds input, checks the complete file set, hashes, exact supported public content/package identities, SQLite schema objects, integrity/foreign keys, record identities, source context/citations and visual checkpoint references before adopting it. Unknown versions, altered schema objects and simulated evidence entering production are refused. Imported code and packages are never executed. This release adopts its known content set; an archive from an unsupported content version must be kept for the application that supports it. A portable archive is a user-supplied record, not a certificate of learning or human approval.

Restore stages and validates the snapshot beside the profile, preserves the fresh database until adoption succeeds, and reopens through the normal migration/recovery path. A malformed or wrong-passphrase archive leaves the existing profile intact. Derived summaries and review decisions are rebuilt from primary evidence on restore, so an archive cannot supply an inflated progress counter. Interrupted private staging copies are removed when that profile next opens or data is intentionally deleted. Do not manually replace an open SQLite file or copy only its journal. Export through the app for a coordinated copy.

Schema upgrades are ordered transactions. Before upgrading an existing schema, the app creates a private `backups/pre-schema-<version>-<id>.sqlite3` snapshot. A future schema is refused. These automatic backups are unencrypted and local; export a protected backup before updating if desired. Intentional deletion removes these app-managed historical copies. Keep user-controlled exported backups elsewhere if you need independent recovery.

A missing or damaged visual checkpoint fails closed with a recovery message. It does not silently substitute a new scene. Unknown or revoked visual content may be shown as an archival record where supported. Preserve the archive and install the version matching its recorded content. Authorization without display acknowledgement is conservatively recovered as possible exposure; it is never erased to improve an independence label.

To test recovery safely, use a new named offline profile and synthetic data, save an archive, quit/reopen, restore into another fresh offline profile, and compare original responses, citations, artifact state and pending/estimated usage. Do not use live learner data as a development fixture. A protected backup should also be tested once with its intended passphrase before relying on it.
