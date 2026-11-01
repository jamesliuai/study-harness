//! Versioned authored unit, authoritative transitions, evidence and deterministic scheduling.
use crate::{
    checks,
    contracts::*,
    curriculum::Material,
    db::{Result, Store, id, json, parse, sql},
    providers::TutorInput,
    scene,
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};

pub const POLICY: &str = "plan-policy-v4";
pub const SCHEDULER: &str = "successive-1-3-7-14-v1";
pub const DIMENSIONS: [&str; 3] = ["setup", "interpretation", "justification"];

fn many<T: serde::de::DeserializeOwned>(c: &Connection, query: &str, sid: &str) -> Result<Vec<T>> {
    let mut s = c.prepare(query).map_err(sql)?;
    s.query_map([sid], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .map(|r| parse(r.map_err(sql)?))
        .collect()
}
fn state(c: &Connection, sid: &str) -> Result<Option<LearningState>> {
    let raw: Option<String> = c
        .query_row(
            "SELECT data FROM learning_sessions WHERE session_id=?",
            [sid],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    raw.map(parse).transpose()
}
fn write_state(c: &Connection, s: &LearningState) -> Result<()> {
    c.execute("INSERT INTO learning_sessions VALUES (?,?) ON CONFLICT(session_id) DO UPDATE SET data=excluded.data", params![s.session_id, json(s)?]).map_err(sql)?;
    Ok(())
}
fn bump(c: &Connection, sid: &str) -> Result<()> {
    let mut s: Session = parse(
        c.query_row("SELECT data FROM sessions WHERE id=?", [sid], |r| r.get(0))
            .map_err(sql)?,
    )?;
    s.revision += 1;
    c.execute(
        "UPDATE sessions SET revision=?,data=? WHERE id=?",
        params![s.revision, json(&s)?, sid],
    )
    .map_err(sql)?;
    Ok(())
}
fn event(c: &Connection, sid: &str, kind: &str, time: &str, data: serde_json::Value) -> Result<()> {
    c.execute(
        "INSERT INTO learning_events VALUES (?,?,?,?,?)",
        params![id(), sid, kind, time, data.to_string()],
    )
    .map_err(sql)?;
    Ok(())
}
fn add_help(
    c: &Connection,
    sid: &str,
    aid: &str,
    attempt: Option<&str>,
    depth: &str,
    content: &str,
    time: &str,
) -> Result<()> {
    let h = Assistance {
        schema_version: 1,
        id: id(),
        session_id: sid.into(),
        activity_id: aid.into(),
        attempt_id: attempt.map(str::to_owned),
        content: content.into(),
        depth: depth.into(),
        visible_at: time.into(),
    };
    c.execute(
        "INSERT INTO assistance VALUES (?,?,?,?,?)",
        params![h.id, sid, aid, attempt, json(&h)?],
    )
    .map_err(sql)?;
    Ok(())
}

/// The original convergence objective keeps its first hand-written task sequence (now one authored content source).
fn legacy_material(c: &Connection, sid: &str, kind: &str, objective_id: &str) -> Result<Material> {
    let (prompt, solution, transfer): (String, String, &str) = match kind {
        "diagnosis" => ("For f(x)=x², x₀=4 and step size 0.75, predict the first two steps and whether the sequence converges. Explain why crossing the minimum might or might not be a problem.".into(), "a=2; q=1−0.75×2=−0.5. x₁=−2, x₂=1. Magnitude shrinks by half, signs alternate, so x tends to 0.".into(), "initial prediction"),
        "completion" => ("Complete the worked step: a=4, η=0.375, x₀=6. The multiplier is q=1−ηa. Find q and x₁, then explain the long-run behavior.".into(), "q=−0.5; x₁=−3. Alternating convergence because |q|=0.5<1.".into(), "completion with a method cue"),
        "guided" => ("For f(x)=1.5x² and x₀=2, compare η=0.5 with η=0.8. Use the multiplier to explain which converges and why.".into(), "a=3. q=−0.5 at η=0.5: alternating convergence. q=−1.4 at η=0.8: alternating divergence.".into(), "guided contrast"),
        "exploration" => ("Before revealing the trace, predict what changes between step sizes 0.25, 0.75 and 1.25 with curvature 2 and start 4. Then test your prediction with the controls.".into(), "Multipliers 0.5, −0.5, −1.5 respectively: same-side convergence, alternating convergence, alternating divergence.".into(), "prediction and exploration"),
        "explanation" => ("Explain what your exploration showed. Why can a step cross the minimum and still converge? What changes when curvature increases at a fixed step size?".into(), "The sign of 1−ηa determines crossing; its magnitude determines growth or shrinkage. Increasing a can move |1−ηa| past 1. For nonzero start and a>0, 0<ηa<2 is necessary and sufficient for convergence.".into(), "explanation after visual support"),
        "independent" => {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM task_specs WHERE json_extract(data,'$.kind')='independent'", [], |r| r.get(0)).map_err(sql)?;
            let a = 5 + count * 2;
            (format!("A new objective is f(x)=({a}/2)x² with a nonzero starting point. Determine all positive constant step sizes that converge to zero. Explain the upper boundary and give one convergent choice that crosses zero."), format!("Derivative {a}x. The update multiplier is 1−{a}η. Convergence iff |1−{a}η|<1, so 0<η<2/{a}. At η=2/{a}, q=−1 gives constant magnitude oscillation. Any 1/{a}<η<2/{a} alternates and converges; η=1.5/{a} gives q=−0.5."), "near transfer; new curvature, same objective")
        },
        "review" => {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM task_specs WHERE json_extract(data,'$.kind')='review'", [], |r| r.get(0)).map_err(sql)?;
            let denominator = 5 + count;
            let numerator = denominator - 1;
            (format!("A process follows uₙ₊₁=−({numerator}/{denominator})uₙ with u₀≠0. Without an instructional visual, decide whether it tends to zero, explain the signs, and determine what happens if the factor is −1 instead."), format!("uₙ=(−{numerator}/{denominator})ⁿu₀. Its magnitude tends to zero because {numerator}/{denominator}<1, while signs alternate. Factor −1 keeps constant magnitude and alternates, so a nonzero start does not tend to zero."), "limited representational transfer; recurrence without a gradient-descent cue")
        },
        "worked_example" => ("Study the related worked example. Then redo its first consequential step yourself — the step the rest of the answer depends on — or ask about any step.".into(), crate::curriculum::worked_example(c, sid)?, "worked example; supported"),
        _ => ("Read the explanation, then restate in your own words why the sign of q = 1 − ηa decides crossing while its size decides convergence. Ask about any step that does not follow.".into(), "For a>0, q=1−ηa; from nonzero start convergence iff |q|<1. q=0 reaches zero in one step. x₀=0 remains zero in every regime.".into(), "instruction; no assessment of independent performance"),
    };
    Ok(Material {
        objective_id: objective_id.into(),
        prompt,
        solution,
        math: if kind == "independent" || kind == "review" { String::new() } else { "f(x)=\\frac{a}{2}x^2,\\quad x_{t+1}=(1-\\eta a)x_t".into() },
        assumptions: if kind == "review" {
            vec!["Real scalar recurrence, constant multiplier, exact arithmetic.".into(), "Nonzero initial state; zero would remain fixed.".into()]
        } else {
            vec!["One dimensional quadratic; a>0; fixed step; exact arithmetic.".into(), "Nonzero initial state unless explicitly stated; x₀=0 is always fixed.".into()]
        },
        criteria: vec!["Setup: derive the correct multiplier or an equivalent closed form from the stated task.".into(),"Interpretation: distinguish sign alternation from magnitude convergence and explain any stated boundary.".into(),"Justification: connect magnitude below one to the limiting behavior under the task assumptions; accept valid alternative methods.".into()],
        rubric: DIMENSIONS.iter().map(|s| s.to_string()).collect(),
        rubric_version: "quadratic-rubric-v2".into(),
        transfer_scope: transfer.into(),
        visual: crate::curriculum::visual_plan(objective_id, kind, 0),
        origin: "authored".into(),
        response_kind: String::new(),
        support: None,
        answer_key: None,
        checks: vec![],
        provenance: crate::curriculum::AUTHORED_PROVENANCE.into(),
        content_version: "quadratic-content-v2".into(),
        validation_version: "analytic-cases-v1".into(),
        validation_status: "Analytically checked authored objective; live grading and learning effectiveness unverified".into(),
    })
}

/// Freezes one task: prompt, reference, assumptions, rubric, support, answer key, checks and versions commit together
/// before any view is returned. Generated material arrives already validated and checked; an authored objective
/// derives its material deterministically. Both take this single path.
pub(crate) fn freeze_task(
    c: &Connection,
    sid: &str,
    kind: &str,
    time: &str,
    generated: Option<Material>,
) -> Result<String> {
    let objective_id = crate::curriculum::selected(c, sid)?;
    let m = match generated {
        Some(m) => m,
        None if crate::goals::is_generated(c, &objective_id)? => {
            return Err(
                "This step needs a newly written task; continue from the lesson controls.".into(),
            );
        }
        None => match crate::curriculum::material(c, sid, kind)? {
            Some(m) => m,
            None => legacy_material(c, sid, kind, &objective_id)?,
        },
    };
    let aid = id();
    let a = Activity {
        schema_version: 1,
        id: aid.clone(),
        session_id: sid.into(),
        objective_id: objective_id.clone(),
        version: 1,
        prompt: m.prompt,
        math: m.math,
        task_family: if objective_id == "quadratic-convergence" {
            format!("quadratic-{kind}-v1")
        } else {
            format!("{objective_id}-{kind}-v1")
        },
        rubric_version: m.rubric_version,
        validation_status: m.validation_status,
    };
    let spec = TaskSpec {
        schema_version: 2,
        activity_id: aid.clone(),
        kind: kind.into(),
        assumptions: m.assumptions,
        reference_solution: m.solution.clone(),
        rubric: m.rubric,
        rubric_criteria: m.criteria,
        content_version: m.content_version,
        validation_version: m.validation_version,
        frozen_at: time.into(),
        transfer_scope: m.transfer_scope,
        origin: m.origin,
        response_kind: m.response_kind,
        support: m.support,
        answer_key: m.answer_key,
        checks: m.checks,
        provenance: m.provenance,
    };
    c.execute(
        "INSERT INTO activities VALUES (?,?,?,?,?,?,?)",
        params![
            aid,
            sid,
            a.objective_id,
            1,
            json(&a)?,
            m.solution,
            json(&spec.rubric)?
        ],
    )
    .map_err(sql)?;
    c.execute(
        "INSERT INTO task_specs VALUES (?,?)",
        params![aid, json(&spec)?],
    )
    .map_err(sql)?;
    c.execute(
        "INSERT INTO scenes VALUES (?,?)",
        params![aid, json(&scene::sample(&aid))?],
    )
    .map_err(sql)?;
    if let Some(plan) = m.visual {
        // Visual parameters are fixed with the task, like its reference and rubric.
        event(
            c,
            sid,
            "visual_plan",
            time,
            serde_json::json!({"activity_id":aid,"plan":plan}),
        )?;
    }
    if ["completion", "guided"].contains(&kind) {
        add_help(
            c,
            sid,
            &aid,
            None,
            "method_cue",
            "This task supplies a method cue; completion is supported.",
            time,
        )?;
    }
    Ok(aid)
}

pub fn invalidate(c: &Connection, sid: &str) -> Result<()> {
    c.execute("UPDATE learning_summaries SET data=json_set(data,'$.stale',json('true')) WHERE session_id=?",[sid]).map_err(sql)?;
    c.execute("UPDATE reviews SET data=json_set(data,'$.status','invalidated') WHERE json_extract(data,'$.session_id')=? AND json_extract(data,'$.status')='scheduled'",[sid]).map_err(sql)?;
    Ok(())
}

pub fn persist_assessment(
    c: &Connection,
    r: &Request,
    p: &TutorProposal,
    time: &str,
) -> Result<()> {
    let Some(g) = &p.grade else {
        return Ok(());
    };
    // Regrading does not silently settle a learner's dispute.
    let disputed: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM disputes WHERE attempt_id=? AND resolved_by IS NULL)",
            [&p.evidence_attempt_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    c.execute(
        "UPDATE assessments SET data=json_set(data,'$.stale',json('true')) WHERE attempt_id=?",
        [&p.evidence_attempt_id],
    )
    .map_err(sql)?;
    // The tutor proposes a verdict; a deterministic check of the quoted final answer against the frozen key is
    // recorded beside it. A disagreement holds the verdict instead of strengthening any claim.
    let spec: Option<TaskSpec> = c
        .query_row(
            "SELECT data FROM task_specs WHERE activity_id=?",
            [&p.activity_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(sql)?
        .map(parse)
        .transpose()?;
    let mut verdict = g.verdict.clone();
    let mut uncertainty = g.uncertainty.clone();
    let mut note = String::new();
    let mut found = vec![];
    if let Some(key) = spec.and_then(|t| t.answer_key) {
        let check = match &g.final_answer {
            Some(f) => checks::check_answer(&key, f.value, &f.unit, &f.expression),
            None => checks::result(
                "final-answer",
                "Final answer checked against the frozen key",
                "inconclusive",
                "No final answer was identified in the response, so nothing was compared automatically.",
            ),
        };
        match (check.outcome.as_str(), verdict.as_str()) {
            ("failed", "correct") => {
                verdict = "unresolved".into();
                uncertainty = "unresolved".into();
                note = format!(
                    "Held for checking: the tutor judged this correct, but the automated answer check disagrees — {} Either the answer or the generated task may be wrong. Correct your final value, challenge this assessment, or report a problem with the task.",
                    check.detail
                );
            }
            ("passed", "incorrect") => {
                note = "The final value matches the frozen answer key; the feedback concerns the reasoning.".into();
            }
            _ => {}
        }
        found.push(check);
    }
    if g.error_kind == "task_flaw" {
        verdict = "unresolved".into();
        uncertainty = "unresolved".into();
        note = "The tutor thinks the task itself may be flawed, so this answer is not counted. Report the problem to get a replacement task.".into();
    }
    let a = Assessment {
        schema_version: 3,
        id: id(),
        attempt_id: p.evidence_attempt_id.clone(),
        rubric_version: p.rubric_version.clone(),
        findings: g.findings.clone(),
        uncertainty,
        dispute_state: if disputed { "disputed" } else { "none" }.into(),
        grader_version: if r.kind.starts_with("offline") {
            "simulated-scenarios-v1".to_owned()
        } else {
            format!("openai-{}", crate::providers::PROMPT_VERSION)
        },
        stale: false,
        verdict,
        assessed_at: time.into(),
        request_id: r.id.clone(),
        error_kind: g.error_kind.clone(),
        checks: found,
        note,
    };
    c.execute(
        "INSERT INTO assessments VALUES (?,?,?)",
        params![a.id, a.attempt_id, json(&a)?],
    )
    .map_err(sql)?;
    invalidate(c, &r.session_id)
}

/// Rebuild from primary records, never from a previous summary. No model can write these counts.
pub fn recompute(c: &Connection, sid: &str, time: &str, simulated: bool) -> Result<()> {
    let ats: Vec<Attempt> = many(
        c,
        "SELECT data FROM attempts WHERE session_id=? ORDER BY rowid",
        sid,
    )?;
    let grades: Vec<Assessment> = many(
        c,
        "SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=? ORDER BY x.rowid",
        sid,
    )?;
    let conditions: Vec<AttemptConditions> = many(
        c,
        "SELECT x.data FROM attempt_conditions x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=?",
        sid,
    )?;
    let simulated = simulated
        || conditions.iter().any(|c| c.simulated)
        || grades
            .iter()
            .any(|g| g.grader_version.contains("simulated"));
    let mut independent = 0u32;
    let mut supported = 0u32;
    let mut delayed = 0u32;
    let mut assessment_ids = vec![];
    let mut supporting = vec![];
    let mut successes = 0usize;
    let mut last_encounter: Option<(String, u32, String)> = None;
    let mut unresolved = false;
    for at in &ats {
        if ats
            .iter()
            .any(|a| a.revision_of.as_deref() == Some(at.id.as_str()))
        {
            continue;
        }
        let Some(cond) = conditions.iter().find(|x| x.attempt_id == at.id) else {
            continue;
        };
        // Questions are discussion: they are never graded and never count toward or against evidence.
        if cond.intent == "question" {
            continue;
        }
        let Some(grade) = grades
            .iter()
            .rev()
            .find(|g| g.attempt_id == at.id && !g.stale)
        else {
            if ["independent", "review"].contains(&cond.task_kind.as_str()) {
                unresolved = true;
            }
            continue;
        };
        if grade.uncertainty != "resolved"
            || grade.dispute_state == "disputed"
            || grade.verdict == "unresolved"
        {
            if ["independent", "review"].contains(&cond.task_kind.as_str()) {
                unresolved = true;
            }
            continue;
        }
        assessment_ids.push(grade.id.clone());
        let clean = at.assistance_ids.is_empty()
            && at.exposure_ids.is_empty()
            && at.revision_of.is_none()
            && !cond.outside_help;
        let independent_task = ["independent", "review"].contains(&cond.task_kind.as_str());
        if grade.verdict == "correct" {
            if clean && independent_task {
                independent += 1;
            } else {
                supported += 1;
            }
        }
        if !independent_task {
            continue;
        }
        // A new resolved check can restore a useful next step after an older unresolved case.
        unresolved = false;
        supporting.push(at.id.clone());
        let current =
            DateTime::parse_from_rfc3339(&cond.submitted_at).map_err(|e| e.to_string())?;
        if clean && grade.verdict == "correct" {
            let early = last_encounter.as_ref().is_some_and(|(base, days, _)| {
                DateTime::parse_from_rfc3339(base)
                    .is_ok_and(|b| current < b + Duration::days(*days as i64))
            });
            if early {
                let days = last_encounter.as_ref().expect("early encounter").1;
                last_encounter=Some((cond.submitted_at.clone(),days,"Early independent practice: keep the interval and measure from the latest successful encounter; not delayed evidence.".into()));
                continue;
            }
            let days = [1, 3, 7, 14][successes.min(3)];
            successes += 1;
            if !simulated
                && !cond.simulated
                && cond.last_relevant_exposure_at.as_ref().is_some_and(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .is_ok_and(|last| current - last >= Duration::days(1))
                })
                && cond.task_kind == "review"
            {
                delayed += 1;
            }
            last_encounter = Some((
                cond.submitted_at.clone(),
                days,
                "Successful independent encounter; interval measured from this submission.".into(),
            ));
        } else {
            successes = 0;
            last_encounter=Some((cond.submitted_at.clone(),1,"Supported or unsuccessful check: shorter independent revisit; no strengthened competence claim.".into()));
        }
    }
    let pending_count = ats
        .iter()
        .filter(|at| {
            !conditions
                .iter()
                .any(|c| c.attempt_id == at.id && c.intent == "question")
                && !ats
                    .iter()
                    .any(|later| later.revision_of.as_deref() == Some(at.id.as_str()))
                && !grades.iter().any(|g| {
                    g.attempt_id == at.id
                        && !g.stale
                        && g.uncertainty == "resolved"
                        && g.dispute_state != "disputed"
                })
        })
        .count();
    let latest_findings = grades
        .iter()
        .rev()
        .find(|g| assessment_ids.contains(&g.id))
        .map(|g| {
            g.findings
                .iter()
                .map(|f| format!("{}: {}", f.dimension, f.result))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_else(|| "no resolved assessment yet".into());
    let text = format!(
        "{}{} independent successful check(s); {} assessed successes with support or in teaching tasks. {} real delayed demonstration(s). {pending_count} submission(s) await a resolved assessment. Latest resolved rubric: {latest_findings}. {} This is evidence for the selected objective, not a mastery percentage. Outside help is self-reported.",
        if simulated {
            "SIMULATED EVIDENCE ONLY. "
        } else {
            ""
        },
        independent,
        supported,
        delayed,
        if unresolved {
            "A check is unresolved or disputed; review decisions need a fresh resolved check."
        } else {
            "Immediate performance is provisional; later retention remains to be checked."
        }
    );
    let summary = LearningSummary {
        schema_version: 1,
        session_id: sid.into(),
        text,
        assessment_ids: assessment_ids.clone(),
        stale: false,
        generated_at: time.into(),
        independent_count: independent,
        supported_count: supported,
        delayed_count: delayed,
        simulated,
    };
    c.execute("INSERT INTO learning_summaries VALUES (?,?) ON CONFLICT(session_id) DO UPDATE SET data=excluded.data",params![sid,json(&summary)?]).map_err(sql)?;
    c.execute("UPDATE reviews SET data=json_set(data,'$.status','superseded') WHERE json_extract(data,'$.session_id')=? AND json_extract(data,'$.status') IN ('scheduled','invalidated')",[sid]).map_err(sql)?;
    if let Some((base, days, reason)) = last_encounter {
        let due = DateTime::parse_from_rfc3339(&base).map_err(|e| e.to_string())?
            + Duration::days(days as i64);
        // Stable decision identity makes recomputation idempotent, preserving all superseded rows.
        let key = format!(
            "{sid}:{}:{}",
            assessment_ids.join(":"),
            if unresolved { "held" } else { "ready" }
        );
        let r = Review {
            schema_version: 2,
            id: key,
            session_id: sid.into(),
            objective_id: crate::curriculum::selected(c, sid)?,
            task_family: format!("{}-review-v1", crate::curriculum::selected(c, sid)?),
            due_at: due.to_rfc3339(),
            reason,
            scheduler_version: SCHEDULER.into(),
            supporting_attempts: supporting,
            assessment_ids,
            interval_days: days,
            status: if unresolved {
                "needs_recompute"
            } else {
                "scheduled"
            }
            .into(),
            simulated,
        };
        c.execute(
            "INSERT INTO reviews VALUES (?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![r.id, r.objective_id, json(&r)?],
        )
        .map_err(sql)?;
    }
    Ok(())
}

impl Store {
    pub fn learning_state(&self, sid: &str) -> Result<Option<LearningState>> {
        state(&self.conn, sid)
    }
    pub fn task_spec(&self, aid: &str) -> Result<Option<TaskSpec>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM task_specs WHERE activity_id=?",
                [aid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        raw.map(parse).transpose()
    }
    pub fn learning_view(&self, sid: &str) -> Result<Option<LearningView>> {
        let Some(s) = self.learning_state(sid)? else {
            return Ok(None);
        };
        let spec = self
            .task_spec(&s.active_activity_id)?
            .ok_or("Missing frozen task")?;
        let (steps, step_note) = self.steps(sid)?;
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM learning_summaries WHERE session_id=?",
                [sid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        Ok(Some(LearningView {
            state: s.clone(),
            assumptions: spec.assumptions.clone(),
            target_dimensions: spec.rubric.clone(),
            transfer_scope: spec.transfer_scope.clone(),
            assessments: many(
                &self.conn,
                "SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=? ORDER BY x.rowid",
                sid,
            )?,
            conditions: many(
                &self.conn,
                "SELECT x.data FROM attempt_conditions x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=?",
                sid,
            )?,
            summary: raw.map(parse).transpose()?,
            reviews: many(
                &self.conn,
                "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? ORDER BY rowid DESC",
                sid,
            )?,
            allowed_actions: self.allowed_actions(sid)?,
            now: self.time(),
            steps,
            step_note,
            visual: self.visual_plan_for(&s.active_activity_id)?,
            wrap_up: self.wrap_up(sid)?,
            objective_outcome: if s.stage == "exploratory" {
                "Exploratory topic outside a plan".into()
            } else {
                crate::goals::outcome(&self.conn, &crate::curriculum::selected(&self.conn, sid)?)
            },
            stage_title: stage_title(&s.stage).into(),
            task_origin: if spec.origin.is_empty() {
                "authored".into()
            } else {
                spec.origin.clone()
            },
            task_provenance: if spec.provenance.is_empty() {
                crate::curriculum::AUTHORED_PROVENANCE.into()
            } else {
                spec.provenance.clone()
            },
            task_checks: spec.checks.clone(),
            rubric_lines: spec.rubric_criteria.clone(),
            response_kind: spec.response_kind.clone(),
            goal_id: crate::goals::goal_of(&self.conn, sid)?,
            generated: crate::goals::is_generated(
                &self.conn,
                &crate::curriculum::selected(&self.conn, sid)?,
            )?,
        }))
    }
    pub fn allowed_actions(&self, sid: &str) -> Result<Vec<String>> {
        if let Some(s) = self.learning_state(sid)? {
            if s.stopped || s.quiet {
                return Ok(vec![]);
            }
            if s.stage == "exploratory" {
                return Ok(["clarify", "hint", "explain", "worked_example"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect());
            }
            if s.stage == "summary" {
                // Reassessment of a saved check, or discussion of the evidence.
                return Ok(vec!["assess".into(), "clarify".into(), "explain".into()]);
            }
            if ["instruction", "worked_example"].contains(&s.stage.as_str()) {
                // Follow-up questions and an optional reconstruction while reading; never a solution reveal.
                return Ok(["assess", "clarify", "hint", "explain"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect());
            }
        }
        Ok(["assess", "clarify", "hint", "explain", "worked_example"]
            .iter()
            .map(|s| s.to_string())
            .collect())
    }
    /// Tutor actions for one saved submission. A question is discussion and can never be assessed.
    pub fn allowed_for(&self, sid: &str, attempt_id: &str) -> Result<Vec<String>> {
        let question = intent(&self.conn, attempt_id)? == "question";
        Ok(self
            .allowed_actions(sid)?
            .into_iter()
            .filter(|a| !(question && a == "assess"))
            .collect())
    }
    pub fn tutor_input(&self, r: Request) -> Result<TutorInput> {
        let attempt = self
            .attempts(&r.session_id)?
            .into_iter()
            .find(|a| Some(&a.id) == r.attempt_id.as_ref())
            .ok_or("Saved attempt not found.")?;
        let activity = self.activity(&r.session_id)?;
        if attempt.activity_id != activity.id {
            return Err("That attempt belongs to an earlier activity.".into());
        }
        if self
            .attempts(&r.session_id)?
            .iter()
            .any(|a| a.revision_of.as_deref() == Some(&attempt.id))
        {
            return Err("Assess the corrected revision instead.".into());
        }
        let allowed_actions = self.allowed_for(&r.session_id, &attempt.id)?;
        if allowed_actions.is_empty() {
            return Err("Resume this task before requesting feedback.".into());
        }
        let flow = self.learning_state(&r.session_id)?;
        let stage = flow.as_ref().map(|f| f.stage.clone()).unwrap_or_default();
        let check = ["independent", "review"].contains(&stage.as_str());
        let visual_summary: Option<String> = if r.id.is_empty() || check {
            None
        } else {
            self.conn
                .query_row(
                    "SELECT json_extract(data,'$.text') FROM learning_events WHERE kind='tutor_visual_context' AND json_extract(data,'$.request_id')=?",
                    [&r.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql)?
        };
        let source_context = self
            .conn
            .query_row(
                "SELECT data FROM request_source_context WHERE request_id=?",
                [&r.id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(sql)?
            .map(parse::<Vec<SourcePassage>>)
            .transpose()?
            .unwrap_or(self.source_context(&r.session_id, &attempt.response)?);
        let objective = if activity.objective_id == "exploratory-unchecked" {
            "Exploratory topic outside a plan; no checked reference".to_owned()
        } else {
            crate::goals::outcome(&self.conn, &activity.objective_id)
        };
        // A prepared request keeps the model it was frozen with; a provisional one uses the current preference.
        let choice = if r.id.is_empty() {
            self.model_choice()?
        } else {
            self.request_choice(&r.id)?
        };
        Ok(TutorInput {
            choice,
            plan_context: self.plan_context(&r.session_id, &activity.objective_id)?,
            source_context,
            reference: self.task_spec(&activity.id)?,
            goal: flow.as_ref().map(|f| f.goal.clone()).unwrap_or_default(),
            objective,
            intent: intent(&self.conn, &attempt.id)?,
            conversation: self.conversation(&r.session_id, &activity.id, check, &attempt.id)?,
            // Independent work is graded on its own: no performance history and no earlier teaching.
            evidence: if check {
                String::new()
            } else {
                self.evidence_summary(&r.session_id)?
            },
            visual_summary,
            step_options: if flow.is_some() {
                self.tutor_steps(&r.session_id)?
            } else {
                vec![]
            },
            stage,
            allowed_actions,
            activity,
            attempt,
            request: r,
        })
    }
    pub fn prepare_tutor_request(&mut self, sid: &str, attempt_id: &str) -> Result<TutorInput> {
        self.prepare_tutor_request_with(sid, attempt_id, None)
    }
    /// `visual` is an already validated, bounded learner-visible interaction summary (see `visual_tutor_summary`).
    pub fn prepare_tutor_request_with(
        &mut self,
        sid: &str,
        attempt_id: &str,
        visual: Option<String>,
    ) -> Result<TutorInput> {
        let provisional = Request {
            schema_version: 1,
            id: String::new(),
            session_id: sid.into(),
            attempt_id: Some(attempt_id.into()),
            session_revision: self.session(sid)?.revision,
            kind: "tutor".into(),
            status: "pending".into(),
            reserved_microusd: 0,
            created_at: self.time(),
        };
        let mut input = self.tutor_input(provisional)?;
        let check = ["independent", "review"].contains(&input.stage.as_str());
        input.visual_summary = visual.filter(|_| !check);
        let conservative = crate::providers::tutor_reservation(&input)?;
        let choice = input.choice;
        input.request = self.reserve_for(
            sid,
            Some(attempt_id),
            "tutor",
            if self.offline { 0 } else { conservative },
            &choice,
        )?;
        if let Some(text) = &input.visual_summary {
            event(
                &self.conn,
                sid,
                "tutor_visual_context",
                &input.request.created_at,
                serde_json::json!({"request_id":input.request.id,"text":text}),
            )?;
        }
        Ok(input)
    }
    pub fn start_goal(&mut self, sid: &str, revision: i32, goal: &str) -> Result<()> {
        self.start_goal_for(sid, revision, goal, "quadratic-convergence")
    }
    pub fn start_goal_for(
        &mut self,
        sid: &str,
        revision: i32,
        goal: &str,
        objective_id: &str,
    ) -> Result<()> {
        if objective_id == "exploratory-unchecked" {
            return self.start_exploratory(sid, revision, goal);
        }
        crate::curriculum::objective(objective_id)?;
        valid_text(goal, 600)?;
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh.".into());
        }
        if self.learning_state(sid)?.is_some() {
            return Err("This session already has a learning goal.".into());
        }
        if self.pending_transcript(sid)?.is_some() || !self.session(sid)?.draft.is_empty() {
            return Err("Save or discard the current draft before starting the goal.".into());
        }
        let time = self.time();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO session_objectives VALUES (?,?,?)",
            params![sid, objective_id, crate::curriculum::CONTENT_VERSION],
        )
        .map_err(sql)?;
        let aid = freeze_task(&tx, sid, "diagnosis", &time, None)?;
        retitle(
            &tx,
            sid,
            crate::curriculum::objective(objective_id)?["outcome"]
                .as_str()
                .unwrap_or(goal),
        )?;
        write_state(
            &tx,
            &LearningState {
                schema_version: 1,
                session_id: sid.into(),
                goal: goal.trim().into(),
                stage: "diagnosis".into(),
                active_activity_id: aid,
                quiet: false,
                stopped: false,
                diagnosis_count: 0,
                policy_version: POLICY.into(),
            },
        )?;
        bump(&tx, sid)?;
        tx.commit().map_err(sql)
    }
    fn start_exploratory(&mut self, sid: &str, revision: i32, goal: &str) -> Result<()> {
        valid_text(goal, 600)?;
        let session = self.session(sid)?;
        if session.revision != revision
            || self.learning_state(sid)?.is_some()
            || self.pending_transcript(sid)?.is_some()
            || !session.draft.trim().is_empty()
        {
            return Err(
                "Use a fresh session with no draft or transcript for exploratory tutoring.".into(),
            );
        }
        let time = self.time();
        let aid = id();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        tx.execute("INSERT OR IGNORE INTO objectives VALUES ('exploratory-unchecked',?)",[serde_json::json!({"schema_version":1,"outcome":"Exploratory tutoring outside the checked unit","validation":"unreviewed; no checked reference or progress claims"}).to_string()]).map_err(sql)?;
        tx.execute(
            "INSERT INTO session_objectives VALUES (?,?,?)",
            params![sid, "exploratory-unchecked", "exploratory-v1"],
        )
        .map_err(sql)?;
        let activity=Activity{schema_version:1,id:aid.clone(),session_id:sid.into(),objective_id:"exploratory-unchecked".into(),version:1,prompt:format!("Explore: {goal}. Save a question, your current reasoning, or a request for an explanation or study plan. Ask the tutor when you want support."),math:String::new(),task_family:"exploratory-v1".into(),rubric_version:"ungraded-exploratory-v1".into(),validation_status:"Exploratory; no checked task, reference validation, assessment or review scheduling.".into()};
        let spec=TaskSpec{schema_version:1,activity_id:aid.clone(),kind:"exploratory".into(),assumptions:vec!["Check factual and mathematical claims against reliable sources.".into()],reference_solution:"No checked reference exists. Provide clearly labeled exploratory explanations or planning suggestions, state uncertainty, and never grade or claim independent success.".into(),rubric:vec![],rubric_criteria:vec![],content_version:"exploratory-v1".into(),validation_version:"unreviewed".into(),frozen_at:time,transfer_scope:"Exploration outside the checked unit; excluded from performance and review decisions".into(),origin:"exploratory".into(),response_kind:String::new(),support:None,answer_key:None,checks:vec![],provenance:"Exploratory discussion; no checked reference.".into()};
        tx.execute(
            "INSERT INTO activities VALUES (?,?,?,?,?,?,?)",
            params![
                aid,
                sid,
                activity.objective_id,
                1,
                json(&activity)?,
                spec.reference_solution,
                "[]"
            ],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO task_specs VALUES (?,?)",
            params![aid, json(&spec)?],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO scenes VALUES (?,?)",
            params![aid, json(&scene::sample(&aid))?],
        )
        .map_err(sql)?;
        retitle(&tx, sid, &format!("Explore: {}", goal.trim()))?;
        write_state(
            &tx,
            &LearningState {
                schema_version: 1,
                session_id: sid.into(),
                goal: goal.trim().into(),
                stage: "exploratory".into(),
                active_activity_id: aid,
                quiet: false,
                stopped: false,
                diagnosis_count: 0,
                policy_version: "exploratory-policy-v1".into(),
            },
        )?;
        bump(&tx, sid)?;
        tx.commit().map_err(sql)
    }
    pub fn submission_conditions(&self, at: &Attempt) -> Result<AttemptConditions> {
        self.submission_conditions_as(at, "answer")
    }
    pub fn submission_conditions_as(
        &self,
        at: &Attempt,
        intent: &str,
    ) -> Result<AttemptConditions> {
        // The same objective can be revisited in another session. Its most recent recorded
        // exposure must still reset the delayed-performance interval.
        let oid: String = self
            .conn
            .query_row(
                "SELECT objective_id FROM activities WHERE id=?",
                [&at.activity_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let last:Option<String>=self.conn.query_row("SELECT MAX(exposed_at) FROM (SELECT json_extract(h.data,'$.visible_at') exposed_at FROM assistance h JOIN activities a ON a.id=h.activity_id WHERE a.objective_id=?1 UNION ALL SELECT json_extract(t.data,'$.created_at') FROM attempts t JOIN activities a ON a.id=t.activity_id WHERE a.objective_id=?1)",[oid],|r|r.get(0)).map_err(sql)?;
        Ok(AttemptConditions {
            schema_version: 2,
            intent: intent.into(),
            attempt_id: at.id.clone(),
            submitted_at: at.created_at.clone(),
            last_relevant_exposure_at: last,
            simulated: self.offline || self.clock.simulated(),
            outside_help: self
                .assistance(&at.session_id)?
                .iter()
                .any(|h| h.activity_id == at.activity_id && h.depth == "outside_help"),
            task_kind: self
                .task_spec(&at.activity_id)?
                .map(|t| t.kind)
                .unwrap_or_else(|| "legacy".into()),
        })
    }
    pub fn validate_submission(&self, sid: &str) -> Result<()> {
        self.validate_submission_for(sid, "answer")
    }
    /// Questions are allowed in every open stage, including while reading an explanation or worked example and
    /// after a summary; they never count as diagnostic attempts. Answers need a task that accepts one.
    pub fn validate_submission_for(&self, sid: &str, intent: &str) -> Result<()> {
        if !["answer", "question"].contains(&intent) {
            return Err("Choose whether this is an answer or a question.".into());
        }
        if let Some(s) = self.learning_state(sid)? {
            if s.stopped {
                return Err("Resume before sending anything.".into());
            }
            if intent == "question" {
                return Ok(());
            }
            if s.stage == "summary" {
                return Err("This check is finished. Ask a question, or start a fresh check to answer a new task.".into());
            }
            if s.stage == "diagnosis" && s.diagnosis_count >= 3 {
                return Err("Diagnosis is bounded to three answers. Continue to the explanation, or ask a question.".into());
            }
        }
        Ok(())
    }
    pub fn learning_action(&mut self, sid: &str, revision: i32, action: &str) -> Result<()> {
        self.learning_state(sid)?
            .ok_or("Start a learning goal first.")?;
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh before acting.".into());
        }
        let time = self.time();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        self.apply_step(&tx, sid, action, None, &time)?;
        tx.commit().map_err(sql)
    }

    /// The stage a transition action leads to after every eligibility check, or None for an action that is not a
    /// transition. The same rules apply whether the next task is authored or must first be written.
    pub fn transition_target(&self, sid: &str, action: &str) -> Result<Option<String>> {
        let s = self
            .learning_state(sid)?
            .ok_or("Start a learning goal first.")?;
        if !["teach", "next", "fresh_check", "review", "another"].contains(&action) {
            return Ok(None);
        }
        if s.stopped || s.quiet {
            return Err("Resume before continuing.".into());
        }
        if s.stage == "exploratory" {
            return Err("Save your question and ask the exploratory tutor. Checked tasks and reviews are available in a planned session.".into());
        }
        if self.pending_transcript(sid)?.is_some() || !self.session(sid)?.draft.trim().is_empty() {
            return Err("Save or discard the draft before moving on.".into());
        }
        let now = self.time();
        match action {
            "review" => {
                if let Some(reason) = crate::evaluation::review_exclusion(&self.conn, sid, &now)? {
                    return Err(reason);
                }
                let reviews: Vec<Review> = many(
                    &self.conn,
                    "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? ORDER BY rowid DESC",
                    sid,
                )?;
                let r = reviews.iter().find(|r| r.status == "scheduled").ok_or(
                    "No valid review decision. Recompute evidence or finish a fresh check.",
                )?;
                if DateTime::parse_from_rfc3339(&r.due_at)
                    .map_err(|e| e.to_string())?
                    .with_timezone(&Utc)
                    > self.clock.now()
                {
                    return Err("Review is not due yet. Continue with a fresh check if you want early practice.".into());
                }
                Ok(Some("review".into()))
            }
            "fresh_check" => {
                if let Some(reason) = crate::evaluation::review_exclusion(&self.conn, sid, &now)? {
                    return Err(reason);
                }
                Ok(Some("independent".into()))
            }
            // Another task of the same kind: a short block of similar practice, or a replacement for a flawed task.
            "another" => {
                if s.stage == "summary" {
                    return Err("Choose a fresh check or the next capability.".into());
                }
                Ok(Some(s.stage.clone()))
            }
            _ => {
                // Only answers move a task on; questions are discussion.
                let current = self
                    .attempts(sid)?
                    .into_iter()
                    .filter(|a| {
                        a.activity_id == s.active_activity_id
                            && intent(&self.conn, &a.id).is_ok_and(|i| i == "answer")
                    })
                    .collect::<Vec<_>>();
                if action == "teach" && s.stage != "diagnosis" {
                    return Err("Direct teaching is available during diagnosis.".into());
                }
                if action == "next"
                    && [
                        "diagnosis",
                        "completion",
                        "guided",
                        "exploration",
                        "explanation",
                        "independent",
                        "review",
                    ]
                    .contains(&s.stage.as_str())
                {
                    let last = current.last().ok_or(
                        "Send an answer first, or ask for the explanation during diagnosis.",
                    )?;
                    if ["independent", "review"].contains(&s.stage.as_str()) {
                        let grades: Vec<Assessment> = many(
                            &self.conn,
                            "SELECT data FROM assessments WHERE attempt_id=? ORDER BY rowid DESC",
                            &last.id,
                        )?;
                        if !grades.iter().any(|g| !g.stale) {
                            return Err(
                                "Get feedback on the saved check before the summary.".into()
                            );
                        }
                    }
                    if s.stage == "exploration" && !self.revealed(&s.active_activity_id)? {
                        return Err("Reveal and explore the trace before explaining it.".into());
                    }
                }
                if s.stage == "summary" {
                    return Err("Choose a fresh check or a due review.".into());
                }
                let next = if ["review", "independent"].contains(&s.stage.as_str()) {
                    "summary".to_owned()
                } else if action == "teach" {
                    "instruction".to_owned()
                } else {
                    self.next_stage(sid, &s)?.ok_or("Unknown stage")?.0.to_owned()
                };
                Ok(Some(next))
            }
        }
    }

    /// Applies one learner action inside `tx`: control (resume, stop, quiet), recorded help, recomputation, or a
    /// transition to a new task. A transition for a generated capability needs `generated` material, already checked.
    pub(crate) fn apply_step(
        &self,
        tx: &Connection,
        sid: &str,
        action: &str,
        generated: Option<Material>,
        time: &str,
    ) -> Result<()> {
        let mut s = self
            .learning_state(sid)?
            .ok_or("Start a learning goal first.")?;
        if action == "resume" {
            s.stopped = false;
            s.quiet = false;
        } else if action == "stop" {
            s.stopped = true;
        } else if action == "quiet" {
            s.quiet = true;
        } else {
            if s.stopped || s.quiet {
                return Err("Resume before requesting instruction.".into());
            }
            if s.stage == "exploratory" && action != "outside_help" {
                return Err("Save your question and ask the exploratory tutor. Checked tasks and reviews are available in a planned session.".into());
            }
            match action {
                "hint" | "worked_example" | "explain" | "outside_help" | "example"
                | "prerequisite" => {
                    if s.stage == "summary" {
                        return Err("Open a task first.".into());
                    }
                    let spec = self
                        .task_spec(&s.active_activity_id)?
                        .ok_or("Task missing")?;
                    let content = match action {
                        "hint" => crate::curriculum::hint(tx, sid).or_else(|e| {
                            if crate::goals::is_generated(tx, &crate::curriculum::selected(tx, sid)?)? {
                                Err(e)
                            } else {
                                Ok(format!(
                                    "Use the task assumptions and separate setup from interpretation. {}",
                                    spec.rubric_criteria.first().cloned().unwrap_or_default()
                                ))
                            }
                        })?,
                        // The current task's own solution: deliberately distinct from a related example.
                        "worked_example" => {
                            format!("Solution to this task: {}", spec.reference_solution)
                        }
                        "example" => crate::curriculum::worked_example(tx, sid)?,
                        "prerequisite" => crate::curriculum::prerequisite(tx, sid)?
                            .map(|(_, text)| text)
                            .ok_or("This capability has no prerequisite review prepared.")?,
                        "outside_help" => {
                            "Learner reported relevant outside help on this task.".into()
                        }
                        _ => {
                            if s.stage == "exploratory" {
                                "Exploratory discussion is unreviewed.".into()
                            } else {
                                crate::curriculum::teaching(tx, sid)?
                            }
                        }
                    };
                    add_help(tx, sid, &s.active_activity_id, None, action, &content, time)?;
                }
                "recompute" => recompute(tx, sid, time, self.offline || self.clock.simulated())?,
                _ => {
                    let next = self
                        .transition_target(sid, action)?
                        .ok_or("Action is not allowed.")?;
                    self.transition(tx, sid, &mut s, action, &next, generated, time)?;
                }
            }
        }
        event(
            tx,
            sid,
            action,
            time,
            serde_json::json!({"activity_id":s.active_activity_id}),
        )?;
        write_state(tx, &s)?;
        bump(tx, sid)
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &self,
        tx: &Connection,
        sid: &str,
        s: &mut LearningState,
        _action: &str,
        next: &str,
        generated: Option<Material>,
        time: &str,
    ) -> Result<()> {
        if next == "summary" {
            s.stage = "summary".into();
            return recompute(tx, sid, time, self.offline || self.clock.simulated());
        }
        let previous = s.active_activity_id.clone();
        let current: Vec<Attempt> = self
            .attempts(sid)?
            .into_iter()
            .filter(|a| a.activity_id == previous && intent(tx, &a.id).is_ok_and(|i| i == "answer"))
            .collect();
        let review = if next == "review" {
            many::<Review>(tx, "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? ORDER BY rowid DESC", sid)?
                .into_iter()
                .find(|r| r.status == "scheduled")
        } else {
            None
        };
        let generated_capability = generated.is_some();
        let aid = freeze_task(tx, sid, next, time, generated)?;
        s.active_activity_id = aid.clone();
        s.stage = next.into();
        if let Some(r) = review {
            tx.execute(
                "UPDATE reviews SET data=json_set(data,'$.status','in_progress') WHERE id=?",
                [&r.id],
            )
            .map_err(sql)?;
        }
        let spec: Option<TaskSpec> = tx
            .query_row(
                "SELECT data FROM task_specs WHERE activity_id=?",
                [&aid],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(sql)?
            .map(parse)
            .transpose()?;
        let support = spec.as_ref().and_then(|t| t.support.clone());
        if next == "instruction" {
            let text = if generated_capability {
                // The generated explanation was written from the learner's latest attempt.
                support
                    .as_ref()
                    .map(|t| t.explanation.clone())
                    .filter(|t| !t.trim().is_empty())
                    .ok_or("The task has no explanation.")?
            } else {
                let grades: Vec<Assessment> = many(
                    tx,
                    "SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=? ORDER BY x.rowid DESC",
                    sid,
                )?;
                let target = match grades.iter().find(|g| !g.stale).map(|g| g.verdict.as_str()) {
                    Some("correct") => {
                        "Your setup was sound; now connect the general bound to the boundary case."
                    }
                    Some("partial") => {
                        "Keep the part you established, and fill the missing magnitude justification."
                    }
                    Some("incorrect") => {
                        "A locally downhill direction does not guarantee that a finite step lowers the objective."
                    }
                    Some("unresolved") => {
                        "Your earlier interpretation remains unresolved; this explanation does not turn it into assessed evidence."
                    }
                    _ => "We will build the derivative and update rule before checking a new case.",
                };
                let opening = match current.last() {
                    Some(a) => format!("Your starting idea was: “{}”. ", clip(&a.response, 400)),
                    None => "You asked to start with the explanation. ".into(),
                };
                if crate::curriculum::selected(tx, sid)? != "quadratic-convergence" {
                    format!("{opening}{}", crate::curriculum::teaching(tx, sid)?)
                } else {
                    format!(
                        "{target} {opening}The distinction to check is sign versus magnitude. f′(x)=ax, so xₜ₊₁=(1−ηa)xₜ. Negative values cross zero; |1−ηa|<1 shrinks the distance. Therefore 0<η<2/a. At ηa=1 the next state is zero; at ηa=2 nonzero states oscillate without shrinking. This reasoning assumes positive curvature and fixed steps."
                    )
                }
            };
            add_help(tx, sid, &aid, None, "explain", &text, time)?;
        }
        if next == "worked_example" {
            // The related example, never this task's or a later task's solution.
            let text = if generated_capability {
                support
                    .as_ref()
                    .map(|t| format!("Related example, not your task.\n{}", t.worked_example))
                    .ok_or("The task has no worked example.")?
            } else {
                crate::curriculum::worked_example(tx, sid)?
            };
            add_help(tx, sid, &aid, None, "example", &text, time)?;
        }
        if next == "explanation" {
            add_help(
                tx,
                sid,
                &aid,
                None,
                "visual",
                "Explanation follows the revealed trace and saved parameter exploration.",
                time,
            )?;
        }
        Ok(())
    }
    pub fn correct_attempt(
        &mut self,
        sid: &str,
        revision: i32,
        attempt_id: &str,
        submission_id: &str,
        text: &str,
    ) -> Result<Attempt> {
        valid_text(text, 12000)?;
        uuid::Uuid::parse_str(submission_id).map_err(|_| "Invalid revision ID")?;
        if let Some(existing) = self
            .conn
            .query_row(
                "SELECT data FROM attempts WHERE submission_id=?",
                [submission_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(sql)?
        {
            let at: Attempt = parse(existing)?;
            if at.session_id != sid
                || at.revision_of.as_deref() != Some(attempt_id)
                || at.response != text.trim()
            {
                return Err("Revision ID belongs to different content.".into());
            }
            return Ok(at);
        }
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh.".into());
        }
        let ats = self.attempts(sid)?;
        let original = ats
            .iter()
            .find(|a| a.id == attempt_id)
            .ok_or("Attempt not found")?;
        if ats
            .iter()
            .any(|a| a.revision_of.as_deref() == Some(attempt_id))
        {
            return Err("Correct the latest revision.".into());
        }
        let mut at = original.clone();
        at.id = id();
        at.revision_of = Some(original.id.clone());
        at.response = text.trim().into();
        at.created_at = self.time();
        for h in self
            .assistance(sid)?
            .iter()
            .filter(|h| h.activity_id == at.activity_id)
        {
            if !at.assistance_ids.contains(&h.id) {
                at.assistance_ids.push(h.id.clone());
            }
            if h.depth == "visual" && !at.exposure_ids.contains(&h.id) {
                at.exposure_ids.push(h.id.clone());
            }
        }
        let cond = self.submission_conditions_as(&at, &intent(&self.conn, attempt_id)?)?;
        let visual_conditions = crate::visualizer::attempt_snapshot(&self.conn, &at, None)?;
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        tx.execute("INSERT INTO attempts(id,session_id,activity_id,submission_id,data,transcript_id) VALUES (?,?,?,?,?,NULL)",params![at.id,sid,at.activity_id,submission_id,json(&at)?]).map_err(sql)?;
        tx.execute(
            "INSERT INTO attempt_conditions VALUES (?,?)",
            params![at.id, json(&cond)?],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO visual_attempt_snapshots VALUES (?,?)",
            params![at.id, json(&visual_conditions)?],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE assessments SET data=json_set(data,'$.stale',json('true')) WHERE attempt_id=?",
            [attempt_id],
        )
        .map_err(sql)?;
        invalidate(&tx, sid)?;
        bump(&tx, sid)?;
        event(
            &tx,
            sid,
            "correction",
            &at.created_at,
            serde_json::json!({"original":attempt_id,"revision":at.id}),
        )?;
        tx.commit().map_err(sql)?;
        Ok(at)
    }
    pub fn dispute_grade(
        &mut self,
        sid: &str,
        revision: i32,
        assessment_id: &str,
        reason: &str,
    ) -> Result<()> {
        valid_text(reason, 2000)?;
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh.".into());
        }
        let mut grade:Assessment=parse(self.conn.query_row("SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE x.id=? AND a.session_id=?",params![assessment_id,sid],|r|r.get(0)).map_err(sql)?)?;
        if grade.stale {
            return Err("That assessment is already stale.".into());
        }
        grade.dispute_state = "disputed".into();
        let time = self.time();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        let dispute_id = id();
        event(
            &tx,
            sid,
            "grade_disputed",
            &time,
            serde_json::json!({"dispute_id":dispute_id,"assessment_id":grade.id}),
        )?;
        tx.execute(
            "INSERT INTO disputes VALUES (?,?,?,?,?,NULL)",
            params![dispute_id, sid, grade.attempt_id, reason, time],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE assessments SET data=? WHERE id=?",
            params![json(&grade)?, assessment_id],
        )
        .map_err(sql)?;
        invalidate(&tx, sid)?;
        bump(&tx, sid)?;
        tx.commit().map_err(sql)
    }
    pub fn resolve_dispute(&mut self, sid: &str, revision: i32, assessment_id: &str) -> Result<()> {
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh.".into());
        }
        let mut grade:Assessment=parse(self.conn.query_row("SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE x.id=? AND a.session_id=?",params![assessment_id,sid],|r|r.get(0)).map_err(sql)?)?;
        if grade.stale || grade.uncertainty != "resolved" {
            return Err("A fresh, resolved reassessment is required.".into());
        }
        let originals: Vec<String> = {
            let mut st=self.conn.prepare("SELECT json_extract(e.data,'$.assessment_id') FROM disputes d JOIN learning_events e ON json_extract(e.data,'$.dispute_id')=d.id WHERE d.attempt_id=? AND d.resolved_by IS NULL AND e.kind='grade_disputed'").map_err(sql)?;
            st.query_map([&grade.attempt_id], |r| r.get(0))
                .map_err(sql)?
                .collect::<std::result::Result<Vec<String>, _>>()
                .map_err(sql)?
        };
        if originals.is_empty() || originals.contains(&grade.id) {
            return Err("Request a new assessment before accepting a resolution.".into());
        }
        grade.dispute_state = "resolved_by_learner".into();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        tx.execute(
            "UPDATE disputes SET resolved_by=? WHERE attempt_id=? AND resolved_by IS NULL",
            params![grade.id, grade.attempt_id],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE assessments SET data=? WHERE id=?",
            params![json(&grade)?, grade.id],
        )
        .map_err(sql)?;
        recompute(
            &tx,
            sid,
            &self.time(),
            self.offline || self.clock.simulated(),
        )?;
        bump(&tx, sid)?;
        tx.commit().map_err(sql)
    }
}

/// Neutral, objective-independent stage names (no quadratic-specific wording).
pub fn stage_title(stage: &str) -> &'static str {
    match stage {
        "diagnosis" => "Start with your own reasoning",
        "instruction" => "The key idea",
        "worked_example" => "A worked example",
        "completion" => "Finish a partly worked problem",
        "guided" => "Compare two cases",
        "exploration" => "Predict, then explore",
        "explanation" => "Explain what you saw",
        "independent" => "Fresh check · on your own",
        "review" => "Later review · on your own",
        "summary" => "Where you stand",
        "exploratory" => "Exploratory discussion · unreviewed",
        _ => "Study activity",
    }
}

fn help_label(depth: &str) -> &'static str {
    match depth {
        "hint" => "Hint",
        "explain" => "Explanation",
        "example" => "Related worked example",
        "worked_example" => "Solution to this task",
        "prerequisite" => "Prerequisite review",
        "clarify" => "Clarifying question",
        "assess" => "Feedback on your answer",
        "method_cue" => "Method cue",
        "outside_help" => "Outside help reported",
        "visual" => "Visual result shown",
        "source" => "Source opened",
        _ => "Tutor note",
    }
}

/// Frozen intent of a submission ("answer" for records made before intents existed).
pub(crate) fn intent(c: &Connection, attempt_id: &str) -> Result<String> {
    Ok(c.query_row(
        "SELECT COALESCE(json_extract(data,'$.intent'),'answer') FROM attempt_conditions WHERE attempt_id=?",
        [attempt_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)?
    .unwrap_or_else(|| "answer".into()))
}

/// Character-safe truncation with an ellipsis.
pub(crate) fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.into();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// "I don't know" and similar: the learner is asking to be taught, not to be probed further.
pub fn unsure(text: &str) -> bool {
    let t = text.trim().to_lowercase().replace('’', "'");
    let t = t.trim_end_matches(['.', '!', '?', ' ']);
    if t.is_empty() {
        return text.trim().starts_with('?');
    }
    t.chars().count() <= 60
        && [
            "i don't know",
            "i dont know",
            "i do not know",
            "don't know",
            "dont know",
            "not sure",
            "i'm not sure",
            "im not sure",
            "i am not sure",
            "no idea",
            "i have no idea",
            "idk",
            "no clue",
            "unsure",
            "i'm stuck",
            "im stuck",
            "stuck",
        ]
        .iter()
        .any(|p| t == *p || t.starts_with(&format!("{p} ")) || t.starts_with(&format!("{p},")))
}

fn all_met(g: &Assessment) -> bool {
    g.verdict == "correct"
        && g.uncertainty == "resolved"
        && g.dispute_state != "disputed"
        && !g.stale
        && g.findings.iter().all(|f| f.result == "met")
}

impl Store {
    /// Latest answer on an activity (corrections replace their original) and its latest current assessment.
    pub fn latest_answer(
        &self,
        sid: &str,
        aid: &str,
    ) -> Result<Option<(Attempt, Option<Assessment>)>> {
        let all = self.attempts(sid)?;
        let Some(at) = all
            .iter()
            .rfind(|a| {
                a.activity_id == aid
                    && !all
                        .iter()
                        .any(|b| b.revision_of.as_deref() == Some(a.id.as_str()))
                    && intent(&self.conn, &a.id).is_ok_and(|i| i == "answer")
            })
            .cloned()
        else {
            return Ok(None);
        };
        let grade = many::<Assessment>(
            &self.conn,
            "SELECT data FROM assessments WHERE attempt_id=? ORDER BY rowid",
            &at.id,
        )?
        .into_iter()
        .rev()
        .find(|g| !g.stale);
        Ok(Some((at, grade)))
    }
    pub fn visual_plan_for(&self, aid: &str) -> Result<Option<VisualPlan>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT json_extract(data,'$.plan') FROM learning_events WHERE kind='visual_plan' AND json_extract(data,'$.activity_id')=? ORDER BY rowid DESC LIMIT 1",
                [aid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        raw.map(parse).transpose()
    }
    /// The lesson stage after the current one, and whether scaffolding was skipped to reach it. Support fades with
    /// evidence: when the latest answer on the key-idea or worked-example task met every rubric part, the remaining
    /// worked steps (related example, completion problem) are skipped for guided practice. Otherwise, or without a
    /// resolved answer, the full route continues. None when the stage is not on this capability's route.
    pub(crate) fn next_stage(
        &self,
        sid: &str,
        s: &LearningState,
    ) -> Result<Option<(&'static str, bool)>> {
        let route = crate::goals::route(&self.conn, &crate::curriculum::selected(&self.conn, sid)?);
        let Some(position) = route.iter().position(|x| *x == s.stage) else {
            return Ok(None);
        };
        let demonstrated = ["instruction", "worked_example"].contains(&s.stage.as_str())
            && self
                .latest_answer(sid, &s.active_activity_id)?
                .and_then(|(_, g)| g)
                .is_some_and(|g| all_met(&g));
        let guided = route.iter().position(|x| *x == "guided");
        Ok(Some(match guided {
            Some(g) if demonstrated && g > position + 1 => (route[g], true),
            _ => (
                route.get(position + 1).copied().unwrap_or("independent"),
                false,
            ),
        }))
    }
    fn revealed(&self, aid: &str) -> Result<bool> {
        let actual: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM visual_exposures e JOIN visual_activities a ON a.id=e.activity_id WHERE a.learning_activity_id=? AND json_extract(e.data,'$.scope')!='exploration' AND json_array_length(json_extract(e.data,'$.groups'))>0)",[aid],|r|r.get(0)).map_err(sql)?;
        Ok(actual || self.scene(aid).is_ok_and(|s| s.revealed))
    }

    /// Learner-selectable next steps for the current task, with one recommendation. The deterministic policy stops
    /// diagnosis once teaching is clearly the useful next action, offers to skip remaining practice only after a
    /// resolved answer met every rubric part, and never treats a provider label as a transition. A validated tutor
    /// suggestion about the latest submission replaces the policy recommendation only if it is still eligible.
    pub fn steps(&self, sid: &str) -> Result<(Vec<StepOption>, Option<String>)> {
        let Some(s) = self.learning_state(sid)? else {
            return Ok((vec![], None));
        };
        if s.stopped || s.quiet || s.stage == "exploratory" {
            return Ok((vec![], None));
        }
        let option = |id: &str, label: String, reason: &str, kind: &str| StepOption {
            id: id.into(),
            label,
            reason: reason.into(),
            kind: kind.into(),
            recommended: false,
        };
        let latest = self.latest_answer(sid, &s.active_activity_id)?;
        let grade = latest.as_ref().and_then(|(_, g)| g.clone());
        let mut out = vec![];
        let mut recommend: Option<(String, String)> = None;
        match s.stage.as_str() {
            "summary" => {
                let due = many::<Review>(&self.conn, "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? ORDER BY rowid DESC", sid)?
                    .into_iter()
                    .find(|r| r.status == "scheduled")
                    .is_some_and(|r| DateTime::parse_from_rfc3339(&r.due_at).is_ok_and(|d| d.with_timezone(&Utc) <= self.clock.now()));
                if due {
                    out.push(option(
                        "review",
                        "Start the due review".into(),
                        "A planned later check is ready.",
                        "transition",
                    ));
                    recommend = Some(("review".into(), "A planned later check is ready; it shows what you can still do after a gap.".into()));
                }
                out.push(option("fresh_check", "Try another fresh check".into(), "Practise on a new variant now. Early practice does not count as a later review.", "transition"));
            }
            "independent" | "review" => {
                let graded = grade.is_some();
                out.push(option(
                    "continue",
                    "Finish and see where you stand".into(),
                    if graded {
                        "Records the outcome of this check and schedules a later review."
                    } else if latest.is_some() {
                        "Get feedback on your answer before finishing."
                    } else {
                        "Send your answer first."
                    },
                    if graded { "transition" } else { "unavailable" },
                ));
                if graded {
                    recommend = Some((
                        "continue".into(),
                        "Your check has feedback. Finishing records it and plans a later review."
                            .into(),
                    ));
                }
            }
            stage => {
                let objective = crate::curriculum::selected(&self.conn, sid)?;
                let (next, skipped) = self.next_stage(sid, &s)?.unwrap_or(("independent", false));
                // Authored variants change with each new task except in the original hand-written sequence.
                let generated = crate::goals::is_generated(&self.conn, &objective)?;
                let another = if generated {
                    ["worked_example", "completion", "guided"].contains(&stage)
                } else {
                    objective != "quadratic-convergence"
                        && ["completion", "guided"].contains(&stage)
                };
                let needs_answer = [
                    "diagnosis",
                    "completion",
                    "guided",
                    "exploration",
                    "explanation",
                ]
                .contains(&stage);
                let ready = stage != "exploration" || self.revealed(&s.active_activity_id)?;
                if stage == "diagnosis" {
                    out.push(option(
                        "teach",
                        if latest.is_some() {
                            "Continue to the explanation".into()
                        } else {
                            "Explain the idea first".into()
                        },
                        "An explanation connected to what you wrote so far.",
                        "transition",
                    ));
                } else {
                    let available = (!needs_answer || latest.is_some()) && ready;
                    out.push(option(
                        "continue",
                        format!("Continue: {}", stage_title(next).to_lowercase()),
                        if !ready {
                            "Send your prediction, then reveal the result in the visual first."
                        } else if available {
                            "The next step in this lesson."
                        } else {
                            "Send an answer to this task first, or ask a question."
                        },
                        if available {
                            "transition"
                        } else {
                            "unavailable"
                        },
                    ));
                }
                out.push(option(
                    "fresh_check",
                    "Skip to a fresh check".into(),
                    "A new task on your own. Earlier teaching is hidden while you work on it.",
                    "transition",
                ));
                if another {
                    out.push(option(
                        "another",
                        "Another problem like this".into(),
                        "A new problem of the same kind before moving on.",
                        "transition",
                    ));
                }
                if let Ok(Some((outcome, _))) = crate::curriculum::prerequisite(&self.conn, sid)
                    && [
                        "diagnosis",
                        "instruction",
                        "worked_example",
                        "completion",
                        "guided",
                    ]
                    .contains(&stage)
                {
                    out.push(option("prerequisite", "Review the prerequisite".into(), &format!("A short explanation and example of the earlier idea this builds on — {} — recorded as help.", outcome.to_lowercase()), "support"));
                }
                let clean = latest
                    .as_ref()
                    .is_some_and(|(a, _)| a.assistance_ids.is_empty() && a.exposure_ids.is_empty());
                recommend = match (stage, latest.as_ref(), grade.as_ref()) {
                    ("diagnosis", Some((a, _)), _) if unsure(&a.response) => Some(("teach".into(), "You said you are not sure yet. An explanation will help more than another guess.".into())),
                    ("diagnosis", Some(_), Some(g)) if all_met(g) && clean => Some(("fresh_check".into(), "Your answer met all three parts without help. A fresh check would confirm it; the explanation is optional.".into())),
                    ("diagnosis", Some(_), Some(g)) if g.uncertainty == "unresolved" || g.verdict == "unresolved" => None,
                    ("diagnosis", Some(_), Some(_)) => Some(("teach".into(), "The explanation connects directly to what you wrote.".into())),
                    ("diagnosis", Some(_), None) if s.diagnosis_count >= 2 => Some(("teach".into(), "Two answers are enough to start; more guessing adds little.".into())),
                    ("completion" | "guided", Some(_), Some(g)) if all_met(g) => Some(("fresh_check".into(), "You solved this one correctly. The remaining practice is optional; a fresh check shows whether you can do it on your own.".into())),
                    ("worked_example" | "completion" | "guided", Some(_), Some(g)) if another && g.verdict == "incorrect" && g.uncertainty == "resolved" => Some(("another".into(), "One more problem of the same kind should make the method reliable before you move on.".into())),
                    ("completion" | "guided", Some(_), Some(_)) => Some(("continue".into(), "Another supported problem is likely to help before a check.".into())),
                    ("exploration", _, _) if !ready => None,
                    (_, Some(_), Some(_)) if skipped => Some(("continue".into(), "You applied the idea correctly, so the remaining worked steps are skipped for practice.".into())),
                    (_, _, _) if (!needs_answer || latest.is_some()) && ready => Some(("continue".into(), format!("Next: {}.", stage_title(next).to_lowercase()))),
                    _ => None,
                };
            }
        }
        // A tutor suggestion counts only for the latest submission on this task and only while it is still eligible.
        let latest_any = self
            .attempts(sid)?
            .into_iter()
            .rfind(|a| a.activity_id == s.active_activity_id);
        let suggestion: Option<(String, String)> = match &latest_any {
            Some(at) => self
                .conn
                .query_row(
                    "SELECT json_extract(data,'$.step'),json_extract(data,'$.reason') FROM learning_events WHERE kind='step_suggested' AND json_extract(data,'$.attempt_id')=? ORDER BY rowid DESC LIMIT 1",
                    [&at.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(sql)?,
            None => None,
        };
        let mut note = None;
        if let Some((step, reason)) = suggestion
            && out.iter().any(|o| o.id == step && o.kind != "unavailable")
            && (step != "fresh_check" || grade.as_ref().is_some_and(all_met))
        {
            recommend = Some((step, format!("Tutor suggestion: {reason}")));
        }
        if let Some((id, reason)) = recommend
            && let Some(o) = out
                .iter_mut()
                .find(|o| o.id == id && o.kind != "unavailable")
        {
            o.recommended = true;
            note = Some(reason);
        }
        Ok((out, note))
    }

    /// A short account of the plan this capability belongs to, for the tutor.
    pub fn plan_context(&self, sid: &str, objective: &str) -> Result<String> {
        let Some(gid) = crate::goals::goal_of(&self.conn, sid)? else {
            return Ok(if crate::goals::is_generated(&self.conn, objective)? {
                "Generated capability; not reviewed by a person.".into()
            } else {
                "Authored example unit; analytically checked; human subject review pending.".into()
            });
        };
        let plan = self.plan_revision(&gid)?;
        let list = plan
            .capabilities
            .iter()
            .filter(|c| c.status == "planned")
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "{}. {}{}",
                    i + 1,
                    clip(&c.outcome, 160),
                    if c.objective_id == objective {
                        " (this session)"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        Ok(clip(
            &format!(
                "Plan “{}” ({}). Assumed level: {}. Background: {}. Capabilities: {list}",
                plan.title,
                if plan.origin == "authored" {
                    "authored example unit"
                } else {
                    "generated for this learner; not reviewed by a person"
                },
                if plan.level.is_empty() {
                    "not stated"
                } else {
                    &plan.level
                },
                if plan.background.is_empty() {
                    "none stated".to_owned()
                } else {
                    plan.background.join("; ")
                },
            ),
            1800,
        ))
    }

    /// Step identities the tutor may suggest for the current task ("stay" keeps working on it).
    pub fn tutor_steps(&self, sid: &str) -> Result<Vec<String>> {
        let mut ids: Vec<String> = self
            .steps(sid)?
            .0
            .into_iter()
            .filter(|o| o.kind != "unavailable")
            .map(|o| o.id)
            .collect();
        ids.push("stay".into());
        Ok(ids)
    }

    /// What was demonstrated, what needed help and what remains unchecked, from primary records only.
    pub fn wrap_up(&self, sid: &str) -> Result<WrapUp> {
        let ats = self.attempts(sid)?;
        let mut demonstrated = vec![];
        let mut needed_help = vec![];
        let mut unchecked = vec![];
        let conditions: Vec<AttemptConditions> = many(
            &self.conn,
            "SELECT x.data FROM attempt_conditions x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=?",
            sid,
        )?;
        for at in ats.iter().filter(|a| {
            !ats.iter()
                .any(|b| b.revision_of.as_deref() == Some(a.id.as_str()))
        }) {
            let Some(c) = conditions.iter().find(|c| c.attempt_id == at.id) else {
                continue;
            };
            if c.intent == "question" || c.task_kind == "legacy" {
                continue;
            }
            let check = ["independent", "review"].contains(&c.task_kind.as_str());
            let what = match c.task_kind.as_str() {
                "independent" => "Fresh check",
                "review" => "Later review",
                "diagnosis" => "Opening answer",
                "completion" => "Partly worked problem",
                "guided" => "Comparison of two cases",
                "exploration" => "Prediction before the visual",
                "explanation" => "Explanation of the visual",
                "worked_example" => "Reconstruction of the worked example",
                "instruction" => "Restatement of the key idea",
                _ => "Answer",
            };
            let grade = many::<Assessment>(
                &self.conn,
                "SELECT data FROM assessments WHERE attempt_id=? ORDER BY rowid",
                &at.id,
            )?
            .into_iter()
            .rev()
            .find(|g| !g.stale);
            let supported = !at.assistance_ids.is_empty()
                || !at.exposure_ids.is_empty()
                || at.revision_of.is_some()
                || c.outside_help;
            match grade {
                None => {
                    if check {
                        unchecked.push(format!("{what}: no feedback yet, so it is not counted."));
                    }
                }
                Some(g)
                    if g.dispute_state == "disputed"
                        || g.uncertainty != "resolved"
                        || g.verdict == "unresolved" =>
                {
                    unchecked.push(format!(
                        "{what}: the assessment is unresolved or disputed, so it is not counted."
                    ));
                }
                Some(g) => {
                    let missing: Vec<&str> = g
                        .findings
                        .iter()
                        .filter(|f| f.result != "met")
                        .map(|f| f.dimension.as_str())
                        .collect();
                    if g.verdict == "correct" && check && !supported {
                        demonstrated.push(format!(
                            "{what}: correct on a new variant with no recorded help{}.",
                            if c.simulated { " (simulated)" } else { "" }
                        ));
                    } else if g.verdict == "correct" {
                        needed_help.push(format!(
                            "{what}: correct with support{}.",
                            if check {
                                ", so it counts as supported rather than independent"
                            } else {
                                ""
                            }
                        ));
                    } else {
                        needed_help.push(format!(
                            "{what}: {}{}.",
                            g.verdict,
                            if missing.is_empty() {
                                String::new()
                            } else {
                                format!(" — still to secure: {}", missing.join(", "))
                            }
                        ));
                    }
                }
            }
        }
        if let Some(r) = many::<Review>(&self.conn, "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? ORDER BY rowid DESC", sid)?
            .into_iter()
            .find(|r| r.status == "scheduled")
        {
            unchecked.push(format!(
                "Whether it lasts: not yet checked. A later review is planned for {} ({}-day interval).",
                r.due_at.get(..10).unwrap_or(&r.due_at),
                r.interval_days
            ));
        } else {
            unchecked.push("Whether it lasts: not yet checked. A later review is planned after a resolved fresh check.".into());
        }
        unchecked
            .push("Transfer beyond the kinds of problems practised here is not assessed.".into());
        let help = self.assistance(sid)?;
        let count = |d: &str| {
            help.iter()
                .filter(|h| h.depth == d && h.attempt_id.is_none())
                .count()
        };
        let mut explained = vec![];
        if count("explain") > 0 {
            explained.push("The key idea was explained, starting from your own first answer where there was one.".to_owned());
        }
        if count("example") > 0 {
            explained.push(format!(
                "{} related worked example(s) were shown.",
                count("example")
            ));
        }
        if count("prerequisite") > 0 {
            explained.push("A prerequisite was reviewed.".into());
        }
        if count("hint") > 0 {
            explained.push(format!("{} hint(s) were used.", count("hint")));
        }
        if count("worked_example") > 0 {
            explained.push(format!(
                "A task's full solution was shown {} time(s).",
                count("worked_example")
            ));
        }
        if help.iter().any(|h| h.depth == "visual") {
            explained.push("An interactive visual's result was revealed.".into());
        }
        let replies = help.iter().filter(|h| h.attempt_id.is_some()).count();
        if replies > 0 {
            explained.push(format!("The tutor replied {replies} time(s)."));
        }
        let keep = |mut v: Vec<String>| {
            if v.len() > 4 {
                let extra = v.len() - 3;
                v.truncate(3);
                v.push(format!("…and {extra} more in the evidence details."));
            }
            v
        };
        Ok(WrapUp {
            explained: keep(explained),
            demonstrated: keep(demonstrated),
            needed_help: keep(needed_help),
            unchecked: keep(unchecked),
        })
    }

    /// Bounded, ordered recent exchanges for the tutor. Teaching requests see recent session turns (latest
    /// corrections only, no feedback on superseded or disputed answers, no visual bookkeeping). An independent check
    /// or review sees only its own task's turns, never earlier instruction. Other sessions and held-out comparison
    /// material are never included; frozen grading references travel separately.
    pub fn conversation(
        &self,
        sid: &str,
        aid: &str,
        check: bool,
        exclude: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let ats = self.attempts(sid)?;
        let superseded = |id: &str| ats.iter().any(|b| b.revision_of.as_deref() == Some(id));
        let disputed: Vec<String> = {
            let mut st = self
                .conn
                .prepare(
                    "SELECT attempt_id FROM disputes WHERE session_id=? AND resolved_by IS NULL",
                )
                .map_err(sql)?;
            st.query_map([sid], |r| r.get(0))
                .map_err(sql)?
                .collect::<std::result::Result<_, _>>()
                .map_err(sql)?
        };
        let mut rows: Vec<(String, serde_json::Value)> = vec![];
        if !check {
            let mut st = self.conn.prepare("SELECT a.data, t.data FROM activities a JOIN task_specs t ON t.activity_id=a.id WHERE a.session_id=? ORDER BY a.rowid").map_err(sql)?;
            let tasks = st
                .query_map([sid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(sql)?;
            for t in tasks {
                let (a, spec) = t.map_err(sql)?;
                let (a, spec): (Activity, TaskSpec) = (parse(a)?, parse(spec)?);
                if a.id != aid {
                    rows.push((spec.frozen_at, serde_json::json!({"role":"task","stage":spec.kind,"text":clip(&a.prompt, 400)})));
                }
            }
        }
        for at in &ats {
            if at.id == exclude || superseded(&at.id) || (check && at.activity_id != aid) {
                continue;
            }
            rows.push((at.created_at.clone(), serde_json::json!({"role":"learner","kind":intent(&self.conn, &at.id)?,"text":clip(&at.response, 600),"same_task":at.activity_id == aid})));
        }
        for h in self.assistance(sid)? {
            if (check && h.activity_id != aid)
                || ["visual", "method_cue"].contains(&h.depth.as_str())
                || h.attempt_id
                    .as_deref()
                    .is_some_and(|a| superseded(a) || disputed.iter().any(|d| d == a))
            {
                continue;
            }
            rows.push((h.visible_at.clone(), serde_json::json!({"role":"tutor","kind":h.depth,"text":clip(&h.content, 600),"same_task":h.activity_id == aid})));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out: Vec<serde_json::Value> = rows.into_iter().map(|r| r.1).collect();
        if out.len() > 12 {
            out.drain(..out.len() - 12);
        }
        while out
            .iter()
            .map(|v| v["text"].as_str().map_or(0, str::len))
            .sum::<usize>()
            > 6000
            && !out.is_empty()
        {
            out.remove(0);
        }
        Ok(out)
    }

    /// Short evidence account for teaching requests: resolved, current, undisputed assessments only.
    pub fn evidence_summary(&self, sid: &str) -> Result<String> {
        let conditions: Vec<AttemptConditions> = many(
            &self.conn,
            "SELECT x.data FROM attempt_conditions x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=?",
            sid,
        )?;
        let grades: Vec<Assessment> = many(
            &self.conn,
            "SELECT x.data FROM assessments x JOIN attempts a ON a.id=x.attempt_id WHERE a.session_id=? ORDER BY x.rowid",
            sid,
        )?;
        let mut parts = vec![];
        for g in grades
            .iter()
            .filter(|g| !g.stale && g.dispute_state != "disputed" && g.uncertainty == "resolved")
        {
            let Some(c) = conditions.iter().find(|c| c.attempt_id == g.attempt_id) else {
                continue;
            };
            if c.intent == "question" {
                continue;
            }
            let findings = g
                .findings
                .iter()
                .map(|f| format!("{} {}", f.dimension, f.result))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("{}: {} ({findings})", c.task_kind, g.verdict));
        }
        if parts.len() > 5 {
            parts.drain(..parts.len() - 5);
        }
        Ok(clip(
            &if parts.is_empty() {
                "No resolved feedback in this session yet.".into()
            } else {
                format!(
                    "Resolved feedback in this session, oldest first: {}. Supported work is not independent evidence.",
                    parts.join("; ")
                )
            },
            900,
        ))
    }

    /// Chronological conversation projected from durable records. During an independent check or review, earlier
    /// tasks, answers and teaching are withheld here (one placeholder), matching what the tutor receives.
    pub fn timeline(&self, sid: &str) -> Result<Vec<TimelineEntry>> {
        let flow = self.learning_state(sid)?;
        let check = flow
            .as_ref()
            .is_some_and(|f| ["independent", "review"].contains(&f.stage.as_str()));
        let current = flow.as_ref().map(|f| f.active_activity_id.clone());
        let visible = |aid: &str| !check || current.as_deref() == Some(aid);
        let ats = self.attempts(sid)?;
        let mut rows: Vec<TimelineEntry> = vec![];
        let mut hidden = 0usize;
        let mut st = self.conn.prepare("SELECT a.data, t.data FROM activities a JOIN task_specs t ON t.activity_id=a.id WHERE a.session_id=? ORDER BY a.rowid").map_err(sql)?;
        let tasks = st
            .query_map([sid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql)?;
        for t in tasks {
            let (a, spec) = t.map_err(sql)?;
            let (a, spec): (Activity, TaskSpec) = (parse(a)?, parse(spec)?);
            if !visible(&a.id) {
                hidden += 1;
                continue;
            }
            rows.push(TimelineEntry {
                id: format!("task:{}", a.id),
                kind: "task".into(),
                at: spec.frozen_at.clone(),
                activity_id: a.id.clone(),
                stage: spec.kind.clone(),
                text: a.prompt.clone(),
                detail: stage_title(&spec.kind).into(),
                attempt_id: None,
                assistance_id: None,
                hidden: false,
            });
        }
        for at in &ats {
            if !visible(&at.activity_id) {
                hidden += 1;
                continue;
            }
            let kind = intent(&self.conn, &at.id)?;
            let revised = ats
                .iter()
                .any(|b| b.revision_of.as_deref() == Some(at.id.as_str()));
            rows.push(TimelineEntry {
                id: format!("attempt:{}", at.id),
                kind,
                at: at.created_at.clone(),
                activity_id: at.activity_id.clone(),
                stage: String::new(),
                text: at.response.clone(),
                detail: format!(
                    "{}{}{}",
                    if at.modality == "recorded_speech" {
                        "Confirmed transcript"
                    } else {
                        "Typed"
                    },
                    if at.revision_of.is_some() {
                        " · corrected version"
                    } else {
                        ""
                    },
                    if revised { " · later corrected" } else { "" }
                ),
                attempt_id: Some(at.id.clone()),
                assistance_id: None,
                hidden: false,
            });
        }
        for h in self.assistance(sid)? {
            if !visible(&h.activity_id) {
                hidden += 1;
                continue;
            }
            let kind = if h.depth == "visual" {
                "visual"
            } else if h.attempt_id.is_some() {
                "tutor"
            } else {
                "help"
            };
            rows.push(TimelineEntry {
                id: format!("help:{}", h.id),
                kind: kind.into(),
                at: h.visible_at.clone(),
                activity_id: h.activity_id.clone(),
                stage: String::new(),
                text: if h.depth == "visual" {
                    "The visual's result was shown. Later answers on this task count as supported."
                        .into()
                } else {
                    h.content.clone()
                },
                detail: help_label(&h.depth).into(),
                attempt_id: h.attempt_id.clone(),
                assistance_id: Some(h.id.clone()),
                hidden: false,
            });
        }
        // Stable: tasks first at equal times, then answers, then the help that followed them.
        rows.sort_by(|a, b| a.at.cmp(&b.at));
        // One reveal can authorize several exposures (stage and exploration scope); show one marker per run.
        rows.dedup_by(|later, earlier| {
            later.kind == "visual"
                && earlier.kind == "visual"
                && later.activity_id == earlier.activity_id
        });
        if hidden > 0 {
            rows.insert(0, TimelineEntry { id: "hidden".into(), kind: "hidden".into(), at: String::new(), activity_id: String::new(), stage: String::new(), text: "Earlier tasks, answers and teaching are hidden while you work on this check. They return when you finish.".into(), detail: String::new(), attempt_id: None, assistance_id: None, hidden: true });
        }
        Ok(rows)
    }
}

fn retitle(c: &Connection, sid: &str, title: &str) -> Result<()> {
    let mut s: Session = parse(
        c.query_row("SELECT data FROM sessions WHERE id=?", [sid], |r| r.get(0))
            .map_err(sql)?,
    )?;
    s.title = clip(title, 80);
    c.execute(
        "UPDATE sessions SET data=? WHERE id=?",
        params![json(&s)?, sid],
    )
    .map_err(sql)?;
    Ok(())
}

#[cfg(test)]
impl Store {
    pub fn recompute_for_test(&mut self, sid: &str) {
        recompute(&self.conn, sid, &self.time(), true).unwrap();
    }
}
