import { describe, expect, it } from 'vitest';
import type { TimelineEntry } from '../src/generated/contracts';
import { isCheck, phaseOf, phasesFor, reading } from '../src/lesson';

const entry = (id: string, kind: string, at: string, activity: string): TimelineEntry => ({
  id,
  kind,
  at,
  activity_id: activity,
  stage: kind === 'task' ? 'instruction' : '',
  text: id,
  detail: '',
  attempt_id: null,
  assistance_id: null,
  hidden: false,
});

describe('lesson phases', () => {
  it('groups the core stages into the phases a learner sees', () => {
    expect(
      ['diagnosis', 'instruction', 'guided', 'independent', 'summary'].map(
        (s) => phaseOf(s)?.label,
      ),
    ).toEqual(['Try', 'Learn', 'Practice', 'Check', 'Wrap-up']);
    expect(phasesFor('review', false).map((p) => p.id)).toEqual(['review', 'done']);
    expect(phasesFor('summary', true).map((p) => p.id)).toEqual(['review', 'done']);
    expect(phasesFor('exploratory', false)).toEqual([]);
  });

  it('treats only independent checks and reviews as checks', () => {
    expect(['independent', 'review'].every(isCheck)).toBe(true);
    expect(['diagnosis', 'guided', 'summary', undefined].some(isCheck)).toBe(false);
  });
});

describe('reading order', () => {
  it('reads teaching written with a task before the task, and keeps everything else in order', () => {
    const t = '2026-10-30T10:00:00Z';
    const later = '2026-10-30T10:05:00Z';
    const ordered = reading([
      entry('answer-1', 'answer', '2026-10-30T09:59:00Z', 'a1'),
      entry('task-2', 'task', t, 'a2'),
      entry('explain-2', 'help', t, 'a2'),
      entry('answer-2', 'answer', later, 'a2'),
      entry('hint-2', 'help', later, 'a2'),
    ]).map((e) => e.id);
    expect(ordered).toEqual(['answer-1', 'explain-2', 'task-2', 'answer-2', 'hint-2']);
  });

  it('never moves help that belongs to another task', () => {
    const t = '2026-10-30T10:00:00Z';
    const ordered = reading([
      entry('task-2', 'task', t, 'a2'),
      entry('help-1', 'help', t, 'a1'),
    ]).map((e) => e.id);
    expect(ordered).toEqual(['task-2', 'help-1']);
  });
});
