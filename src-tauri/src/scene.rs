use crate::contracts::*;
pub fn sample(activity_id: &str) -> SceneSpec {
    SceneSpec {
        schema_version: 1,
        capability_version: "reference-1".into(),
        model_id: "quadratic-descent".into(),
        model_version: "1".into(),
        objective_id: "quadratic-convergence".into(),
        activity_id: activity_id.into(),
        parameters: SceneParameters {
            curvature: 2.,
            step_size: 0.75,
            start: 4.,
            steps: 12,
        },
        prediction: "Predict whether the iterates converge, alternate, or diverge.".into(),
        follow_up: "Explain how the multiplier changes the behavior.".into(),
        assumptions: vec![
            "Exact one-dimensional quadratic; positive curvature; fixed step size.".into(),
        ],
        validation: "analytic-cases-v1".into(),
        accessibility: "Keyboard controls and an equivalent iteration table.".into(),
        revealed: false,
    }
}
pub fn validate(s: &SceneSpec) -> Result<(), String> {
    let p = &s.parameters;
    if s.schema_version != 1
        || s.capability_version != "reference-1"
        || s.model_id != "quadratic-descent"
        || s.model_version != "1"
        || !p.curvature.is_finite()
        || !p.step_size.is_finite()
        || !p.start.is_finite()
        || !(0.25..=5.).contains(&p.curvature)
        || !(0.0..=1.5).contains(&p.step_size)
        || !(-10.0..=10.).contains(&p.start)
        || !(1..=40).contains(&p.steps)
    {
        return Err("Unsupported scene or out-of-range parameters.".into());
    }
    Ok(())
}
pub fn positions(p: &SceneParameters) -> Vec<f64> {
    let q = 1. - p.curvature * p.step_size;
    let mut x = p.start;
    let mut result = vec![x];
    for _ in 0..p.steps {
        x *= q;
        if !x.is_finite() || x.abs() > 1e6 {
            break;
        }
        result.push(x);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_analytic_cases() {
        for (eta, expected) in [
            (0.25, vec![4., 2., 1., 0.5]),
            (0.75, vec![4., -2., 1., -0.5]),
            (1.25, vec![4., -6., 9., -13.5]),
            (0., vec![4., 4., 4., 4.]),
            (0.5, vec![4., 0., 0., 0.]),
            (1., vec![4., -4., 4., -4.]),
        ] {
            assert_eq!(
                positions(&SceneParameters {
                    curvature: 2.,
                    step_size: eta,
                    start: 4.,
                    steps: 3
                }),
                expected
            )
        }
    }
    #[test]
    fn zero_and_bounds() {
        let mut s = sample("a");
        s.parameters.start = 0.;
        s.parameters.step_size = 1.5;
        assert_eq!(positions(&s.parameters), vec![0.; 13]);
        s.parameters.curvature = f64::NAN;
        assert!(validate(&s).is_err());
        s.parameters.curvature = 2.;
        s.schema_version = 99;
        assert!(validate(&s).is_err());
    }
}
