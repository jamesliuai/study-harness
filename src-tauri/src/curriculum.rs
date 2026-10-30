//! Small authored unit. Content relationships and observed performance are separate, reproducible views.
use crate::{
    contracts::*,
    db::{Result, Store, parse, sql},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::sync::OnceLock;
/// v2 replaces worked examples that duplicated the explanations with separate step-by-step related examples.
pub const CONTENT_VERSION: &str = "unit-content-v2";
pub fn unit() -> &'static Value {
    static U: OnceLock<Value> = OnceLock::new();
    U.get_or_init(|| {
        serde_json::from_str(include_str!("../../content/unit-v1.json")).expect("authored unit")
    })
}
pub fn objective(oid: &str) -> Result<&'static Value> {
    unit()["objectives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == oid)
        .ok_or("Choose an objective in the checked unit".into())
}
pub fn selected(c: &Connection, sid: &str) -> Result<String> {
    Ok(c.query_row(
        "SELECT objective_id FROM session_objectives WHERE session_id=?",
        [sid],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)?
    .unwrap_or("quadratic-convergence".into()))
}
/// The session's capability, or None before a goal or capability has started in it.
pub fn selected_if_any(c: &Connection, sid: &str) -> Result<Option<String>> {
    c.query_row(
        "SELECT objective_id FROM session_objectives WHERE session_id=?",
        [sid],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)
}
pub fn install(c: &Connection) -> Result<()> {
    for o in unit()["objectives"].as_array().unwrap() {
        c.execute(
            "INSERT OR IGNORE INTO objectives VALUES (?,?)",
            params![o["id"].as_str(), o.to_string()],
        )
        .map_err(sql)?;
    }
    Ok(())
}
/// The active task's frozen generated support, when the session's capability comes from a generated plan.
fn generated_support(c: &Connection, sid: &str) -> Result<Option<TaskSupport>> {
    if !crate::goals::is_generated(c, &selected(c, sid)?)? {
        return Ok(None);
    }
    let raw: Option<String> = c
        .query_row(
            "SELECT t.data FROM task_specs t JOIN learning_sessions l ON json_extract(l.data,'$.active_activity_id')=t.activity_id WHERE l.session_id=?",
            [sid],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    let spec: Option<TaskSpec> = raw.map(parse).transpose()?;
    Ok(Some(spec.and_then(|s| s.support).unwrap_or(TaskSupport {
        explanation: String::new(),
        hints: vec![],
        worked_example: String::new(),
        prerequisite: String::new(),
    })))
}
pub fn teaching(c: &Connection, sid: &str) -> Result<String> {
    if let Some(s) = generated_support(c, sid)? {
        return if s.explanation.trim().is_empty() {
            Err(
                "No explanation was prepared with this task. Ask the tutor a question instead."
                    .into(),
            )
        } else {
            Ok(s.explanation)
        };
    }
    let oid = selected(c, sid)?;
    let o = objective(&oid)?;
    Ok(format!(
        "{} Contrasting cases: {}",
        o["explanation"].as_str().unwrap(),
        o["contrasting_cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    ))
}
/// The authored step-by-step worked example: a related problem, never the current task's own solution.
pub fn worked_example(c: &Connection, sid: &str) -> Result<String> {
    if let Some(s) = generated_support(c, sid)? {
        return if s.worked_example.trim().is_empty() {
            Err("No related worked example was prepared with this task.".into())
        } else {
            Ok(format!(
                "Related example, not your task.\n{}",
                s.worked_example
            ))
        };
    }
    Ok(objective(&selected(c, sid)?)?["worked_example"]
        .as_str()
        .unwrap()
        .to_owned())
}
/// A small method cue from the authored assumptions and accepted methods; it never states the task's result.
pub fn hint(c: &Connection, sid: &str) -> Result<String> {
    if let Some(s) = generated_support(c, sid)? {
        if s.hints.is_empty() {
            return Err(
                "No hint was prepared with this task. Ask the tutor a question instead.".into(),
            );
        }
        // Hints escalate: each request reveals the next one, and the last repeats.
        let used: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM assistance h JOIN learning_sessions l ON json_extract(l.data,'$.active_activity_id')=h.activity_id WHERE l.session_id=? AND json_extract(h.data,'$.depth')='hint'",
                [sid],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let k = (used as usize).min(s.hints.len() - 1);
        return Ok(format!(
            "Hint {} of {}: {}",
            k + 1,
            s.hints.len(),
            s.hints[k]
        ));
    }
    let o = objective(&selected(c, sid)?)?;
    let first = |key: &str| {
        o[key]
            .as_array()
            .and_then(|v| v.first())
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok(format!(
        "Start from the setup: {} One way in: {}",
        first("assumptions"),
        first("acceptable_alternatives")
    ))
}
/// Explanation and worked example of the nearest prerequisite, for repairing it inside this session.
pub fn prerequisite(c: &Connection, sid: &str) -> Result<Option<(String, String)>> {
    if let Some(s) = generated_support(c, sid)? {
        return Ok((!s.prerequisite.trim().is_empty())
            .then(|| ("an earlier idea this builds on".into(), s.prerequisite)));
    }
    let o = objective(&selected(c, sid)?)?;
    let Some(pid) = o["prerequisites"]
        .as_array()
        .and_then(|p| p.last())
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let p = objective(pid)?;
    Ok(Some((
        p["outcome"].as_str().unwrap().to_owned(),
        format!(
            "Prerequisite: {}. {}\n{}",
            p["outcome"].as_str().unwrap(),
            p["explanation"].as_str().unwrap(),
            p["worked_example"].as_str().unwrap()
        ),
    )))
}
/// Everything frozen with one task. Authored objectives derive it deterministically; generated capabilities receive
/// it from a validated, checked task proposal. Both take the same path into the activity and task records.
pub struct Material {
    pub objective_id: String,
    pub prompt: String,
    pub solution: String,
    pub math: String,
    pub assumptions: Vec<String>,
    pub criteria: Vec<String>,
    pub rubric: Vec<String>,
    pub rubric_version: String,
    pub transfer_scope: String,
    pub visual: Option<VisualPlan>,
    pub origin: String,
    pub response_kind: String,
    pub support: Option<TaskSupport>,
    pub answer_key: Option<AnswerKey>,
    pub checks: Vec<CheckResult>,
    pub provenance: String,
    pub content_version: String,
    pub validation_version: String,
    pub validation_status: String,
}
pub const AUTHORED_PROVENANCE: &str = "Authored example unit (unit-content-v2): analytically checked against reference cases; independent human subject review pending.";

/// Short decimal for learner-facing text (at most four places, trailing zeros removed).
pub fn number(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}
fn params(values: &[(&str, f64)]) -> Vec<VisualParameter> {
    values
        .iter()
        .map(|(name, value)| VisualParameter {
            name: (*name).into(),
            value: *value,
        })
        .collect()
}
fn gd(a: f64, eta: f64, x0: f64) -> Vec<VisualParameter> {
    params(&[("a", a), ("eta", eta), ("x0", x0)])
}

/// Visual parameters for the diagnosis and exploration tasks of each objective, fixed with the task. Values match the
/// task wherever the template's bounds allow; otherwise the relation states the contrasting example and its exact
/// values in learner language. `n` is the task's own authored parameter (unused for the fixed convergence tasks).
pub fn visual_plan(oid: &str, kind: &str, n: i64) -> Option<VisualPlan> {
    if !["diagnosis", "exploration"].contains(&kind) {
        return None;
    }
    let template = objective(oid).ok()?["visual_template"].as_str()?.to_owned();
    // Authored task parameters start at 3 (see `material`); guard against a smaller value.
    let n = n.max(3);
    let nf = n as f64;
    let plan = |parameters, relation: String, purpose: &str, matches_task| VisualPlan {
        template_id: template.clone(),
        parameters,
        relation,
        purpose: purpose.into(),
        matches_task,
    };
    Some(match oid {
        "quadratic-derivative" => {
            if nf / 2.0 <= 2.0 {
                plan(
                    params(&[
                        ("A", nf / 2.0),
                        ("B", 1.0),
                        ("C", 3.0),
                        ("p", 2.0),
                        ("h", 0.5),
                    ]),
                    format!(
                        "Same function as your task: f(x) = ({n}/2)x² + x + 3, probe at x = 2, secant step h = 0.5."
                    ),
                    "Predict the tangent slope at x = 2, then watch secant slopes approach it as h shrinks.",
                    true,
                )
            } else {
                plan(
                    params(&[("A", 1.5), ("B", 1.0), ("C", 3.0), ("p", 2.0), ("h", 0.5)]),
                    format!(
                        "Contrasting example: the coefficient control stops at 2, so the visual shows f(x) = 1.5x² + x + 3 (n = 3) instead of your f(x) = ({n}/2)x² + x + 3. The method is the same: here f′(x) = 3x + 1; for your task f′(x) = {n}x + 1."
                    ),
                    "Predict the tangent slope at x = 2, then watch secant slopes approach it as h shrinks.",
                    false,
                )
            }
        }
        "quadratic-update" => {
            let (a, note) = if nf <= 10.0 {
                (nf, String::new())
            } else {
                (3.0, " The curvature control stops at 10, so the visual uses a = 3; because both step sizes scale with 1/a, the multipliers −1/2 and −3/2 are the same as in your task.".into())
            };
            plan(
                gd(a, 1.5 / a, 4.0),
                format!(
                    "{}a = {}, x₀ = 4, starting with η = 3/(2a) = {}. Then set your second step size, η = 5/(2a) = {}, with the step control.{note}",
                    if note.is_empty() {
                        "Same function and start as your task: "
                    } else {
                        "Contrasting example with the same multipliers: "
                    },
                    number(a),
                    number(1.5 / a),
                    number(2.5 / a)
                ),
                "Predict where one step lands and whether the loss falls, then compare the two step sizes.",
                note.is_empty(),
            )
        }
        "quadratic-convergence" => {
            if kind == "diagnosis" {
                plan(
                    gd(2.0, 0.75, 4.0),
                    "Same values as your task: f(x) = x² (a = 2), x₀ = 4, η = 0.75.".into(),
                    "Predict the first steps, reveal the iterations, then test other step sizes.",
                    true,
                )
            } else {
                plan(gd(2.0, 0.25, 4.0), "Same values as your task: a = 2 and x₀ = 4. The visual starts at η = 0.25; set η = 0.75 and then 1.25 with the step control.".into(),
                    "Compare same-side convergence, alternating convergence and divergence.", true)
            }
        }
        "quadratic-regimes" => {
            let eta = (2.0 * nf + 1.0) / (2.0 * (nf + 1.0));
            plan(
                gd(2.0, eta, 4.0),
                format!(
                    "This visual uses f(x) = x², so its multiplier is q = 1 − 2η. It starts at η = {} to give q = −{n}/{}; set η = {} for q = +{n}/{}, and η = 1 for q = −1. The factor {}/{n} > 1 would need a negative step, which this visual does not allow — reason about that case from xₖ = qᵏx₀.",
                    number(eta),
                    n + 1,
                    number(1.0 / (2.0 * (nf + 1.0))),
                    n + 1,
                    n + 1
                ),
                "Separate what the sign of the multiplier does from what its size does.",
                false,
            )
        }
        "quadratic-curvature" => {
            if 2.0 * nf <= 10.0 {
                plan(
                    gd(nf, 1.5 / nf, 4.0),
                    format!(
                        "Same step as your task: η = 3/(2·{n}) = {}, starting with a = {n}. Then set a = {} with the curvature control.",
                        number(1.5 / nf),
                        2 * n
                    ),
                    "Hold the step fixed and watch how a steeper curve changes the run.",
                    true,
                )
            } else {
                plan(
                    gd(3.0, 0.5, 4.0),
                    format!(
                        "Contrasting example with the same multipliers: the curvature control stops at 10, so the visual uses η = 0.5 with a = 3, then a = 6 (your task uses a = {n} and {} with η = 3/(2·{n})). Both give q = −1/2 and then q = −2.",
                        2 * n
                    ),
                    "Hold the step fixed and watch how a steeper curve changes the run.",
                    false,
                )
            }
        }
        "quadratic-boundaries" => {
            let a = if nf <= 10.0 { nf } else { 3.0 };
            plan(
                gd(a, 1.0 / a, 4.0),
                format!(
                    "{}a = {}. The visual starts at η = 1/a = {}; also try η = 0, η = 2/a = {} and η = 3/a = {}, then set x₀ = 0 to see a zero start.",
                    if nf <= 10.0 {
                        "Same curvature as your task: "
                    } else {
                        "Contrasting example with the same multipliers (the curvature control stops at 10): "
                    },
                    number(a),
                    number(1.0 / a),
                    number(2.0 / a),
                    number(3.0 / a)
                ),
                "Check each boundary case, including a zero start.",
                nf <= 10.0,
            )
        }
        "recurrence-transfer" => {
            let eta = (2.0 * nf + 1.0) / (2.0 * (nf + 1.0));
            plan(
                gd(2.0, eta, 4.0),
                format!(
                    "This visual plots the error eₖ = zₖ − {n}, not z itself. Gradient descent on f(x) = x² with η = {} has the same update eₖ₊₁ = −({n}/{})·eₖ, starting from e₀ = 4 (that is, z₀ = {}). Add {n} to every plotted position to read zₖ.",
                    number(eta),
                    n + 1,
                    n + 4
                ),
                "See the shifted recurrence as an error that shrinks toward zero.",
                false,
            )
        }
        "surface-gradient" => {
            let (a, c) = (nf, nf + 2.0);
            if c <= 4.0 {
                plan(
                    params(&[("a", a), ("b", 1.0), ("c", c), ("px", 1.0), ("py", -2.0)]),
                    format!(
                        "Same surface and point as your task: a = {n}, b = 1, c = {}, at (1, −2).",
                        n + 2
                    ),
                    "Predict the gradient direction at the point, then compare it with the contour lines.",
                    true,
                )
            } else {
                plan(
                    params(&[
                        ("a", 2.0),
                        ("b", 1.0),
                        ("c", 4.0),
                        ("px", 1.0),
                        ("py", -2.0),
                    ]),
                    format!(
                        "Contrasting example: the coefficient controls stop at 4, so the visual shows f(x, y) = x² + xy + 2y² (a = 2, b = 1, c = 4) at your task's point (1, −2). Your surface has a = {n} and c = {}; its gradient has the same form, ∇f = (ax + by, bx + cy).",
                        n + 2
                    ),
                    "Predict the gradient direction at the point, then compare it with the contour lines.",
                    false,
                )
            }
        }
        _ => return None,
    })
}
pub fn material(c: &Connection, sid: &str, kind: &str) -> Result<Option<Material>> {
    let oid = selected(c, sid)?;
    if oid == "quadratic-convergence" {
        return Ok(None);
    }
    let o = objective(&oid)?;
    let count:i64=c.query_row("SELECT COUNT(*) FROM activities a JOIN task_specs t ON a.id=t.activity_id WHERE a.objective_id=? AND json_extract(t.data,'$.kind')=?",params![oid,kind],|r|r.get(0)).map_err(sql)?;
    let n = match kind {
        "independent" => 11,
        "review" => 17,
        "guided" => 7,
        "completion" => 5,
        _ => 3,
    } + 2 * count;
    let p = 2;
    let b = 1;
    let (problem,solution)=match oid.as_str(){
        "quadratic-derivative"=>(format!("For f(x)=({n}/2)x²+x+3, find the tangent slope at x={p}. Derive the secant slope from {p} to {p}+h for h≠0 and explain what h=0 and the limit h→0 mean."),format!("f′(x)={n}x+1, so f′(2)={}. The secant slope is {n}·2+1+({n}/2)h={}+({n}/2)h for h≠0. The original quotient at h=0 is undefined; its limit is {}. The derivative is a slope, not the function value.",2*n+b,2*n+b,2*n+b)),
        "quadratic-update"=>(format!("For f(x)=({n}/2)x², x₀=4, compare one gradient step with η=3/(2·{n}) and with η=5/(2·{n}). Compute the next positions and the loss ratios. Explain why a locally downhill direction may still raise the loss after a finite step."),"x′=(1−ηa)x. The multipliers are −1/2 and −3/2, giving x₁=−2 and −6. Loss ratios are q²=1/4 and 9/4. Crossing zero is compatible with loss decrease; a larger step overshoots far enough to raise the loss.".into()),
        "quadratic-regimes"=>(format!("Starting from a nonzero scalar, compare multipliers {n}/({n}+1), −{n}/({n}+1), −1, and ({n}+1)/{n}. For each, state convergence to zero and sign changes, and justify the distinction."),format!("±{n}/({n}+1) have magnitude below one and converge to zero; only the negative factor alternates signs. Factor −1 alternates at constant nonzero magnitude. ({n}+1)/{n}>1 grows with the same sign. Sign and magnitude answer different questions.")),
        "quadratic-curvature"=>(format!("Hold η=3/(2·{n}) fixed. Compare positive curvatures a={n} and a=2·{n} for f(x)=a x²/2 from a nonzero start. Find both multipliers, classify the runs, and derive how the safe upper step bound depends on a."),"At a=n, q=1−3/2=−1/2: alternating convergence. At a=2n, q=1−3=−2: alternating divergence. The strict upper bound is 2/a, so doubling curvature halves it. This is an exact fixed-step scalar-quadratic result.".into()),
        "quadratic-boundaries"=>(format!("For curvature a={n}, classify η=0, η=1/{n}, η=2/{n}, and η=3/{n}, first with x₀≠0 and then x₀=0. State which cases reach zero and distinguish exact behavior from numerical overflow."),"The factors are 1,0,−1,−2. Nonzero starts respectively stay fixed, reach zero in one step, alternate at constant magnitude, and alternate with growth. Zero starts stay zero in all four cases. Overflow is a finite representation limit, not mathematical undefinedness.".into()),
        "recurrence-transfer"=>(format!("A process follows zₖ₊₁={n}−({n}/({n}+1))(zₖ−{n}), with z₀≠{n}. Determine its limit and explain whether it crosses that limit. What changes if the negative factor becomes −1? Do not assume the limit is zero."),format!("Let eₖ=zₖ−{n}. Then eₖ₊₁=−{n}/({n}+1)eₖ and eₖ=(−{n}/({n}+1))ᵏe₀. Errors alternate and shrink; z tends to {n} while crossing it. With factor −1 the error has constant magnitude, so z alternates in a two-cycle unless it starts at the fixed point.")),
        "surface-gradient"=>(format!("For f(x,y)=({n}/2)x²+xy+({}/2)y², find the gradient at (1,−2), the direction of steepest local decrease, and its relationship to a contour tangent. Check that the quadratic is a bowl. Explain the special case at the origin.",n+2),format!("∇f=({n}x+y,x+{}y). At (1,−2) it is ({},{}); a steepest descent direction is its negative, or that vector normalized. Along a contour, ∇f·t=0. The Hessian has a={n}>0 and ac−b²={}>0, so it is positive definite. At the origin the gradient is zero and has no direction.",n+2,n-2,-2*n-3,n*(n+2)-1)),
        _=>return Err("Unsupported authored objective".into()),
    };
    let visual = visual_plan(&oid, kind, n);
    let (prompt, transfer) = match kind {
        "instruction" => (
            format!(
                "Read the explanation of how to {}. Then restate its key step in your own words and say what you would change in your earlier answer. Ask about any step that does not follow.",
                lower_first(o["outcome"].as_str().unwrap())
            ),
            "instruction; no independent assessment",
        ),
        "worked_example" => (
            "Study the related worked example. Then redo its first consequential step yourself — the step the rest of the answer depends on — or ask about any step.".into(),
            "worked example; supported",
        ),
        "completion" => (
            format!("Complete the reasoning using the worked method. {problem}"),
            "completion with an explicit method cue",
        ),
        "guided" => (
            format!(
                "Compare the contrasting cases and explain the first consequential difference. {problem}"
            ),
            "guided contrast; method selection is cued",
        ),
        "exploration" => (
            format!(
                "Before revealing the visual's result, predict what it will show. {} Send your prediction, then reveal the result, explore the controls, and compare what you see with what you predicted.",
                visual.as_ref().map(|v| v.relation.as_str()).unwrap_or("")
            ),
            "prediction and local visual exploration",
        ),
        "explanation" => (
            format!(
                "Explain the change you observed, identify the relevant assumptions, and connect it to: {}.",
                o["outcome"].as_str().unwrap()
            ),
            "explanation after visual support",
        ),
        "review" => (
            format!("Later check, without earlier teaching: {problem}"),
            if oid == "recurrence-transfer" {
                "limited representational transfer; shifted recurrence"
            } else {
                "later fresh numerical variant; near transfer"
            },
        ),
        "independent" => (
            problem.clone(),
            "fresh numerical variant; near transfer within one objective",
        ),
        _ => (problem.clone(), "initial diagnostic prediction"),
    };
    let solution = if kind == "worked_example" {
        o["worked_example"].as_str().unwrap().to_owned()
    } else if ["instruction", "exploration", "explanation"].contains(&kind) {
        o["explanation"].as_str().unwrap().to_owned()
    } else {
        solution
    };
    Ok(Some(Material {
        objective_id: oid,
        prompt,
        solution,
        math: String::new(),
        assumptions: o["assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().into())
            .collect(),
        criteria: o["rubric_criteria"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().into())
            .collect(),
        rubric: crate::learning::DIMENSIONS.iter().map(|d| d.to_string()).collect(),
        rubric_version: o["rubric_version"].as_str().unwrap().into(),
        transfer_scope: transfer.into(),
        visual,
        origin: "authored".into(),
        response_kind: String::new(),
        support: None,
        answer_key: None,
        checks: vec![],
        provenance: AUTHORED_PROVENANCE.into(),
        content_version: CONTENT_VERSION.into(),
        validation_version: "unit-oracles-v1".into(),
        validation_status: "Analytically checked authored objective; live grading and learning effectiveness unverified".into(),
    }))
}
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}
impl Store {
    pub fn unit_view(&self, sid: &str) -> Result<UnitView> {
        let active = selected(&self.conn, sid)?;
        let mut objectives = vec![];
        for o in unit()["objectives"].as_array().unwrap() {
            let oid = o["id"].as_str().unwrap();
            let coverage:i64=self.conn.query_row("SELECT COUNT(*) FROM assistance h JOIN activities a ON a.id=h.activity_id WHERE a.objective_id=?",[oid],|r|r.get(0)).map_err(sql)?;
            let counts: Vec<String> = {
                let mut s=self.conn.prepare("SELECT l.data FROM learning_summaries l JOIN session_objectives u ON u.session_id=l.session_id WHERE u.objective_id=? AND json_extract(l.data,'$.stale')=0").map_err(sql)?;
                s.query_map([oid], |r| r.get(0))
                    .map_err(sql)?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(sql)?
            };
            let summaries = counts
                .into_iter()
                .map(parse::<LearningSummary>)
                .collect::<Result<Vec<_>>>()?;
            objectives.push(UnitObjective {
                id: oid.into(),
                outcome: o["outcome"].as_str().unwrap().into(),
                prerequisites: o["prerequisites"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().into())
                    .collect(),
                visual_template: o["visual_template"].as_str().unwrap().into(),
                validation_status: "Analytically checked; independent human review pending".into(),
                coverage_count: coverage as u32,
                independent_count: summaries
                    .iter()
                    .filter(|s| !s.simulated)
                    .map(|s| s.independent_count)
                    .sum(),
                simulated_independent_count: summaries
                    .iter()
                    .filter(|s| s.simulated)
                    .map(|s| s.independent_count)
                    .sum(),
                delayed_count: summaries
                    .iter()
                    .filter(|s| !s.simulated)
                    .map(|s| s.delayed_count)
                    .sum(),
            });
        }
        let mut plan = objective(&active)
            .ok()
            .and_then(|o| o["prerequisites"].as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if active != "exploratory-unchecked" {
            plan.push(active.clone());
        }
        let due:Vec<Review>={let mut stmt=self.conn.prepare("SELECT data FROM reviews WHERE json_extract(data,'$.status')='scheduled' ORDER BY json_extract(data,'$.due_at')").map_err(sql)?;stmt.query_map([],|r|r.get::<_,String>(0)).map_err(sql)?.map(|r|parse(r.map_err(sql)?)).collect::<Result<Vec<Review>>>()?}.into_iter().filter(|r|chrono::DateTime::parse_from_rfc3339(&r.due_at).is_ok_and(|at|at<=self.clock.now())).collect();
        let minutes: u32 = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='review_minutes'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(sql)?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or(10);
        let due = due
            .into_iter()
            .filter(|r| {
                crate::evaluation::review_exclusion(&self.conn, &r.session_id, &self.time())
                    .is_ok_and(|blocked| blocked.is_none())
            })
            .collect::<Vec<_>>();
        let due_count = due.len() as u32;
        Ok(UnitView {
            version: CONTENT_VERSION.into(),
            title: unit()["title"].as_str().unwrap().into(),
            notice: unit()["provisional_notice"].as_str().unwrap().into(),
            active_objective_id: active,
            plan,
            objectives,
            continue_session_id: self
                .conn
                .query_row(
                    "SELECT session_id FROM learning_sessions ORDER BY rowid DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?,
            due_review_count: due_count,
            due_reviews: due
                .into_iter()
                .take((minutes / 3).clamp(1, 5) as usize)
                .collect(),
            available_review_minutes: minutes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unit_has_eight_acyclic_objectives_and_honest_provenance() {
        let rows = unit()["objectives"].as_array().unwrap();
        assert_eq!(rows.len(), 8);
        let mut prior = std::collections::HashSet::new();
        for o in rows {
            for p in o["prerequisites"].as_array().unwrap() {
                assert!(prior.contains(p.as_str().unwrap()));
            }
            assert!(o["validation"]["human_subject_review"].is_null());
            prior.insert(o["id"].as_str().unwrap());
        }
    }
    #[test]
    fn independently_check_the_unit_references() {
        // Separate finite-difference and geometric checks; no visualizer kernels or task generators are imported.
        for a in [3.0_f64, 4.0, 7.0, 11.0] {
            let f = |x: f64| a * x * x / 2.0 + x + 3.0;
            let x = 2.0;
            let h = 1e-5;
            assert!(((f(x + h) - f(x - h)) / (2.0 * h) - (a * x + 1.0)).abs() < 1e-7);
            for eta in [1.5 / a, 2.5 / a] {
                let before = a * 16.0 / 2.0;
                let next = 4.0 - eta * a * 4.0;
                let ratio = a * next * next / 2.0 / before;
                assert!((ratio - (1.0 - eta * a).powi(2)).abs() < 1e-12);
            }
            let grad = [a - 2.0, -2.0 * a - 3.0];
            let tangent = [-grad[1], grad[0]];
            assert_eq!(grad[0] * tangent[0] + grad[1] * tangent[1], 0.0);
            assert!(a * (a + 2.0) - 1.0 > 0.0);
        }
        let mut z = 9.0_f64;
        for _ in 0..200 {
            z = 3.0 - 0.8 * (z - 3.0);
        }
        assert!((z - 3.0).abs() < 1e-12);
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    /// Every planned visual stays inside its template's parameter bounds, and a plan that claims to match its task
    /// reproduces the task's own numbers.
    #[test]
    fn visual_plans_stay_in_template_bounds_and_state_their_relation() {
        let catalog: Value =
            serde_json::from_str(include_str!("../visualizer/catalog.json")).unwrap();
        for o in unit()["objectives"].as_array().unwrap() {
            let oid = o["id"].as_str().unwrap();
            for kind in ["diagnosis", "exploration"] {
                for n in [0, 3, 5, 7, 9, 11, 13] {
                    let plan = visual_plan(oid, kind, n).expect("plan");
                    let entry = catalog["templates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|t| t["template"]["templateId"] == plan.template_id.as_str())
                        .expect("installed template");
                    for p in &plan.parameters {
                        let bounds = &entry["parameters"][&p.name];
                        let (lo, hi) = (bounds[0].as_f64().unwrap(), bounds[1].as_f64().unwrap());
                        assert!(
                            p.value.is_finite() && lo <= p.value && p.value <= hi,
                            "{oid} {kind} n={n} {}={}",
                            p.name,
                            p.value
                        );
                    }
                    assert!(!plan.relation.is_empty() && !plan.relation.contains("may differ"));
                }
            }
            assert!(visual_plan(oid, "independent", 3).is_none());
        }
        let p = visual_plan("quadratic-update", "diagnosis", 5).unwrap();
        assert!(p.matches_task);
        let value = |name: &str| p.parameters.iter().find(|x| x.name == name).unwrap().value;
        assert_eq!(value("a"), 5.0);
        assert!((value("eta") - 0.3).abs() < 1e-12 && value("x0") == 4.0);
        let r = visual_plan("recurrence-transfer", "diagnosis", 3).unwrap();
        let eta = r.parameters.iter().find(|x| x.name == "eta").unwrap().value;
        assert!(
            ((1.0 - 2.0 * eta) + 0.75).abs() < 1e-12,
            "error multiplier −3/4"
        );
        assert!(r.relation.contains("error"));
    }
    /// Independent arithmetic checks of the authored step-by-step worked examples.
    #[test]
    fn worked_examples_are_distinct_and_arithmetically_consistent() {
        for o in unit()["objectives"].as_array().unwrap() {
            assert_ne!(o["worked_example"], o["explanation"], "{}", o["id"]);
            assert!(o["worked_example"].as_str().unwrap().contains("\n1."));
        }
        let f = |x: f64| 2.0 * x * x - 3.0 * x + 1.0;
        for h in [0.5, 0.1, -0.25] {
            assert!(((f(1.0 + h) - f(1.0)) / h - (2.0 * h + 1.0)).abs() < 1e-12);
        }
        let g = |x: f64| 1.5 * x * x;
        assert_eq!((2.0 - 0.5 * 6.0, g(-1.0)), (-1.0, 1.5));
        assert!((g(2.0 - 0.8 * 6.0) - 11.76).abs() < 1e-12);
        let mut x = 5.0_f64;
        for expected in [-1.0, 0.2, -0.04] {
            x *= 1.0 - 0.3 * 4.0;
            assert!((x - expected).abs() < 1e-12);
        }
        assert!((10.0 * 1.2_f64.powi(3) - 17.28).abs() < 1e-12);
        assert_eq!(
            [1.0 - 0.25 * 2.0, 1.0 - 0.25 * 6.0, 1.0 - 0.25 * 10.0],
            [0.5, -0.5, -1.5]
        );
        let mut z = 6.0_f64;
        for expected in [0.0, 3.0, 1.5] {
            z = 2.0 - 0.5 * (z - 2.0);
            assert!((z - expected).abs() < 1e-12);
        }
        let grad = |x: f64, y: f64| (2.0 * x + 2.0 * y, 2.0 * x + 6.0 * y);
        assert_eq!(grad(1.0, 1.0), (4.0, 8.0));
        assert_eq!(4.0 * 2.0 - 8.0, 0.0);
        assert_eq!(2.0 * 6.0 - 2.0 * 2.0, 8.0);
    }
}
