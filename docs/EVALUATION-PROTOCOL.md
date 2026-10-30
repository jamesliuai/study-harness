# Personal comparison protocol v1

No participants, reviews, outcomes or benefits have been collected by this implementation. Offline records are synthetic engineering evidence. Live model quality, subject review and the personal comparison remain external gates.

## Before study begins

Prepare genuinely distinct matched topics from actual unfamiliar material. Six to ten pairs over several weeks is an exploratory planning target, not a powered population experiment. Several objectives in the one gradient-descent lesson are not independent topic pairs. Extend real material when the current unit cannot supply comparable independent topics. Record why the topics match, prior knowledge and relevant sources. Have a subject-competent person check the items, assumptions, solutions, alternative methods and rubrics; retain their actual name, scope, date and findings. Pending review stays pending.

In Learning comparison, register both topics, one familiar-form and one changed-context held-out item per topic, checked solutions and five-point rubrics. Describe the actual transfer demand. A new number is weak near transfer; do not call it broad transfer. Enter review provenance honestly. Registration freezes this material and records app, model, prompt hash, content hash/version, scheduler and visualizer package identity. Questions/solutions are separate from tutoring FTS and provider contexts. The core randomly assigns tool/baseline across A/B and draws study order; retain the assignment draw. Registration records the author's judgment of comparability, not independent proof.

Specify a strong baseline (current best method or an unchanged comparison tool), its provider/model and sources. Keep provider/model consistent where feasible and record confounds where it is not. Match the total time allowance including setup, review, transcript correction and other differing overhead. Pre-register a personal decision criterion; for example, a useful improvement at similar time/cost or similar scores with materially less time. A proposed threshold is a decision rule, never a predicted effect.

## During each topic

For the Study Harness condition, link the study session before marking the initial lesson complete. Its original attempts, assistance, frozen conditions, requests and costs accompany the export. Track study, review, setup and transcript correction with the native wall-clock timer. Stop it before leaving; if it stays active while the app is closed, that elapsed time is retained and labeled. Record manual time/cost separately rather than treating it as measured. For the baseline, record the same categories externally in the comparison view.

Record outside exposure, waiting, unwanted help, repeated questions, abandoned sessions and voluntary return. Note any model/prompt/content change; do not combine materially different interventions silently. Confidence is separate from correctness. Do not require continuous speech in either condition.

## Timing and assessment

Complete initial lesson sets the assessment at +168 hours. Planned review is excluded from +120 hours (the preceding 48 hours) until the held-out response is saved, including a late assessment. Linked scheduled reviews and fresh checks are withheld in that window. The app cannot prevent or prove absence of outside study: record it, and inspect linked help exposures. This outcome is seven days after the initial lesson, including earlier reviews. It is not seven days after final exposure.

At the registered time, Begin held-out assessment reveals only the prompts. Work unaided without the tutor, prior solutions or sources. Save both answers and confidence, reporting any help. Only then does scoring material become available. A subject-competent reviewer should grade a blinded sample if feasible. Record reviewer/self-score provenance, rubric points, rationale and whether condition blinding occurred. The app appends user-entered scores and never calls its own tutor to judge its benefit. These records do not constitute verified human sign-off. A correction to a score is a new score record.

Record the actual date/delay and all deviations; do not backdate an early or late test to seven days. The test itself is an exposure. Plan a later follow-up if practical and record intervening practice separately. Offline simulated clocks, fixtures and scores remain simulated and cannot enter a production profile through restore.

## Export and analysis

Export comparison records includes both conditions, frozen items/solutions, raw responses, confidence, all manual score revisions, native evidence, observations, times, known/estimated costs and intervention versions. It is private. Protect it appropriately before any deliberate sharing. Use `python3 scripts/analyze-evaluation.py <export.json> <output-directory>` to produce raw topic and paired CSV tables. The script excludes simulated records from paired outcome calculations and labels assistance, exposure and missing-score/timing problems. It reports raw within-pair differences; no significance or effectiveness claim is generated automatically. Keep inconclusive results inconclusive. The author's topic-equivalence judgment and self-reported outside exposure remain practical limits even when all software checks pass.

## Ordinary-session usability sheet (5–10 sessions)

For each actual session record: date and app version; intended task; prior familiarity; text/optional speech choice; time to useful work; provider waiting; transcript corrections; repeated questions; unwanted help; task abandonment; review offered/completed/stopped; voluntary return; source/visual usefulness; accessibility barriers; recovery surprises; and one concrete improvement. Obtain consent for any observation or recording. Keep personal identifiers separate. No participants or observations are pre-filled.

## Subject review sheet

Record reviewer, expertise, date, exact content/diagnostic/package hashes and reviewed scope. For each task: assumptions, reference correctness, units/notation, ambiguity/flaw, alternatives, rubric coverage, hint leakage and transfer claim. Record pass/change/abstain with reasoning. Re-run affected analytic and software checks after changes, version the content, and review the revised scope. No automatic action writes an approval record.

## Manual accessibility and installation sheet

On an available target Mac: verify opening/install trust, useful first work, keyboard-only navigation/focus return, 200% text, reduced motion, VoiceOver/math reading, source anchors and PDF text alternatives, exact numeric entries instead of dragging, 2D/3D alternatives, microphone permission/record/discard, correction/dispute, backup/restore and native pixels. Record OS, app/hash, assistive tool/version, steps, observed result and unresolved issue. Automated browser checks and AX interactions do not replace these sessions.
