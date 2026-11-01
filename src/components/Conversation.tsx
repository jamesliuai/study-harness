import { useState } from 'react';
import type { ReactNode, Ref } from 'react';
import type { Assessment, Attempt, TimelineEntry, Workspace } from '../generated/contracts';
import { phaseOf, reading } from '../lesson';
import { PastTask } from './TaskCard';
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
const helpTone = (label: string) =>
  /solution/i.test(label) ? 'solution' : /outside/i.test(label) ? 'note' : 'teach';

/**
 * The study conversation: a projection of durable records (tasks, answers, questions, tutor replies, authored help)
 * computed by the core. It invents no records; during an independent check the core has already withheld earlier
 * entries, so nothing here can reopen hidden teaching. The current task renders as `task`; requests in flight,
 * a failed reply and the next step follow the newest entry.
 */
export function Conversation({
  workspace: w,
  busy,
  canAsk,
  task,
  taskRef,
  after,
  onCommand,
  onAsk,
  onSource,
}: {
  workspace: Workspace;
  busy: boolean;
  canAsk: boolean;
  task: ReactNode;
  taskRef?: Ref<HTMLLIElement>;
  /** Pending request, failed reply and next-step card, in that order. */
  after?: ReactNode;
  onCommand: Command;
  onAsk: (attemptId: string) => void;
  onSource?: (sourceId: string, anchor: string) => void;
}) {
  const answered = (id: string) => w.assistance.some((h) => h.attempt_id === id);
  // The check banner on the task says what is withheld; the core's marker would repeat it.
  const entries = reading(w.timeline).filter((e) => e.kind !== 'hidden');
  // A divider marks where a new phase begins: before the first entry of a task whose phase differs.
  const stageOf = new Map(
    w.timeline.filter((e) => e.kind === 'task').map((e) => [e.activity_id, e.stage]),
  );
  let phase = '';
  return (
    <ol className="conversation" aria-label="Study conversation">
      {entries.map((e) => {
        const turn =
          e.kind === 'task' && e.activity_id === w.activity.id ? (
            <li
              key={e.id}
              className="turn-slot"
              ref={taskRef}
              data-entry=""
              data-activity={e.activity_id}
            >
              {task}
            </li>
          ) : (
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
          );
        const stage = stageOf.get(e.activity_id);
        const next = stage ? phaseOf(stage) : undefined;
        if (!next || next.id === phase) return turn;
        phase = next.id;
        return [
          <li
            key={`phase:${e.id}`}
            className="phase-divider"
            aria-hidden="true"
            data-activity={e.activity_id}
          >
            <span>{next.label}</span>
          </li>,
          turn,
        ];
      })}
      {after}
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
  if (e.kind === 'visual')
    return (
      <li className="system-note" data-entry="" data-activity={e.activity_id}>
        <Icon name="chart" />
        <p>{e.text}</p>
      </li>
    );
  if (e.kind === 'task')
    return (
      <li className="turn-slot" data-entry="" data-activity={e.activity_id}>
        <PastTask stage={e.stage} title={e.detail} text={e.text} />
      </li>
    );
  if (e.kind === 'answer' || e.kind === 'question') {
    const attempt = w.attempts.find((a) => a.id === e.attempt_id);
    return (
      <li className="turn learner" data-kind={e.kind} data-entry="" data-activity={e.activity_id}>
        <div className="band">
          <p className="band-who">{e.kind === 'question' ? 'You asked' : 'You'}</p>
          <RichText text={e.text} className="prose" />
          {attempt ? (
            <LearnerControls
              attempt={attempt}
              entry={e}
              workspace={w}
              busy={busy}
              onCommand={onCommand}
              feedback={
                current && canAsk ? (
                  <button className="chip-button" disabled={busy} onClick={() => onAsk(attempt.id)}>
                    {e.kind === 'question' ? 'Ask the tutor' : 'Get feedback'}
                  </button>
                ) : null
              }
            />
          ) : null}
        </div>
      </li>
    );
  }
  const tutor = e.kind === 'tutor';
  const citations = w.sources.citations.filter((c) => c.assistance_id === e.assistance_id);
  const attempt = tutor ? w.attempts.find((a) => a.id === e.attempt_id) : undefined;
  // The verdict sits on the newest reply to an answer; questions are discussion and never graded.
  const newest =
    attempt &&
    w.timeline.filter((x) => x.kind === 'tutor' && x.attempt_id === attempt.id).at(-1)?.id === e.id;
  const grade = newest
    ? w.learning?.assessments.filter((g) => g.attempt_id === attempt!.id).at(-1)
    : undefined;
  return (
    <li
      className="turn tutor"
      data-kind={e.kind}
      data-tone={tutor ? undefined : helpTone(e.detail)}
      data-entry=""
      data-activity={e.activity_id}
    >
      <span className="avatar" aria-hidden="true">
        {tutor ? <LogoMark /> : <Icon name={helpIcon(e.detail)} />}
      </span>
      <div className="turn-body">
        <header className="turn-head">
          <span className="who">
            {tutor ? (w.offline ? 'Simulated tutor' : 'Tutor') : e.detail}
          </span>
          <span className="meta">
            {tutor && e.detail ? `${e.detail} · ` : ''}
            {time(e.at)}
          </span>
        </header>
        {grade && attempt ? <Verdict grade={grade} attempt={attempt} /> : null}
        <RichText text={e.text} className="prose" />
        {grade && attempt ? (
          <Evidence
            grade={grade}
            attempt={attempt}
            workspace={w}
            busy={busy}
            onCommand={onCommand}
          />
        ) : null}
        {citations.length ? (
          <details className="citations">
            <summary>
              <Icon name="doc" />
              {citations.length} source passage{citations.length === 1 ? '' : 's'} cited
            </summary>
            {citations.map((c, i) => (
              <blockquote className="citation" key={`${c.chunk_id}:${i}`}>
                <p>{c.excerpt}</p>
                <footer>
                  <span>
                    {c.title} · {c.anchor} ·{' '}
                    {c.kind === 'direct' ? 'direct source passage' : 'reasoning derived from it'}
                  </span>
                  {onSource ? (
                    <button className="text-button" onClick={() => onSource(c.source_id, c.anchor)}>
                      Open passage
                    </button>
                  ) : null}
                </footer>
              </blockquote>
            ))}
          </details>
        ) : null}
      </div>
    </li>
  );
}

const VERDICT: Record<string, { label: string; icon: IconName }> = {
  correct: { label: 'Correct', icon: 'checkCircle' },
  partial: { label: 'Partly correct', icon: 'circleDashed' },
  incorrect: { label: 'Not yet correct', icon: 'alert' },
  unresolved: { label: 'Needs clarification', icon: 'question' },
  disputed: { label: 'Challenged · excluded from progress', icon: 'flag' },
  stale: { label: 'Out of date after a correction', icon: 'refresh' },
  held: { label: 'Held · the answer check disagrees', icon: 'alert' },
};
const ERROR_KIND: Record<string, string> = {
  conceptual: 'a conceptual point',
  procedural: 'a step in the method',
  arithmetic_slip: 'an arithmetic slip',
  notation_or_transcription: 'notation or transcription',
  incomplete: 'an incomplete argument',
  task_flaw: 'a possible problem with the task',
  unclear: 'something unclear',
};
const verdictState = (g: Assessment) =>
  g.stale
    ? 'stale'
    : g.dispute_state === 'disputed'
      ? 'disputed'
      : g.verdict === 'unresolved' && g.checks.some((c) => c.outcome === 'failed')
        ? 'held'
        : g.verdict;

function Verdict({ grade, attempt }: { grade: Assessment; attempt: Attempt }) {
  const state = verdictState(grade);
  const v = VERDICT[state] ?? { label: state, icon: 'info' as IconName };
  const supported = attempt.assistance_ids.length > 0 || attempt.exposure_ids.length > 0;
  return (
    <p className="verdict" data-verdict={state}>
      <Icon name={v.icon} />
      <strong>{v.label}</strong>
      {!grade.stale && grade.verdict !== 'correct' && ERROR_KIND[grade.error_kind] ? (
        <span>· {ERROR_KIND[grade.error_kind]}</span>
      ) : null}
      {supported ? (
        <span
          className="tag"
          title="Help or a revealed result was recorded before this answer, so it counts as supported work"
        >
          with help
        </span>
      ) : null}
    </p>
  );
}

/** Rubric findings, automated checks and the learner's right to challenge the assessment. */
function Evidence({
  grade,
  attempt: a,
  workspace: w,
  busy,
  onCommand,
}: {
  grade: Assessment;
  attempt: Attempt;
  workspace: Workspace;
  busy: boolean;
  onCommand: Command;
}) {
  const [disputing, setDisputing] = useState(false);
  const [reason, setReason] = useState('');
  const session = w.sessions.find((s) => s.id === w.active_session_id)!;
  const base = { sessionId: session.id, revision: session.revision };
  const superseded = w.attempts.some((b) => b.revision_of === a.id);
  return (
    <>
      {!grade.stale && grade.note ? <p className="assessment-note">{grade.note}</p> : null}
      <details className="evidence">
        <summary>How this was assessed</summary>
        <ul className="findings">
          {grade.findings.map((f) => (
            <li key={f.dimension} data-result={f.result}>
              <span className="finding-head">
                <span className="finding-dimension">{f.dimension.replaceAll('_', ' ')}</span>
                <span className="finding-result">{f.result}</span>
              </span>
              <q>{f.excerpt}</q>
            </li>
          ))}
        </ul>
        {grade.checks.map((c) => (
          <p key={c.id} className="check-line" data-outcome={c.outcome}>
            Automated check · {c.label}: {c.outcome.replace('_', ' ')}. {c.detail}
          </p>
        ))}
        <p className="versions">
          {grade.grader_version} · {grade.rubric_version} · {grade.uncertainty}
        </p>
        {!grade.stale && !superseded ? (
          <div className="actions">
            <button
              className="ghost compact"
              disabled={busy}
              aria-expanded={disputing}
              onClick={() => setDisputing((v) => !v)}
            >
              <Icon name="flag" />
              Challenge this assessment
            </button>
            {grade.dispute_state === 'disputed' && grade.uncertainty === 'resolved' ? (
              <button
                className="compact"
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
            className="inline-form"
            onSubmit={async (event) => {
              event.preventDefault();
              try {
                await onCommand('dispute_grade', { ...base, assessmentId: grade.id, reason });
                setDisputing(false);
                setReason('');
              } catch {
                /* The app shows the error. */
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
            <p className="hint-text">
              A challenged assessment is excluded from your progress until it is resolved.
            </p>
            <div className="actions">
              <button className="primary compact" disabled={busy || !reason.trim()}>
                Save challenge
              </button>
              <button type="button" className="text-button" onClick={() => setDisputing(false)}>
                Cancel
              </button>
            </div>
          </form>
        ) : null}
      </details>
    </>
  );
}

/**
 * What the learner can do with their own saved submission: ask for feedback, read the original transcript, or save
 * an explicit correction that keeps the original.
 */
function LearnerControls({
  attempt: a,
  entry: e,
  workspace: w,
  busy,
  onCommand,
  feedback,
}: {
  attempt: Attempt;
  entry: TimelineEntry;
  workspace: Workspace;
  busy: boolean;
  onCommand: Command;
  feedback?: ReactNode;
}) {
  const [edit, setEdit] = useState(false);
  const [text, setText] = useState(a.response);
  const [revisionId, setRevisionId] = useState(() => crypto.randomUUID());
  const session = w.sessions.find((s) => s.id === w.active_session_id)!;
  const superseded = w.attempts.some((b) => b.revision_of === a.id);
  return (
    <>
      <footer className="band-foot">
        <span className="meta">
          {e.kind === 'question' ? 'Question · never graded' : 'Answer'}
          {e.detail ? ` · ${e.detail}` : ''} · {time(e.at)}
        </span>
        <span className="band-actions">
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
              Fix {a.recognized_transcript ? 'transcript' : 'text'}
            </button>
          ) : null}
        </span>
      </footer>
      {a.recognized_transcript !== null ? (
        <details className="transcript">
          <summary>Original recognized transcript</summary>
          <p>{a.recognized_transcript}</p>
        </details>
      ) : null}
      {edit ? (
        <form
          className="inline-form"
          onSubmit={async (event) => {
            event.preventDefault();
            try {
              await onCommand('correct_attempt', {
                sessionId: session.id,
                revision: session.revision,
                attemptId: a.id,
                submissionId: revisionId,
                text,
              });
              setEdit(false);
            } catch {
              /* The app shows the error. */
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
          <p className="hint-text">
            The original is kept. Feedback that depended on it becomes out of date, and a corrected
            answer counts as supported.
          </p>
          <div className="actions">
            <button className="primary compact" disabled={busy || !text.trim()}>
              Save correction
            </button>
            <button type="button" className="text-button" onClick={() => setEdit(false)}>
              Cancel
            </button>
          </div>
        </form>
      ) : null}
    </>
  );
}
