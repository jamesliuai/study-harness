import { useEffect, useState } from 'react';
import { Icon, LogoMark } from './Icon';

/** Whole seconds since mount, updated once a second. */
function useElapsed() {
  const [start] = useState(() => performance.now());
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setSeconds(Math.floor((performance.now() - start) / 1000)), 1000);
    return () => clearInterval(id);
  }, [start]);
  return seconds;
}
const clock = (s: number) => (s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`);

/**
 * A request in flight, shown where its result will appear. The learner's work is already saved; cancelling keeps it
 * and records the request's usage.
 */
export function Thinking({ label, onCancel }: { label: string; onCancel: () => void }) {
  const seconds = useElapsed();
  return (
    <li className="turn tutor thinking" role="status" aria-live="polite">
      <span className="avatar" aria-hidden="true">
        <LogoMark />
      </span>
      <div className="turn-body">
        <p className="thinking-row">
          <span className="spinner" aria-hidden="true" />
          <span>{label}</span>
          <span className="elapsed numeric" aria-hidden="true">
            {clock(seconds)}
          </span>
          <button className="text-button" onClick={onCancel}>
            Cancel
          </button>
        </p>
      </div>
    </li>
  );
}

/** The same request status for screens without a conversation (home, the plan). */
export function RequestPill({ label, onCancel }: { label: string; onCancel: () => void }) {
  const seconds = useElapsed();
  return (
    <div className="request-pill" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <span>{label}</span>
      <span className="elapsed numeric" aria-hidden="true">
        {clock(seconds)}
      </span>
      <button className="text-button" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}

/** A tutor reply that did not arrive; the saved answer is untouched and nothing resends on its own. */
export function FailedReply({
  message,
  busy,
  onRetry,
  onDismiss,
}: {
  message: string;
  busy: boolean;
  onRetry: () => void;
  onDismiss: () => void;
}) {
  return (
    <li className="turn tutor failed" role="alert">
      <span className="avatar" aria-hidden="true">
        <Icon name="alert" />
      </span>
      <div className="turn-body">
        <p>
          <strong>Your answer is saved, but the tutor’s reply didn’t arrive.</strong> {message}
        </p>
        <div className="actions">
          <button className="primary compact" disabled={busy} onClick={onRetry}>
            <Icon name="refresh" />
            Try again
          </button>
          <button className="text-button" onClick={onDismiss}>
            Not now
          </button>
        </div>
      </div>
    </li>
  );
}
