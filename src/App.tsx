import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { PrivacySettings, StepOption, Workspace } from './generated/contracts';
import { command, money } from './api';
import { isCheck, phasesFor, STAGE_ICON } from './lesson';
import { Icon, LogoMark } from './components/Icon';
import type { VisualHandle } from './components/RealVisualPanel';
import { Composer, type Intent } from './components/Composer';
import { Settings } from './components/Settings';
import { LearningRecord } from './components/LearningRecord';
import { Conversation } from './components/Conversation';
import { Home } from './components/Home';
import { PlanView } from './components/PlanView';
import { Menu, MenuItem } from './components/Menu';
import { ModelControls } from './components/ModelControls';
import { RichText } from './components/MathText';
import { Sidebar, isPlaceholder } from './components/Sidebar';
import { SessionHeader, type PanelKind } from './components/SessionHeader';
import { PastTask, TaskCard } from './components/TaskCard';
import { DraftConfirm, NextStep } from './components/NextStep';
import { WrapUp } from './components/WrapUp';
import { DevControls } from './components/DevControls';
import { FailedReply, RequestPill, Thinking } from './components/Thinking';
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
// Steps that open a new task; for a generated capability the task is written and checked first.
const TRANSITIONS = ['teach', 'next', 'fresh_check', 'review', 'another'];
// Only a setup problem is worth a word; a working connection stays quiet.
const PROVIDER_ISSUE: Record<string, string> = {
  not_configured: 'Add your OpenAI key for tutor replies',
  attention: 'OpenAI needs attention',
};
// A per-device layout preference; it never leaves this Mac.
const SIDEBAR_KEY = 'study-harness:sidebar';
const readCollapsed = () => {
  try {
    return localStorage.getItem(SIDEBAR_KEY) === 'collapsed';
  } catch {
    return false;
  }
};

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
  // The side panel for the current task; null shows its default (a planned visual), 'none' keeps it closed.
  const [panel, setPanel] = useState<{ kind: PanelKind | 'none'; activityId: string } | null>(null);
  // A plan or task being written: what to show while waiting, and a deliberate retry if it fails.
  const [generating, setGenerating] = useState('');
  const [genRetry, setGenRetry] = useState<{ label: string; run: () => Promise<void> } | null>(
    null,
  );
  const [sourceTarget, setSourceTarget] = useState<
    { id: string; anchor: string; activityId: string } | undefined
  >();
  const [collapsed, setCollapsed] = useState(readCollapsed);
  // A next step waiting for the learner to confirm that their unsent draft may be discarded.
  const [pendingStep, setPendingStep] = useState<StepOption | null>(null);
  const [farFromEnd, setFarFromEnd] = useState(false);
  // Whether the current task card is on screen; when it is not, the composer says which task it answers.
  const [taskInView, setTaskInView] = useState(true);
  const active = useRef<string | null>(null);
  // The newest workspace, so a deliberate retry uses the current session revision rather than a stale one.
  const latest = useRef<Workspace | null>(null);
  latest.current = workspace;
  // Answer/Question choice per task, kept when the composer remounts (for example after a transcription).
  const intentMemo = useRef<Record<string, Intent>>({});
  const startupReported = useRef(false);
  const scroller = useRef<HTMLElement>(null);
  const stick = useRef(true);
  const seen = useRef({ scope: '', stage: '', count: 0 });
  const draftText = useRef('');
  const openPanel = useRef(false);
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
    try {
      localStorage.setItem(SIDEBAR_KEY, collapsed ? 'collapsed' : 'open');
    } catch {
      // The preference simply resets next time.
    }
  }, [collapsed]);
  // A narrow window gives the lesson the room; the sidebar returns with ⌘\ or its button.
  useEffect(() => {
    const narrow = matchMedia('(max-width: 760px)');
    const fit = () => {
      if (narrow.matches) setCollapsed(true);
    };
    fit();
    narrow.addEventListener('change', fit);
    return () => narrow.removeEventListener('change', fit);
  }, []);
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
  // A retry, a pending step or another task's panel never carries over to a different task.
  const scope = workspace ? `${workspace.active_session_id}:${workspace.activity.id}` : '';
  useEffect(() => {
    setRetry(null);
    setGenRetry(null);
    setPendingStep(null);
    setPanel((p) => (p && `${active.current}:${p.activityId}` === scope ? p : null));
    const w = latest.current;
    draftText.current = w?.sessions.find((s) => s.id === w.active_session_id)?.draft ?? '';
  }, [scope]);
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
    draftText.current = text;
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

  // Derived state (also used by the hooks below, so computed before the early return).
  const w = workspace;
  const session = w?.sessions.find((s) => s.id === w.active_session_id);
  const learning = w?.learning ?? null;
  const flow = learning?.state;
  const stage = flow?.stage;
  const goal = w?.goal ?? null;
  const home = !!w && !flow && !goal;
  const inFlow = !!flow;
  const working = busy || composerBusy || tutorBusy || !!generating;

  // Keep new work in view: a new task (or the wrap-up) scrolls to its start; new turns on the same task scroll to
  // the first new turn, so a long reply is read from its beginning. A learner reading further up is left alone.
  const entries = w?.timeline.length ?? 0;
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || !w) return;
    if (!inFlow) {
      seen.current = { scope: '', stage: '', count: 0 };
      return;
    }
    const items = el.querySelectorAll<HTMLElement>('[data-entry]');
    const prev = seen.current;
    seen.current = { scope, stage: stage ?? '', count: items.length };
    let target: HTMLElement | null = null;
    if (stage === 'summary' && (prev.scope !== scope || prev.stage !== 'summary'))
      target = el.querySelector<HTMLElement>('.wrap-up');
    else if (prev.scope !== scope)
      target = el.querySelector<HTMLElement>(`[data-activity="${CSS.escape(w.activity.id)}"]`);
    else if (items.length > prev.count) target = items[prev.count];
    else if (!stick.current) return;
    // A new task replaces the control that opened it: focus moves to the task, not to the page.
    if (prev.scope && prev.scope !== scope && stage !== 'summary') {
      const card = document.getElementById('current-task');
      const typing = document.activeElement?.matches('input, textarea, select');
      if (card && !typing) card.focus({ preventScroll: true });
    }
    if (target) {
      const top =
        target.getBoundingClientRect().top - el.getBoundingClientRect().top + el.scrollTop - 24;
      el.scrollTop = Math.max(0, top);
    } else el.scrollTop = el.scrollHeight;
    stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 96;
    setFarFromEnd(!stick.current);
  }, [entries, scope, stage, inFlow, tutorBusy, generating, retry, w]);
  // While the composer grows (a transcript to confirm, a recording), keep the newest turn in view if it was.
  useEffect(() => {
    const el = scroller.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      if (stick.current) el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [inFlow]);

  useEffect(() => {
    const root = scroller.current;
    const card = document.getElementById('current-task');
    if (!root || !card || typeof IntersectionObserver === 'undefined') {
      setTaskInView(true);
      return;
    }
    const observer = new IntersectionObserver(([entry]) => setTaskInView(entry.isIntersecting), {
      root,
      threshold: 0.15,
    });
    observer.observe(card);
    return () => observer.disconnect();
  }, [scope, stage, entries]);

  const askTutor = async (attemptId: string) => {
    if (!w || !session) return;
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
    const runIt = async () => {
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
          scenario: latest.current?.offline ? scenario : null,
        });
        active.current = next.active_session_id;
        setWorkspace(next);
        setPanel(null);
      } catch (e) {
        setGenRetry({ label: String(e), run: runIt });
        await refresh().catch(() => {});
      } finally {
        setGenerating('');
      }
    };
    await runIt();
  };
  const action = (name: string, note?: string) => {
    if (!session) return;
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
  /** A next step from the core. Moving to a new task with unsent text asks before the draft is discarded. */
  const takeStep = async (step: StepOption) => {
    const name = STEP_ACTION[step.id] ?? step.id;
    if (TRANSITIONS.includes(name)) {
      await flushDraft();
      if (draftText.current.trim() || latest.current?.pending_transcript) {
        setPendingStep(step);
        return;
      }
    }
    setPendingStep(null);
    action(name);
  };
  const discardDraftAndStep = () =>
    void task(async () => {
      const step = pendingStep;
      const current = latest.current;
      if (!step || !current) return;
      await flushDraft();
      await command('save_draft', { sessionId: current.active_session_id, text: '' });
      if (current.pending_transcript)
        await command('discard_transcript', {
          sessionId: current.active_session_id,
          transcriptId: current.pending_transcript.id,
        });
      draftText.current = '';
      setPendingStep(null);
      await refresh();
      action(STEP_ACTION[step.id] ?? step.id);
    });
  const begin = (objectiveId: string, mode: 'learn' | 'check') =>
    goal &&
    session &&
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
    session &&
    run('plan_decision', {
      sessionId: session.id,
      revision: session.revision,
      goalId: goal.id,
      objectiveId,
      decision,
    });
  const revise = (instruction: string) =>
    goal &&
    session &&
    void generate('Revising the plan…', 'revise_plan', {
      sessionId: session.id,
      revision: session.revision,
      goalId: goal.id,
      instruction,
    });
  const openSource = (id: string, anchor: string) =>
    w &&
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
      await refresh();
    });
  // “New goal” reuses an unstarted session instead of piling up empty ones.
  const newGoal = () => {
    if (!w) return;
    if (home) {
      document.getElementById('goal')?.focus();
      return;
    }
    const grouped = new Set(w.goals.flatMap((g) => g.session_ids));
    const empty = w.sessions.find((s) => !grouped.has(s.id) && isPlaceholder(s));
    if (empty) switchSession(empty.id);
    else run('new_session', { title: 'New session' });
  };
  const startReview = () =>
    w &&
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
    if (!session) return;
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
    draftText.current = '';
    setDraftState('Saved locally');
    if (tutor) await askTutor(submissionId);
  };
  const cancelRequest = () => void command('cancel_request').catch(() => {});
  const togglePanel = (kind: PanelKind, open: boolean) => {
    if (!w) return;
    const activityId = w.activity.id;
    if (kind === 'sources')
      void task(async () => {
        await visual.current?.flush();
        setPanel(open ? null : { kind, activityId });
      });
    else if (kind === 'visual') setPanel({ kind: open ? 'none' : 'visual', activityId });
    else setPanel(open ? null : { kind, activityId });
  };

  // Global shortcuts: new goal, sidebar, settings; Escape closes a side panel when nothing else claims it.
  const shortcuts = useRef<(e: KeyboardEvent) => void>(() => {});
  shortcuts.current = (e: KeyboardEvent) => {
    const mod = e.metaKey || e.ctrlKey;
    if (document.querySelector('dialog[open]')) return;
    if (mod && e.key.toLowerCase() === 'n' && !e.shiftKey) {
      e.preventDefault();
      if (!working) newGoal();
    } else if (mod && (e.key === '\\' || e.key.toLowerCase() === 'b')) {
      e.preventDefault();
      setCollapsed((v) => !v);
    } else if (mod && e.key === ',') {
      e.preventDefault();
      if (!working) openSettings();
    } else if (e.key === 'Escape' && !e.defaultPrevented && openPanel.current) {
      setPanel({ kind: 'none', activityId: latest.current?.activity.id ?? '' });
    }
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => shortcuts.current(e);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!w || !session)
    return (
      <main className="startup">
        <LogoMark />
        <h1 className="display">Study Harness</h1>
        <p role={error ? 'alert' : 'status'}>{error || 'Opening your local workspace…'}</p>
        {error ? (
          <button onClick={() => void refresh().catch((e) => setError(String(e)))}>
            Try again
          </button>
        ) : null}
      </main>
    );

  const paused = !!flow && (flow.quiet || flow.stopped);
  const check = isCheck(stage);
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
        : ['answer', 'question'];
  const showComposer = (!flow && !!w.pending_transcript) || (!!flow && !flow.stopped);
  const sourcesOpen = panel?.kind === 'sources' && panel.activityId === w.activity.id;
  const planOpen = panel?.kind === 'plan' && !!goal && !!flow && !check;
  // A visual appears only when this task planned one; there is no default visual for unrelated topics.
  const visualAvailable =
    !!flow && ['diagnosis', 'exploration'].includes(flow.stage) && !paused && !!learning?.visual;
  const visualOpen = !sourcesOpen && !planOpen && visualAvailable && panel?.kind !== 'none';
  openPanel.current = sourcesOpen || planOpen || visualOpen;
  const steps = learning?.steps ?? [];
  const prerequisite = steps.find((s) => s.id === 'prerequisite' && !s.recommended);
  const nextCapability = goal?.capabilities.find((c) => c.objective_id === goal.next_objective_id);
  const plannedCapabilities = goal?.capabilities.filter((c) => c.status === 'planned') ?? [];
  const capabilityIndex = plannedCapabilities.findIndex(
    (c) => c.objective_id === w.activity.objective_id,
  );
  const currentTask = w.timeline.find((e) => e.kind === 'task' && e.activity_id === w.activity.id);
  const reviewing = currentTask?.stage === 'review';
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
  const tray = (
    <>
      {providerIssue ? (
        <button
          type="button"
          className="tray-link"
          data-state={w.provider.state}
          onClick={openSettings}
        >
          <Icon name={w.provider.state === 'attention' ? 'alert' : 'key'} />
          {providerIssue}
        </button>
      ) : null}
      <span
        className="tray-cost"
        data-near={nearLimit || undefined}
        title={`This session: ${money(budget.session_spent_microusd)} of ${money(budget.limits.session_microusd)}. This month (UTC): ${money(budget.month_spent_microusd)} of ${money(budget.limits.monthly_microusd)}. Pending and uncertain charges count toward both.`}
      >
        <span className="meter" aria-hidden="true">
          <span style={{ width: `${Math.min(100, Math.max(sessionShare, monthShare) * 100)}%` }} />
        </span>
        <span className="numeric">{money(budget.session_spent_microusd)}</span>
        <span>
          {nearLimit
            ? monthShare >= 0.8
              ? 'near your monthly limit'
              : 'near your session limit'
            : w.offline
              ? 'simulated · no charges'
              : 'this session'}
        </span>
        {budget.unresolved_count ? <span>· {budget.unresolved_count} pending</span> : null}
      </span>
      {visualOpen && hasAnswer && tutorReady ? (
        <span title="Sends a short, bounded summary of what you explored; slider movement itself is never sent">
          · includes a visual summary
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
        : 'The tutor is reading your answer…'
      : '';
  const devControls = w.offline ? (
    <DevControls
      scenarios={w.scenarios}
      scenario={scenario}
      onScenario={setScenario}
      cases={diagnosticCases}
      caseId={diagnosticId}
      onCase={setDiagnosticId}
      now={learning?.now ?? null}
      working={working}
      canLoadTranscript={!w.pending_transcript && showComposer}
      canAdvanceTime={!!learning?.reviews.some((r) => r.status === 'scheduled')}
      onAction={(name) => {
        if (name.startsWith('case:'))
          setScenario(diagnosticCases.find((c) => `case:${c.id}` === name)?.scenario ?? 'correct');
        run('development_action', { sessionId: session.id, action: name });
      }}
    />
  ) : null;
  const help =
    flow && !paused && stage !== 'exploratory' && stage !== 'summary' ? (
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
              detail="A nudge toward the next move"
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
              detail="Later answers on this task count as supported"
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
    ) : null;
  const scrollToTask = () =>
    document.getElementById('current-task')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  const context =
    flow && stage !== 'summary' && stage !== 'exploratory' && !taskInView ? (
      <button
        type="button"
        className="composer-context"
        onClick={scrollToTask}
        title="Show the task"
      >
        <Icon name={STAGE_ICON[stage ?? ''] ?? 'pencil'} />
        <span className="composer-context-stage">{learning!.stage_title}</span>
        <span className="composer-context-text">
          <RichText text={w.activity.prompt.split(/\n\s*\n/)[0]} />
        </span>
        <Icon name="arrowUp" />
      </button>
    ) : null;
  const pending = requestLabel ? <Thinking label={requestLabel} onCancel={cancelRequest} /> : null;
  const generationFailure = genRetry ? (
    <div className="notice" data-tone="danger" role="alert">
      <Icon name="alert" />
      <p>
        {genRetry.label} <span>Your work is saved and nothing was resent automatically.</span>
      </p>
      <button className="compact" disabled={working} onClick={() => void genRetry.run()}>
        <Icon name="refresh" />
        Try again
      </button>
      <button className="text-button" onClick={() => setGenRetry(null)}>
        Dismiss
      </button>
    </div>
  ) : null;
  const crumbs = goal
    ? [
        goal.title,
        ...(capabilityIndex >= 0
          ? [`Part ${capabilityIndex + 1} of ${plannedCapabilities.length}`]
          : []),
      ]
    : learning
      ? [w.unit.title]
      : [];

  return (
    <div className="app" data-sidebar={collapsed ? 'collapsed' : undefined}>
      <a className="skip-link" href="#study-work">
        Skip to study activity
      </a>
      {collapsed ? null : (
        <Sidebar
          workspace={w}
          home={home}
          working={working}
          providerIssue={providerIssue}
          onHome={newGoal}
          onReview={startReview}
          onOpen={(id) => switchSession(id)}
          onSettings={openSettings}
          onCollapse={() => setCollapsed(true)}
        />
      )}
      <div className="main">
        {busy && !generating && !tutorBusy ? <div className="busy-bar" aria-hidden="true" /> : null}
        {flow ? (
          <SessionHeader
            crumbs={crumbs}
            title={learning!.objective_outcome}
            phases={phasesFor(stage!, reviewing)}
            stage={stage!}
            collapsed={collapsed}
            paused={paused}
            stopped={flow.stopped}
            working={working}
            extra={devControls}
            panels={[
              ...(goal
                ? [
                    {
                      kind: 'plan' as const,
                      label: 'Plan',
                      icon: 'list' as const,
                      active: planOpen,
                      disabled: check,
                      title: check
                        ? 'The plan returns when this check is finished'
                        : 'Your study plan',
                    },
                  ]
                : []),
              {
                kind: 'visual' as const,
                label: 'Visual',
                icon: 'chart' as const,
                active: visualOpen,
                disabled: !visualAvailable,
                title: visualAvailable ? 'The interactive visual' : 'This step has no visual',
              },
              {
                kind: 'sources' as const,
                label: 'Sources',
                icon: 'doc' as const,
                active: sourcesOpen,
                disabled: tutorBusy || !!generating,
                title: check
                  ? 'Opening sources during a check is recorded as support'
                  : 'Your imported notes and documents',
                note: check ? 'recorded' : undefined,
              },
            ]}
            onPanel={(kind) =>
              togglePanel(
                kind,
                kind === 'plan' ? planOpen : kind === 'sources' ? sourcesOpen : visualOpen,
              )
            }
            onSessionAction={(a) => action(a)}
            onExpand={() => setCollapsed(false)}
          />
        ) : (
          <header className="page-bar">
            {collapsed ? (
              <button
                className="icon-button"
                onClick={() => setCollapsed(false)}
                aria-label="Show sidebar"
                title="Show sidebar (⌘\)"
              >
                <Icon name="sidebar" />
              </button>
            ) : null}
            <p className="crumbs">
              <span>{home ? 'New goal' : 'Plan'}</span>
            </p>
            <span className="spacer" />
            {devControls}
          </header>
        )}
        <p className="sr-only" role="status">
          {flow
            ? `${learning!.stage_title}${flow.stopped ? ' · stopped' : flow.quiet ? ' · quiet work' : ''} · ${draftState}`
            : draftState}
        </p>
        <div
          className="main-body"
          data-panel={
            visualOpen ? 'visual' : sourcesOpen ? 'sources' : planOpen ? 'plan' : undefined
          }
        >
          <div className="stage">
            <main
              id="study-work"
              tabIndex={-1}
              className="scroller"
              ref={scroller}
              onScroll={(e) => {
                const el = e.currentTarget;
                stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 96;
                if (farFromEnd === stick.current) setFarFromEnd(!stick.current);
              }}
            >
              {home ? (
                <>
                  {generationFailure ? <div className="column">{generationFailure}</div> : null}
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
                </>
              ) : !flow && goal ? (
                <div className="column plan-page">
                  {generationFailure}
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
                    <p className="notice" data-tone="warn">
                      <Icon name="key" />
                      <span>Writing tasks for this plan needs your OpenAI key.</span>
                      <button className="text-button" onClick={openSettings}>
                        Open Settings
                      </button>
                    </p>
                  ) : null}
                </div>
              ) : (
                <div className="column">
                  {learning ? (
                    <header className="lesson-head">
                      {crumbs.length ? (
                        <p className="crumbs">
                          {crumbs.map((c, i) => (
                            <span key={i}>{c}</span>
                          ))}
                        </p>
                      ) : null}
                      <h2 className="display">{learning.objective_outcome}</h2>
                    </header>
                  ) : null}
                  {stage === 'exploratory' ? (
                    <p className="notice" data-tone="warn">
                      <Icon name="info" />
                      <span>
                        Exploratory discussion from an earlier version: no checked reference, rubric
                        or plan. Replies are unreviewed and create no grades, progress or reviews.
                      </span>
                    </p>
                  ) : null}
                  <Conversation
                    workspace={w}
                    busy={working}
                    canAsk={tutorReady}
                    onCommand={mutate}
                    onAsk={(id) => void askTutor(id)}
                    onSource={check || tutorBusy || generating ? undefined : openSource}
                    task={
                      learning && stage !== 'summary' ? (
                        <TaskCard
                          key={w.activity.id}
                          workspace={w}
                          learning={learning}
                          check={check}
                          working={working}
                          onReport={(note) => action('another', note)}
                        />
                      ) : currentTask ? (
                        <PastTask
                          stage={currentTask.stage}
                          title={currentTask.detail}
                          text={currentTask.text}
                        />
                      ) : null
                    }
                    after={
                      <>
                        {pending}
                        {retry && !tutorBusy ? (
                          <FailedReply
                            message={retry.message}
                            busy={working}
                            onRetry={() => void askTutor(retry.attemptId)}
                            onDismiss={() => setRetry(null)}
                          />
                        ) : null}
                        {generationFailure ? (
                          <li className="turn-slot">{generationFailure}</li>
                        ) : null}
                        {flow &&
                        !paused &&
                        stage !== 'summary' &&
                        stage !== 'exploratory' &&
                        !pending ? (
                          <NextStep
                            steps={steps}
                            note={learning?.step_note ?? null}
                            hasAnswer={hasAnswer}
                            busy={working}
                            pending={pendingStep}
                            onStep={(s) => void takeStep(s)}
                            onConfirm={discardDraftAndStep}
                            onCancel={() => setPendingStep(null)}
                          />
                        ) : null}
                      </>
                    }
                  />
                  {stage === 'summary' && learning ? (
                    <WrapUp
                      learning={learning}
                      next={goal ? nextCapability : undefined}
                      lastInPlan={!!goal && !nextCapability}
                      steps={steps}
                      busy={working}
                      onRecompute={() => action('recompute')}
                      onBegin={begin}
                      onStep={(s) => void takeStep(s)}
                      record={
                        <LearningRecord
                          workspace={w}
                          busy={working}
                          onCommand={mutate}
                          onAsk={(id) => void askTutor(id)}
                          onSource={tutorBusy || generating ? undefined : openSource}
                          showAll
                        />
                      }
                    />
                  ) : null}
                  {stage === 'summary' && pendingStep ? (
                    <DraftConfirm
                      step={pendingStep}
                      busy={working}
                      onConfirm={discardDraftAndStep}
                      onCancel={() => setPendingStep(null)}
                    />
                  ) : null}
                </div>
              )}
            </main>
            {!flow && requestLabel ? (
              <RequestPill label={requestLabel} onCancel={cancelRequest} />
            ) : null}
            {flow || w.pending_transcript ? (
              <div className="dock">
                {inFlow && farFromEnd ? (
                  <button
                    className="jump"
                    onClick={() =>
                      scroller.current?.scrollTo({
                        top: scroller.current.scrollHeight,
                        behavior: 'smooth',
                      })
                    }
                  >
                    <Icon name="arrowUp" />
                    Latest
                  </button>
                ) : null}
                {paused ? (
                  <div className="paused-note" role="status">
                    <Icon name={flow!.stopped ? 'pause' : 'moon'} />
                    <p>
                      {flow!.stopped
                        ? 'Stopped. Your work and place are saved.'
                        : 'Quiet work: no tutor requests or prompts. Save as you go, then resume to ask the tutor.'}
                    </p>
                    <button className="compact" disabled={working} onClick={() => action('resume')}>
                      <Icon name="play" />
                      Resume
                    </button>
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
                    help={help}
                    context={context}
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
            ) : null}
          </div>
          {planOpen && goal ? (
            <aside className="side-panel plan-panel" aria-label="Plan">
              <div className="panel-head">
                <h2 className="panel-title">
                  <Icon name="list" />
                  Plan
                </h2>
                <button
                  className="icon-button"
                  aria-label="Close plan"
                  onClick={() => setPanel(null)}
                >
                  <Icon name="close" />
                </button>
              </div>
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
                <aside className="visual-panel side-panel source-panel" role="status">
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
                <aside className="visual-panel side-panel real-visual" role="status">
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
      {error ? (
        <div className="toasts" role="region" aria-label="Notifications">
          <div className="toast" role="alert">
            <Icon name="alert" />
            <p>{error}</p>
            <button className="icon-button" aria-label="Dismiss" onClick={() => setError('')}>
              <Icon name="close" />
            </button>
          </div>
        </div>
      ) : null}
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
