#!/usr/bin/env bash
set -euo pipefail
TASK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$TASK_ROOT"
TASK_PRODUCTION=false
TASK_PROFILE=""
TASK_NO_KEYCHAIN=false
TASK_VERIFY=false
TASK_MEASURE_STARTUP=false
while (($#)); do
  case "$1" in
    --production) TASK_PRODUCTION=true; shift ;;
    --profile) TASK_PROFILE="${2:?Pass a profile name}"; shift 2 ;;
    --no-keychain) TASK_NO_KEYCHAIN=true; shift ;;
    --verify) TASK_VERIFY=true; shift ;;
    --measure-startup) TASK_MEASURE_STARTUP=true; shift ;;
    *) printf 'Usage: %s [--production] [--profile NAME] [--no-keychain] [--measure-startup] [--verify]\n' "$0" >&2; exit 2 ;;
  esac
done
if $TASK_PRODUCTION; then
  TASK_BUNDLE="$TASK_ROOT/src-tauri/target/release/bundle/macos/Study Harness.app"
else
  TASK_BUNDLE="$TASK_ROOT/src-tauri/target/debug/bundle/macos/Study Harness Offline.app"
fi
TASK_BINARY="$TASK_BUNDLE/Contents/MacOS/study-harness"
# Stop only the chosen built bundle. A fresh named profile remains the safest QA path.
while IFS= read -r TASK_PID; do
  TASK_COMMAND="$(ps -p "$TASK_PID" -o comm= 2>/dev/null || true)"
  if [[ "$TASK_COMMAND" == "$TASK_BINARY" ]]; then kill -TERM "$TASK_PID"; fi
done < <(pgrep -x study-harness || true)
if $TASK_PRODUCTION; then npm run package; else npm run package:offline; fi
TASK_ARGS=()
if [[ -n "$TASK_PROFILE" ]]; then TASK_ARGS+=(--profile "$TASK_PROFILE"); fi
if $TASK_NO_KEYCHAIN; then TASK_ARGS+=(--no-keychain); fi
if $TASK_MEASURE_STARTUP; then TASK_ARGS+=(--measure-startup); fi
/usr/bin/open -n "$TASK_BUNDLE" --args "${TASK_ARGS[@]}"
if $TASK_VERIFY; then
  for TASK_TRY in {1..30}; do
    while IFS= read -r TASK_PID; do
      TASK_COMMAND="$(ps -p "$TASK_PID" -o comm= 2>/dev/null || true)"
      if [[ "$TASK_COMMAND" == "$TASK_BINARY" ]]; then
        printf 'Selected bundle process is running. Verify its usable UI separately.\n'
        exit 0
      fi
    done < <(pgrep -x study-harness || true)
    sleep .1
  done
  printf 'Application process did not remain running. Inspect its startup error.\n' >&2
  exit 1
fi
