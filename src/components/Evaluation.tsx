import { useEffect, useRef, useState } from 'react';
import { command } from '../api';
import { evaluationCall, type EvaluationTopic, type EvaluationView } from '../evaluation';
import type { Workspace } from '../generated/contracts';
import { Icon } from './Icon';
const blankItem = (form: string) => ({
  form,
  prompt: '',
  rubric: '',
  reference_solution: '',
  max_points: 5,
  review_provenance: 'Pending independent subject review',
});
const blankTopic = () => ({
  name: '',
  prior_knowledge: '',
  source_ids: [] as string[],
  items: [blankItem('familiar'), blankItem('changed')],
});
type TopicInput = ReturnType<typeof blankTopic>;
function TopicEditor({
  label,
  value,
  onChange,
  sources,
}: {
  label: string;
  value: TopicInput;
  onChange: (v: TopicInput) => void;
  sources: Workspace['sources']['documents'];
}) {
  return (
    <fieldset>
      <legend>{label}</legend>
      <label>
        Topic name
        <input
          required
          value={value.name}
          maxLength={300}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
        />
      </label>
      {sources.length ? (
        <fieldset>
          <legend>Registered sources (hashes frozen in the export)</legend>
          {sources.map((s) => (
            <label className="check" key={s.id}>
              <input
                type="checkbox"
                checked={value.source_ids.includes(s.id)}
                onChange={(e) =>
                  onChange({
                    ...value,
                    source_ids: e.target.checked
                      ? [...value.source_ids, s.id]
                      : value.source_ids.filter((id) => id !== s.id),
                  })
                }
              />
              {s.title} · {s.sha256.slice(0, 12)}
            </label>
          ))}
        </fieldset>
      ) : (
        <p className="small">
          No sources imported. Record any external baseline material in the matching rationale; it
          is not inferred.
        </p>
      )}
      <label>
        Prior knowledge
        <textarea
          required
          value={value.prior_knowledge}
          maxLength={2000}
          onChange={(e) => onChange({ ...value, prior_knowledge: e.target.value })}
        />
      </label>
      {value.items.map((item, index) => (
        <fieldset key={item.form}>
          <legend>
            {item.form === 'familiar'
              ? 'Familiar-form held-out item'
              : 'Changed-context held-out item'}
          </legend>
          {(['prompt', 'rubric', 'reference_solution', 'review_provenance'] as const).map((key) => (
            <label key={key}>
              {
                (
                  {
                    prompt: 'Question',
                    rubric: 'Scoring rubric',
                    reference_solution: 'Checked solution',
                    review_provenance: 'Review provenance',
                  } as const
                )[key]
              }
              <textarea
                required
                value={item[key]}
                maxLength={10000}
                onChange={(e) =>
                  onChange({
                    ...value,
                    items: value.items.map((i, n) =>
                      n === index ? { ...i, [key]: e.target.value } : i,
                    ),
                  })
                }
              />
            </label>
          ))}
          <p className="small">
            Five points per item. State the scoring criteria, actual transfer demand, acceptable
            alternatives, and reviewer status. A pending review remains pending.
          </p>
        </fieldset>
      ))}
    </fieldset>
  );
}
function TopicRecord({
  topic: t,
  run,
  busy,
  workspace,
}: {
  topic: EvaluationTopic;
  run: (method: string, q: Record<string, unknown>) => Promise<void>;
  busy: boolean;
  workspace: Workspace;
}) {
  const [kind, setKind] = useState('study'),
    [eventKind, setEventKind] = useState('outside_exposure'),
    [note, setNote] = useState(''),
    [seconds, setSeconds] = useState(0),
    [cost, setCost] = useState(0);
  const [answers, setAnswers] = useState<Record<string, string>>({}),
    [confidence, setConfidence] = useState<Record<string, number>>({}),
    [help, setHelp] = useState(false),
    [scores, setScores] = useState<Record<string, number>>({}),
    [scoreNotes, setScoreNotes] = useState<Record<string, string>>({}),
    [reviewer, setReviewer] = useState('Self-score'),
    [blinded, setBlinded] = useState(false);
  const call = (m: string, q: Record<string, unknown> = {}) => run(m, { topicId: t.id, ...q });
  const total = t.events.reduce((sum, e) => sum + (e.seconds ?? 0), 0);
  return (
    <article className="comparison-topic">
      <h4>
        {t.name} · {t.condition}
      </h4>
      <p>Prior knowledge: {t.prior_knowledge}</p>
      <p>
        {(total / 60).toFixed(1)} recorded minutes (manual observations and tracked intervals remain
        separate in the export).
      </p>
      {t.condition === 'Study Harness' && !t.evidence.session_id && !t.lesson ? (
        <button
          disabled={busy}
          onClick={() => void call('link', { sessionId: workspace.active_session_id })}
        >
          Link current study session
        </button>
      ) : null}
      {t.evidence.session_id ? (
        <p className="small">
          Linked session:{' '}
          {workspace.sessions.find((s) => s.id === t.evidence.session_id)?.title ??
            t.evidence.session_id}
          . Attempts, help, charges and versions are exported from its authoritative record.
        </p>
      ) : null}
      <div className="actions">
        {t.timer ? (
          <>
            <p role="status">
              Tracking {t.timer.kind} since {new Date(t.timer.started_at).toLocaleString()}. Stop
              before leaving; elapsed wall time includes closed-app time.
            </p>
            <button disabled={busy} onClick={() => void call('timer_stop')}>
              Stop timer
            </button>
          </>
        ) : (
          <>
            <label>
              Time category
              <select value={kind} onChange={(e) => setKind(e.target.value)}>
                <option value="study">Study</option>
                <option value="review">Review</option>
                <option value="setup">Setup</option>
                <option value="transcript_correction">Transcript correction</option>
              </select>
            </label>
            <button
              disabled={busy || (kind === 'review' && t.planned_review_excluded)}
              onClick={() => void call('timer_start', { kind })}
            >
              Start timer
            </button>
          </>
        )}
      </div>
      {!t.lesson ? (
        <button disabled={busy || !!t.timer} onClick={() => void call('finish_lesson')}>
          Complete initial lesson
        </button>
      ) : (
        <p>
          Initial lesson: {new Date(t.lesson.completed_at).toLocaleString()}
          <br />
          Seven-day assessment: {new Date(t.lesson.assessment_due_at).toLocaleString()}
          <br />
          Planned review excluded from {new Date(
            t.lesson.review_exclusion_at,
          ).toLocaleString()}{' '}
          until the assessment is saved.
        </p>
      )}
      {t.planned_review_excluded ? (
        <p className="notice">
          The preceding 48-hour review exclusion is active. Linked planned reviews and fresh checks
          are withheld. Record outside exposure and any self-directed study; the app cannot
          establish absence of outside help.
        </p>
      ) : null}
      {workspace.offline && t.lesson && !t.responses ? (
        <details className="offline-banner">
          <summary>Simulated comparison time controls</summary>
          <p>Development only. Advancing time never produces real delayed-learning evidence.</p>
          <button
            disabled={busy}
            onClick={() => void call('development_advance', { point: 'exclusion' })}
          >
            Simulate start of review exclusion
          </button>
          <button
            disabled={busy}
            onClick={() => void call('development_advance', { point: 'assessment' })}
          >
            Simulate seven-day assessment time
          </button>
        </details>
      ) : null}
      <details>
        <summary>Record outside exposure, time, costs or usability</summary>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void call('event', {
              kind: eventKind,
              note,
              seconds,
              cost_microusd: Math.round(cost * 1e6),
            }).then(() => setNote(''));
          }}
        >
          <label>
            Observation
            <select value={eventKind} onChange={(e) => setEventKind(e.target.value)}>
              {[
                'outside_exposure',
                'waiting',
                'unwanted_help',
                'repeated_question',
                'abandoned',
                'voluntary_return',
                'manual_time_and_cost',
              ].map((k) => (
                <option key={k} value={k}>
                  {k.replaceAll('_', ' ')}
                </option>
              ))}
            </select>
          </label>
          <label>
            What happened?
            <textarea
              value={note}
              maxLength={5000}
              required
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <label>
            Manually reported seconds
            <input
              type="number"
              min={0}
              max={86400}
              value={seconds}
              onChange={(e) => setSeconds(Number(e.target.value))}
            />
          </label>
          <label>
            Manually reported cost in USD (0 if none)
            <input
              type="number"
              min={0}
              max={1000}
              step=".0001"
              value={cost}
              onChange={(e) => setCost(Number(e.target.value))}
            />
          </label>
          <button disabled={busy}>Save observation</button>
        </form>
      </details>
      {t.events.length ? (
        <details>
          <summary>Recorded intervals and observations ({t.events.length})</summary>
          <ul>
            {t.events.map((e, i) => (
              <li key={i}>
                {e.kind.replaceAll('_', ' ')} · {e.seconds ?? 0}s {e.note}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      {!t.exam ? (
        <button
          disabled={busy || !t.assessment_ready || !!t.timer}
          onClick={() => void call('begin_exam')}
        >
          Begin held-out assessment
        </button>
      ) : null}
      {t.exam && !t.responses ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void call('submit_exam', {
              outside_help: help,
              responses: t.items!.map((i) => ({
                item_id: i.id,
                response: answers[i.id] ?? '',
                confidence: confidence[i.id] ?? 50,
              })),
            });
          }}
        >
          <h5>Unaided held-out assessment</h5>
          <p>
            Answer without the tutor, prior solutions, or sources. Scoring material opens after
            saving both answers. This test is itself a learning exposure; record later follow-ups
            separately.
          </p>
          {t.items?.map((i) => (
            <fieldset key={i.id}>
              <legend>{i.form === 'familiar' ? 'Familiar form' : 'Changed context'}</legend>
              <p>{i.prompt}</p>
              <label>
                Your held-out response
                <textarea
                  required
                  maxLength={12000}
                  value={answers[i.id] ?? ''}
                  onChange={(e) => setAnswers({ ...answers, [i.id]: e.target.value })}
                />
              </label>
              <label>
                Confidence, 0–100
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={confidence[i.id] ?? 50}
                  onChange={(e) => setConfidence({ ...confidence, [i.id]: Number(e.target.value) })}
                />
              </label>
            </fieldset>
          ))}
          <label className="check">
            <input type="checkbox" checked={help} onChange={(e) => setHelp(e.target.checked)} />I
            used help during this assessment
          </label>
          <button disabled={busy}>Save held-out responses</button>
        </form>
      ) : null}
      {t.responses ? (
        <details>
          <summary>Saved held-out responses and separate scoring</summary>
          <p>
            {t.responses.outside_help
              ? 'Help was reported; this is assisted performance.'
              : 'No help reported by the learner; absence of outside help is not independently verified.'}
          </p>
          {t.responses.responses.map((a) => (
            <blockquote key={a.item_id}>
              {a.response}
              <br />
              Confidence {a.confidence}/100
            </blockquote>
          ))}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void call('score', {
                reviewer,
                blinded,
                scores: t.items!.map((i) => ({
                  item_id: i.id,
                  points: scores[i.id] ?? 0,
                  notes: scoreNotes[i.id] ?? '',
                })),
              });
            }}
          >
            {t.items?.map((i) => (
              <fieldset key={i.id}>
                <legend>
                  {i.form} · maximum {i.max_points}
                </legend>
                <p>Rubric: {i.rubric}</p>
                <p>Reference: {i.reference_solution}</p>
                <p>Review provenance: {i.review_provenance}</p>
                <label>
                  Manual points
                  <input
                    type="number"
                    min={0}
                    max={i.max_points}
                    step=".5"
                    value={scores[i.id] ?? 0}
                    onChange={(e) => setScores({ ...scores, [i.id]: Number(e.target.value) })}
                  />
                </label>
                <label>
                  Scoring rationale
                  <textarea
                    required
                    maxLength={3000}
                    value={scoreNotes[i.id] ?? ''}
                    onChange={(e) => setScoreNotes({ ...scoreNotes, [i.id]: e.target.value })}
                  />
                </label>
              </fieldset>
            ))}
            <label>
              Reviewer or self-score label
              <input
                value={reviewer}
                maxLength={300}
                required
                onChange={(e) => setReviewer(e.target.value)}
              />
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={blinded}
                onChange={(e) => setBlinded(e.target.checked)}
              />
              Reviewer was blinded to the study condition
            </label>
            <button disabled={busy}>Append manual score</button>
          </form>
          {t.scores.map((s, i) => (
            <p key={i}>
              {s.reviewer} · {s.scores.reduce((n, x) => n + x.points, 0)} points ·{' '}
              {new Date(s.recorded_at).toLocaleString()} · user-entered review, unverified by the
              app
            </p>
          ))}
        </details>
      ) : null}
      <p className="small">
        {t.recorded_exposures_in_exclusion_window} linked help exposures recorded in the exclusion
        window. Inspect the export together with outside-exposure reports.
      </p>
    </article>
  );
}
export function Evaluation({ workspace, close }: { workspace: Workspace; close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [view, setView] = useState<EvaluationView | null>(null),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState('');
  const [a, setA] = useState(blankTopic),
    [b, setB] = useState(blankTopic),
    [baseline, setBaseline] = useState(''),
    [model, setModel] = useState(''),
    [rationale, setRationale] = useState(''),
    [criterion, setCriterion] = useState(''),
    [minutes, setMinutes] = useState(30),
    [distinct, setDistinct] = useState(false),
    [deleteId, setDeleteId] = useState(''),
    [deleteText, setDeleteText] = useState('');
  const refresh = async () => setView(await evaluationCall<EvaluationView>('view'));
  useEffect(() => {
    dialog.current?.showModal();
    void refresh().catch((e) => setNotice(String(e)));
    return () => dialog.current?.close();
  }, []);
  const run = async (method: string, q: Record<string, unknown>) => {
    setBusy(true);
    setNotice('');
    try {
      if (method === 'development_advance') {
        await command('development_action', {
          sessionId: workspace.active_session_id,
          action: `advance_comparison:${q.topicId}:${q.point}`,
        });
      } else {
        await evaluationCall(method, q);
      }
      await refresh();
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <dialog
      className="evaluation-dialog"
      ref={dialog}
      aria-labelledby="evaluation-title"
      onCancel={close}
    >
      <header className="dialog-header">
        <h2 id="evaluation-title">Personal learning comparison</h2>
        <button className="dialog-close" aria-label="Close comparison" onClick={close}>
          <Icon name="close" />
          <span>Close</span>
        </button>
      </header>
      <p>{view?.notice}</p>
      {workspace.offline ? (
        <p className="offline-banner">
          OFFLINE DEVELOPMENT · every record is simulated engineering evidence, including elapsed
          time and manual scores.
        </p>
      ) : null}
      <p>
        Prepare matched topics and held-out items before tutoring. Keep total time, provider, model
        and sources comparable; document differences. Question and solution content stays outside
        tutor retrieval. This tool collects records and makes no learning-benefit claim.
      </p>
      <button
        disabled={busy}
        onClick={() => {
          setBusy(true);
          void command<string | null>('export_evaluation')
            .then((p) => {
              if (p)
                setNotice(
                  `Private comparison export saved to ${p}. It includes held-out questions and solutions; share deliberately.`,
                );
            })
            .catch((e) => setNotice(String(e)))
            .finally(() => setBusy(false));
        }}
      >
        Export comparison records
      </button>
      <details>
        <summary>Register a matched topic pair</summary>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void run('register', {
              topic_a: a,
              topic_b: b,
              baseline_method: baseline,
              baseline_model: model,
              matched_rationale: rationale,
              decision_criterion: criterion,
              time_allowance_minutes: minutes,
              confirm_distinct_topics: distinct,
            });
          }}
        >
          <TopicEditor
            label="Topic A"
            value={a}
            onChange={setA}
            sources={workspace.sources.documents}
          />
          <TopicEditor
            label="Topic B"
            value={b}
            onChange={setB}
            sources={workspace.sources.documents}
          />
          <label>
            Strong baseline study method
            <textarea
              value={baseline}
              required
              maxLength={3000}
              onChange={(e) => setBaseline(e.target.value)}
            />
          </label>
          <label>
            Baseline provider and model (or no model)
            <input
              value={model}
              required
              maxLength={3000}
              onChange={(e) => setModel(e.target.value)}
            />
          </label>
          <label>
            Why these topics are comparable and distinct
            <textarea
              value={rationale}
              required
              maxLength={3000}
              onChange={(e) => setRationale(e.target.value)}
            />
          </label>
          <label>
            Personal decision criterion before collecting outcomes
            <textarea
              value={criterion}
              required
              maxLength={3000}
              onChange={(e) => setCriterion(e.target.value)}
            />
          </label>
          <label>
            Total time allowance per topic, including review and setup (minutes)
            <input
              type="number"
              min={5}
              max={180}
              value={minutes}
              onChange={(e) => setMinutes(Number(e.target.value))}
            />
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={distinct}
              onChange={(e) => setDistinct(e.target.checked)}
            />
            These are distinct matched topics, not multiple objectives from one lesson
          </label>
          <p className="small">
            The core freezes material and versions, randomly assigns Study Harness versus baseline
            to A/B, and draws study order. Registration records your judgment of comparability; it
            does not validate it.
          </p>
          <button disabled={busy || !distinct}>Freeze and assign pair</button>
        </form>
      </details>
      {view?.pairs.map((p) => (
        <section className="comparison-pair" key={p.id}>
          <h3>Matched pair · {p.topics.map((t) => t.name).join(' / ')}</h3>
          <p>
            Assigned order: {p.order}. Baseline: {p.registration.baseline_method}. Equal allowance:{' '}
            {p.registration.time_allowance_minutes} minutes per topic.
          </p>
          <p>Decision criterion: {p.registration.decision_criterion}</p>
          {p.topics.map((t) => (
            <TopicRecord key={t.id} topic={t} workspace={workspace} run={run} busy={busy} />
          ))}
          <details>
            <summary>Frozen intervention versions</summary>
            <dl>
              {Object.entries(p.versions).map(([k, v]) => (
                <div key={k}>
                  <dt>{k.replaceAll('_', ' ')}</dt>
                  <dd>
                    <code>{v}</code>
                  </dd>
                </div>
              ))}
            </dl>
          </details>
          <details>
            <summary>Delete this comparison pair</summary>
            <p>
              Both conditions, observations, held-out material, responses and scores will be
              removed. Associated study sessions remain. App-managed migration backups are also
              removed.
            </p>
            <label>
              Type DELETE for this pair
              <input
                value={deleteId === p.id ? deleteText : ''}
                onChange={(e) => {
                  setDeleteId(p.id);
                  setDeleteText(e.target.value);
                }}
              />
            </label>
            <button
              className="danger"
              disabled={busy || deleteId !== p.id || deleteText !== 'DELETE'}
              onClick={() => {
                setBusy(true);
                void command('delete_records', {
                  scope: 'comparison',
                  targetId: p.id,
                  confirmation: deleteText,
                })
                  .then(refresh)
                  .then(() => {
                    setDeleteId('');
                    setDeleteText('');
                  })
                  .catch((e) => setNotice(String(e)))
                  .finally(() => setBusy(false));
              }}
            >
              Delete comparison pair
            </button>
          </details>
        </section>
      ))}
      <p role="status">{busy ? 'Updating local comparison records…' : notice}</p>
    </dialog>
  );
}
