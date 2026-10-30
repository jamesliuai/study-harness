import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { createLazyCatalog } from '@explorable-models/catalog';
import {
  archivalView,
  type ActivityArtifact,
  type ActivityRequest,
  type ActivityTemplate,
  type AssistanceSnapshot,
  type HostPolicy,
  type PreparedActivity,
} from '@explorable-models/activity';
import {
  createStudyHarnessVisualizer,
  mountArchivalRecord,
  archivalLabels,
  mountStudyActivityPlaceholder,
  type StudyHarnessVisualizer,
} from '@explorable-models/player/activity';
import type { Attempt, VisualPlan } from '../generated/contracts';
import { nativeHost, visualCall } from '../visualizer/host';
import { Icon } from './Icon';

type OpenRecord = {
  request: ActivityRequest;
  policy: HostPolicy;
  artifact: ActivityArtifact | null;
};
export type VisualHandle = {
  freeze(responseId: string): { activityId: string; snapshot: AssistanceSnapshot } | null;
  flush(): Promise<void>;
  /**
   * Bounded, deterministic, learner-visible summary of what was explored (tutor audience), or null when nothing
   * meaningful happened yet. Computed locally; the core validates it before it can reach the tutor.
   */
  summary(): unknown | null;
};
const operation = () => crypto.randomUUID();
// The SDK's own warm-neutral dark defaults match the app; the embedded variant leaves the lesson prompt, composer
// and title to the app. System fonts for controls; KaTeX fonts come from the app's stylesheet.
const surfaceTheme = { theme: 'dark', variant: 'embedded' } as const;
function failure(outcome: { ok: boolean; diagnostics?: { message: string }[] }) {
  if (!outcome.ok)
    throw new Error(
      outcome.diagnostics?.map((d) => d.message).join(' ') ||
        'The visual operation could not finish.',
    );
}

export const RealVisualPanel = forwardRef<
  VisualHandle,
  {
    sessionId: string;
    activityId: string;
    sessionRevision: number;
    latest?: Attempt;
    /** An answer (not a question) is saved for this task, so a prediction exists before any reveal. */
    hasAnswer: boolean;
    /** The template planned with this task; there is no default visual. */
    templateId: string;
    /** Parameters fixed with the task and how they relate to it. */
    plan: VisualPlan | null;
    /** A tutor or generation request is out: nothing here may change the session until it settles. */
    locked?: boolean;
    changed: () => Promise<void>;
  }
