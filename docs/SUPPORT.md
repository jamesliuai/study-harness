# Support and troubleshooting

Study Harness is an early macOS preview. Start with your own goal and **Make a plan**, or open **Try the sample unit** to explore authored content without an API key. The sidebar retains your sessions; each planned capability has its own study record.

## Common problems

- **No API key:** the sample unit's tasks, hints, examples, visuals and local saving work without a key. New plans, generated tasks, tutor replies and transcription require a key in **Settings → Tutor**. The offline development build deliberately uses fixtures instead.
- **A request exceeds the budget:** inspect the recorded usage and limits in Settings. Pending or uncertain charges retain reservations. Change a limit or retry deliberately; other profiles and other uses of the key have separate spending.
- **A provider request fails:** the saved answer remains local. Incomplete, rejected or stale replies are not silently applied, and live requests never fall back to simulated responses. Check the connection and retry when ready.
- **A task or grade looks wrong:** correct your answer, challenge the assessment, or report a problem with the task. A held verdict means the tutor's proposed success disagreed with an automatic check. Corrections retain the original and invalidate dependent evidence.
- **A scanned PDF has no useful text:** OCR is not included. Preview the original pages, verify equation extraction, or paste corrected text as a separate source.
- **A citation seems misleading:** the app verifies that a quoted passage was supplied, not that the passage is scientifically correct. Check the original material and the task's assumptions.
- **Microphone access is denied:** type instead, or enable Study Harness in macOS **System Settings → Privacy & Security → Microphone**. Transcription is a separate explicit action. Review and confirm the editable transcript before submission.
- **A visual is disabled or absent:** only relevant installed models are attached to tasks. A saved answer is required before revealing results. Earlier teaching and visuals are hidden during independent checks.
- **A visual checkpoint cannot be restored:** preserve the archive and use a compatible app version. The core will not silently replace damaged or unsupported scientific state with a new scene.
- **Restore is refused:** use a fresh profile of the same normal/offline mode, verify the passphrase, and retain the original archive. See [recovery](RECOVERY.md).
- **The profile is already open:** quit the other instance or choose another named profile. Only one writer may use the same database.

## Display and accessibility

Settings includes text-size and review-workload controls. The interface supports keyboard navigation, exact numeric entries, math text alternatives, and narrow layouts. Visual models expose tables and numeric descriptions alongside plots. Automated checks are described in [verification](VERIFICATION.md); a complete human assistive-technology audit remains outstanding.

## Report a problem

Use the [GitHub issue tracker](https://github.com/jamesliuai/study-harness/issues). Include the app version, macOS version, normal/offline mode, the smallest reproducible steps, and expected versus observed behavior. Prefer a synthetic example or redacted screenshot. Do not include API keys, passphrases, private notes, or a full study archive.

See [building and packaging](RELEASE.md), [privacy](PRIVACY.md), and [backup and recovery](RECOVERY.md). Bundled third-party notices are in `Contents/Resources/licenses/THIRD-PARTY-NOTICES.txt`; the app's MIT license is in `Contents/Resources/LICENSE`.
