import { useState } from 'react';
import type { Workspace } from '../generated/contracts';
import { TutorText } from './MathText';

export function LearningRecord({
  workspace: w,
  busy,
  onCommand,
  showAll = false,
  onSource,
  onAsk,
}: {
  workspace: Workspace;
  busy: boolean;
  showAll?: boolean;
  onSource?: (sourceId: string, anchor: string) => void;
  /** Requests a tutor response through the same path as the composer (progress, cancel, retry). */
  onAsk?: (attemptId: string) => void;
  onCommand: (name: string, args: Record<string, unknown>) => Promise<void>;
}) {
  const [edit, setEdit] = useState<string | null>(null);
  const [text, setText] = useState('');
  const [reason, setReason] = useState('');
  const [disputing, setDisputing] = useState<string | null>(null);
  const [revisionId, setRevisionId] = useState(crypto.randomUUID());
  const session = w.sessions.find((s) => s.id === w.active_session_id)!;
  const base = { sessionId: session.id, revision: session.revision };
  const attempts = w.attempts.filter((a) => showAll || a.activity_id === w.activity.id);
  return (
    <section className="history" aria-label="Saved learning record">
      {attempts.map((a) => {
        const grade = w.learning?.assessments.filter((g) => g.attempt_id === a.id).at(-1);
        const conditions = w.learning?.conditions.find((c) => c.attempt_id === a.id);
        const superseded = w.attempts.some((b) => b.revision_of === a.id);
        return (
          <article key={a.id}>
            <div className="history-heading">
              <h3>{a.revision_of ? 'Corrected revision' : 'Saved attempt'}</h3>
              <span data-assisted={a.assistance_ids.length ? '' : undefined}>
                {a.assistance_ids.length ? 'With recorded support' : 'No recorded task assistance'}
                {superseded ? ' · revised' : ''}
              </span>
            </div>
            <p className="response-text">{a.response}</p>
            <small className="record-meta">
              {a.modality === 'recorded_speech' ? 'Confirmed transcript' : 'Typed reasoning'} ·{' '}
              {new Date(a.created_at).toLocaleString()}
            </small>
            {conditions?.last_relevant_exposure_at ? (
              <p className="small muted">
                Most recent recorded exposure before submission:{' '}
                {new Date(conditions.last_relevant_exposure_at).toLocaleString()}
                {conditions.simulated ? ' · simulated conditions' : ''}
              </p>
            ) : null}
            {a.recognized_transcript !== null ? (
              <details>
                <summary>Original recognized transcript</summary>
                <p>{a.recognized_transcript}</p>
              </details>
            ) : null}
            {grade ? (
              <div className="assessment">
                <strong
                  className="verdict"
                  data-verdict={
                    grade.stale
                      ? 'stale'
                      : grade.dispute_state === 'disputed'
                        ? 'disputed'
                        : grade.verdict
                  }
                >
                  {grade.stale
                    ? 'Stale assessment — recomputation required'
                    : `${grade.verdict} · ${grade.dispute_state === 'disputed' ? 'disputed; excluded from progress' : grade.uncertainty}`}
                </strong>
                <details>
                  <summary>Rubric findings and evidence</summary>
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
                  <p className="small">
                    {grade.grader_version} · {grade.rubric_version}
                  </p>
                </details>
                {!grade.stale && !superseded ? (
                  <div className="actions">
                    <button
                      disabled={busy}
                      onClick={() => {
                        setDisputing(grade.id);
                        setReason('');
                      }}
                    >
                      Challenge assessment
                    </button>
                    {grade.dispute_state === 'disputed' && grade.uncertainty === 'resolved' ? (
                      <button
                        disabled={busy}
                        onClick={() =>
                          void onCommand('resolve_dispute', {
                            ...base,
                            assessmentId: grade.id,
                          }).catch(() => {})
                        }
                      >
                        Accept reassessment as resolution
                      </button>
                    ) : null}
                  </div>
                ) : null}
                {disputing === grade.id ? (
                  <form
                    onSubmit={async (e) => {
                      e.preventDefault();
                      try {
                        await onCommand('dispute_grade', {
                          ...base,
                          assessmentId: grade.id,
                          reason,
                        });
                        setDisputing(null);
                      } catch {
                        /* Parent displays the error. */
                      }
                    }}
                  >
                    <label htmlFor={`dispute-${a.id}`}>What did the assessment get wrong?</label>
                    <textarea
                      id={`dispute-${a.id}`}
                      value={reason}
                      onChange={(e) => setReason(e.target.value)}
                      maxLength={2000}
                      required
                    />
                    <div className="actions">
                      <button disabled={busy || !reason.trim()}>Save dispute</button>
                      <button type="button" onClick={() => setDisputing(null)}>
                        Cancel
                      </button>
                    </div>
                  </form>
                ) : null}
              </div>
            ) : null}
            {!superseded ? (
              <div className="record-actions">
                {a.activity_id === w.activity.id &&
                (!w.learning || w.learning.allowed_actions.includes('assess')) ? (
                  <button
                    disabled={busy || (!w.offline && !w.credential.configured)}
                    onClick={() =>
                      onAsk
                        ? onAsk(a.id)
                        : void onCommand('ask_tutor', {
                            sessionId: session.id,
                            attemptId: a.id,
                          }).catch(() => {})
                    }
                  >
                    {grade ? 'Reassess response' : 'Assess this response'}
                  </button>
                ) : null}
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => {
                    setEdit(a.id);
                    setText(a.response);
                    setRevisionId(crypto.randomUUID());
                  }}
                >
                  Correct {a.recognized_transcript ? 'transcript' : 'response'}
                </button>
              </div>
            ) : null}
            {edit === a.id ? (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  try {
                    await onCommand('correct_attempt', {
                      ...base,
                      attemptId: a.id,
                      submissionId: revisionId,
                      text,
                    });
                    setEdit(null);
                  } catch {
                    /* Parent displays the error. */
                  }
                }}
              >
                <label htmlFor={`correction-${a.id}`}>Corrected intended response</label>
                <textarea
                  id={`correction-${a.id}`}
                  value={text}
                  onChange={(e) => {
                    setText(e.target.value);
                    setRevisionId(crypto.randomUUID());
                  }}
                  maxLength={12000}
                  required
                />
                <p className="small">
                  The original is retained. Dependent assessments, summaries and reviews become
                  stale. A correction after feedback needs a fresh task for independent evidence.
                </p>
                <div className="actions">
                  <button disabled={busy || !text.trim()}>Save correction</button>
                  <button type="button" onClick={() => setEdit(null)}>
                    Cancel
                  </button>
                </div>
              </form>
            ) : null}
          </article>
        );
      })}
      {w.assistance
        .filter((h) => (showAll || h.activity_id === w.activity.id) && h.depth !== 'visual')
        .map((h) => (
          <article className="tutor-response" key={h.id}>
            <h3>
              {w.offline ? 'Offline session' : 'Study support'} · {h.depth}
            </h3>
            <TutorText text={h.content} />
            {w.sources.citations.some((c) => c.assistance_id === h.id) ? (
              <details className="source-citations">
                <summary>Source citations</summary>
                {w.sources.citations
                  .filter((c) => c.assistance_id === h.id)
                  .map((c, i) => (
                    <blockquote className="source-citation" key={`${c.chunk_id}:${i}`}>
                      <p>{c.excerpt}</p>
                      <p className="small">
                        {c.title} · {c.anchor} ·{' '}
                        {c.kind === 'direct'
                          ? 'direct source passage'
                          : 'reasoning derived from this passage'}
                      </p>
                      {onSource ? (
                        <button onClick={() => onSource(c.source_id, c.anchor)}>
                          Open cited passage
                        </button>
                      ) : null}
                    </blockquote>
                  ))}
              </details>
            ) : null}
            <small>Saved before display · {new Date(h.visible_at).toLocaleString()}</small>
          </article>
        ))}
    </section>
  );
}
