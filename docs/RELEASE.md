# Building and packaging

Study Harness is a source-code preview for macOS 14 or later. Development has been exercised on Apple Silicon. The MIT license covers original code; dependencies retain their own notices.

## Build from a checkout

Install Node.js 22.12 or later, Rust/Cargo with Rust 2024 edition support, and Xcode Command Line Tools. From the repository root:

```sh
npm ci --ignore-scripts
npm run check
npm run package
```

The normal application is written to `src-tauri/target/release/bundle/macos/Study Harness.app`:

```sh
open "src-tauri/target/release/bundle/macos/Study Harness.app"
```

For development, use `npm run desktop`. The packaged app contains its frontend, local workers and fonts, visualization packages, user guides, and license notices. It does not depend on a running Vite server or the Explorable Models source repository.

## Offline development

```sh
npm run desktop:offline
# Or build an explicitly labelled debug bundle:
npm run package:offline
open "src-tauri/target/debug/bundle/macos/Study Harness Offline.app"
```

Offline mode has separate data and simulated provider responses. Release compilation with `offline-provider` is intentionally refused. `script/build_and_run.sh` is a convenience launcher; its default is offline, and `--production` selects the normal application.

Use an isolated profile when testing:

```sh
open -n "src-tauri/target/release/bundle/macos/Study Harness.app" \
  --args --profile release-check --no-keychain
```

Only one writer may open a profile. `--no-keychain` avoids saved-key access for that launch.

## Distribution status

Local bundles use ad-hoc signing. Developer ID signing, notarization and installation on a clean supported Mac have not been completed. This repository does not ship a notarized installer or an automatic updater.

Before distributing a signed binary, build an exact candidate, verify its embedded resources and signatures, notarize and staple it with the owner's signing setup, and test installation, launch, microphone permissions, backup/restore and upgrades on a clean Mac. Keep the existing [verification limits](VERIFICATION.md) visible.

For manual updates, export a tested backup, quit the old app, retain its bundle and compatible backup, then install and check the new build. Never downgrade a migrated database into an older application. See [recovery](RECOVERY.md).
