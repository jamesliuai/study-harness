import type { ReactNode } from 'react';
import type { CapabilityProgress, LearningView, StepOption } from '../generated/contracts';
import { Icon, type IconName } from './Icon';

function Tile({
  tone,
  icon,
  title,
  items,
  empty,
}: {
  tone: string;
  icon: IconName;
  title: string;
  items: string[];
  empty: string;
}) {
  return (
    <section className="tile" data-tone={tone} data-empty={!items.length || undefined}>
      <h3>
        <Icon name={icon} />
        {title}
      </h3>
      {items.length ? (
        <ul>
          {items.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      ) : (
        <p>{empty}</p>
      )}
    </section>
  );
}

/**
 * The end of a lesson: what was observed, what needed help and what has not been checked — labels from recorded
 * evidence, never a score — then the planned review and the next capability.
 */
export function WrapUp({
  learning,
  next,
  lastInPlan,
  steps,
  busy,
  record,
  onRecompute,
  onBegin,
  onStep,
}: {
  learning: LearningView;
  next: CapabilityProgress | undefined;
  lastInPlan: boolean;
  steps: StepOption[];
  busy: boolean;
  record: ReactNode;
  onRecompute: () => void;
  onBegin: (objectiveId: string, mode: 'learn' | 'check') => void;
  onStep: (step: StepOption) => void;
}) {
  const w = learning.wrap_up;
  const scheduled = learning.reviews.find((r) => r.status === 'scheduled');
  const due = !!scheduled && new Date(scheduled.due_at) <= new Date(learning.now);
  const review = steps.find((s) => s.id === 'review');
  const another = steps.find((s) => s.id === 'fresh_check');
  return (
    <section className="wrap-up" aria-labelledby="wrap-up-title">
      <header className="wrap-up-head">
        <p className="eyebrow">
          <Icon name="checkCircle" />
          Where you stand
        </p>
        <h2 id="wrap-up-title" className="display">
          {learning.objective_outcome}
        </h2>
      </header>
      {learning.summary?.stale ? (
        <div className="notice" data-tone="warn">
          <Icon name="refresh" />
          <p>A correction or challenge changed the evidence behind this summary.</p>
          <button className="compact" disabled={busy} onClick={onRecompute}>
            Recompute
          </button>
        </div>
      ) : null}
      <div className="tiles">
        <Tile
          tone="own"
          icon="checkCircle"
          title="Shown on your own"
          items={w.demonstrated}
          empty="Nothing yet — no resolved independent check."
        />
        <Tile
          tone="help"
          icon="lifebuoy"
          title="Done with help"
          items={w.needed_help}
          empty="No supported work was assessed."
        />
        <Tile
          tone="explained"
          icon="book"
          title="Explained to you"
          items={w.explained}
          empty="No explanations or hints were used."
        />
        <Tile
          tone="unchecked"
          icon="circleDashed"
          title="Not yet checked"
          items={w.unchecked}
          empty="Everything here has been checked."
        />
      </div>
      {scheduled ? (
        <div className="review-line" data-due={due || undefined}>
          <Icon name="clock" />
          <p>
            <strong>
              {due ? 'Your review is ready now' : 'Next review'} ·{' '}
              {new Date(scheduled.due_at).toLocaleString([], {
                weekday: 'short',
                month: 'short',
                day: 'numeric',
                hour: 'numeric',
                minute: '2-digit',
              })}
            </strong>
            <span>
              {scheduled.reason.replace(/\.?\s*$/, '.')}
              {scheduled.simulated ? ' Simulated time; no retention claim.' : ''} Overdue means
              ready to revisit, not failed.
            </span>
          </p>
          {review ? (
            <button
              className="primary compact"
              disabled={busy}
              title={review.reason}
              onClick={() => onStep(review)}
            >
              {review.label}
              <Icon name="arrow" />
            </button>
          ) : null}
        </div>
      ) : null}
      {next ? (
        <div className="next-capability">
          <div>
            <p className="eyebrow">Next in your plan</p>
            <p className="next-capability-outcome">{next.outcome}</p>
          </div>
          <div className="actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => onBegin(next.objective_id, 'learn')}
            >
              Continue
              <Icon name="arrow" />
            </button>
            <button
              className="ghost"
              disabled={busy}
              title="Go straight to a fresh check. Nothing is assumed known until you show it."
              onClick={() => onBegin(next.objective_id, 'check')}
            >
              I know it — check me
            </button>
          </div>
        </div>
      ) : lastInPlan ? (
        <p className="wrap-note">
          That was the last capability in this plan. Come back for the planned reviews, or change
          the plan to go further.
        </p>
      ) : null}
      {another ? (
        <div className="wrap-more">
          <button
            className="ghost compact"
            disabled={busy}
            title={another.reason}
            onClick={() => onStep(another)}
          >
            <Icon name="target" />
            {another.label}
          </button>
          <span>Early practice doesn’t count as the later review.</span>
        </div>
      ) : null}
      <details className="disclosure record-disclosure">
        <summary>All attempts, feedback and help in this session</summary>
        {record}
      </details>
    </section>
  );
}
