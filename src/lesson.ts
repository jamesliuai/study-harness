import type { IconName } from './components/Icon';
import type { TimelineEntry } from './generated/contracts';

/**
 * The learner-facing shape of a lesson. The core keeps finer stages (and decides every transition); the interface
 * groups them into a few phases so a lesson reads as one activity rather than a sequence of mode screens.
 */
export type Phase = { id: string; label: string; stages: string[] };

const LESSON: Phase[] = [
  { id: 'try', label: 'Try', stages: ['diagnosis'] },
  { id: 'learn', label: 'Learn', stages: ['instruction', 'worked_example'] },
  {
    id: 'practice',
    label: 'Practice',
    stages: ['completion', 'guided', 'exploration', 'explanation'],
  },
  { id: 'check', label: 'Check', stages: ['independent'] },
  { id: 'done', label: 'Wrap-up', stages: ['summary'] },
];
const REVIEW: Phase[] = [
  { id: 'review', label: 'Review', stages: ['review'] },
  { id: 'done', label: 'Wrap-up', stages: ['summary'] },
];

/** The phase a task stage belongs to, for dividers between phases. */
export const phaseOf = (stage: string) =>
  [...LESSON, ...REVIEW].find((p) => p.stages.includes(stage));

/** The phases for a stage, or none for the legacy unreviewed discussion. */
export function phasesFor(stage: string, reviewing: boolean): Phase[] {
  if (stage === 'exploratory') return [];
  return stage === 'review' || (reviewing && stage === 'summary') ? REVIEW : LESSON;
}

export const STAGE_ICON: Record<string, IconName> = {
  diagnosis: 'pencil',
  instruction: 'book',
  worked_example: 'stairs',
  completion: 'pencil',
  guided: 'compare',
  exploration: 'chart',
  explanation: 'chart',
  independent: 'target',
  review: 'review',
  summary: 'checkCircle',
  exploratory: 'question',
};

/** Independent checks and later reviews: earlier teaching is withheld and any help is recorded. */
export const isCheck = (stage: string | undefined) => stage === 'independent' || stage === 'review';

/**
 * Teaching written when a task opens (an explanation or a related example) shares the task's timestamp; it reads
 * before the task that asks the learner to use it. Otherwise the core's chronological order is kept.
 */
export function reading(entries: TimelineEntry[]) {
  const out: TimelineEntry[] = [];
  const moved = new Set<string>();
  entries.forEach((e, i) => {
    if (moved.has(e.id)) return;
    if (e.kind === 'task')
      for (const h of entries.slice(i + 1)) {
        if (h.at !== e.at) break;
        if (h.kind === 'help' && h.activity_id === e.activity_id) {
          out.push(h);
          moved.add(h.id);
        }
      }
    out.push(e);
  });
  return out;
}
