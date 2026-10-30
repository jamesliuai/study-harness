//! Plan and task generation through the same bounded provider boundary as tutoring. The model proposes; the core
//! validates the proposal, runs deterministic checks and freezes the result before anything is shown. A generated
//! plan or task is labelled generated and unreviewed wherever it appears, and its checks are recorded beside it.
use crate::{
    checks::{self, result},
    contracts::*,
    curriculum::Material,
    db::Result,
    learning::clip,
    providers,
    registry::{ModelChoice, Purpose, TutorModel},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub const PLAN_PROMPT_VERSION: &str = "planner-v1";
pub const PLAN_PROMPT: &str = include_str!("../../prompts/planner-v1.txt");
pub const TASK_PROMPT_VERSION: &str = "task-v1";
pub const TASK_PROMPT: &str = include_str!("../../prompts/task-v1.txt");
const MAX_PAYLOAD: usize = 60_000;

pub const ASSESSMENTS: [&str; 8] = [
    "numeric",
    "expression",
    "explanation",
    "proof",
    "derivation",
    "code_reasoning",
    "classification",
    "short_answer",
];
/// The rubric vocabulary from the specification: which part of the work a task actually tests.
pub const DIMENSIONS: [&str; 5] = [
    "method_selection",
    "setup",
    "execution",
    "interpretation",
    "justification",
];
pub const ERROR_KINDS: [&str; 8] = [
    "none",
    "conceptual",
    "procedural",
    "arithmetic_slip",
    "notation_or_transcription",
    "incomplete",
    "task_flaw",
    "unclear",
];

pub fn dimension_label(d: &str) -> &'static str {
    match d {
        "method_selection" => "Method selection",
        "setup" => "Setup",
        "execution" => "Execution",
        "interpretation" => "Interpretation",
        "justification" => "Justification",
        _ => "Criterion",
    }
}

/// What each stage's task is for, in the words the task writer receives.
pub fn stage_purpose(stage: &str) -> &'static str {
    match stage {
        "diagnosis" => {
            "A representative opening question that reveals the learner's current approach."
        }
        "instruction" => {
            "Teach the key idea in support.explanation, connected to the learner's latest attempt; the statement asks for a reconstruction or one application."
        }
        "worked_example" => {
            "A fully worked related example in support.worked_example; the statement asks for the first consequential step of a similar case."
        }
        "completion" => {
            "A partly worked problem with one or two key steps left; the method may be cued."
        }
        "guided" => {
            "Contrasting cases or a changed representation that separate the idea from a plausible misconception."
        }
        "exploration" => {
            "A prediction about the visual's model before its result is revealed, then exploration."
        }
        "explanation" => "Explain what the visual showed and connect it to the capability.",
        "independent" => {
            "A fresh check without cues: the learner must choose or apply the idea on a new problem."
        }
        "review" => {
            "A later check after a gap: a new problem, ideally a changed representation, without naming the method."
        }
        _ => "A task for this capability.",
    }
}

pub struct PlanInput {
    pub request: Request,
    /// Model and effort frozen when the request was prepared; plans think at medium effort or more.
    pub choice: ModelChoice,
    pub goal_text: String,
    pub instruction: String,
    pub prior: Option<PlanRevision>,
    pub progress: Vec<Value>,
    pub passages: Vec<SourcePassage>,
}

