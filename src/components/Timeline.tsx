import { useState } from 'react';
import type { ReactNode } from 'react';
import type { Attempt, TimelineEntry, Workspace } from '../generated/contracts';
import { RichText } from './MathText';
import { Icon, LogoMark, type IconName } from './Icon';

type Command = (name: string, args: Record<string, unknown>) => Promise<void>;
const time = (at: string) =>
  at ? new Date(at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '';
// Authored help is labelled by what it is; the icon follows the label.
const helpIcon = (label: string): IconName =>
  /hint/i.test(label)
    ? 'bulb'
    : /solution/i.test(label)
      ? 'eye'
      : /example/i.test(label)
        ? 'stairs'
        : /prerequisite|review/i.test(label)
          ? 'review'
          : /outside/i.test(label)
            ? 'clipboard'
            : /explain|idea|explanation/i.test(label)
              ? 'book'
              : 'lifebuoy';

/**
 * The study conversation: a chronological projection of durable records (tasks, answers, questions, tutor replies
 * and authored help) computed by the core. It invents no records; during an independent check the core has already
 * withheld earlier entries, so nothing here can reopen hidden teaching.
 */
export function Timeline({
  workspace: w,
  busy,
  canAsk,
  onCommand,
  onAsk,
  onSource,
}: {
  workspace: Workspace;
  busy: boolean;
  canAsk: boolean;
  onCommand: Command;
  onAsk: (attemptId: string) => void;
  onSource?: (sourceId: string, anchor: string) => void;
}) {
  const answered = (id: string) => w.assistance.some((h) => h.attempt_id === id);
  // Nothing has happened in this task yet: say where the conversation will appear instead of an empty column.
  const fresh = w.timeline.every((e) => e.kind === 'task' && e.activity_id === w.activity.id);
  if (fresh)
    return (
      <div className="timeline-empty">
        <Icon name="pencil" />
        <p>
          Your answers, questions and the tutor’s replies will collect here. Start with the task
          below — a rough first attempt is useful.
        </p>
      </div>
    );
  return (
    <ol className="timeline" aria-label="Study conversation">
      {w.timeline.map((e) => (
        <Turn
          key={e.id}
          entry={e}
          workspace={w}
          busy={busy}
          canAsk={canAsk && !!e.attempt_id && !answered(e.attempt_id)}
          onCommand={onCommand}
          onAsk={onAsk}
          onSource={onSource}
        />
      ))}
    </ol>
  );
}

function Turn({
  entry: e,
  workspace: w,
  busy,
  canAsk,
  onCommand,
  onAsk,
  onSource,
}: {
  entry: TimelineEntry;
  workspace: Workspace;
  busy: boolean;
  canAsk: boolean;
  onCommand: Command;
  onAsk: (attemptId: string) => void;
  onSource?: (sourceId: string, anchor: string) => void;
}) {
  const current = e.activity_id === w.activity.id;
  if (e.kind === 'hidden')
    return (
      <li className="turn turn-hidden">
        <Icon name="eyeOff" />
        <p>{e.text}</p>
      </li>
    );
  if (e.kind === 'task')
    return current ? (
      <li className="turn turn-divider" aria-label={`Current task: ${e.detail}`}>
        <span>{e.detail}</span>
      </li>
    ) : (
      <li className="turn turn-task">
        <details>
          <summary>
            <span>{e.detail}</span>
            <time>{time(e.at)}</time>
          </summary>
          <RichText text={e.text} className="prose" />
        </details>
      </li>
    );
  if (e.kind === 'visual')
    return (
      <li className="turn turn-visual">
        <Icon name="chart" />
        <p>{e.text}</p>
      </li>
    );
  if (e.kind === 'answer' || e.kind === 'question') {
    const attempt = w.attempts.find((a) => a.id === e.attempt_id);
    return (
      <li className="turn turn-learner" data-kind={e.kind}>
        <header>
          <span className="turn-who">
            <Icon name={e.kind === 'question' ? 'question' : 'pencil'} />
            {e.kind === 'question' ? 'You asked' : 'Your answer'}
          </span>
          <span className="turn-meta">
            {e.detail ? `${e.detail} · ` : ''}
            {time(e.at)}
          </span>
        </header>
        <RichText text={e.text} />
        {attempt ? (
          <AttemptEvidence
            attempt={attempt}
            workspace={w}
            busy={busy}
            onCommand={onCommand}
            feedback={
              current && canAsk ? (
                <button className="text-button" disabled={busy} onClick={() => onAsk(attempt.id)}>
                  <Icon name="sparkle" />
                  {e.kind === 'question' ? 'Ask the tutor' : 'Get feedback'}
                </button>
              ) : null
            }
          />
        ) : null}
      </li>
    );
  }
  const citations = w.sources.citations.filter((c) => c.assistance_id === e.assistance_id);
  return (
    <li className="turn turn-tutor" data-kind={e.kind}>
      <header>
        <span className="turn-who">
          {e.kind === 'tutor' ? (
            <span className="turn-avatar" aria-hidden="true">
              <LogoMark />
            </span>
          ) : (
            <Icon name={helpIcon(e.detail)} />
          )}
          {e.kind === 'tutor' ? (w.offline ? 'Simulated tutor' : 'Tutor') : e.detail}
        </span>
        <span className="turn-meta">
          {e.kind === 'tutor' ? `${e.detail} · ` : ''}
          {time(e.at)}
        </span>
      </header>
      <RichText text={e.text} className="prose" />
      {citations.length ? (
        <details className="source-citations">
          <summary>Sources cited ({citations.length})</summary>
          {citations.map((c, i) => (
            <blockquote className="source-citation" key={`${c.chunk_id}:${i}`}>
              <p>{c.excerpt}</p>
              <p className="small">
                {c.title} · {c.anchor} ·{' '}
                {c.kind === 'direct'
                  ? 'direct source passage'
                  : 'reasoning derived from this passage'}
              </p>
              {onSource ? (
                <button className="text-button" onClick={() => onSource(c.source_id, c.anchor)}>
                  <Icon name="doc" />
                  Open cited passage
                </button>
              ) : null}
            </blockquote>
          ))}
        </details>
      ) : null}
    </li>
  );
}

/**
 * Feedback state and learner controls for one saved submission: the latest assessment (with its rubric evidence in a
 * disclosure), challenge/accept-resolution, and an explicit correction that keeps the original.
 */
export function AttemptEvidence({
  attempt: a,
  workspace: w,
  busy,
  onCommand,
  feedback,
}: {
  attempt: Attempt;
  workspace: Workspace;
  busy: boolean;
  onCommand: Command;
  feedback?: ReactNode;
}) {
  const [edit, setEdit] = useState(false);
  const [text, setText] = useState(a.response);
  const [revisionId, setRevisionId] = useState(() => crypto.randomUUID());
  const [disputing, setDisputing] = useState(false);
  const [reason, setReason] = useState('');
  const session = w.sessions.find((s) => s.id === w.active_session_id)!;
  const base = { sessionId: session.id, revision: session.revision };
  const grade = w.learning?.assessments.filter((g) => g.attempt_id === a.id).at(-1);
  const superseded = w.attempts.some((b) => b.revision_of === a.id);
  const state = grade
    ? grade.stale
      ? 'stale'
      : grade.dispute_state === 'disputed'
        ? 'disputed'
        : grade.verdict === 'unresolved' && grade.checks.some((c) => c.outcome === 'failed')
          ? 'held'
          : grade.verdict
    : null;
  const label: Record<string, string> = {
    correct: 'Correct',
    partial: 'Partly correct',
    incorrect: 'Not yet correct',
    unresolved: 'Needs clarification',
    disputed: 'Challenged · excluded from progress',
    stale: 'Out of date after a correction',
    held: 'Held · the answer check disagrees',
  };
  const kind: Record<string, string> = {
    conceptual: 'a conceptual point',
    procedural: 'a step in the method',
    arithmetic_slip: 'an arithmetic slip',
    notation_or_transcription: 'notation or transcription',
    incomplete: 'an incomplete argument',
    task_flaw: 'a possible problem with the task',
    unclear: 'something unclear',
  };
  return (
    <div className="turn-evidence">
      <div className="turn-actions">
        {state ? (
          <span className="verdict" data-verdict={state}>
            {label[state] ?? state}
            {a.assistance_ids.length || a.exposure_ids.length ? ' · with support' : ''}
            {grade && !grade.stale && kind[grade.error_kind] && grade.verdict !== 'correct'
              ? ` · ${kind[grade.error_kind]}`
              : ''}
          </span>
        ) : null}
        {feedback}
        {!superseded ? (
          <button
            className="text-button"
            disabled={busy}
            aria-expanded={edit}
            title="Save a corrected version. The original is kept, and feedback that depended on it becomes out of date."
            onClick={() => {
              setEdit((v) => !v);
              setText(a.response);
              setRevisionId(crypto.randomUUID());
            }}
          >
            <Icon name="pencil" />
            Fix {a.recognized_transcript ? 'transcript' : 'text'}
          </button>
        ) : null}
      </div>
      {grade && !grade.stale && grade.note ? (
        <p className="assessment-note small">{grade.note}</p>
      ) : null}
      {grade ? (
        <details className="assessment">
          <summary>Rubric evidence</summary>
          <ul>
            {grade.findings.map((f) => (
              <li key={f.dimension} data-result={f.result}>
                <strong>
                  {f.dimension}: {f.result}
                </strong>
                <p>“{f.excerpt}”</p>
              </li>
            ))}
          </ul>
          {grade.checks.map((c) => (
            <p key={c.id} className="small check-line" data-outcome={c.outcome}>
              Automated check · {c.label}: {c.outcome.replace('_', ' ')}. {c.detail}
            </p>
          ))}
          <p className="small">
            {grade.grader_version} · {grade.rubric_version} · {grade.uncertainty}
          </p>
          {!grade.stale && !superseded ? (
            <div className="actions">
              <button
                className="ghost"
                disabled={busy}
                aria-expanded={disputing}
                onClick={() => setDisputing((v) => !v)}
              >
                <Icon name="flag" />
                Challenge this assessment
              </button>
              {grade.dispute_state === 'disputed' && grade.uncertainty === 'resolved' ? (
                <button
                  disabled={busy}
                  onClick={() =>
                    void onCommand('resolve_dispute', { ...base, assessmentId: grade.id }).catch(
                      () => {},
                    )
                  }
                >
                  Accept reassessment as resolution
                </button>
              ) : null}
            </div>
          ) : null}
          {disputing ? (
            <form
              onSubmit={async (event) => {
                event.preventDefault();
                try {
                  await onCommand('dispute_grade', { ...base, assessmentId: grade.id, reason });
                  setDisputing(false);
                  setReason('');
                } catch {
                  /* Parent displays the error. */
                }
              }}
            >
              <label htmlFor={`dispute-${a.id}`}>What did the assessment get wrong?</label>
              <textarea
                id={`dispute-${a.id}`}
                value={reason}
                onChange={(event) => setReason(event.target.value)}
                maxLength={2000}
                required
              />
              <div className="actions">
                <button disabled={busy || !reason.trim()}>Save challenge</button>
                <button type="button" className="text-button" onClick={() => setDisputing(false)}>
                  Cancel
                </button>
              </div>
            </form>
          ) : null}
        </details>
      ) : null}
      {a.recognized_transcript !== null ? (
        <details className="turn-transcript">
          <summary>Original recognized transcript</summary>
          <p>{a.recognized_transcript}</p>
        </details>
      ) : null}
      {edit ? (
        <form
          className="turn-correction"
          onSubmit={async (event) => {
            event.preventDefault();
            try {
              await onCommand('correct_attempt', {
                ...base,
                attemptId: a.id,
                submissionId: revisionId,
                text,
              });
              setEdit(false);
            } catch {
              /* Parent displays the error. */
            }
          }}
        >
          <label htmlFor={`correction-${a.id}`}>Corrected text</label>
          <textarea
            id={`correction-${a.id}`}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setRevisionId(crypto.randomUUID());
            }}
            maxLength={12000}
            required
          />
          <p className="small">
            The original is kept. Feedback, summaries and reviews that depended on it become out of
            date, and a corrected answer counts as supported.
          </p>
          <div className="actions">
            <button disabled={busy || !text.trim()}>Save correction</button>
            <button type="button" className="text-button" onClick={() => setEdit(false)}>
              Cancel
            </button>
          </div>
        </form>
      ) : null}
    </div>
  );
}
