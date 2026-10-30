import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { PrivacySettings, Workspace } from './generated/contracts';
import { command, money } from './api';
import { MathText, RichText } from './components/MathText';
import { Icon, LogoMark, type IconName } from './components/Icon';
import type { VisualHandle } from './components/RealVisualPanel';
import { Composer, type Intent } from './components/Composer';
import { Settings } from './components/Settings';
import { LearningRecord } from './components/LearningRecord';
import { Timeline } from './components/Timeline';
import { Home } from './components/Home';
import { PlanView } from './components/PlanView';
import { Menu, MenuItem } from './components/Menu';
import { ModelControls } from './components/ModelControls';
const RealVisualPanel = lazy(() =>
  import('./components/RealVisualPanel').then((m) => ({ default: m.RealVisualPanel })),
);
const SourcePanel = lazy(() =>
  import('./components/SourcePanel').then((m) => ({ default: m.SourcePanel })),
);
const Evaluation = lazy(() =>
  import('./components/Evaluation').then((m) => ({ default: m.Evaluation })),
);

// Learning-action identities behind the core's next-step options.
const STEP_ACTION: Record<string, string> = {
  teach: 'teach',
  continue: 'next',
  fresh_check: 'fresh_check',
  review: 'review',
  prerequisite: 'prerequisite',
  another: 'another',
};
const STEP_ICON: Record<string, IconName> = {
  teach: 'book',
  continue: 'arrow',
  fresh_check: 'target',
  review: 'review',
  prerequisite: 'stairs',
  another: 'shuffle',
};
// Steps that open a new task; for a generated capability the task is written and checked first.
const TRANSITIONS = ['teach', 'next', 'fresh_check', 'review', 'another'];
const CHECK_LABEL: Record<string, string> = {
  passed: 'passed',
  failed: 'failed',
  inconclusive: 'not conclusive',
  not_applicable: 'not applicable',
};
const STAGE_ICON: Record<string, IconName> = {
  diagnosis: 'pencil',
  exploration: 'chart',
  instruction: 'book',
  worked_example: 'stairs',
  practice: 'pencil',
  independent: 'target',
  review: 'review',
  summary: 'checkCircle',
  exploratory: 'question',
};
// Only a setup problem is worth a word; a working connection stays quiet.
const PROVIDER_ISSUE: Record<string, string> = {
  not_configured: 'Add your OpenAI key for tutor replies',
  attention: 'OpenAI needs attention',
};
// An unstarted session made by “New goal”: it is the home screen, not a session worth listing.
const isPlaceholder = (s: { title: string; revision: number }) =>
  s.title === 'New session' && s.revision === 0;

