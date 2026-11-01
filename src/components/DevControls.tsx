import { useEffect, useRef } from 'react';

/**
 * Offline development only: the simulated-response scenario, simulated time and versioned diagnostic cases. Nothing
 * here exists in a production build, makes provider requests or judges an answer.
 */
export function DevControls({
  scenarios,
  scenario,
  onScenario,
  cases,
  caseId,
  onCase,
  now,
  working,
  canLoadTranscript,
  canAdvanceTime,
  onAction,
}: {
  scenarios: string[];
  scenario: string;
  onScenario: (s: string) => void;
  cases: { id: string; category: string; scenario: string }[];
  caseId: string;
  onCase: (id: string) => void;
  now: string | null;
  working: boolean;
  canLoadTranscript: boolean;
  canAdvanceTime: boolean;
  onAction: (action: string) => void;
}) {
  const root = useRef<HTMLDetailsElement>(null);
  // Close on a click outside, like a popover.
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (root.current?.open && !root.current.contains(e.target as Node)) root.current.open = false;
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, []);
  return (
    <details className="dev-controls" ref={root}>
      <summary title="Offline development · simulated responses">
        <span className="dev-dot" aria-hidden="true" />
        Simulated
      </summary>
      <div className="dev-panel" aria-label="Offline development controls">
        <p className="dev-note">
          <strong>Offline development · simulated responses.</strong> No provider requests or
          billing; separate development data. The scenario decides the outcome — it does not judge
          your answer or test speech recognition.
        </p>
        <label htmlFor="scenario">Response scenario</label>
        <select id="scenario" value={scenario} onChange={(e) => onScenario(e.target.value)}>
          {scenarios.map((s) => (
            <option key={s} value={s}>
              {s.replaceAll('_', ' ')}
            </option>
          ))}
        </select>
        <div className="actions">
          <button
            className="compact"
            disabled={working || !canLoadTranscript}
            onClick={() => onAction('ambiguous_transcript')}
          >
            Load simulated transcript
          </button>
          <button
            className="compact"
            disabled={working || !canAdvanceTime}
            onClick={() => onAction('advance_to_review')}
          >
            Simulate time to next review
          </button>
        </div>
        <p className="dev-note">
          Development clock: {now ? new Date(now).toLocaleString() : 'current time'}. Simulated
          elapsed time never counts as real delayed learning.
        </p>
        <label htmlFor="diagnostic-case">Versioned diagnostic case</label>
        <select id="diagnostic-case" value={caseId} onChange={(e) => onCase(e.target.value)}>
          {cases.map((c) => (
            <option key={c.id} value={c.id}>
              {c.id.replaceAll('_', ' ')}
            </option>
          ))}
        </select>
        <div className="actions">
          <button
            className="compact"
            disabled={working || !caseId}
            onClick={() => onAction(`case:${caseId}`)}
          >
            Load diagnostic case
          </button>
        </div>
        <p className="dev-note">
          Creates a separate simulated session and saves the case response through the core. Fixture
          outcomes test contracts and integrity, not live model quality.
        </p>
      </div>
    </details>
  );
}