pub struct TaskInput {
    pub request: Request,
    /// Model and effort frozen when the request was prepared; tasks think at medium effort or more.
    pub choice: ModelChoice,
    pub stage: String,
    pub goal_text: String,
    pub plan: Value,
    pub capability: Value,
    pub note: String,
    pub prior_tasks: Vec<String>,
    pub learner: Vec<Value>,
    pub passages: Vec<SourcePassage>,
    /// Template description with exact parameter ranges, present only when this stage uses the capability's visual.
    pub visual: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProposal {
    pub key: String,
    pub outcome: String,
    pub why: String,
    pub prerequisites: Vec<String>,
    pub assessment: String,
    pub minutes: i64,
    pub visual: Option<PlanVisual>,
    pub keep: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanProposal {
    pub title: String,
    pub summary: String,
    pub level: String,
    pub background: Vec<String>,
    pub clarification: Option<Clarification>,
    pub capabilities: Vec<CapabilityProposal>,
    pub source_use: String,
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RubricLine {
    pub dimension: String,
    pub criterion: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskVisual {
    pub parameters: Vec<VisualParameter>,
    pub relation: String,
    pub matches_task: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfCheck {
    pub solvable: bool,
    pub ambiguities: Vec<String>,
    pub notes: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProposal {
    pub statement: String,
    pub assumptions: Vec<String>,
    pub response_kind: String,
    pub rubric: Vec<RubricLine>,
    pub reference_solution: String,
    pub acceptable_alternatives: Vec<String>,
    pub common_errors: Vec<String>,
    pub answer_key: AnswerKey,
    pub support: TaskSupport,
    pub transfer: String,
    pub visual: Option<TaskVisual>,
    pub self_check: SelfCheck,
}

fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
fn parameters() -> Value {
    json!({"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"},"value":{"type":"number"}},"required":["name","value"]}})
}

fn envelope(
    prompt: &str,
    name: &str,
    schema: Value,
    input: Value,
    choice: &ModelChoice,
    purpose: Purpose,
) -> Value {
    json!({"model":choice.model.id,"store":false,"service_tier":"default","reasoning":{"effort":choice.effort_for(purpose)},"max_output_tokens":choice.max_output(purpose),"instructions":prompt,"input":input.to_string(),"text":{"format":{"type":"json_schema","name":name,"strict":true,"schema":schema}}})
}

fn passages_json(passages: &[SourcePassage]) -> Value {
    json!(
        passages
            .iter()
            .map(|p| json!({"chunk_id":p.id,"title":p.title,"anchor":p.anchor,"text":p.text}))
            .collect::<Vec<_>>()
    )
}

pub fn plan_payload(i: &PlanInput) -> Value {
    let templates: Vec<String> = crate::visualizer::installed()["templates"]
        .as_array()
        .map(|t| {
            t.iter()
                .filter_map(|e| e["template"]["templateId"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let visual = json!({"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"properties":{"template_id":{"type":"string","enum":templates},"parameters":parameters(),"purpose":{"type":"string"}},"required":["template_id","parameters","purpose"]}]});
    let schema = json!({"type":"object","additionalProperties":false,"properties":{
        "title":{"type":"string"},"summary":{"type":"string"},"level":{"type":"string"},"background":strings(),
        "clarification":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"properties":{"question":{"type":"string"},"options":strings(),"blocking":{"type":"boolean"}},"required":["question","options","blocking"]}]},
        "capabilities":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{
            "key":{"type":"string"},"outcome":{"type":"string"},"why":{"type":"string"},"prerequisites":strings(),
            "assessment":{"type":"string","enum":ASSESSMENTS},"minutes":{"type":"integer"},"visual":visual,"keep":{"type":["string","null"]}},
            "required":["key","outcome","why","prerequisites","assessment","minutes","visual","keep"]}},
        "source_use":{"type":"string"},"notes":{"type":"string"}},
        "required":["title","summary","level","background","clarification","capabilities","source_use","notes"]});
    let prior = i.prior.as_ref().map(|p| json!({"title":p.title,"level":p.level,"background":p.background,"capabilities":p.capabilities.iter().map(|c| json!({"objective_id":c.objective_id,"outcome":c.outcome,"why":c.why,"assessment":c.assessment,"minutes":c.minutes,"status":c.status})).collect::<Vec<_>>()}));
    envelope(
        PLAN_PROMPT,
        "study_plan_v1",
        schema,
        json!({"learner_goal":i.goal_text,"instruction":i.instruction,"prior_plan":prior,"progress":i.progress,"untrusted_source_passages":passages_json(&i.passages),"visual_catalog":crate::visualizer::planner_catalog()}),
        &i.choice,
        Purpose::Plan,
    )
}

pub fn task_payload(i: &TaskInput) -> Value {
    let visual = if i.visual.is_some() {
        json!({"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"properties":{"parameters":parameters(),"relation":{"type":"string"},"matches_task":{"type":"boolean"}},"required":["parameters","relation","matches_task"]}]})
    } else {
        json!({"type":"null"})
    };
    let schema = json!({"type":"object","additionalProperties":false,"properties":{
        "statement":{"type":"string"},"assumptions":strings(),"response_kind":{"type":"string","enum":ASSESSMENTS},
        "rubric":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"dimension":{"type":"string","enum":DIMENSIONS},"criterion":{"type":"string"}},"required":["dimension","criterion"]}},
        "reference_solution":{"type":"string"},"acceptable_alternatives":strings(),"common_errors":strings(),
        "answer_key":{"type":"object","additionalProperties":false,"properties":{
            "kind":{"type":"string","enum":["numeric","expression","none"]},
            "givens":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"},"value":{"type":"number"},"unit":{"type":"string"}},"required":["name","value","unit"]}},
            "expression":{"type":"string"},"value":{"type":["number","null"]},"unit":{"type":"string"},"tolerance":{"type":"number"},
            "variables":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"},"min":{"type":"number"},"max":{"type":"number"}},"required":["name","min","max"]}}},
            "required":["kind","givens","expression","value","unit","tolerance","variables"]},
        "support":{"type":"object","additionalProperties":false,"properties":{"explanation":{"type":"string"},"hints":strings(),"worked_example":{"type":"string"},"prerequisite":{"type":"string"}},"required":["explanation","hints","worked_example","prerequisite"]},
        "transfer":{"type":"string"},"visual":visual,
        "self_check":{"type":"object","additionalProperties":false,"properties":{"solvable":{"type":"boolean"},"ambiguities":strings(),"notes":{"type":"string"}},"required":["solvable","ambiguities","notes"]}},
        "required":["statement","assumptions","response_kind","rubric","reference_solution","acceptable_alternatives","common_errors","answer_key","support","transfer","visual","self_check"]});
    envelope(
        TASK_PROMPT,
        "study_task_v1",
        schema,
        json!({"stage":i.stage,"stage_purpose":stage_purpose(&i.stage),"learner_goal":i.goal_text,"plan":i.plan,"capability":i.capability,"learner_context":i.learner,"prior_tasks":i.prior_tasks,"problem_report":i.note,"untrusted_source_passages":passages_json(&i.passages),"visual":i.visual}),
        &i.choice,
        Purpose::Task,
    )
}

/// Conservative reservation at the frozen model's rates: every input byte priced as a token plus the full output
/// allowance for the effort actually sent.
pub fn reservation(payload: &Value, choice: &ModelChoice, purpose: Purpose) -> Result<i32> {
    let bytes = payload.to_string().len();
    if bytes > MAX_PAYLOAD {
        return Err(
            "Context limit reached: shorten the goal or attach fewer passages, then try again."
                .into(),
        );
    }
    Ok(
        ((bytes + 2048) as f64 * choice.model.input_microusd_per_token
            + choice.max_output(purpose) as f64 * choice.model.output_microusd_per_token)
            .ceil() as i32,
    )
}

pub fn parse_plan(
    v: &Value,
    request: &Request,
    model: &TutorModel,
) -> (Result<PlanProposal>, Usage) {
    let usage = providers::usage_for(v, request, model);
    let proposal = providers::output_text(v).and_then(|text| {
        serde_json::from_str::<PlanProposal>(&text)
            .map_err(|_| "Plan response did not match the versioned contract.".into())
    });
    (proposal, usage)
}

pub fn parse_task(
    v: &Value,
    request: &Request,
    model: &TutorModel,
) -> (Result<TaskProposal>, Usage) {
    let usage = providers::usage_for(v, request, model);
    let proposal = providers::output_text(v).and_then(|text| {
        serde_json::from_str::<TaskProposal>(&text)
            .map_err(|_| "Task response did not match the versioned contract.".into())
    });
    (proposal, usage)
}

fn bounded(text: &str, min: usize, max: usize, what: &str) -> Result<String> {
    let t = text.trim();
    let n = t.chars().count();
    if n < min || n > max {
        return Err(format!(
            "Plan response failed validation: {what} must be {min}–{max} characters."
        ));
    }
    Ok(t.to_owned())
}

/// Validated capabilities of a plan proposal: bounded text, known assessment kinds, earlier-only prerequisites and
/// catalog-checked visuals. Returns the capabilities and plain notes about anything that was dropped.
pub fn validate_plan(p: &PlanProposal) -> Result<(Vec<CapabilityProposal>, Vec<String>)> {
    bounded(&p.title, 3, 120, "the title")?;
    bounded(&p.summary, 0, 800, "the summary")?;
    bounded(&p.level, 0, 200, "the level")?;
    bounded(&p.notes, 0, 1200, "the notes")?;
    bounded(&p.source_use, 0, 600, "the source note")?;
    if p.background.len() > 6 || p.background.iter().any(|b| b.chars().count() > 240) {
        return Err(
            "Plan response failed validation: background is limited to six short items.".into(),
        );
    }
    if let Some(c) = &p.clarification
        && (c.question.trim().is_empty()
            || c.question.chars().count() > 300
            || c.options.len() > 4
            || c.options
                .iter()
                .any(|o| o.trim().is_empty() || o.chars().count() > 160))
    {
        return Err(
            "Plan response failed validation: one short question with at most four options.".into(),
        );
    }
    if p.capabilities.is_empty() && p.clarification.as_ref().is_none_or(|c| !c.blocking) {
        return Err("Plan response contained no capabilities and no blocking question.".into());
    }
    if p.capabilities.len() > 5 {
        return Err(
            "Plan response failed validation: at most five capabilities in one plan.".into(),
        );
    }
    let mut notes = vec![];
    let mut out: Vec<CapabilityProposal> = vec![];
    for c in &p.capabilities {
        let key = c.key.trim().to_lowercase();
        if key.is_empty()
            || key.chars().count() > 40
            || !key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            || out.iter().any(|o| o.key == key)
        {
            return Err(
                "Plan response failed validation: capability keys must be distinct short slugs."
                    .into(),
            );
        }
        let mut cap = c.clone();
        cap.key = key;
        cap.outcome = bounded(&c.outcome, 8, 300, "each outcome")?;
        cap.why = bounded(&c.why, 0, 400, "each reason")?;
        if !ASSESSMENTS.contains(&c.assessment.as_str()) {
            return Err("Plan response failed validation: unknown assessment kind.".into());
        }
        cap.minutes = c.minutes.clamp(3, 60);
        let earlier: Vec<String> = out.iter().map(|o| o.key.clone()).collect();
        let (kept, dropped): (Vec<String>, Vec<String>) = c
            .prerequisites
            .iter()
            .map(|k| k.trim().to_lowercase())
            .partition(|k| earlier.contains(k));
        if !dropped.is_empty() {
            notes.push(format!(
                "Ignored prerequisite links that did not point to an earlier capability: {}.",
                dropped
                    .iter()
                    .map(|d| clip(d, 40))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        cap.prerequisites = kept;
        if let Some(v) = &c.visual {
            match crate::visualizer::validate_visual(&v.template_id, &v.parameters) {
                Ok(parameters)
                    if !v.purpose.trim().is_empty() && v.purpose.chars().count() <= 300 =>
                {
                    cap.visual = Some(PlanVisual {
                        template_id: v.template_id.clone(),
                        parameters,
                        purpose: v.purpose.trim().into(),
                    })
                }
                Ok(_) => {
                    notes.push(format!(
                        "Dropped a visual without a clear purpose for “{}”.",
                        clip(&cap.outcome, 60)
                    ));
                    cap.visual = None;
                }
                Err(reason) => {
                    notes.push(format!(
                        "Dropped a visual suggestion for “{}”: {reason}",
                        clip(&cap.outcome, 60)
                    ));
                    cap.visual = None;
                }
            }
        }
        out.push(cap);
    }
    Ok((out, notes))
}

fn normalized(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Everything known about the capability that a task for it needs.
pub struct TaskContext<'a> {
    pub objective_id: &'a str,
    pub stage: &'a str,
    pub prior_tasks: &'a [String],
    pub request: &'a Request,
    /// The model the request was frozen with, named in the task's provenance.
    pub model: &'a str,
    pub content_version: String,
    /// The capability's planned visual (template and default parameters), when it has one.
    pub visual: Option<PlanVisual>,
}

/// Validates a task proposal and runs the deterministic checks. Any failed check means the task is not shown.
pub fn check_task(p: TaskProposal, ctx: &TaskContext) -> Result<Material> {
    let reject = |why: String| {
        Err(format!(
            "The generated task was not shown because it failed a check: {why} Prepare another task to continue."
        ))
    };
    let statement = p.statement.trim().to_owned();
    if statement.chars().count() < 15 || statement.chars().count() > 6000 {
        return reject("the statement is empty or too long.".into());
    }
    if !p.self_check.solvable || !p.self_check.ambiguities.is_empty() {
        return reject(format!(
            "the task writer reported it as {}{}.",
            if p.self_check.solvable {
                "ambiguous"
            } else {
                "not solvable as written"
            },
            p.self_check
                .ambiguities
                .first()
                .map(|a| format!(" ({})", clip(a, 160)))
                .unwrap_or_default()
        ));
    }
    if !ASSESSMENTS.contains(&p.response_kind.as_str()) {
        return reject("unknown response kind.".into());
    }
    if p.rubric.is_empty() || p.rubric.len() > 4 {
        return reject("the rubric needs one to four lines.".into());
    }
    let mut dims: Vec<String> = vec![];
    let mut criteria = vec![];
    for line in &p.rubric {
        if !DIMENSIONS.contains(&line.dimension.as_str()) || dims.contains(&line.dimension) {
            return reject("each rubric dimension must be known and listed once.".into());
        }
        let text = line.criterion.trim();
        if text.chars().count() < 8 || text.chars().count() > 600 {
            return reject("a rubric criterion is empty or too long.".into());
        }
        dims.push(line.dimension.clone());
        criteria.push(format!("{}: {text}", dimension_label(&line.dimension)));
    }
    if p.reference_solution.trim().chars().count() < 10
        || p.reference_solution.chars().count() > 9000
    {
        return reject("the reference solution is missing or too long.".into());
    }
    if p.assumptions.len() > 10 || p.assumptions.iter().any(|a| a.chars().count() > 400) {
        return reject("too many or overlong assumptions.".into());
    }
    let mut support = p.support.clone();
    for (text, what, limit) in [
        (&support.explanation, "explanation", 7000),
        (&support.worked_example, "worked example", 7000),
        (&support.prerequisite, "prerequisite review", 4000),
    ] {
        if text.chars().count() > limit {
            return reject(format!("the {what} is too long."));
        }
    }
    if ctx.stage == "instruction" && support.explanation.trim().chars().count() < 40 {
        return reject("the teaching explanation is missing.".into());
    }
    if ctx.stage == "worked_example" && support.worked_example.trim().chars().count() < 40 {
        return reject("the worked example is missing.".into());
    }
    let mut checks = vec![];
    let repeated = ctx
        .prior_tasks
        .iter()
        .any(|t| normalized(t) == normalized(&statement));
    if repeated {
        return reject("it repeats an earlier task for this capability.".into());
    }
    checks.push(result(
        "fresh",
        "Not a repeat of an earlier task",
        "passed",
        format!(
            "Compared with {} earlier task(s) for this capability.",
            ctx.prior_tasks.len()
        ),
    ));
    let task_text = format!("{statement}\n{}", p.assumptions.join("\n"));
    let mut key = p.answer_key.clone();
    if key.kind == "none" {
        key.givens.clear();
        key.variables.clear();
        key.expression.clear();
        key.value = None;
        key.unit.clear();
    }
    key.tolerance = if key.tolerance.is_finite() && key.tolerance > 0.0 {
        key.tolerance.min(0.1)
    } else {
        0.01
    };
    let key_checks = checks::check_key(&key, &task_text);
    if let Some(failed) = key_checks.iter().find(|c| c.outcome == "failed") {
        return reject(format!(
            "{} — {}",
            failed.label.to_lowercase(),
            failed.detail
        ));
    }
    checks.extend(key_checks);
    if key.kind == "numeric"
        && let Some(v) = key.value
        && !key.givens.iter().any(|g| checks::close(g.value, v, 1e-9))
        && checks::numbers(&statement)
            .iter()
            .any(|n| checks::close(*n, v, 1e-6))
        && v != 0.0
    {
        return reject("the statement already contains the final answer.".into());
    }
    let before = support.hints.len();
    support.hints = support
        .hints
        .iter()
        .map(|h| h.trim().to_owned())
        .filter(|h| !h.is_empty() && h.chars().count() <= 900 && !checks::hint_leaks(&key, h))
        .take(3)
        .collect();
    let removed = before.saturating_sub(support.hints.len()).min(before);
    checks.push(result(
        "hints",
        "Hints do not state the final answer",
        "passed",
        if key.kind != "numeric" {
            format!("{} hint(s); the final-answer check applies to numerical keys only.", support.hints.len())
        } else if removed > 0 {
            format!("{removed} hint(s) removed before freezing because they stated the answer or were empty; {} kept.", support.hints.len())
        } else {
            format!("{} hint(s) checked against the frozen answer.", support.hints.len())
        },
    ));
    // Visual parameters fixed with the task, validated against the model's own ranges and constraints.
    let visual = if ["diagnosis", "exploration", "explanation"].contains(&ctx.stage) {
        ctx.visual.as_ref().map(|planned| {
            let proposed = p.visual.as_ref().and_then(|v| {
                crate::visualizer::validate_visual(&planned.template_id, &v.parameters)
                    .ok()
                    .map(|parameters| (parameters, v))
            });
            match proposed {
                Some((parameters, v)) if !v.relation.trim().is_empty() => {
                    checks.push(result("visual", "Visual parameters are within the model's ranges", "passed", "The task's visual values satisfy the installed model's bounds and constraints."));
                    VisualPlan { template_id: planned.template_id.clone(), parameters, relation: clip(&v.relation, 500), purpose: planned.purpose.clone(), matches_task: v.matches_task }
                }
                _ => {
                    checks.push(result("visual", "Visual parameters are within the model's ranges", "inconclusive", "The task's own visual values were missing or out of range, so the visual uses the plan's example values instead."));
                    VisualPlan { template_id: planned.template_id.clone(), parameters: planned.parameters.clone(), relation: "A related example chosen with the plan, not your task's exact values. Use it to test the idea, then return to the task.".into(), purpose: planned.purpose.clone(), matches_task: false }
                }
            }
        })
    } else {
        None
    };
    let mut solution = p.reference_solution.trim().to_owned();
    if !p.acceptable_alternatives.is_empty() {
        solution.push_str(&format!(
            "\n\nAlso accepted: {}",
            p.acceptable_alternatives
                .iter()
                .take(4)
                .map(|a| clip(a, 400))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    if !p.common_errors.is_empty() {
        solution.push_str(&format!(
            "\n\nCommon errors: {}",
            p.common_errors
                .iter()
                .take(4)
                .map(|a| clip(a, 300))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    let digest = crate::sources::sha(criteria.join("\n").as_bytes());
    let passed = checks.iter().filter(|c| c.outcome == "passed").count();
    let open = checks
        .iter()
        .filter(|c| c.outcome == "inconclusive")
        .count();
    Ok(Material {
        objective_id: ctx.objective_id.into(),
        prompt: statement,
        solution,
        math: String::new(),
        assumptions: p
            .assumptions
            .iter()
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty())
            .collect(),
        criteria,
        rubric: dims,
        rubric_version: format!("task-rubric-v1:{}", &digest[..12]),
        transfer_scope: if p.transfer.trim().is_empty() {
            "generated task".into()
        } else {
            clip(&p.transfer, 200)
        },
        visual,
        origin: "generated".into(),
        response_kind: p.response_kind.clone(),
        support: Some(support),
        answer_key: (key.kind != "none").then_some(key),
        checks,
        provenance: format!(
            "Generated by {} with prompt {TASK_PROMPT_VERSION} (request {}). Not reviewed by a person.",
            if ctx.request.kind.starts_with("offline:") {
                "the offline development fixture"
            } else {
                ctx.model
            },
            ctx.request.id
        ),
        content_version: ctx.content_version.clone(),
        validation_version: "generated-checks-v1".into(),
        validation_status: format!(
            "Generated task: {passed} automated check(s) passed{}; not reviewed by a person.",
            if open > 0 {
                format!(", {open} inconclusive")
            } else {
                String::new()
            }
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            schema_version: 1,
            id: "r".into(),
            session_id: "s".into(),
            attempt_id: None,
            session_revision: 1,
            kind: "task".into(),
            status: "pending".into(),
            reserved_microusd: 1,
            created_at: "2026-10-29T00:00:00Z".into(),
        }
    }
    fn proposal() -> TaskProposal {
        TaskProposal {
            statement: "A cyclist rides 12 km in 0.5 h. What is the average speed in km/h?".into(),
            assumptions: vec!["Constant direction.".into()],
            response_kind: "numeric".into(),
            rubric: vec![
                RubricLine {
                    dimension: "setup".into(),
                    criterion: "Uses average speed = distance / time.".into(),
                },
                RubricLine {
                    dimension: "execution".into(),
                    criterion: "Computes the value with units.".into(),
                },
            ],
            reference_solution: "v = d/t = 12 km / 0.5 h = 24 km/h.".into(),
            acceptable_alternatives: vec![],
            common_errors: vec!["slip: multiplying instead of dividing".into()],
            answer_key: AnswerKey {
                kind: "numeric".into(),
                givens: vec![
                    Given {
                        name: "d".into(),
                        value: 12.0,
                        unit: "km".into(),
                    },
                    Given {
                        name: "t".into(),
                        value: 0.5,
                        unit: "h".into(),
                    },
                ],
                expression: "d/t".into(),
                value: Some(24.0),
                unit: "km/h".into(),
                tolerance: 0.01,
                variables: vec![],
            },
            support: TaskSupport {
                explanation: "Average speed compares the whole distance with the whole time."
                    .into(),
                hints: vec![
                    "Which two quantities does speed compare?".into(),
                    "It is 24 km/h.".into(),
                ],
                worked_example: "1. A walker covers 3 km in 1.5 h.\n2. 3/1.5 = 2 km/h.".into(),
                prerequisite: String::new(),
            },
            transfer: "new numbers".into(),
            visual: None,
            self_check: SelfCheck {
                solvable: true,
                ambiguities: vec![],
                notes: String::new(),
            },
        }
    }
    #[test]
    fn generation_payloads_follow_the_frozen_choice_with_a_medium_floor() {
        let plan = |choice| PlanInput {
            request: request(),
            choice,
            goal_text: "Understand eigenvalues".into(),
            instruction: String::new(),
            prior: None,
            progress: vec![],
            passages: vec![],
        };
        let quick = plan_payload(&plan(ModelChoice::resolve("gpt-6.1-terra", "low")));
        assert_eq!(quick["model"], "gpt-6.1-terra");
        assert_eq!(
            quick["reasoning"]["effort"], "medium",
            "plans never think below medium"
        );
        assert_eq!(quick["max_output_tokens"], 6000);
        let deep = plan(ModelChoice::resolve("gpt-6-astra", "high"));
        let payload = plan_payload(&deep);
        assert_eq!(payload["reasoning"]["effort"], "high");
        assert_eq!(payload["max_output_tokens"], 12000);
        let sol = plan(ModelChoice::default());
        assert!(
            reservation(&plan_payload(&deep), &deep.choice, Purpose::Plan).unwrap()
                > 5 * reservation(&plan_payload(&sol), &sol.choice, Purpose::Plan).unwrap()
        );
        let v = json!({"status":"completed","output":[],"usage":{"input_tokens":1000,"output_tokens":100}});
        assert_eq!(
            parse_plan(&v, &request(), deep.choice.model)
                .1
                .cost_microusd,
            15000
        );
    }
    #[test]
    fn checked_tasks_drop_leaking_hints_and_reject_flaws() {
        let r = request();
        let ctx = TaskContext {
            objective_id: "cap",
            stage: "independent",
            prior_tasks: &[],
            request: &r,
            model: "gpt-6.1-sol",
            content_version: "generated:g:plan-r1".into(),
            visual: None,
        };
        let m = check_task(proposal(), &ctx).unwrap();
        assert_eq!(
            m.support.as_ref().unwrap().hints.len(),
            1,
            "the hint stating 24 km/h is removed"
        );
        assert_eq!(m.rubric, vec!["setup", "execution"]);
        assert!(
            m.checks
                .iter()
                .any(|c| c.id == "arithmetic" && c.outcome == "passed")
        );
        let mut wrong = proposal();
        wrong.answer_key.value = Some(30.0);
        assert!(
            check_task(wrong, &ctx).is_err(),
            "a key that disagrees with its own formula is never shown"
        );
        let mut leak = proposal();
        leak.statement = "A cyclist averages 24 km/h over 12 km in 0.5 h. Check it.".into();
        assert!(check_task(leak, &ctx).is_err());
        let mut unsolvable = proposal();
        unsolvable.self_check.solvable = false;
        assert!(check_task(unsolvable, &ctx).is_err());
        let prior = vec![proposal().statement];
        let repeat = TaskContext {
            prior_tasks: &prior,
            ..ctx
        };
        assert!(check_task(proposal(), &repeat).is_err());
    }
}