export default function App() {
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [tutorBusy, setTutorBusy] = useState(false);
  const [composerBusy, setComposerBusy] = useState(false);
  const [settings, setSettings] = useState(false);
  const [evaluation, setEvaluation] = useState(false);
  const [draftState, setDraftState] = useState('Saved locally');
  const [scenario, setScenario] = useState('correct');
  const [retry, setRetry] = useState<{ attemptId: string; message: string } | null>(null);
  const [diagnosticCases, setDiagnosticCases] = useState<
      { id: string; category: string; scenario: string }[]
    >([]),
    [diagnosticId, setDiagnosticId] = useState('');
  const [panel, setPanel] = useState<{
    kind: 'visual' | 'sources' | 'plan';
    activityId: string;
  } | null>(null);
  // A plan or task being written: what to show while waiting, and a deliberate retry if it fails.
  const [generating, setGenerating] = useState('');
  const [genRetry, setGenRetry] = useState<{ label: string; run: () => Promise<void> } | null>(
    null,
  );
  const [report, setReport] = useState<string | null>(null);
  const [sourceTarget, setSourceTarget] = useState<
    { id: string; anchor: string; activityId: string } | undefined
  >();
  const active = useRef<string | null>(null);
  // The newest workspace, so a deliberate retry uses the current session revision rather than a stale one.
  const latest = useRef<Workspace | null>(null);
  latest.current = workspace;
  // Answer/Question choice per task, kept when the composer remounts (for example after a transcription).
  const intentMemo = useRef<Record<string, Intent>>({});
  const startupReported = useRef(false);
  const scroller = useRef<HTMLElement>(null);
  const stack = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (workspace && !startupReported.current) {
      startupReported.current = true;
      requestAnimationFrame(() =>
        requestAnimationFrame(() => void command('record_startup').catch(() => {})),
      );
    }
  }, [workspace]);
  useEffect(() => {
    document.documentElement.style.fontSize = `${(16 * (workspace?.privacy.text_scale_percent ?? 100)) / 100}px`;
  }, [workspace?.privacy.text_scale_percent]);
  useEffect(() => {
    let mounted = true;
    if (workspace?.offline)
      void command<{ id: string; category: string; scenario: string }[]>('development_cases')
        .then((cases) => {
          if (mounted) {
            setDiagnosticCases(cases);
            setDiagnosticId(cases[0]?.id ?? '');
          }
        })
        .catch((e) => setError(String(e)));
    return () => {
      mounted = false;
    };
  }, [workspace?.offline]);
  const draftTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingDraft = useRef<{ sessionId: string; text: string } | null>(null);
  const draftWrites = useRef<Promise<void>>(Promise.resolve());
  const visual = useRef<VisualHandle>(null);
  const refresh = useCallback(async () => {
    const w = await command<Workspace>('workspace', { sessionId: active.current });
    active.current = w.active_session_id;
    setWorkspace(w);
  }, []);
  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, [refresh]);
  // A retry, a half-written report or another task's panel never carries over to a different task.
  const scope = workspace ? `${workspace.active_session_id}:${workspace.activity.id}` : '';
  useEffect(() => {
    setRetry(null);
    setGenRetry(null);
    setReport(null);
    setPanel((p) => (p && `${active.current}:${p.activityId}` === scope ? p : null));
  }, [scope]);
  // Keep the newest turn in view when the conversation grows or the task changes.
  const entries = workspace?.timeline.length ?? 0;
  const inFlow = !!workspace?.learning;
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = inFlow ? el.scrollHeight : 0;
  }, [entries, workspace?.activity.id, inFlow]);
  // When the pinned task and composer grow (a transcript to confirm, a retry notice), keep the newest turn in
  // view — unless the learner had scrolled up to read.
  useEffect(() => {
    const el = scroller.current;
    const pinned = stack.current;
    if (!el || !pinned || typeof ResizeObserver === 'undefined') return;
    let last = pinned.offsetHeight;
    const observer = new ResizeObserver(() => {
      const grew = pinned.offsetHeight - last;
      last = pinned.offsetHeight;
      if (grew > 0 && el.scrollHeight - el.scrollTop - el.clientHeight <= grew + 32)
        el.scrollTop = el.scrollHeight;
    });
    observer.observe(pinned);
    return () => observer.disconnect();
  }, [inFlow, workspace?.activity.id]);
  const flushDraft = async () => {
    if (draftTimer.current) clearTimeout(draftTimer.current);
    const d = pendingDraft.current;
    if (d) {
      pendingDraft.current = null;
      // Each write settles on its own: one refused draft is reported once and never blocks later actions.
      draftWrites.current = draftWrites.current
        .then(() => command('save_draft', d))
        .then(() => setDraftState('Saved locally'))
        .catch((e) => {
          setDraftState('Draft not saved');
          setError(String(e));
        });
    }
    await draftWrites.current;
  };
  const task = async (f: () => Promise<void>) => {
    setError('');
    setBusy(true);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const mutate = async (name: string, args: Record<string, unknown>) => {
    setError('');
    setBusy(true);
    try {
      await flushDraft();
      await visual.current?.flush();
      const w = await command<Workspace>(
        name,
        ['ask_tutor', 'advance', 'plan_goal', 'revise_plan', 'begin_capability'].includes(name)
          ? { ...args, scenario: workspace?.offline ? scenario : null }
          : args,
      );
      active.current = w.active_session_id;
      setWorkspace(w);
    } catch (e) {
      setError(String(e));
      if (name === 'ask_tutor') await refresh();
      throw e;
    } finally {
      setBusy(false);
    }
  };
  const run = (name: string, args: Record<string, unknown>) => {
    void mutate(name, args).catch(() => {});
  };
  const saveDraft = (text: string) => {
    if (!workspace) return;
    setDraftState('Saving draft…');
    pendingDraft.current = { sessionId: workspace.active_session_id, text };
    if (draftTimer.current) clearTimeout(draftTimer.current);
    draftTimer.current = setTimeout(() => void flushDraft(), 0);
  };
  /** Saves one local preference (model, effort, text size…); nothing is sent anywhere. */
  const setPreference = (privacy: PrivacySettings) =>
    void command('update_privacy', { privacy })
      .then(refresh)
      .catch((e) => setError(String(e)));
  if (!workspace)
    return (
      <main className="startup">
        <LogoMark />
        <h1>Study Harness</h1>
        <p role={error ? 'alert' : 'status'}>{error || 'Opening your local workspace…'}</p>
        {error ? (
          <button onClick={() => void refresh().catch((e) => setError(String(e)))}>
            Try again
          </button>
        ) : null}
      </main>
    );
  const w = workspace;
  const session = w.sessions.find((s) => s.id === w.active_session_id)!;
  const learning = w.learning;
  const flow = learning?.state;
  const stage = flow?.stage;
  const goal = w.goal;
  const home = !flow && !goal;
  const paused = !!flow && (flow.quiet || flow.stopped);
  const check = stage === 'independent' || stage === 'review';
  const working = busy || composerBusy || tutorBusy || !!generating;
  const currentAnswers = w.attempts.filter(
    (a) =>
      a.activity_id === w.activity.id &&
      learning?.conditions.find((c) => c.attempt_id === a.id)?.intent !== 'question',
  );
  const hasAnswer = currentAnswers.length > 0;
  const canGenerate = w.offline || w.credential.configured;
  // The live tutor needs a saved key; offline development returns labeled simulated responses.
  const tutorReady =
    canGenerate && !paused && (!flow || (learning?.allowed_actions.length ?? 0) > 0);
  const intents: Intent[] = !flow
    ? ['answer']
    : stage === 'exploratory' || stage === 'summary'
      ? ['question']
      : stage === 'diagnosis' && flow.diagnosis_count >= 3
        ? ['question']
        : stage === 'instruction' || stage === 'worked_example'
          ? ['question', 'answer']
          : ['answer', 'question'];
  const showComposer = (!flow && !!w.pending_transcript) || (!!flow && !flow.stopped);
  const sourcesOpen = panel?.kind === 'sources' && panel.activityId === w.activity.id;
  const planOpen = panel?.kind === 'plan' && !!goal && !!flow && !check;
  // A visual appears only when this task planned one; there is no default visual for unrelated topics.
  const visualAvailable =
    !!flow && ['diagnosis', 'exploration'].includes(flow.stage) && !paused && !!learning?.visual;
  const visualOpen = !sourcesOpen && !planOpen && visualAvailable;
  const scheduled = learning?.reviews.find((r) => r.status === 'scheduled');
  const due = scheduled && new Date(scheduled.due_at) <= new Date(learning!.now);
  const recommended = learning?.steps.find((s) => s.recommended);
  // Support options (a prerequisite review) live in the help menu unless they are the recommendation.
  const others = learning?.steps.filter((s) => !s.recommended && s.kind !== 'support') ?? [];
  const prerequisite = learning?.steps.find((s) => s.id === 'prerequisite' && !s.recommended);
  const nextCapability = goal?.capabilities.find((c) => c.objective_id === goal.next_objective_id);
  const plannedCapabilities = goal?.capabilities.filter((c) => c.status === 'planned') ?? [];
  const capabilityIndex = plannedCapabilities.findIndex(
    (c) => c.objective_id === w.activity.objective_id,
  );
  const askTutor = async (attemptId: string) => {
    setRetry(null);
    setError('');
    setTutorBusy(true);
    try {
      await flushDraft();
      await visual.current?.flush();
      const next = await command<Workspace>('ask_tutor', {
        sessionId: session.id,
        attemptId,
        scenario: w.offline ? scenario : null,
        visualSummary: visual.current?.summary() ?? null,
      });
      active.current = next.active_session_id;
      setWorkspace(next);
      // The core keeps (and bills) a reply it set aside without failing the call; surface that instead of silence.
      const request = next.requests
        .filter((r) => r.attempt_id === attemptId && r.kind === 'tutor')
        .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
      if (request?.status === 'stale')
        setRetry({
          attemptId,
          message:
            'The reply arrived after something else changed in this session, so it was set aside. Its cost is still recorded.',
        });
      else if (request?.status === 'rejected')
        setRetry({
          attemptId,
          message:
            'The reply did not pass the app’s checks, so it was set aside. Its cost is still recorded.',
        });
    } catch (e) {
      // The submission is already saved; a failed or refused request never resends automatically.
      setRetry({ attemptId, message: String(e) });
      await refresh().catch(() => {});
    } finally {
      setTutorBusy(false);
    }
  };
  /** Runs a request that writes a plan or a task, with visible progress, cancellation and a deliberate retry. */
  const generate = async (label: string, name: string, args: Record<string, unknown>) => {
    const run = async () => {
      setError('');
      setGenRetry(null);
      setGenerating(label);
      try {
        await flushDraft();
        await visual.current?.flush();
        const revision = latest.current?.sessions.find((s) => s.id === args.sessionId)?.revision;
        const next = await command<Workspace>(name, {
          ...args,
          ...('revision' in args && revision !== undefined ? { revision } : {}),
          scenario: w.offline ? scenario : null,
        });
        active.current = next.active_session_id;
        setWorkspace(next);
        setPanel(null);
      } catch (e) {
        setGenRetry({ label: String(e), run });
        await refresh().catch(() => {});
      } finally {
        setGenerating('');
      }
    };
    await run();
  };
  const action = (name: string, note?: string) => {
    if (!TRANSITIONS.includes(name)) {
      run('learning_action', { sessionId: session.id, revision: session.revision, action: name });
      return;
    }
    const label =
      name === 'fresh_check'
        ? 'Writing a fresh check…'
        : name === 'review'
          ? 'Writing your review problem…'
          : name === 'teach'
            ? 'Preparing an explanation from your answer…'
            : 'Preparing the next task…';
    void generate(learning?.generated ? label : 'Opening the next task…', 'advance', {
      sessionId: session.id,
      revision: session.revision,
      action: name,
      note: note ?? null,
    });
  };
  const begin = (objectiveId: string, mode: 'learn' | 'check') =>
    goal &&
    void generate(
      mode === 'check' ? 'Writing a fresh check…' : 'Writing an opening question…',
      'begin_capability',
      {
        sessionId: session.id,
        revision: session.revision,
        goalId: goal.id,
        objectiveId,
        mode,
      },
    );
  const decide = (objectiveId: string, decision: 'skip' | 'restore') =>
    goal &&
    run('plan_decision', {
      sessionId: session.id,
      revision: session.revision,
      goalId: goal.id,
      objectiveId,
      decision,
    });
  const revise = (instruction: string) =>
    goal &&
    void generate('Revising the plan…', 'revise_plan', {
      sessionId: session.id,
      revision: session.revision,
      goalId: goal.id,
      instruction,
    });
  const openSource = (id: string, anchor: string) =>
    void task(async () => {
      await visual.current?.flush();
      setSourceTarget({ id, anchor, activityId: w.activity.id });
      setPanel({ kind: 'sources', activityId: w.activity.id });
    });
  const openSettings = () =>
    void task(async () => {
      await flushDraft();
      await visual.current?.flush();
      setSettings(true);
    });
  const switchSession = (id: string | null) =>
    void task(async () => {
      await flushDraft();
      await visual.current?.flush();
      active.current = id;
      setRetry(null);
      setGenRetry(null);
      setPanel(null);
      setReport(null);
      await refresh();
    });
  // Sessions grouped by goal: a goal's capabilities are studied in their own sessions.
  const grouped = new Set(w.goals.flatMap((g) => g.session_ids));
  // “New goal” reuses an unstarted session instead of piling up empty ones.
  const newGoal = () => {
    if (home) {
      document.getElementById('goal')?.focus();
      return;
    }
    const empty = w.sessions.find((s) => !grouped.has(s.id) && isPlaceholder(s));
    if (empty) switchSession(empty.id);
    else run('new_session', { title: 'New session' });
  };
  const startReview = () =>
    void task(async () => {
      const first = w.unit.due_reviews[0];
      if (!first) {
        await refresh();
        return;
      }
      await flushDraft();
      await visual.current?.flush();
      const next = await command<Workspace>('workspace', { sessionId: first.session_id });
      active.current = next.active_session_id;
      setWorkspace(next);
      setPanel(null);
      const target = next.sessions.find((s) => s.id === first.session_id)!;
      setGenerating(
        next.learning?.generated ? 'Writing your review problem…' : 'Opening your review…',
      );
      try {
        const done = await command<Workspace>('advance', {
          sessionId: first.session_id,
          revision: target.revision,
          action: 'review',
          note: null,
          scenario: next.offline ? scenario : null,
        });
        active.current = done.active_session_id;
        setWorkspace(done);
      } finally {
        setGenerating('');
      }
    });
  const send = async (
    text: string,
    transcriptId: string | null,
    submissionId: string,
    expectedRevision: number,
    intent: Intent,
    tutor: boolean,
  ) => {
    setError('');
    setRetry(null);
    await flushDraft();
    await visual.current?.flush();
    const next = await command<Workspace>('save_attempt', {
      visualSubmission: visual.current?.freeze(submissionId) ?? null,
      intent,
      input: {
        session_id: session.id,
        expected_revision: expectedRevision,
        submission_id: submissionId,
        response: text,
        transcript_id: transcriptId,
      },
    });
    setWorkspace(next);
    setDraftState('Saved locally');
    if (tutor) await askTutor(submissionId);
  };
  const sessionButton = (s: (typeof w.sessions)[number], nested: boolean) => (
    <button
      key={s.id}
      aria-current={s.id === session.id ? 'page' : undefined}
      data-nested={nested || undefined}
      disabled={working}
      onClick={() => switchSession(s.id)}
      title={s.title}
    >
      <span>{s.title}</span>
      {nested ? null : (
        <small>
          {new Date(s.created_at).toLocaleDateString([], { month: 'short', day: 'numeric' })}
        </small>
      )}
    </button>
  );
  const loose = w.sessions.filter((s) => !grouped.has(s.id) && !isPlaceholder(s));
  const providerIssue = w.offline ? null : (PROVIDER_ISSUE[w.provider.state] ?? null);
  // Spending sits with the composer: this session's total, with the month in the tooltip and in Settings.
  const { budget } = w;
  const sessionShare = budget.limits.session_microusd
    ? budget.session_spent_microusd / budget.limits.session_microusd
    : 1;
  const monthShare = budget.limits.monthly_microusd
    ? budget.month_spent_microusd / budget.limits.monthly_microusd
    : 1;
  const nearLimit = !w.offline && (sessionShare >= 0.8 || monthShare >= 0.8);
  const spending = (
    <span
      className="tray-cost"
      data-near={nearLimit || undefined}
      title={`This session: ${money(budget.session_spent_microusd)} of ${money(budget.limits.session_microusd)}. This month (UTC): ${money(budget.month_spent_microusd)} of ${money(budget.limits.monthly_microusd)}. Pending and uncertain charges count toward both.`}
    >
      <span className="meter" aria-hidden="true">
        <span style={{ width: `${Math.min(100, Math.max(sessionShare, monthShare) * 100)}%` }} />
      </span>
      <span className="numeric">{money(budget.session_spent_microusd)}</span>
      <span className="tray-muted">
        {nearLimit
          ? monthShare >= 0.8
            ? 'near your monthly limit'
            : 'near your session limit'
          : 'this session'}
      </span>
      {budget.unresolved_count ? (
        <span className="tray-muted">· {budget.unresolved_count} pending</span>
      ) : null}
    </span>
  );
  const setupHint = providerIssue ? (
    <button
      type="button"
      className="tray-link"
      data-state={w.provider.state}
      onClick={openSettings}
    >
      <Icon name={w.provider.state === 'attention' ? 'alert' : 'key'} />
      {providerIssue}
    </button>
  ) : w.offline ? (
    <span className="tray-offline">Offline · simulated replies</span>
  ) : null;
  const tray = (
    <>
      {spending}
      {setupHint}
      {visualOpen && hasAnswer && tutorReady ? (
        <span
          className="tray-muted"
          title="Sends a short, bounded summary of what you explored; slider movement itself is never sent"
        >
          + visual summary
        </span>
      ) : null}
    </>
  );
  const modelTools =
    canGenerate && w.models?.length ? (
      <ModelControls workspace={w} disabled={working} onChange={setPreference} />
    ) : null;
  const requestLabel = generating
    ? w.offline
      ? `${generating.replace(/…$/, '')} (simulated)…`
      : generating
    : tutorBusy
      ? w.offline
        ? 'Simulating a tutor reply…'
        : 'Waiting for the tutor…'
      : '';
  const breadcrumb = goal ? goal.title : learning ? w.unit.title : '';
  return (
    <div className="app-shell">
      <a className="skip-link" href="#study-work">
        Skip to study activity
      </a>
      <nav className="sidebar" aria-label="Sessions">
        <div className="brand">
          <LogoMark />
          <span className="brand-name">Study Harness</span>
        </div>
        <button
          className="new-session"
          aria-current={home ? 'page' : undefined}
          disabled={working && !home}
          onClick={newGoal}
        >
          <Icon name="plus" />
          New goal
        </button>
        {w.unit.due_reviews.length ? (
          <button className="review-entry" disabled={working} onClick={startReview}>
            <Icon name="review" />
            <span>Reviews ready</span>
            <span className="count">{w.unit.due_reviews.length}</span>
          </button>
        ) : null}
        <div className="session-list">
          {w.goals.map((g) => {
            const members = g.session_ids
              .map((id) => w.sessions.find((s) => s.id === id))
              .filter((s): s is (typeof w.sessions)[number] => !!s);
            return members.length ? (
              <div className="session-group" key={g.id}>
                <h2 title={g.title}>
                  <span>{g.title}</span>
                  {g.capabilities ? (
                    <span
                      className="group-progress"
                      title={`${g.checked} of ${g.capabilities} shown on your own`}
                    >
                      {g.checked}/{g.capabilities}
                    </span>
                  ) : null}
                </h2>
                {members.map((s) => sessionButton(s, true))}
              </div>
            ) : null;
          })}
          {loose.length ? (
            <div className="session-group">
              <h2>{w.goals.length ? 'Other sessions' : 'Sessions'}</h2>
              {loose.map((s) => sessionButton(s, false))}
            </div>
          ) : null}
        </div>
        <div className="sidebar-footer">
          <button className="settings-button" disabled={working} onClick={openSettings}>
            <Icon name="sliders" />
            <span>Settings</span>
            {providerIssue ? (
              <span
                className="setup-dot"
                data-state={w.provider.state}
                title={providerIssue}
                aria-label={providerIssue}
              />
            ) : null}
          </button>
        </div>
      </nav>
      <div className="workspace">
        {busy && !generating && !tutorBusy ? <div className="busy-bar" aria-hidden="true" /> : null}
        {flow ? (
          <header className="workspace-header">
            <div className="title-block">
              {breadcrumb ? (
                <p className="eyebrow">
                  <span>{breadcrumb}</span>
                  {goal && capabilityIndex >= 0 ? (
                    <span className="numeric">
                      {' '}
                      · part {capabilityIndex + 1} of {plannedCapabilities.length}
                    </span>
                  ) : null}
                </p>
              ) : null}
              <h1 title={learning!.objective_outcome}>{learning!.objective_outcome}</h1>
              <p className="sr-only" role="status">
                {learning!.stage_title}
                {flow.stopped ? ' · stopped' : flow.quiet ? ' · quiet work' : ''} · {draftState}
              </p>
            </div>
            <div className="header-tools">
              <div className="session-controls">
                {paused ? (
                  <button
                    className="ghost"
                    disabled={working}
                    title={flow.stopped ? 'Pick up where you stopped' : 'End quiet work'}
                    onClick={() => action('resume')}
                  >
                    <Icon name="play" />
                    Resume
                  </button>
                ) : (
                  <>
                    <button
                      className="ghost"
                      disabled={working}
                      title="No tutor requests or prompts until you resume — work at your own pace"
                      onClick={() => action('quiet')}
                    >
                      <Icon name="moon" />
                      Quiet
                    </button>
                    <button
                      className="ghost"
                      disabled={working}
                      title="Stop for now; your place and work are saved"
                      onClick={() => action('stop')}
                    >
                      <Icon name="pause" />
                      Stop
                    </button>
                  </>
                )}
              </div>
              <div className="panel-tabs segmented" role="group" aria-label="Side panel">
                {goal ? (
                  <button
                    data-active={planOpen || undefined}
                    aria-pressed={planOpen}
                    disabled={check}
                    title={
                      check ? 'The plan returns when this check is finished' : 'Your study plan'
                    }
                    onClick={() =>
                      setPanel(planOpen ? null : { kind: 'plan', activityId: w.activity.id })
                    }
                  >
                    <Icon name="list" />
                    Plan
                  </button>
                ) : null}
                <button
                  data-active={visualOpen || undefined}
                  aria-pressed={visualOpen}
                  disabled={!visualAvailable}
                  title={
                    visualAvailable ? 'Show the interactive visual' : 'This step has no visual'
                  }
                  onClick={() => setPanel({ kind: 'visual', activityId: w.activity.id })}
                >
                  <Icon name="chart" />
                  Visual
                </button>
                <button
                  data-active={sourcesOpen || undefined}
                  aria-pressed={sourcesOpen}
                  disabled={tutorBusy || !!generating}
                  title={
                    check
                      ? 'Opening sources during a check is recorded as support'
                      : 'Your imported notes and documents'
                  }
                  onClick={() =>
                    void task(async () => {
                      await visual.current?.flush();
                      setPanel(sourcesOpen ? null : { kind: 'sources', activityId: w.activity.id });
                    })
                  }
                >
                  <Icon name="doc" />
                  Sources
                  {check ? <span className="tab-note">recorded</span> : null}
                </button>
              </div>
            </div>
          </header>
        ) : (
          <p className="sr-only" role="status">
            {draftState}
          </p>
        )}
        {w.offline ? (
          <section className="offline-banner" aria-label="Offline development controls">
            <strong>Offline development · simulated responses</strong>
            <details>
              <summary>Scenarios and simulated time</summary>
              <div className="dev-panel">
                <p className="dev-note">
                  No provider requests or billing. Separate development data. The selected scenario
                  controls the outcome; it does not judge your answer or test speech recognition.
                </p>
                <div className="dev-group">
                  <label htmlFor="scenario">Response scenario</label>
                  <select
                    id="scenario"
                    value={scenario}
                    onChange={(e) => setScenario(e.target.value)}
                  >
                    {w.scenarios.map((s) => (
                      <option key={s} value={s}>
                        {s.replaceAll('_', ' ')}
                      </option>
                    ))}
                  </select>
                  <div className="actions">
                    <button
                      disabled={working || !!w.pending_transcript || !showComposer}
                      onClick={() =>
                        run('development_action', {
                          sessionId: session.id,
                          action: 'ambiguous_transcript',
                        })
                      }
                    >
                      Load simulated transcript
                    </button>
                    <button
                      disabled={working || !scheduled}
                      onClick={() =>
                        run('development_action', {
                          sessionId: session.id,
                          action: 'advance_to_review',
                        })
                      }
                    >
                      Simulate time to next review
                    </button>
                  </div>
                  <p className="small">
                    Development clock:{' '}
                    {learning ? new Date(learning.now).toLocaleString() : 'current time'}. Simulated
                    elapsed time never counts as real delayed learning.
                  </p>
                </div>
                <div className="dev-group">
                  <label htmlFor="diagnostic-case">Versioned diagnostic case</label>
                  <select
                    id="diagnostic-case"
                    value={diagnosticId}
                    onChange={(e) => setDiagnosticId(e.target.value)}
                  >
                    {diagnosticCases.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.id.replaceAll('_', ' ')}
                      </option>
                    ))}
                  </select>
                  <div className="actions">
                    <button
                      disabled={working || !diagnosticId}
                      onClick={() => {
                        setScenario(
                          diagnosticCases.find((c) => c.id === diagnosticId)?.scenario ?? 'correct',
                        );
                        run('development_action', {
                          sessionId: session.id,
                          action: `case:${diagnosticId}`,
                        });
                      }}
                    >
                      Load diagnostic case
                    </button>
                  </div>
                  <p className="small">
                    Creates a separate simulated session and saves the case response through the
                    core. Selected fixture outcomes test contracts and integrity, not live model
                    quality. Human review of the diagnostic corpus is pending.
                  </p>
                </div>
              </div>
            </details>
          </section>
        ) : null}
        {error ? (
          <div className="error" role="alert">
            <Icon name="alert" />
            <span>{error}</span>
            <button className="ghost" onClick={() => setError('')}>
              Dismiss
            </button>
          </div>
        ) : null}
        {genRetry ? (
          <div className="error retry-generation" role="alert">
            <Icon name="alert" />
            <span>{genRetry.label} Your work is saved and nothing was resent automatically.</span>
            <button disabled={working} onClick={() => void genRetry.run()}>
              Try again
            </button>
            <button className="ghost" onClick={() => setGenRetry(null)}>
              Dismiss
            </button>
          </div>
        ) : null}
        <div
          className="workspace-body"
          data-panel={
            visualOpen ? 'visual' : sourcesOpen ? 'sources' : planOpen ? 'plan' : undefined
          }
        >
          {requestLabel ? (
            <div className="request-toast" role="status">
              <span className="spinner" aria-hidden="true" />
              <span>{requestLabel}</span>
              <button className="text-button" onClick={() => void command('cancel_request')}>
                Cancel
              </button>
            </div>
          ) : null}
          <main id="study-work" tabIndex={-1} className="activity" ref={scroller}>
            {home ? (
              <Home
                key={session.id}
                workspace={w}
                busy={working}
                planning={generating === 'Drafting your plan…'}
                draft={session.draft}
                tools={modelTools}
                onDraft={saveDraft}
                onPlan={(text, sourceIds) =>
                  void generate('Drafting your plan…', 'plan_goal', {
                    sessionId: session.id,
                    revision: session.revision,
                    goal: text,
                    sourceIds,
                  })
                }
                onSample={(objectiveId) =>
                  run('start_sample', {
                    sessionId: session.id,
                    revision: session.revision,
                    objectiveId,
                  })
                }
                onOpen={(id) => switchSession(id)}
                onReview={startReview}
                onSettings={openSettings}
              />
            ) : !flow && goal ? (
              <div className="activity-inner plan-page">
                <PlanView
                  goal={goal}
                  busy={working}
                  canRevise={goal.origin === 'generated' && canGenerate}
                  onBegin={begin}
                  onDecision={decide}
                  onRevise={revise}
                  onOpen={(id) => switchSession(id)}
                />
                {!canGenerate && goal.origin === 'generated' ? (
                  <p className="notice small">
                    Writing tasks for this plan needs your OpenAI key.{' '}
                    <button className="text-button inline" onClick={openSettings}>
                      Open Settings
                    </button>
                  </p>
                ) : null}
              </div>
            ) : (
              <div className="activity-inner conversation">
                {stage === 'exploratory' ? (
                  <p className="notice notice-warn">
                    Exploratory discussion from an earlier version: no checked reference, rubric or
                    plan. Replies are unreviewed and create no grades, progress or reviews.
                  </p>
                ) : learning?.generated && !check ? (
                  <p className="content-note">
                    <Icon name="info" />
                    <span>
                      Written for your plan and checked automatically, but not reviewed by a person.
                      If a task looks wrong, report it under <em>About this task</em> — a flawed
                      task never counts against you.
                    </span>
                  </p>
                ) : null}
                <Timeline
                  workspace={w}
                  busy={working}
                  canAsk={tutorReady}
                  onCommand={mutate}
                  onAsk={(id) => void askTutor(id)}
                  onSource={check || tutorBusy || generating ? undefined : openSource}
                />
                {stage === 'summary' && learning ? (
                  <section className="wrap-up" aria-label="Where you stand">
                    <p className="eyebrow">Where you stand</p>
                    <h2>{learning.objective_outcome}</h2>
                    {learning.summary?.stale ? (
                      <p className="notice notice-warn">
                        A correction or challenge changed the evidence.{' '}
                        <button
                          className="text-button inline"
                          disabled={working}
                          onClick={() => action('recompute')}
                        >
                          Recompute
                        </button>
                      </p>
                    ) : null}
                    <div className="wrap-up-grid">
                      <section data-tone="explained">
                        <h3>
                          <Icon name="book" />
                          Explained to you
                        </h3>
                        {learning.wrap_up.explained.length ? (
                          <ul>
                            {learning.wrap_up.explained.map((t) => (
                              <li key={t}>{t}</li>
                            ))}
                          </ul>
                        ) : (
                          <p className="muted">No explanations or hints were used.</p>
                        )}
                      </section>
                      <section data-tone="help">
                        <h3>
                          <Icon name="lifebuoy" />
                          Done with help
                        </h3>
                        {learning.wrap_up.needed_help.length ? (
                          <ul>
                            {learning.wrap_up.needed_help.map((t) => (
                              <li key={t}>{t}</li>
                            ))}
                          </ul>
                        ) : (
                          <p className="muted">No supported work was assessed.</p>
                        )}
                      </section>
                      <section data-tone="own">
                        <h3>
                          <Icon name="checkCircle" />
                          Shown on your own
                        </h3>
                        {learning.wrap_up.demonstrated.length ? (
                          <ul>
                            {learning.wrap_up.demonstrated.map((t) => (
                              <li key={t}>{t}</li>
                            ))}
                          </ul>
                        ) : (
                          <p className="muted">Nothing yet — no resolved independent check.</p>
                        )}
                      </section>
                      <section data-tone="unchecked">
                        <h3>
                          <Icon name="circleDashed" />
                          Not yet checked
                        </h3>
                        {learning.wrap_up.unchecked.length ? (
                          <ul>
                            {learning.wrap_up.unchecked.map((t) => (
                              <li key={t}>{t}</li>
                            ))}
                          </ul>
                        ) : (
                          <p className="muted">Everything here has been checked.</p>
                        )}
                      </section>
                    </div>
                    {scheduled ? (
                      <p className="review-line" data-due={due || undefined}>
                        <Icon name="clock" />
                        <span>
                          <strong>
                            {due ? 'Your review is ready now' : 'Next review'} ·{' '}
                            {new Date(scheduled.due_at).toLocaleString([], {
                              weekday: 'short',
                              month: 'short',
                              day: 'numeric',
                              hour: 'numeric',
                              minute: '2-digit',
                            })}
                          </strong>{' '}
                          — {scheduled.reason.replace(/\.?\s*$/, '.')}
                          {scheduled.simulated ? ' Simulated time; no retention claim.' : ''}{' '}
                          Overdue means ready to revisit, not failed.
                        </span>
                      </p>
                    ) : null}
                    {goal && nextCapability ? (
                      <div className="next-capability">
                        <p className="eyebrow">Next in your plan</p>
                        <p className="next-capability-outcome">{nextCapability.outcome}</p>
                        <div className="actions">
                          <button
                            className="primary"
                            disabled={working}
                            onClick={() => begin(nextCapability.objective_id, 'learn')}
                          >
                            Continue
                            <Icon name="arrow" />
                          </button>
                          <button
                            className="text-button"
                            disabled={working}
                            onClick={() => begin(nextCapability.objective_id, 'check')}
                          >
                            I know it — check me
                          </button>
                        </div>
                      </div>
                    ) : goal ? (
                      <p className="small muted">
                        That was the last capability in this plan. Come back for the planned
                        reviews, or change the plan to go further.
                      </p>
                    ) : null}
                    <details className="disclosure">
                      <summary>All attempts, feedback and help in this session</summary>
                      <LearningRecord
                        workspace={w}
                        busy={working}
                        onCommand={mutate}
                        onAsk={(id) => void askTutor(id)}
                        onSource={tutorBusy || generating ? undefined : openSource}
                        showAll
                      />
                    </details>
                  </section>
                ) : null}
              </div>
            )}
            {flow || w.pending_transcript ? (
              <div className="bottom-stack" ref={stack}>
                {flow && stage !== 'summary' ? (
                  <section
                    className="current-task"
                    aria-label="Current task"
                    data-check={check || undefined}
                  >
                    <div className="current-task-inner">
                      <details className="task-scope" key={w.activity.id}>
                        <summary>
                          <span className="current-task-label">
                            <Icon name={STAGE_ICON[stage ?? ''] ?? 'pencil'} />
                            <span>{learning!.stage_title}</span>
                            {check ? (
                              <span className="check-note">
                                Earlier teaching and visuals are hidden; any help is recorded
                              </span>
                            ) : null}
                          </span>
                          <span className="task-scope-toggle">
                            <Icon name="info" />
                            About this task
                          </span>
                        </summary>
                        <div className="task-about small">
                          {learning!.assumptions.length ? (
                            <p>
                              <strong>Assumptions.</strong> {learning!.assumptions.join(' ')}
                            </p>
                          ) : null}
                          {learning!.rubric_lines.length ? (
                            <div>
                              <strong>What this task checks.</strong>
                              <ul>
                                {learning!.rubric_lines.map((r) => (
                                  <li key={r}>{r}</li>
                                ))}
                              </ul>
                            </div>
                          ) : null}
                          <p>
                            <strong>Scope.</strong> {learning!.transfer_scope}.
                          </p>
                          <p>
                            <strong>Source.</strong> {learning!.task_provenance}
                          </p>
                          {learning!.task_checks.length ? (
                            <div>
                              <strong>Automated checks before you saw it.</strong>
                              <ul className="check-list">
                                {learning!.task_checks.map((c) => (
                                  <li key={c.id} data-outcome={c.outcome}>
                                    {c.label}: {CHECK_LABEL[c.outcome] ?? c.outcome}. {c.detail}
                                  </li>
                                ))}
                              </ul>
                            </div>
                          ) : null}
                          {learning!.generated ? (
                            report === null ? (
                              <button
                                className="text-button"
                                disabled={working}
                                onClick={() => setReport('')}
                              >
                                <Icon name="flag" />
                                Report a problem with this task
                              </button>
                            ) : (
                              <form
                                className="task-report"
                                onSubmit={(e) => {
                                  e.preventDefault();
                                  action('another', report.trim());
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
                                <div className="actions">
                                  <button disabled={working || !report.trim()}>
                                    Replace this task
                                  </button>
                                  <button
                                    type="button"
                                    className="text-button"
                                    onClick={() => setReport(null)}
                                  >
                                    Cancel
                                  </button>
                                </div>
                                <p className="small muted">
                                  Your answers to this task are kept; a replacement is a new task.
                                </p>
                              </form>
                            )
                          ) : null}
                        </div>
                      </details>
                      <RichText text={w.activity.prompt} className="task-prompt prose" />
                      {w.activity.math && !check ? (
                        <p className="task-math">
                          <MathText tex={w.activity.math} />
                        </p>
                      ) : null}
                    </div>
                  </section>
                ) : null}
                <div className="dock">
                  <div className="dock-inner">
                    {flow && !paused && stage !== 'exploratory' ? (
                      <div className="dock-row">
                        {stage !== 'summary' ? (
                          <Menu
                            className="help-menu"
                            icon="lifebuoy"
                            label="Help"
                            title={
                              check
                                ? 'Help during a check is saved first and makes later answers on this task count as supported'
                                : 'Hints, explanations and examples; each is saved before it is shown'
                            }
                            disabled={working}
                            placement="top"
                            panelClassName="help-options"
                          >
                            {(close) => (
                              <>
                                <MenuItem
                                  icon="bulb"
                                  label="A hint"
                                  onSelect={() => {
                                    close();
                                    action('hint');
                                  }}
                                />
                                <MenuItem
                                  icon="book"
                                  label="Explain the idea"
                                  onSelect={() => {
                                    close();
                                    action('explain');
                                  }}
                                />
                                <MenuItem
                                  icon="stairs"
                                  label="A related worked example"
                                  onSelect={() => {
                                    close();
                                    action('example');
                                  }}
                                />
                                <MenuItem
                                  icon="eye"
                                  label="Show this task’s solution"
                                  onSelect={() => {
                                    close();
                                    action('worked_example');
                                  }}
                                />
                                {prerequisite ? (
                                  <MenuItem
                                    icon="review"
                                    label={prerequisite.label}
                                    title={prerequisite.reason}
                                    onSelect={() => {
                                      close();
                                      action('prerequisite');
                                    }}
                                  />
                                ) : null}
                                <MenuItem
                                  icon="clipboard"
                                  label="I used outside help"
                                  onSelect={() => {
                                    close();
                                    action('outside_help');
                                  }}
                                />
                                <p className="menu-note">
                                  {check
                                    ? 'Help during a check is saved before it is shown; later answers on this task count as supported.'
                                    : 'Help is saved before it is shown, so your record reflects what you used.'}
                                </p>
                              </>
                            )}
                          </Menu>
                        ) : null}
                        <div className="next-steps">
                          {others.map((s) => (
                            <button
                              key={s.id}
                              className="text-button"
                              disabled={working || s.kind === 'unavailable'}
                              title={s.reason}
                              onClick={() => action(STEP_ACTION[s.id] ?? s.id)}
                            >
                              {STEP_ICON[s.id] ? <Icon name={STEP_ICON[s.id]} /> : null}
                              {s.label}
                            </button>
                          ))}
                          {recommended ? (
                            <button
                              className="step-primary"
                              disabled={working}
                              title={recommended.reason}
                              onClick={() => action(STEP_ACTION[recommended.id] ?? recommended.id)}
                            >
                              {recommended.label}
                              <Icon name="arrow" />
                            </button>
                          ) : null}
                        </div>
                      </div>
                    ) : null}
                    {flow && learning?.step_note && !paused ? (
                      <p className="step-note" title={learning.step_note}>
                        {learning.step_note}
                      </p>
                    ) : null}
                    {paused ? (
                      <div className="paused-note" role="status">
                        <Icon name={flow!.stopped ? 'pause' : 'moon'} />
                        <p>
                          {flow!.stopped
                            ? 'Stopped. Your work and place are saved.'
                            : 'Quiet work: no tutor requests or prompts. Save as you go, then resume to ask the tutor.'}
                        </p>
                        <button disabled={working} onClick={() => action('resume')}>
                          <Icon name="play" />
                          Resume
                        </button>
                      </div>
                    ) : null}
                    {retry ? (
                      <div className="retry-notice" role="alert">
                        <Icon name="alert" />
                        <div>
                          <p>
                            <strong>Saved — the tutor reply did not arrive.</strong> {retry.message}
                          </p>
                          <div className="actions">
                            <button
                              disabled={working}
                              onClick={() => void askTutor(retry.attemptId)}
                            >
                              Retry tutor response
                            </button>
                            <button className="text-button" onClick={() => setRetry(null)}>
                              Not now
                            </button>
                          </div>
                        </div>
                      </div>
                    ) : null}
                    {showComposer ? (
                      <Composer
                        key={`${session.id}:${w.activity.id}:${w.pending_transcript?.id ?? ''}:${intents.join()}`}
                        initialDraft={session.draft}
                        initialTranscript={w.pending_transcript}
                        sessionId={session.id}
                        revision={session.revision}
                        configured={w.credential.configured && !w.offline}
                        offline={w.offline}
                        retainFailedAudio={w.privacy.retain_failed_audio}
                        intents={intents}
                        defaultIntent={intentMemo.current[scope] ?? intents[0]}
                        onIntent={(choice) => {
                          intentMemo.current[scope] = choice;
                        }}
                        tutorReady={tutorReady}
                        answerLabel={
                          stage === 'exploration'
                            ? 'Send prediction'
                            : stage === 'diagnosis'
                              ? 'Send answer'
                              : 'Check answer'
                        }
                        placeholder={
                          stage === 'exploration'
                            ? 'Predict first: what will happen, and why?'
                            : stage === 'diagnosis'
                              ? 'Start from what you already think — rough reasoning is fine'
                              : undefined
                        }
                        tools={tutorReady ? modelTools : null}
                        tray={tray}
                        disabled={busy || tutorBusy || !!generating}
                        onDraft={saveDraft}
                        refresh={refresh}
                        flushDraft={flushDraft}
                        onWorking={setComposerBusy}
                        error={setError}
                        onSend={send}
                      />
                    ) : null}
                  </div>
                </div>
              </div>
            ) : null}
          </main>
          {planOpen && goal ? (
            <aside className="visual-panel plan-panel" aria-label="Plan">
              <PlanView
                goal={goal}
                busy={working}
                compact
                currentObjective={w.activity.objective_id}
                canRevise={goal.origin === 'generated' && canGenerate}
                onBegin={begin}
                onDecision={decide}
                onRevise={revise}
                onOpen={(id) => switchSession(id)}
              />
            </aside>
          ) : sourcesOpen ? (
            <Suspense
              fallback={
                <aside className="visual-panel source-panel" role="status">
                  Opening sources…
                </aside>
              }
            >
              <SourcePanel
                key={`${session.id}:${w.activity.id}`}
                sessionId={session.id}
                activityId={w.activity.id}
                sources={w.sources}
                initialSource={
                  sourceTarget?.activityId === w.activity.id ? sourceTarget : undefined
                }
                changed={refresh}
                locked={tutorBusy || !!generating}
                close={() => setPanel(null)}
              />
            </Suspense>
          ) : visualOpen && learning?.visual ? (
            <Suspense
              fallback={
                <aside className="visual-panel real-visual" role="status">
                  Loading the local visual player…
                </aside>
              }
            >
              <RealVisualPanel
                key={w.activity.id}
                ref={visual}
                sessionId={session.id}
                activityId={w.activity.id}
                sessionRevision={session.revision}
                latest={currentAnswers.at(-1)}
                hasAnswer={hasAnswer}
                templateId={learning.visual.template_id}
                plan={learning.visual}
                locked={tutorBusy || !!generating}
                changed={refresh}
              />
            </Suspense>
          ) : null}
        </div>
      </div>
      {settings ? (
        <Settings
          workspace={w}
          close={() => {
            setSettings(false);
            requestAnimationFrame(() =>
              document.querySelector<HTMLButtonElement>('.settings-button')?.focus(),
            );
          }}
          refresh={refresh}
          error={setError}
          openComparison={() => {
            setSettings(false);
            setEvaluation(true);
          }}
        />
      ) : null}
      {evaluation ? (
        <Suspense fallback={<p role="status">Opening comparison records…</p>}>
          <Evaluation workspace={w} close={() => setEvaluation(false)} />
        </Suspense>
      ) : null}
    </div>
  );
}
