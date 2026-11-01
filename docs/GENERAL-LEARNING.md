# Goal-based learning

## Start from a goal

Enter a topic, question, difficult problem, or a description of what you want to do. Optionally attach text, Markdown, text-based PDF files, or pasted notes. The app shows the extracted material and warns about unreadable scans or potentially damaged equations.

The planner proposes 2–4 concrete capabilities, their order, assumed background, and approximate duration. Review the plan, revise it in plain language, start any capability, skip one, or choose **I know this — check me**. Skipping never records a capability as known. Each revision preserves earlier records.

## Work through a capability

Each capability has a separate session that reads as one document. A lesson moves through four phases — **Try**, **Learn**, **Practice**, **Check** — and ends with a wrap-up; the header shows where you are. A representative opening question leads into an explanation connected to your answer, a related worked example, gradually reduced support, and a fresh independent check. Relevant capabilities can include a prediction and an interactive model. Questions are always discussion rather than graded attempts.

Support fades with evidence rather than following a fixed sequence. When an answer on the explanation or worked-example step meets every rubric part, the remaining worked steps (a related example and a completion problem) are skipped for guided practice; otherwise the full route continues. The skip is offered as the next step, never taken silently, and it creates no evidence for the steps it skips. A suggested next step appears only after you have attempted the current task, so the task itself stays the obvious next move.

The task writer produces the statement, assumptions, rubric, reference solution, answer key and hidden support together. Rust validates the proposal and runs supported checks before freezing it. Checks cover stated numerical values, reference arithmetic, dimensions, sampled expression domains, repeated statements, and numerical answer leakage. These checks do not establish that every scientific assumption or reference solution is correct.

The learner can ask for a hint, explanation, related example, prerequisite review or full solution. Every reveal is recorded as assistance. Quiet work and stopping preserve the session. A flawed task can be reported and replaced while retaining its earlier answers.

## Assessment and return

Feedback separates the tutor's rubric judgment from deterministic checks. A proposed success that conflicts with an answer check is held. The learner can correct the response, dispute feedback, or report a task problem. Corrections preserve the original and invalidate dependent evidence.

An independent check hides earlier teaching and visuals. The wrap-up distinguishes what was explained, completed with help, shown independently, and not yet checked. Later reviews use new tasks from the capability's task family, with 1/3/7/14-day intervals. These intervals are a scheduling heuristic, not a measured guarantee of retention.

## Limits

- Generated plans, tasks, rubrics and feedback have not been independently reviewed for subject accuracy.
- Symbolic agreement is sampled within a stated domain, not proved.
- Proofs, explanations and written code rely on the tutor's rubric judgment. Code is not executed.
- Five interactive models are installed; unrelated topics have no visual.
- Live provider quality, latency, transcription accuracy, and learning benefit remain unverified.
- Offline fixtures exercise the workflow and storage. They do not analyse arbitrary goals or establish learning outcomes.

See [providers](PROVIDERS.md), [verification](VERIFICATION.md), and the [personal comparison protocol](EVALUATION-PROTOCOL.md).
