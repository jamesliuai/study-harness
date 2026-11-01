import { useState } from 'react';
import type { LearningView, Workspace } from '../generated/contracts';
import { STAGE_ICON } from '../lesson';
import { MathText, RichText } from './MathText';
import { Icon } from './Icon';

const CHECK_LABEL: Record<string, string> = {
  passed: 'passed',
  failed: 'failed',
  inconclusive: 'not conclusive',
  not_applicable: 'not applicable',
};

/** An earlier task in this session, kept readable in place so the answers below it make sense. */
export function PastTask({ stage, title, text }: { stage: string; title: string; text: string }) {
  return (
    <article className="task-card" data-past="">
      <header className="task-head">
        <span className="stage-chip">
          <Icon name={STAGE_ICON[stage] ?? 'pencil'} />
          {title}
        </span>
      </header>
      <RichText text={text} className="task-prompt prose" />
    </article>
  );
}

/**
 * The current task, frozen before it was shown: what to do, and on request what it assumes, what it checks, where it
 * came from and a way to report a flawed task. During a check, earlier teaching is hidden and any help is recorded.
 */
export function TaskCard({
  workspace: w,
  learning,
  check,
  working,
  onReport,
}: {
  workspace: Workspace;
  learning: LearningView;
  check: boolean;
  working: boolean;
  onReport: (note: string) => void;
}) {
  const [about, setAbout] = useState(false);
  const [report, setReport] = useState<string | null>(null);
  const stage = learning.state.stage;
  return (
    <article
      className="task-card"
      data-current=""
      data-check={check || undefined}
      id="current-task"
      tabIndex={-1}
      aria-labelledby="current-task-stage"
      aria-describedby="current-task-prompt"
    >
      <header className="task-head">
        <span className="stage-chip" id="current-task-stage">
          <Icon name={STAGE_ICON[stage] ?? 'pencil'} />
          {learning.stage_title}
        </span>
        <span className="task-head-spacer" />
        <button
          className="text-button task-about-toggle"
          aria-expanded={about}
          aria-controls="task-about"
          onClick={() => setAbout((v) => !v)}
        >
          <Icon name="info" />
          About this task
        </button>
      </header>
      {check ? (
        <p className="check-banner">
          <Icon name="eyeOff" />
          <span>
            <strong>On your own.</strong> Earlier teaching and visuals are hidden until you finish;
            any help you ask for is recorded.
          </span>
        </p>
      ) : null}
      <div id="current-task-prompt">
        <RichText text={w.activity.prompt} className="task-prompt prose" />
      </div>
      {w.activity.math && !check ? (
        <p className="task-math">
          <MathText tex={w.activity.math} block />
        </p>
      ) : null}
      {about ? (
        <div className="task-about" id="task-about">
          {learning.generated ? (
            <p>
              Written for your plan and checked automatically. If it looks wrong, report it below —
              a flawed task never counts against you.
            </p>
          ) : null}
          {learning.assumptions.length ? (
            <p>
              <strong>Assumptions.</strong> {learning.assumptions.join(' ')}
            </p>
          ) : null}
          {learning.rubric_lines.length ? (
            <div>
              <strong>What this task checks</strong>
              <ul>
                {learning.rubric_lines.map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ul>
            </div>
          ) : null}
          <p>
            <strong>Scope.</strong> {learning.transfer_scope}.
          </p>
          <p>
            <strong>Source.</strong> {learning.task_provenance}
          </p>
          {learning.task_checks.length ? (
            <div>
              <strong>Automated checks before you saw it</strong>
              <ul className="check-list">
                {learning.task_checks.map((c) => (
                  <li key={c.id} data-outcome={c.outcome}>
                    {c.label}: {CHECK_LABEL[c.outcome] ?? c.outcome}. {c.detail}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {learning.generated ? (
            report === null ? (
              <button className="ghost compact" disabled={working} onClick={() => setReport('')}>
                <Icon name="flag" />
                Report a problem with this task
              </button>
            ) : (
              <form
                className="inline-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  onReport(report.trim());
                  setReport(null);
                }}
              >
                <label htmlFor="task-report">What is wrong with it?</label>
                <input
                  id="task-report"
                  value={report}
                  maxLength={1000}
                  placeholder="e.g. a value is missing, the units don’t match, two answers are possible"
                  onChange={(e) => setReport(e.target.value)}
                />
                <p className="hint-text">
                  Your answers to this task are kept; the replacement is a new task.
                </p>
                <div className="actions">
                  <button className="primary compact" disabled={working || !report.trim()}>
                    Replace this task
                  </button>
                  <button type="button" className="text-button" onClick={() => setReport(null)}>
                    Cancel
                  </button>
                </div>
              </form>
            )
          ) : null}
        </div>
      ) : null}
    </article>
  );
}
