import type { StepOption } from '../generated/contracts';
import { Icon, type IconName } from './Icon';

const STEP_ICON: Record<string, IconName> = {
  teach: 'book',
  continue: 'arrow',
  fresh_check: 'target',
  review: 'review',
  prerequisite: 'stairs',
  another: 'shuffle',
};

/** Moving to a new task discards text that was never sent; the learner confirms it first. */
export function DraftConfirm({
  step,
  busy,
  onConfirm,
  onCancel,
}: {
  step: StepOption;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="confirm-row" role="alert">
      <Icon name="alert" />
      <p>
        Your unsent draft will be discarded before <strong>{step.label.toLowerCase()}</strong>.
      </p>
      <button className="primary compact" disabled={busy} onClick={onConfirm}>
        Discard draft and continue
      </button>
      <button className="text-button" onClick={onCancel}>
        Keep writing
      </button>
    </div>
  );
}

/**
 * Where to go from here. The core offers the eligible steps and at most one recommendation with its reason; the
 * learner always chooses. Before an answer the alternatives stay quiet so the task itself is the obvious next move.
 */
export function NextStep({
  steps,
  note,
  hasAnswer,
  busy,
  pending,
  onStep,
  onConfirm,
  onCancel,
}: {
  steps: StepOption[];
  note: string | null;
  hasAnswer: boolean;
  busy: boolean;
  /** A step waiting for the learner to confirm that their unsent draft may be discarded. */
  pending: StepOption | null;
  onStep: (step: StepOption) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  // A suggestion is put forward only after an attempt on this task: before one, the task is the next move.
  const suggested = steps.find((s) => s.recommended && s.kind !== 'unavailable');
  const recommended = hasAnswer ? suggested : undefined;
  const others = steps.filter(
    (s) => s !== recommended && s.kind !== 'support' && (recommended || s.kind !== 'unavailable'),
  );
  const option = (s: StepOption) => (
    <button
      key={s.id}
      className="ghost compact"
      disabled={busy || s.kind === 'unavailable'}
      title={s.reason}
      onClick={() => onStep(s)}
    >
      {STEP_ICON[s.id] ? <Icon name={STEP_ICON[s.id]} /> : null}
      {s.label}
    </button>
  );
  const confirm = pending ? (
    <DraftConfirm step={pending} busy={busy} onConfirm={onConfirm} onCancel={onCancel} />
  ) : null;
  if (!recommended && !others.length)
    return confirm ? <li className="next-step">{confirm}</li> : null;
  if (!recommended)
    return (
      <li className="next-step" data-quiet="">
        <div className="next-step-row">
          <span className="next-step-lead">{hasAnswer ? 'Next' : 'Not ready to answer?'}</span>
          {others.map(option)}
        </div>
        {confirm}
      </li>
    );
  return (
    <li className="next-step">
      <div className="next-step-card">
        <p className="next-step-eyebrow">Suggested next step</p>
        <div className="next-step-main">
          <button
            className="primary"
            disabled={busy}
            title={recommended.reason}
            onClick={() => onStep(recommended)}
          >
            {recommended.label}
            <Icon name="arrow" />
          </button>
          {note ? <p className="next-step-why">{note}</p> : null}
        </div>
        {others.length ? (
          <div className="next-step-row">
            <span className="next-step-lead">Or</span>
            {others.map(option)}
          </div>
        ) : null}
        {confirm}
      </div>
    </li>
  );
}
