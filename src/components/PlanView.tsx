import { useState } from 'react';
import type { CapabilityProgress, GoalView } from '../generated/contracts';
import { RichText } from './MathText';
import { Icon } from './Icon';

const ASSESSMENT: Record<string, string> = {
  numeric: 'a computed value',
  expression: 'a symbolic result',
  explanation: 'an explanation',
  proof: 'a proof',
  derivation: 'a derivation',
  code_reasoning: 'code reasoning',
  classification: 'a classification',
  short_answer: 'a short answer',
};

/** Colour carries meaning only: shown on your own, needs a fresh check, started. */
const tone = (c: CapabilityProgress) =>
  c.status !== 'planned'
    ? 'inactive'
    : c.label.startsWith('Shown')
      ? 'ok'
      : c.label === 'Needs a fresh check'
        ? 'warn'
        : c.session_id
          ? 'started'
          : 'todo';

/**
 * The editable study plan. Capabilities are concrete outcomes with stable identities; progress labels come from each
 * capability's own recorded work. Every change — a revision, a skip, an answered question — is a new plan revision.
 */
export function PlanView({
  goal,
  busy,
  currentObjective,
  compact = false,
  canRevise,
  onBegin,
  onDecision,
  onRevise,
  onOpen,
}: {
  goal: GoalView;
  busy: boolean;
  currentObjective?: string;
  compact?: boolean;
  canRevise: boolean;
  onBegin: (objectiveId: string, mode: 'learn' | 'check') => void;
  onDecision: (objectiveId: string, decision: 'skip' | 'restore') => void;
  onRevise: (instruction: string) => void;
  onOpen: (sessionId: string) => void;
}) {
  const [change, setChange] = useState('');
  const generated = goal.origin === 'generated';
  const blocking = !!goal.clarification?.blocking;
  const next = goal.next_objective_id;
  const planned = goal.capabilities.filter((c) => c.status === 'planned');
  const shown = planned.filter((c) => tone(c) === 'ok').length;
  return (
    <section className={compact ? 'plan plan-compact' : 'plan'} aria-label="Study plan">
      <header className="plan-head">
        <p className="eyebrow">{generated ? 'Your plan' : 'Sample unit'}</p>
        {compact ? <h2>{goal.title}</h2> : <h1 className="display plan-title">{goal.title}</h1>}
        <p className="plan-meta">
          <span className="tag">
            <Icon name="list" />
            {planned.length} capabilit{planned.length === 1 ? 'y' : 'ies'}
          </span>
          <span className="tag">
            <Icon name="clock" />
            about {goal.minutes} min
          </span>
          {generated ? null : (
            <span className="tag" title="Hand-written tasks with analytic checks">
              <Icon name="checkCircle" />
              Authored and checked
            </span>
          )}
          {goal.revision > 1 ? (
            <span className="tag">
              <Icon name="review" />
              revision {goal.revision}
            </span>
          ) : null}
        </p>
        {planned.length ? (
          <div
            className="plan-progress"
            role="img"
            aria-label={`${shown} of ${planned.length} shown on your own`}
          >
            <span className="plan-bar">
              {planned.map((c) => (
                <span key={c.objective_id} data-tone={tone(c)} />
              ))}
            </span>
            <span className="plan-progress-text">
              {shown} of {planned.length} shown on your own
            </span>
          </div>
        ) : null}
        {goal.summary && !compact ? (
          <RichText text={goal.summary} className="plan-summary prose" />
        ) : null}
        {goal.level || goal.background.length ? (
          <p className="plan-assumes">
            {goal.level ? (
              <>
                Pitched at <strong>{goal.level}</strong>.{' '}
              </>
            ) : null}
            {goal.background.length ? <>Assumes {goal.background.join('; ')}. </> : null}
            {canRevise ? 'Not right? Say so below and the plan changes.' : null}
          </p>
        ) : null}
      </header>
      {goal.clarification ? (
        <div className="clarify" role={blocking ? 'alert' : undefined}>
          <p className="eyebrow">
            <Icon name="question" />
            {blocking ? 'One question before planning' : 'Optional question'}
          </p>
          <p className="clarify-question">{goal.clarification.question}</p>
          <div className="actions">
            {goal.clarification.options.map((o) => (
              <button
                key={o}
                className="compact"
                disabled={busy || !canRevise}
                onClick={() =>
                  onRevise(`Answer to your question “${goal.clarification!.question}”: ${o}`)
                }
              >
                {o}
              </button>
            ))}
          </div>
        </div>
      ) : null}
      <ol className="capabilities">
        {goal.capabilities.map((c, i) => {
          const isNext = c.objective_id === next && !blocking;
          const current = c.objective_id === currentObjective;
          const inactive = c.status !== 'planned';
          return (
            <li
              key={c.objective_id}
              data-status={c.status}
              data-tone={tone(c)}
              data-current={current || undefined}
              data-next={isNext || undefined}
            >
              <span className="capability-node" aria-hidden="true">
                {tone(c) === 'ok' ? <Icon name="check" /> : i + 1}
              </span>
              <div className="capability-body">
                <p className="capability-outcome">{c.outcome}</p>
                {c.why && !compact ? <p className="capability-why">{c.why}</p> : null}
                <p className="capability-meta">
                  <span className="status-pill" data-tone={tone(c)}>
                    {current ? 'This session · ' : ''}
                    {c.label}
                  </span>
                  {c.detail ? <span>{c.detail}</span> : null}
                  {!compact ? (
                    <>
                      <span>~{c.minutes} min</span>
                      <span>shown by {ASSESSMENT[c.assessment] ?? c.assessment}</span>
                      {c.visual ? (
                        <span>
                          <Icon name="chart" /> visual
                        </span>
                      ) : null}
                    </>
                  ) : null}
                </p>
                <div className="capability-actions">
                  {c.session_id && !current ? (
                    <button
                      className="ghost compact"
                      disabled={busy}
                      onClick={() => onOpen(c.session_id!)}
                    >
                      Open
                      <Icon name="arrow" />
                    </button>
                  ) : null}
                  {!c.session_id && !inactive ? (
                    <>
                      <button
                        className={isNext ? 'primary compact' : 'ghost compact'}
                        disabled={busy || blocking}
                        onClick={() => onBegin(c.objective_id, 'learn')}
                      >
                        {isNext ? 'Start' : 'Start this one'}
                        {isNext ? <Icon name="arrow" /> : null}
                      </button>
                      <button
                        className="text-button"
                        disabled={busy || blocking}
                        title="Go straight to a fresh check. Nothing is assumed known until you show it."
                        onClick={() => onBegin(c.objective_id, 'check')}
                      >
                        I know this — check me
                      </button>
                      <button
                        className="text-button"
                        disabled={busy}
                        title="Leave it out for now. Skipping records nothing as known."
                        onClick={() => onDecision(c.objective_id, 'skip')}
                      >
                        Skip
                      </button>
                    </>
                  ) : null}
                  {c.status === 'skipped' ? (
                    <button
                      className="text-button"
                      disabled={busy}
                      onClick={() => onDecision(c.objective_id, 'restore')}
                    >
                      Put back in the plan
                    </button>
                  ) : null}
                </div>
              </div>
            </li>
          );
        })}
      </ol>
      {canRevise ? (
        <form
          className="plan-change"
          onSubmit={(e) => {
            e.preventDefault();
            if (change.trim()) {
              onRevise(change.trim());
              setChange('');
            }
          }}
        >
          <label htmlFor={compact ? 'plan-change-compact' : 'plan-change'}>Change the plan</label>
          <div className="input-row">
            <input
              id={compact ? 'plan-change-compact' : 'plan-change'}
              value={change}
              maxLength={1500}
              disabled={busy}
              placeholder="e.g. “I’m in high school”, “skip the proof”, “add units and dimensions”"
              onChange={(e) => setChange(e.target.value)}
            />
            <button className="compact" disabled={busy || !change.trim()}>
              Revise
            </button>
          </div>
          <p className="hint-text">
            Capabilities you keep keep their records. Work already done is never dropped silently.
          </p>
        </form>
      ) : null}
      {goal.sources.length ? (
        <details className="disclosure">
          <summary>
            Built from your material ({goal.sources.length} source
            {goal.sources.length === 1 ? '' : 's'})
          </summary>
          {goal.source_note && goal.source_note !== 'none' ? (
            <p className="hint-text">{goal.source_note}</p>
          ) : null}
          <ul className="plan-sources">
            {goal.sources.map((s) => (
              <li key={s.id}>
                <strong>{s.title}</strong>
                {s.warnings.map((w) => (
                  <p key={w} className="notice" data-tone="warn">
                    {w}
                  </p>
                ))}
                <p className="source-text">{s.excerpt}</p>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      {goal.notes && !compact ? (
        <p className="plan-notes">
          <strong>Notes:</strong> {goal.notes}
        </p>
      ) : null}
      {!compact ? (
        <details className="disclosure">
          <summary>Where this plan came from</summary>
          <p className="hint-text">{goal.provenance}</p>
          <p className="hint-text">
            Revision {goal.revision}: {goal.reason}
          </p>
          <p className="hint-text">Goal, in your words: “{goal.goal_text}”</p>
        </details>
      ) : null}
    </section>
  );
}
