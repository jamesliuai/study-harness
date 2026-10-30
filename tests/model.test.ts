import { describe, it, expect } from 'vitest';
import { compute, validateScene } from './reference-model';
import { encodeWav } from '../src/audio';
import type { SceneSpec } from '../src/generated/contracts';
const spec: SceneSpec = {
  schema_version: 1,
  capability_version: 'reference-1',
  model_id: 'quadratic-descent',
  model_version: '1',
  objective_id: 'quadratic-convergence',
  activity_id: 'test',
  parameters: { curvature: 2, step_size: 0.75, start: 4, steps: 3 },
  prediction: 'Predict',
  follow_up: 'Explain',
  assumptions: ['quadratic'],
  validation: 'analytic-v1',
  accessibility: 'table',
  revealed: false,
};
describe('independent analytic checkpoints', () => {
  it.each([
    [0.25, [4, 2, 1, 0.5]],
    [0.75, [4, -2, 1, -0.5]],
    [1.25, [4, -6, 9, -13.5]],
    [0, [4, 4, 4, 4]],
    [0.5, [4, 0, 0, 0]],
    [1, [4, -4, 4, -4]],
  ])('eta=%s', (eta, expected) =>
    expect(compute({ ...spec.parameters, step_size: eta as number }).positions).toEqual(expected),
  );
  it('handles zero start and truncated growth', () => {
    expect(compute({ ...spec.parameters, start: 0 }).positions).toEqual([0, 0, 0, 0]);
    expect(compute({ curvature: 5, step_size: 1.5, start: 10, steps: 40 }).truncated).toBe(true);
  });
  it('rejects incompatible and unbounded scenes', () => {
    for (const p of [
      { schema_version: 2 },
      { parameters: { ...spec.parameters, curvature: NaN } },
      { parameters: { ...spec.parameters, steps: 1e9 } },
      { model_id: 'eval' },
    ])
      expect(() => validateScene({ ...spec, ...p })).toThrow();
    expect(() => validateScene(spec)).not.toThrow();
  });
});
it('normalizes recording to genuine 16kHz mono PCM WAV', () => {
  const data = encodeWav(new Float32Array([-1, 0, 1]));
  const v = new DataView(data.buffer);
  expect(new TextDecoder().decode(data.slice(0, 4))).toBe('RIFF');
  expect(v.getUint32(24, true)).toBe(16000);
  expect(v.getUint16(22, true)).toBe(1);
  expect(v.getInt16(44, true)).toBe(-32768);
  expect(v.getInt16(48, true)).toBe(32767);
  expect(v.getUint32(40, true)).toBe(6);
});
