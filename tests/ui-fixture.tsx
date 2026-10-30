// Browser-only UI test fixture. Never imported by the production entry point.
import { mockIPC } from '@tauri-apps/api/mocks';
import type { Workspace } from '../src/generated/contracts';
Object.assign(globalThis, { isTauri: true });
let w: Workspace = {
  schema_version: 11,
  goal: null,
  goals: [],
  timeline: [],
  provider: {
    state: 'not_configured',
    verified_at: null,
    last_error_kind: null,
    last_error: null,
    last_error_at: null,
  },
  privacy: {
    retain_failed_audio: true,
    review_minutes: 10,
    text_scale_percent: 100,
    tutor_model: 'gpt-6.1-sol',
    reasoning_effort: 'low',
  },
  sources: { documents: [], selected_passage_ids: [], citations: [] },
  unit: {
    version: 'layout-only',
    title: 'Fixture unit',
    notice: 'Layout-only fixture',
    active_objective_id: 'quadratic-convergence',
    plan: ['quadratic-convergence'],
    objectives: [],
    continue_session_id: null,
    due_reviews: [],
    due_review_count: 0,
    available_review_minutes: 10,
  },
  offline: false,
  scenarios: [],
  learning: null,
  pending_transcript: null,
  sessions: [
    {
      schema_version: 1,
      id: 'fixture-session',
      title: 'Gradient descent',
      revision: 0,
      created_at: '2026-10-25T22:00:00Z',
      draft: '',
    },
  ],
  active_session_id: 'fixture-session',
  activity: {
    schema_version: 1,
    id: 'fixture-activity',
    session_id: 'fixture-session',
    objective_id: 'quadratic-convergence',
    version: 1,
    prompt: 'Predict',
    math: 'x_{t+1} = (1 - \\eta a)x_t',
    task_family: 'quadratic-prediction-v1',
    rubric_version: 'quadratic-v1',
    validation_status: 'UI fixture only',
  },
  attempts: [],
  assistance: [],
  requests: [],
  request_versions: [],
  usage: [],
  budget: {
    session_spent_microusd: 0,
    month_spent_microusd: 0,
    unresolved_count: 0,
    limits: { session_microusd: 1000000, monthly_microusd: 10000000 },
    month: '2026-10',
  },
  credential: {
    configured: false,
    storage: 'not configured',
    masked: 'Not configured',
    message: 'UI test fixture. No live provider.',
  },
  scene: {
    schema_version: 1,
    capability_version: 'reference-1',
    model_id: 'quadratic-descent',
    model_version: '1',
    objective_id: 'quadratic-convergence',
    activity_id: 'fixture-activity',
    parameters: { curvature: 2, step_size: 0.75, start: 4, steps: 12 },
    prediction: 'Predict',
    follow_up: 'Explain',
    assumptions: ['Exact quadratic'],
    validation: 'analytic-cases-v1',
    accessibility: 'table',
    revealed: false,
  },
  provider_model: 'UI fixture — no live provider',
  models: [
    {
      id: 'gpt-6.1-sol',
      label: 'Sol',
      note: 'UI fixture',
      input_usd_per_million: 2,
      output_usd_per_million: 10,
    },
  ],
  transcription_model: 'UI fixture',
  rate_version: 'fixture-only',
};
// Layout-only snapshots exported by the real Rust workspace view; never learning acceptance.
const snapshot = new URLSearchParams(location.search).get('goal2');
if (snapshot && /^[a-z]+$/.test(snapshot)) {
  const response = await fetch(`/tests/.goal2-${snapshot}.json`);
  if (!response.ok) throw new Error('Export the offline Rust workspace snapshot first.');
  w = (await response.json()) as Workspace;
}
if (new URLSearchParams(location.search).has('transcript')) {
  w.pending_transcript = {
    id: 'fixture-transcript',
    recognized_text: 'FIXTURE: the multiplier is positive one half',
    audio_seconds: 3,
    format: 'UI fixture — no recorded or uploaded audio',
  };
  w.sessions[0].draft = 'FIXTURE: the multiplier is negative one half';
}
mockIPC((cmd, args) => {
  if (cmd === 'workspace') return structuredClone(w);
  if (cmd === 'save_draft') {
    w.sessions[0].draft = (args as { text: string }).text;
    return null;
  }
  if (cmd === 'discard_transcript') {
    w.pending_transcript = null;
    w.sessions[0].draft = '';
    return null;
  }
  if (cmd === 'save_scene' || cmd === 'discard_recording') return null;
  if (cmd === 'reveal_scene') {
    w.scene.revealed = true;
    w.sessions[0].revision++;
    return structuredClone(w);
  }
  if (cmd === 'save_attempt') {
    const input = (args as { input: { response: string; submission_id: string } }).input;
    w.attempts.push({
      schema_version: 1,
      id: input.submission_id,
      session_id: w.active_session_id,
      activity_id: w.activity.id,
      task_version: 1,
      response: input.response,
      recognized_transcript: w.pending_transcript?.recognized_text ?? null,
      modality: w.pending_transcript ? 'recorded_speech' : 'text',
      created_at: '2026-10-25T22:00:00Z',
      assistance_ids: [],
      exposure_ids: [],
      revision_of: null,
    });
    w.pending_transcript = null;
    w.sessions[0].draft = '';
    w.sessions[0].revision++;
    return structuredClone(w);
  }
  throw new Error(`UI fixture does not implement ${cmd}`);
});
await import('../src/main');
