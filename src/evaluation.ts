import { invoke } from '@tauri-apps/api/core';
import { command, desktop } from './api';
export async function evaluationCall<T>(method: string, request: unknown = {}): Promise<T> {
  const bytes = new TextEncoder().encode(JSON.stringify({ method, request }));
  if (import.meta.env.MODE === 'offline-test')
    return command<T>('evaluation_rpc', { body: Array.from(bytes) });
  if (!desktop) throw new Error('Open the packaged app to collect local comparison records.');
  return invoke<T>('evaluation_rpc', bytes, {
    headers: { 'Content-Type': 'application/octet-stream' },
  });
}
export type EvaluationItem = {
  id: string;
  form: string;
  prompt: string;
  rubric?: string;
  reference_solution?: string;
  max_points: number;
  review_provenance: string;
};
export type EvaluationTopic = {
  id: string;
  name: string;
  condition: string;
  prior_knowledge: string;
  items?: EvaluationItem[];
  lesson: null | { completed_at: string; assessment_due_at: string; review_exclusion_at: string };
  timer: null | { kind: string; started_at: string };
  events: { kind: string; seconds?: number; note?: string }[];
  exam: null | { started_at: string };
  responses: null | {
    submitted_at: string;
    outside_help: boolean;
    responses: { item_id: string; response: string; confidence: number }[];
  };
  scores: {
    reviewer: string;
    recorded_at: string;
    scores: { item_id: string; points: number; notes: string }[];
  }[];
  evidence: { session_id: string | null };
  assessment_ready: boolean;
  planned_review_excluded: boolean;
  recorded_exposures_in_exclusion_window: number;
};
export type EvaluationView = {
  now: string;
  simulated: boolean;
  notice: string;
  pairs: {
    id: string;
    order: string;
    versions: Record<string, string>;
    registration: {
      baseline_method: string;
      baseline_model: string;
      matched_rationale: string;
      decision_criterion: string;
      time_allowance_minutes: number;
    };
    topics: EvaluationTopic[];
  }[];
};