>(function RealVisualPanel(
  { sessionId, activityId, sessionRevision, latest, hasAnswer, templateId, plan, locked, changed },
  ref,
) {
  const target = useRef<HTMLDivElement>(null);
  const live = useRef<{
    visualizer: StudyHarnessVisualizer;
    prepared: PreparedActivity;
    template: ActivityTemplate;
    save: () => Promise<void>;
  } | null>(null);
  const callbacks = useRef({ changed });
  callbacks.current = { changed };
  const revision = useRef(sessionRevision);
  revision.current = sessionRevision;
  const response = useRef(latest);
  response.current = latest;
  const planned = useRef(!!plan);
  planned.current = !!plan;
  const [status, setStatus] = useState('Preparing local visual…');
  const [error, setError] = useState('');
  const [ready, setReady] = useState(false);
  const [stage, setStage] = useState('');
  const [saving, setSaving] = useState(false);
  const [reload, setReload] = useState(0);
  const queue = useRef(Promise.resolve());
  useImperativeHandle(
    ref,
    () => ({
      freeze: (responseId) =>
        live.current
          ? {
              activityId: live.current.prepared.activityId,
              snapshot: live.current.prepared.host.freezeAssistanceSnapshot({ responseId }),
            }
          : null,
      // A failed checkpoint is shown in the panel; it never blocks the learner's next action.
      flush: async () => {
        try {
          await queue.current.catch(() => {});
          await live.current?.save();
        } catch (e) {
          setError(String(e));
        }
      },
      summary: () => {
        const held = live.current;
        if (!held) return null;
        const result = held.visualizer.summarizeInteraction(held.prepared.activityId, {
          audience: 'tutor',
          limits: { committedConfigurations: 12, previewObservations: 4, bytes: 6000 },
        });
        if (!('contract' in result)) return null;
        return result.revealed.length || result.committedConfigurations.length > 1 ? result : null;
      },
    }),
    [],
  );

  useEffect(() => {
    let active = true;
    let visualizer: StudyHarnessVisualizer | undefined;
    let prepared: PreparedActivity | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unsubscribe: (() => void) | undefined;
    let fingerprint = '';
    const container = target.current!;
    const placeholder = mountStudyActivityPlaceholder(container, {
      state: 'preparing',
      title: 'Preparing the visual',
      ...surfaceTheme,
    });
    setReady(false);
    setError('');
    const start = async () => {
      const catalog = createLazyCatalog();
      const opened = await visualCall<OpenRecord>('open', { sessionId, templateId });
      if (!active) return;
      await catalog.load(opened.request.template!);
      const template = catalog.getTemplate(opened.request.template!);
      if (!template)
        throw new Error(
          'The exact visual content is unavailable. Keep the backup and reinstall its recorded version.',
        );
      visualizer = createStudyHarnessVisualizer({
        catalog,
        host: nativeHost(() => {
          if (active) void callbacks.current.changed();
        }),
        maxWorkers: 2,
        surfaceDefaults: { ...surfaceTheme, fonts: 'system' },
      });
      const outcome = opened.artifact
        ? await visualizer.restore(opened.artifact, {
            operationId: operation(),
            hostSessionRevision: opened.request.hostSessionRevision,
          })
        : await visualizer.prepare(opened.request, { operationId: operation() });
      if (!active) {
        visualizer.dispose();
        return;
      }
      if (!outcome.ok) {
        placeholder.dispose();
        if (
          opened.artifact &&
          outcome.diagnostics.some((d) =>
            ['UNSUPPORTED_ARTIFACT', 'CONTENT_REVOKED'].includes(d.code),
          )
        ) {
          const archive = archivalView(opened.artifact, { audience: 'learner' });
          if (archive.ok)
            mountArchivalRecord(container, archive.view, {
              ...surfaceTheme,
              labels: archivalLabels(template),
              returnAction: { label: 'Return to the session', onSelect: () => container.focus() },
            });
        }
        failure(outcome);
        return;
      }
      prepared = outcome.prepared;
      const binding = prepared.binding('learner', `surface-${operation()}`);
      if (!opened.artifact && templateId === 'function-derivative') {
        const state = visualizer.getState(opened.request.activityId, 'host');
        if ('audience' in state)
          failure(
            binding.setCamera({
              operationId: operation(),
              expectedPresentationRevision: state.guards.presentationRevision,
              viewId: 'function-plot',
              camera: { kind: '2d', center: [1, 8], zoom: 1 },
            }),
          );
      }
      let queued = false;
      const save = async () => {
        if (!prepared || !visualizer || queued) return queue.current;
        queued = true;
        queue.current = queue.current
          .catch(() => {})
          .then(async () => {
            if (active) setSaving(true);
            try {
              const saved = await prepared!.host.save({ operationId: operation(), kind: 'backup' });
              failure(saved);
              if (active) setStatus(`Saved locally at ${new Date().toLocaleTimeString()}`);
            } finally {
              if (active) setSaving(false);
            }
          })
          .finally(() => {
            queued = false;
          });
        return queue.current;
      };
      live.current = { visualizer, prepared, template, save };
      unsubscribe = binding.subscribe((projection) => {
        if (!active || !visualizer) return;
        setStage(projection.stage.id);
        container.dataset.frameId = projection.frame?.frameId ?? '';
        const host = visualizer.getState(opened.request.activityId, 'host');
        if (!('audience' in host)) return;
        const key = JSON.stringify([
          host.guards.scientificVersion,
          projection.frame?.frameId,
          host.guards.presentationRevision,
          host.guards.activityRevision,
          host.instance.responses,
          host.exposures,
        ]);
        if (
          key === fingerprint ||
          projection.status.ui === 'tentative-preview' ||
          projection.status.ui === 'playing'
        )
          return;
        fingerprint = key;
        if (timer) clearTimeout(timer);
        timer = setTimeout(
          () =>
            void save().catch((e) => {
              if (active) {
                setError(String(e));
                setSaving(false);
              }
            }),
          200,
        );
      });
      placeholder.dispose();
      const surface = visualizer.mount(container, prepared, {
        headingLevel: 3,
        // The task's own values are stated above the card; the template's illustrative wording could contradict them.
        instruction: planned.current
          ? 'Compare the result with what you predicted, then change one value at a time.'
          : undefined,
        spatialLoader: () => import('@explorable-models/player-3d'),
        onRecoveryAction: () => setReload((v) => v + 1),
        onError: (event) => {
          if (active && event.kind === 'diagnostic') setError(event.diagnostic.message);
        },
      });
      await surface.ready;
      if (!active) return;
      setStage(binding.getProjection().stage.id);
      setReady(true);
      setStatus('Ready · computed locally');
      await save();
    };
    void start().catch((e) => {
      if (active)
        setError(typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e));
    });
    return () => {
      active = false;
      if (timer) clearTimeout(timer);
      unsubscribe?.();
      queueMicrotask(() => placeholder.dispose());
      const held = live.current;
      live.current = null;
      if (held && held.visualizer === visualizer) {
        held.prepared.host.suspend({ operationId: operation(), reason: 'host' });
        // Immediately remove prior instructional visuals; finish the durable checkpoint, then release workers.
        container.replaceChildren();
        void held
          .save()
          .catch(() => {})
          .finally(() => held.visualizer.dispose());
      } else visualizer?.dispose();
    };
  }, [sessionId, activityId, templateId, reload]);

  useEffect(() => {
    const held = live.current;
    if (!held || !ready) return;
    const sync = async () => {
      const state = held.visualizer.getState(held.prepared.activityId, 'host');
      if (!('audience' in state)) return;
      const next = response.current;
      if (
        next &&
        !state.instance.responses.some((r) => r.responseId === next.id) &&
        revision.current > state.guards.hostSessionRevision
      ) {
        const stage = held.template.stages.find((s) => s.id === state.instance.stage.id);
        const result = await held.prepared.host.submitResponseRef({
          operationId: operation(),
          expectedHostSessionRevision: state.guards.hostSessionRevision,
          hostSessionRevision: revision.current,
          response: {
            responseId: next.id,
            taskId: activityId,
            taskRevision: next.task_version,
            stageId: state.instance.stage.id,
            kind: stage?.response?.kind ?? 'answer',
            modality: next.modality === 'recorded_speech' ? 'speech-derived' : 'text',
          },
        });
        failure(result);
        await held.save();
      } else if (revision.current > state.guards.hostSessionRevision) {
        failure(
          held.prepared.host.observeHostSessionRevision({
            operationId: operation(),
            expectedHostSessionRevision: state.guards.hostSessionRevision,
            hostSessionRevision: revision.current,
          }),
        );
      }
    };
    void sync().catch((e) => setError(String(e)));
  }, [latest?.id, sessionRevision, ready, activityId]);

  const advance = async () => {
    const held = live.current;
    if (!held) return;
    setError('');
    try {
      const state = held.visualizer.getState(held.prepared.activityId, 'host');
      if (!('audience' in state)) return;
      const transition = held.template.transitions.find(
        (t) => t.from === state.instance.stage.id && t.to !== 'closed',
      );
      if (!transition) return;
      const target = held.template.stages.find((s) => s.id === transition.to)!;
      const reveals = held.template.groups.some((g) => g.solutionBearing && g.id in target.visible);
      if (reveals) {
        const policy = await visualCall<HostPolicy>('authorizeReveal', {
          activityId: held.prepared.activityId,
        });
        if (policy.revision > state.guards.policyRevision)
          failure(await held.prepared.host.updatePolicy({ operationId: operation(), policy }));
      }
      let current = held.visualizer.getState(held.prepared.activityId, 'host');
      if (!('audience' in current)) return;
      failure(
        await held.prepared.host.requestStageTransition({
          operationId: operation(),
          expectedActivityRevision: current.guards.activityRevision,
          expectedPolicyRevision: current.guards.policyRevision,
          toStage: transition.to,
        }),
      );
      current = held.visualizer.getState(held.prepared.activityId, 'host');
      if (target.kind === 'explore' && 'audience' in current) {
        failure(
          await held.prepared.host.requestExplorationScope({
            operationId: operation(),
            expectedActivityRevision: current.guards.activityRevision,
            expectedPolicyRevision: current.guards.policyRevision,
            groups: held.template.groups.filter((g) => g.id in target.visible).map((g) => g.id),
          }),
        );
      }
      await held.save();
      await callbacks.current.changed();
    } catch (e) {
      setError(typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e));
    }
  };
  const template = live.current?.template;
  const transition = template?.transitions.find((t) => t.from === stage && t.to !== 'closed');
  const nextStage = template?.stages.find((s) => s.id === transition?.to);
  // Showing solution-bearing results needs a saved prediction first; the core enforces this as well.
  const reveals =
    !!nextStage && !!template?.groups.some((g) => g.solutionBearing && g.id in nextStage.visible);
  const blocked = reveals && !hasAnswer;
  return (
    <aside className="visual-panel real-visual" aria-label="Interactive visual">
      <h2 className="eyebrow panel-eyebrow">
        <Icon name="chart" />
        Interactive visual
      </h2>
      {plan ? (
        <div className="visual-intro">
          <p className="visual-purpose">{plan.purpose}</p>
          <p className="visual-relation" data-match={plan.matches_task || undefined}>
            {plan.relation}
          </p>
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="error">
          {error} <button onClick={() => setReload((v) => v + 1)}>Reopen visual</button>
        </p>
      ) : null}
      {transition ? (
        <div className="stage-advance-row">
          <button
            className="stage-advance"
            disabled={!ready || saving || blocked || locked}
            title={locked ? 'Available when the current request finishes' : undefined}
            onClick={() => void advance()}
          >
            {transition.label}
            <Icon name="arrow" />
          </button>
          {blocked ? (
            <p className="small">Send your prediction first, then reveal the result.</p>
          ) : null}
        </div>
      ) : null}
      <div ref={target} className="visual-surface" tabIndex={-1} />
      <details className="visual-details">
        <summary>About this visual</summary>
        <p className="small">
          Provisional, for personal use: reference-checked, independent subject review pending.
          Exploring it is not evidence of understanding; your answers are.
        </p>
        <p className="small" role="status">
          {saving ? 'Saving…' : status}
        </p>
        {ready ? (
          <button
            className="text-button"
            disabled={saving}
            onClick={() => void live.current?.save().catch((e) => setError(String(e)))}
          >
            Save a checkpoint now
          </button>
        ) : null}
      </details>
    </aside>
  );
});
