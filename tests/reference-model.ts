// Independent Goal 2 analytic regression oracle. Never imported by learner code.
import type { SceneSpec, SceneParameters } from '../src/generated/contracts';
export type Observation = {
  positions: number[];
  multiplier: number;
  truncated: boolean;
  behavior: string;
};
export function validateScene(spec: SceneSpec) {
  const p = spec.parameters;
  if (
    spec.schema_version !== 1 ||
    spec.capability_version !== 'reference-1' ||
    spec.model_id !== 'quadratic-descent' ||
    spec.model_version !== '1' ||
    ![p.curvature, p.step_size, p.start, p.steps].every(Number.isFinite) ||
    p.curvature < 0.25 ||
    p.curvature > 5 ||
    p.step_size < 0 ||
    p.step_size > 1.5 ||
    Math.abs(p.start) > 10 ||
    !Number.isInteger(p.steps) ||
    p.steps < 1 ||
    p.steps > 40
  )
    throw new Error('Unsupported or out-of-bounds scene.');
}
export function compute(p: SceneParameters): Observation {
  const multiplier = 1 - p.curvature * p.step_size;
  const positions = [p.start];
  let x = p.start;
  for (let t = 0; t < p.steps; t++) {
    x = x === 0 ? 0 : x * multiplier;
    if (!Number.isFinite(x) || Math.abs(x) > 1e6) break;
    positions.push(x);
  }
  const behavior =
    p.start === 0
      ? 'Fixed at the minimum'
      : multiplier === 1
        ? 'Unchanged position'
        : multiplier === 0
          ? 'Reaches zero in one step'
          : multiplier === -1
            ? 'Constant-magnitude oscillation'
            : Math.abs(multiplier) < 1
              ? multiplier < 0
                ? 'Converges while alternating'
                : 'Converges from one side'
              : 'Diverges while alternating';
  return { positions, multiplier, truncated: positions.length !== p.steps + 1, behavior };
}
