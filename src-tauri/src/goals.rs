//! Learner goals, plan revisions and capability progress. A goal's plan is a list of capabilities with stable IDs;
//! each capability is studied in its own session so tasks, evidence and reviews stay scoped to one objective. The
//! authored unit is one content source (an authored plan); a generated plan is another. Both drive the same engine.
use crate::{
    contracts::*,
    db::{Result, Store, id, json, parse, sql},
    generation::{self, PlanInput, PlanProposal, TaskInput, TaskProposal},
    learning::clip,
    registry::Purpose,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json as j};

pub const PLAN_SCHEMA: u32 = 1;

/// Objective data from the store: authored unit rows and generated capabilities alike.
pub fn objective_data(c: &Connection, oid: &str) -> Result<Value> {
    let raw: Option<String> = c
        .query_row("SELECT data FROM objectives WHERE id=?", [oid], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sql)?;
    raw.map(parse)
        .transpose()?
        .ok_or_else(|| "Unknown capability".into())
}
pub fn is_generated(c: &Connection, oid: &str) -> Result<bool> {
    Ok(c.query_row(
        "SELECT COALESCE(json_extract(data,'$.origin'),'')='generated' FROM objectives WHERE id=?",
        [oid],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)?
    .unwrap_or(false))
}
pub fn outcome(c: &Connection, oid: &str) -> String {
    objective_data(c, oid)
        .ok()
        .and_then(|o| o["outcome"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "Study activity".into())
}
/// A generated capability's planned visual. Authored objectives plan their visuals per task (`curriculum::visual_plan`).
pub fn capability_visual(c: &Connection, oid: &str) -> Option<PlanVisual> {
    objective_data(c, oid)
        .ok()
        .filter(|o| o["origin"] == "generated")
        .and_then(|o| serde_json::from_value(o["visual"].clone()).ok())
}
/// Lesson stages for a capability. Exploration and explaining a visual exist only when the capability has a visual.
pub fn route(c: &Connection, oid: &str) -> Vec<&'static str> {
    let visual = if is_generated(c, oid).unwrap_or(false) {
        capability_visual(c, oid).is_some()
    } else {
        crate::curriculum::objective(oid).is_ok()
    };
    [
        "diagnosis",
        "instruction",
        "worked_example",
        "completion",
        "guided",
        "exploration",
        "explanation",
        "independent",
    ]
    .into_iter()
    .filter(|s| visual || !["exploration", "explanation"].contains(s))
    .collect()
}
pub fn goal_of(c: &Connection, sid: &str) -> Result<Option<String>> {
    c.query_row(
        "SELECT goal_id FROM goal_sessions WHERE session_id=?",
        [sid],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)
}
fn latest_plan(c: &Connection, gid: &str) -> Result<PlanRevision> {
    parse(
        c.query_row(
            "SELECT data FROM plans WHERE goal_id=? ORDER BY revision DESC LIMIT 1",
            [gid],
            |r| r.get(0),
        )
        .map_err(sql)?,
    )
}
fn goal_record(c: &Connection, gid: &str) -> Result<GoalRecord> {
    parse(
        c.query_row("SELECT data FROM goals WHERE id=?", [gid], |r| r.get(0))
            .map_err(sql)?,
    )
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
fn insert_plan(c: &Connection, p: &PlanRevision) -> Result<()> {
    c.execute(
        "INSERT INTO plans VALUES (?,?,?)",
        params![p.goal_id, p.revision, json(p)?],
    )
    .map_err(sql)?;
    Ok(())
}

/// Passages for planning or task writing: the goal's chosen passages, then lexical matches restricted to the goal's
/// own sources, then the opening passages of each attached source so its material always reaches the planner.
pub fn goal_passages(
    c: &Connection,
    goal: &GoalRecord,
    query: &str,
    limit: usize,
) -> Result<Vec<SourcePassage>> {
    let mut out: Vec<SourcePassage> = vec![];
    let push = |out: &mut Vec<SourcePassage>, raw: String| -> Result<()> {
        let p: SourcePassage = parse(raw)?;
        if out.len() < limit && !out.iter().any(|x| x.id == p.id) {
            out.push(p);
        }
        Ok(())
    };
    for pid in &goal.passage_ids {
        if let Some(raw) = c
            .query_row("SELECT data FROM source_chunks WHERE id=?", [pid], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(sql)?
        {
            push(&mut out, raw)?;
        }
    }
    if goal.source_ids.is_empty() {
        return Ok(out);
    }
    let tokens = query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|s| s.chars().count() > 2)
        .take(16)
        .map(|s| format!("\"{}\"", s.chars().take(40).collect::<String>()))
        .collect::<Vec<_>>();
    if !tokens.is_empty() {
        let mut st = c
            .prepare("SELECT c.data, c.source_id FROM source_search f JOIN source_chunks c ON c.id=f.source_id WHERE source_search MATCH ? ORDER BY bm25(source_search) LIMIT 40")
            .map_err(sql)?;
        let rows = st
            .query_map([tokens.join(" OR ")], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql)?;
        for row in rows {
            let (raw, source) = row.map_err(sql)?;
            if goal.source_ids.contains(&source) {
                push(&mut out, raw)?;
            }
        }
    }
    for sid in &goal.source_ids {
        let mut st = c
            .prepare("SELECT data FROM source_chunks WHERE source_id=? ORDER BY ordinal LIMIT 2")
            .map_err(sql)?;
        let rows: Vec<String> = st
            .query_map([sid], |r| r.get(0))
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)?;
        for raw in rows {
            push(&mut out, raw)?;
        }
    }
    Ok(out)
}

/// Everything needed to commit a plan request once the provider replies.
pub struct PlanContext {
    pub session_id: String,
    pub goal_id: String,
    pub new_goal: bool,
    pub goal_text: String,
    pub source_ids: Vec<String>,
    pub instruction: String,
    pub reason: String,
    pub passage_ids: Vec<String>,
    /// The model the plan request was frozen with, named in the plan's provenance.
    pub model: &'static str,
}

/// Everything needed to commit a generated task once the provider replies.
pub struct TaskPlan {
    pub session_id: String,
    pub goal_id: String,
    pub objective_id: String,
    /// "start", "check", or a transition action ("teach", "next", "fresh_check", "review", "another").
    pub action: String,
    pub stage: String,
    pub note: String,
    pub content_version: String,
    pub prior_tasks: Vec<String>,
    /// The model the task request was frozen with, named in the task's provenance.
    pub model: &'static str,
}

impl Store {
    pub fn goal_id(&self, sid: &str) -> Result<Option<String>> {
        goal_of(&self.conn, sid)
    }
    pub fn plan_revision(&self, gid: &str) -> Result<PlanRevision> {
        latest_plan(&self.conn, gid)
    }

    /// The latest session that studies `oid` within the goal, with its learning state.
    fn capability_session(
        &self,
        gid: &str,
        oid: &str,
    ) -> Result<Option<(String, Option<LearningState>)>> {
        let sid: Option<String> = self
            .conn
            .query_row(
                "SELECT g.session_id FROM goal_sessions g JOIN session_objectives o ON o.session_id=g.session_id WHERE g.goal_id=? AND o.objective_id=? ORDER BY g.rowid DESC LIMIT 1",
                params![gid, oid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        Ok(match sid {
            Some(s) => {
                let flow = self.learning_state(&s)?;
                Some((s, flow))
            }
            None => None,
        })
    }

    /// Plain-language progress for one capability from its session's primary records. Labels follow the
    /// specification: introduced, completed with help, shown on your own, shown again later, needs a fresh check.
    fn progress(&self, gid: &str, cap: &PlanCapability) -> Result<CapabilityProgress> {
        let found = self.capability_session(gid, &cap.objective_id)?;
        let (label, detail, session) = match (&found, cap.status.as_str()) {
            (None, "skipped") => ("Skipped".to_owned(), cap.note.clone(), None),
            (None, "retired") => ("Removed from the plan".to_owned(), cap.note.clone(), None),
            (None, _) => ("Not started".to_owned(), String::new(), None),
            (Some((sid, flow)), _) => {
                let summary: Option<LearningSummary> = self
                    .conn
                    .query_row(
                        "SELECT data FROM learning_summaries WHERE session_id=?",
                        [sid],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql)?
                    .map(parse)
                    .transpose()?;
                let help: i64 = self
                    .conn
                    .query_row(
                        "SELECT COUNT(*) FROM assistance WHERE session_id=? AND json_extract(data,'$.depth') IN ('explain','example','hint','worked_example','prerequisite')",
                        [sid],
                        |r| r.get(0),
                    )
                    .map_err(sql)?;
                let review: Option<Review> = self
                    .conn
                    .query_row(
                        "SELECT data FROM reviews WHERE json_extract(data,'$.session_id')=? AND json_extract(data,'$.status')='scheduled' ORDER BY rowid DESC LIMIT 1",
                        [sid],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql)?
                    .map(parse)
                    .transpose()?;
                let due = review.as_ref().is_some_and(|r| {
                    chrono::DateTime::parse_from_rfc3339(&r.due_at)
                        .is_ok_and(|d| d.with_timezone(&chrono::Utc) <= self.clock.now())
                });
                let stage = flow.as_ref().map(|f| f.stage.clone()).unwrap_or_default();
                let simulated = summary.as_ref().is_some_and(|s| s.simulated);
                let label = match &summary {
                    _ if due => "Needs a fresh check".to_owned(),
                    Some(s) if s.delayed_count > 0 && !s.stale => "Shown again later".to_owned(),
                    Some(s) if s.independent_count > 0 && !s.stale => {
                        "Shown on your own".to_owned()
                    }
                    Some(s) if s.supported_count > 0 && !s.stale => {
                        "Completed with help".to_owned()
                    }
                    _ if stage == "summary" => "Checked · not yet shown on your own".to_owned(),
                    _ if !stage.is_empty() => format!(
                        "In progress · {}",
                        crate::learning::stage_title(&stage).to_lowercase()
                    ),
                    _ if help > 0 => "Introduced".to_owned(),
                    _ => "Not started".to_owned(),
                };
                let mut detail = vec![];
                if let Some(s) = &summary
                    && !s.stale
                {
                    if s.independent_count > 0 {
                        detail.push(format!("{} independent check(s)", s.independent_count));
                    }
                    if s.delayed_count > 0 {
                        detail.push(format!("{} later review(s)", s.delayed_count));
                    }
                }
                if let Some(r) = &review {
                    detail.push(format!(
                        "review {} {}",
                        if due { "ready since" } else { "planned for" },
                        r.due_at.get(..10).unwrap_or(&r.due_at)
                    ));
                }
                if simulated {
                    detail.push("simulated evidence".into());
                }
                (label, detail.join(" · "), Some(sid.clone()))
            }
        };
        Ok(CapabilityProgress {
            objective_id: cap.objective_id.clone(),
            outcome: cap.outcome.clone(),
            why: cap.why.clone(),
            status: cap.status.clone(),
            minutes: cap.minutes,
            assessment: cap.assessment.clone(),
            origin: cap.origin.clone(),
            prerequisites: cap.prerequisites.clone(),
            visual: cap.visual.as_ref().map(|v| {
                crate::visualizer::installed()["templates"]
                    .as_array()
                    .and_then(|t| {
                        t.iter()
                            .find(|e| e["template"]["templateId"] == v.template_id.as_str())
                    })
                    .and_then(|e| e["title"].as_str())
                    .unwrap_or("Interactive visual")
                    .to_owned()
            }),
            session_id: session,
            label,
            detail,
        })
    }

    pub fn goal_view(&self, sid: &str) -> Result<Option<GoalView>> {
        let Some(gid) = goal_of(&self.conn, sid)? else {
            return Ok(None);
        };
        let goal = goal_record(&self.conn, &gid)?;
        let plan = latest_plan(&self.conn, &gid)?;
        let mut capabilities = vec![];
        for cap in &plan.capabilities {
            capabilities.push(self.progress(&gid, cap)?);
        }
        let current = crate::curriculum::selected_if_any(&self.conn, sid)?;
        let next = capabilities
            .iter()
            .filter(|c| c.status == "planned")
            .find(|c| {
                let mine = current.as_deref() == Some(c.objective_id.as_str());
                !mine
                    && ![
                        "Shown on your own",
                        "Shown again later",
                        "Completed with help",
                        "Checked · not yet shown on your own",
                    ]
                    .contains(&c.label.as_str())
            })
            .map(|c| c.objective_id.clone());
        let mut sources = vec![];
        for source in &goal.source_ids {
            let doc: Option<String> = self
                .conn
                .query_row("SELECT data FROM sources WHERE id=?", [source], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(sql)?;
            if let Some(doc) = doc {
                let doc: SourceDocument = parse(doc)?;
                let first: Option<String> = self
                    .conn
                    .query_row("SELECT json_extract(data,'$.text') FROM source_chunks WHERE source_id=? ORDER BY ordinal LIMIT 1", [source], |r| r.get(0))
                    .optional()
                    .map_err(sql)?;
                sources.push(GoalSource {
                    id: doc.id,
                    title: doc.title,
                    kind: doc.kind,
                    warnings: doc.warnings,
                    excerpt: clip(&first.unwrap_or_default(), 400),
                });
            }
        }
        Ok(Some(GoalView {
            id: gid,
            title: plan.title.clone(),
            goal_text: plan.goal_text.clone(),
            summary: plan.summary.clone(),
            level: plan.level.clone(),
            background: plan.background.clone(),
            revision: plan.revision,
            origin: plan.origin.clone(),
            provenance: plan.provenance.clone(),
            clarification: plan.clarification.clone(),
            minutes: plan
                .capabilities
                .iter()
                .filter(|c| c.status == "planned")
                .map(|c| c.minutes)
                .sum(),
            capabilities,
            sources,
            source_note: plan.source_note.clone(),
            notes: plan.notes.clone(),
            reason: plan.reason.clone(),
            next_objective_id: next,
        }))
    }

    pub fn goals_list(&self) -> Result<Vec<GoalSummary>> {
        let mut st = self
            .conn
            .prepare("SELECT id FROM goals ORDER BY rowid DESC")
            .map_err(sql)?;
        let ids: Vec<String> = st
            .query_map([], |r| r.get(0))
            .map_err(sql)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql)?;
        let mut out = vec![];
        for gid in ids {
            let goal = goal_record(&self.conn, &gid)?;
            let plan = latest_plan(&self.conn, &gid)?;
            let mut st = self
                .conn
                .prepare("SELECT session_id FROM goal_sessions WHERE goal_id=? ORDER BY rowid")
                .map_err(sql)?;
            let session_ids: Vec<String> = st
                .query_map([&gid], |r| r.get(0))
                .map_err(sql)?
                .collect::<std::result::Result<_, _>>()
                .map_err(sql)?;
            let mut checked = 0;
            for cap in plan.capabilities.iter().filter(|c| c.status == "planned") {
                if self.progress(&gid, cap)?.label.starts_with("Shown") {
                    checked += 1;
                }
            }
            out.push(GoalSummary {
                id: gid,
                title: plan.title,
                created_at: goal.created_at,
                session_ids,
                capabilities: plan
                    .capabilities
                    .iter()
                    .filter(|c| c.status == "planned")
                    .count() as u32,
                checked,
            });
        }
        Ok(out)
    }

    /// Validates a planning request and reserves it. `goal_id` is set for a revision of an existing plan.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_plan(
        &mut self,
        sid: &str,
        revision: i32,
        goal_text: &str,
        source_ids: &[String],
        instruction: &str,
        goal_id: Option<&str>,
    ) -> Result<(PlanInput, PlanContext)> {
        let session = self.session(sid)?;
        if session.revision != revision {
            return Err("Session changed. Refresh before planning.".into());
        }
        if instruction.chars().count() > 1500 {
            return Err("Keep the change request under 1,500 characters.".into());
        }
        let (gid, new_goal, text, sources, prior, passage_ids) = match goal_id {
            Some(gid) => {
                if goal_of(&self.conn, sid)?.as_deref() != Some(gid) {
                    return Err("That plan belongs to another session.".into());
                }
                valid_text(instruction, 1500)?;
                let goal = goal_record(&self.conn, gid)?;
                let plan = latest_plan(&self.conn, gid)?;
                if plan.origin == "authored" {
                    return Err("The sample unit's plan is authored and fixed. Start your own goal to plan freely.".into());
                }
                (
                    gid.to_owned(),
                    false,
                    plan.goal_text.clone(),
                    goal.source_ids.clone(),
                    Some(plan),
                    goal.passage_ids,
                )
            }
            None => {
                valid_text(goal_text, 4000)?;
                if goal_of(&self.conn, sid)?.is_some() || self.learning_state(sid)?.is_some() {
                    return Err(
                        "This session already has a goal. Start a new session for another topic."
                            .into(),
                    );
                }
                if self.pending_transcript(sid)?.is_some() {
                    return Err("Confirm or discard the pending transcript first.".into());
                }
                if source_ids.len() > 6 {
                    return Err("Attach at most six sources to one goal.".into());
                }
                for s in source_ids {
                    let exists: bool = self
                        .conn
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM sources WHERE id=?)",
                            [s],
                            |r| r.get(0),
                        )
                        .map_err(sql)?;
                    if !exists {
                        return Err("An attached source is no longer available.".into());
                    }
                }
                let passages: Vec<String> = {
                    let mut st = self
                        .conn
                        .prepare("SELECT chunk_id FROM source_selections WHERE session_id=? ORDER BY rowid")
                        .map_err(sql)?;
                    st.query_map([sid], |r| r.get(0))
                        .map_err(sql)?
                        .collect::<std::result::Result<_, _>>()
                        .map_err(sql)?
                };
                (
                    id(),
                    true,
                    goal_text.trim().to_owned(),
                    source_ids.to_vec(),
                    None,
                    passages,
                )
            }
        };
        let record = GoalRecord {
            schema_version: 1,
            id: gid.clone(),
            text: text.clone(),
            created_at: self.time(),
            origin: "generated".into(),
            source_ids: sources.clone(),
            passage_ids: passage_ids.clone(),
        };
        let passages = goal_passages(&self.conn, &record, &format!("{text} {instruction}"), 6)?;
        let progress = match &prior {
            Some(p) => {
                let mut out = vec![];
                for cap in &p.capabilities {
                    let pr = self.progress(&gid, cap)?;
                    if pr.session_id.is_some() {
                        out.push(j!({"objective_id":cap.objective_id,"outcome":cap.outcome,"progress":pr.label}));
                    }
                }
                out
            }
            None => vec![],
        };
        let mut input = PlanInput {
            request: Request {
                schema_version: 1,
                id: String::new(),
                session_id: sid.into(),
                attempt_id: None,
                session_revision: revision,
                kind: "plan".into(),
                status: "pending".into(),
                reserved_microusd: 0,
                created_at: self.time(),
            },
            choice: self.model_choice()?,
            goal_text: text.clone(),
            instruction: instruction.trim().into(),
            prior,
            progress,
            passages,
        };
        let amount = generation::reservation(
            &generation::plan_payload(&input),
            &input.choice,
            Purpose::Plan,
        )?;
        input.request = self.reserve_with(
            sid,
            None,
            "plan",
            if self.offline { 0 } else { amount },
            &input.passages,
            &input.choice,
        )?;
        let used: Vec<String> = input.passages.iter().map(|p| p.id.clone()).collect();
        let model = input.choice.model.id;
        Ok((
            input,
            PlanContext {
                session_id: sid.into(),
                goal_id: gid,
                new_goal,
                goal_text: text,
                source_ids: sources,
                reason: if new_goal {
                    "First plan for this goal.".into()
                } else {
                    format!("Revised at your request: “{}”", clip(instruction, 200))
                },
                instruction: instruction.trim().into(),
                passage_ids: used,
                model,
            },
        ))
    }

    /// Records a request's outcome on its own, keeping its usage when nothing else is committed.
    pub fn close_only(&mut self, r: &Request, status: &str, usage: Usage) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        crate::db::close_request(&tx, r, status, &usage)?;
        tx.commit().map_err(sql)
    }

    /// Settles a planning request. When the proposal validates against current state, the goal and a new frozen plan
    /// revision commit in the same transaction as the request's usage; otherwise only the usage is kept.
    pub fn commit_plan(
        &mut self,
        ctx: &PlanContext,
        r: &Request,
        status: &str,
        usage: Usage,
        proposal: Option<&PlanProposal>,
    ) -> Result<()> {
        let Some(p) = proposal else {
            return self.close_only(r, status, usage);
        };
        let validated = match generation::validate_plan(p) {
            Ok(v) => v,
            Err(e) => {
                self.close_only(r, "rejected", usage)?;
                return Err(e);
            }
        };
        if self.session(&ctx.session_id)?.revision != r.session_revision {
            self.close_only(r, "stale", usage)?;
            return Err("The session changed while the plan was being drafted. Nothing was changed; draft it again.".into());
        }
        let time = self.time();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        crate::db::close_request(&tx, r, "completed", &usage)?;
        match self.apply_plan(&tx, ctx, r, validated, p, &time) {
            Ok(()) => tx.commit().map_err(sql),
            Err(e) => {
                drop(tx);
                self.close_only(r, "rejected", usage)?;
                Err(e)
            }
        }
    }

    fn apply_plan(
        &self,
        tx: &Connection,
        ctx: &PlanContext,
        r: &Request,
        (capabilities, dropped): (Vec<generation::CapabilityProposal>, Vec<String>),
        p: &PlanProposal,
        time: &str,
    ) -> Result<()> {
        let prior = if ctx.new_goal {
            None
        } else {
            Some(latest_plan(tx, &ctx.goal_id)?)
        };
        if ctx.new_goal {
            let goal = GoalRecord {
                schema_version: 1,
                id: ctx.goal_id.clone(),
                text: ctx.goal_text.clone(),
                created_at: time.into(),
                origin: "generated".into(),
                source_ids: ctx.source_ids.clone(),
                passage_ids: ctx.passage_ids.clone(),
            };
            tx.execute(
                "INSERT INTO goals VALUES (?,?)",
                params![goal.id, json(&goal)?],
            )
            .map_err(sql)?;
            tx.execute(
                "INSERT INTO goal_sessions VALUES (?,?)",
                params![ctx.session_id, ctx.goal_id],
            )
            .map_err(sql)?;
        }
        let revision = prior.as_ref().map(|p| p.revision + 1).unwrap_or(1);
        let mut caps: Vec<PlanCapability> = vec![];
        let mut keys: Vec<(String, String)> = vec![];
        for c in &capabilities {
            // A kept capability must keep its exact outcome; any change is a new capability with its own evidence.
            let kept = c.keep.as_ref().and_then(|k| {
                prior.as_ref().and_then(|p| {
                    p.capabilities
                        .iter()
                        .find(|x| &x.objective_id == k && x.outcome == c.outcome)
                })
            });
            let oid = match kept {
                Some(k) => k.objective_id.clone(),
                None => {
                    let oid = format!("cap-{}", id());
                    let data = j!({"schema_version":1,"id":oid,"outcome":c.outcome,"why":c.why,"subject":p.title,"origin":"generated","goal_id":ctx.goal_id,"plan_revision":revision,"assessment":c.assessment,"minutes":c.minutes,"visual":c.visual,"level":p.level,"validation":{"status":"Generated by the tutor model; not reviewed by a person."},"provenance":format!("Plan request {}", r.id)});
                    tx.execute(
                        "INSERT INTO objectives VALUES (?,?)",
                        params![oid, data.to_string()],
                    )
                    .map_err(sql)?;
                    oid
                }
            };
            keys.push((c.key.clone(), oid.clone()));
            caps.push(PlanCapability {
                objective_id: oid,
                key: c.key.clone(),
                outcome: c.outcome.clone(),
                why: c.why.clone(),
                status: "planned".into(),
                minutes: c.minutes as u32,
                assessment: c.assessment.clone(),
                origin: "generated".into(),
                prerequisites: c
                    .prerequisites
                    .iter()
                    .filter_map(|k| {
                        keys.iter()
                            .find(|(key, _)| key == k)
                            .map(|(_, o)| o.clone())
                    })
                    .collect(),
                visual: c.visual.clone(),
                note: String::new(),
            });
        }
        // Work already recorded is never dropped silently: a removed capability with a session is retained.
        if let Some(prior) = &prior {
            for old in &prior.capabilities {
                if !caps.iter().any(|c| c.objective_id == old.objective_id)
                    && self
                        .capability_session(&ctx.goal_id, &old.objective_id)?
                        .is_some()
                {
                    let mut kept = old.clone();
                    kept.status = "retired".into();
                    kept.note =
                        "Removed by a revision; its recorded work and reviews are kept.".into();
                    caps.push(kept);
                }
            }
        }
        let mut notes = p.notes.trim().to_owned();
        for d in dropped {
            notes = format!("{notes} {d}").trim().to_owned();
        }
        let plan = PlanRevision {
            schema_version: PLAN_SCHEMA,
            goal_id: ctx.goal_id.clone(),
            revision,
            title: clip(&p.title, 120),
            goal_text: ctx.goal_text.clone(),
            summary: p.summary.trim().into(),
            level: p.level.trim().into(),
            background: p
                .background
                .iter()
                .map(|b| b.trim().to_owned())
                .filter(|b| !b.is_empty())
                .collect(),
            capabilities: caps,
            clarification: p
                .clarification
                .clone()
                .filter(|c| !c.question.trim().is_empty()),
            reason: ctx.reason.clone(),
            origin: "generated".into(),
            provenance: format!(
                "Generated by {} with prompt {} (request {}). Not reviewed by a person.",
                if r.kind.starts_with("offline:") {
                    "the offline development fixture"
                } else {
                    ctx.model
                },
                generation::PLAN_PROMPT_VERSION,
                r.id
            ),
            request_id: Some(r.id.clone()),
            source_passage_ids: ctx.passage_ids.clone(),
            source_note: p.source_use.trim().into(),
            notes,
            created_at: time.into(),
        };
        insert_plan(tx, &plan)?;
        if ctx.new_goal {
            let mut s = self.session(&ctx.session_id)?;
            s.draft.clear();
            s.title = clip(&plan.title, 80);
            tx.execute(
                "UPDATE sessions SET data=? WHERE id=?",
                params![json(&s)?, s.id],
            )
            .map_err(sql)?;
        }
        bump(tx, &ctx.session_id)
    }

    /// Learner plan decisions that need no model: skip a capability, restore it, or answer nothing. Each is a new
    /// frozen revision so the plan's history stays inspectable.
    pub fn plan_decision(
        &mut self,
        sid: &str,
        revision: i32,
        gid: &str,
        oid: &str,
        decision: &str,
    ) -> Result<()> {
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh first.".into());
        }
        if goal_of(&self.conn, sid)?.as_deref() != Some(gid) {
            return Err("That plan belongs to another session.".into());
        }
        let mut plan = latest_plan(&self.conn, gid)?;
        let cap = plan
            .capabilities
            .iter_mut()
            .find(|c| c.objective_id == oid)
            .ok_or("That capability is not in the plan.")?;
        let (status, reason) = match decision {
            "skip" => (
                "skipped",
                format!("Skipped at your request: {}", clip(&cap.outcome, 120)),
            ),
            "restore" => (
                "planned",
                format!("Restored to the plan: {}", clip(&cap.outcome, 120)),
            ),
            _ => return Err("Unknown plan decision.".into()),
        };
        if cap.status == status {
            return Ok(());
        }
        cap.status = status.into();
        cap.note = if status == "skipped" {
            "Skipped; nothing about it is recorded as known. “I know this — check me” gives evidence instead.".into()
        } else {
            String::new()
        };
        plan.revision += 1;
        plan.reason = reason;
        plan.created_at = self.time();
        plan.request_id = None;
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        insert_plan(&tx, &plan)?;
        bump(&tx, sid)?;
        tx.commit().map_err(sql)
    }

    /// The authored unit as a content source: the chosen objective and up to two prerequisites form an authored
    /// plan, and the chosen objective starts at once in this session. No model call is needed.
    pub fn start_authored_goal(&mut self, sid: &str, revision: i32, oid: &str) -> Result<()> {
        let o = crate::curriculum::objective(oid)?;
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh.".into());
        }
        if goal_of(&self.conn, sid)?.is_some() || self.learning_state(sid)?.is_some() {
            return Err("This session already has a goal.".into());
        }
        if !self.session(sid)?.draft.trim().is_empty() && self.pending_transcript(sid)?.is_none() {
            self.save_draft(sid, "")?;
        }
        let mut chain: Vec<String> = vec![oid.into()];
        let mut current = o;
        while chain.len() < 3 {
            let Some(p) = current["prerequisites"]
                .as_array()
                .and_then(|p| p.last())
                .and_then(Value::as_str)
            else {
                break;
            };
            chain.insert(0, p.into());
            current = crate::curriculum::objective(p)?;
        }
        let gid = id();
        let time = self.time();
        let unit = crate::curriculum::unit();
        let capabilities = chain
            .iter()
            .enumerate()
            .map(|(i, cid)| {
                let o = crate::curriculum::objective(cid).expect("authored objective");
                PlanCapability {
                    objective_id: cid.clone(),
                    key: cid.clone(),
                    outcome: o["outcome"].as_str().unwrap_or_default().into(),
                    why: if cid == oid {
                        "The capability you chose.".into()
                    } else {
                        "A prerequisite in the authored unit; check it quickly or skip it.".into()
                    },
                    status: "planned".into(),
                    minutes: if cid == oid { 25 } else { 10 },
                    assessment: "derivation".into(),
                    origin: "authored".into(),
                    prerequisites: if i == 0 {
                        vec![]
                    } else {
                        vec![chain[i - 1].clone()]
                    },
                    visual: o["visual_template"].as_str().map(|t| PlanVisual {
                        template_id: t.into(),
                        parameters: vec![],
                        purpose: "Authored visual activity".into(),
                    }),
                    note: String::new(),
                }
            })
            .collect::<Vec<_>>();
        let plan = PlanRevision {
            schema_version: PLAN_SCHEMA,
            goal_id: gid.clone(),
            revision: 1,
            title: unit["title"].as_str().unwrap_or("Sample unit").into(),
            goal_text: o["outcome"].as_str().unwrap_or_default().into(),
            summary: "A short authored sequence from the sample unit. It works without an API key for tasks, hints and examples; tutor feedback needs a key.".into(),
            level: "Calculus-based, introductory university".into(),
            background: vec!["Derivatives of polynomials".into()],
            capabilities,
            clarification: None,
            reason: "Authored sample plan.".into(),
            origin: "authored".into(),
            provenance: crate::curriculum::AUTHORED_PROVENANCE.into(),
            request_id: None,
            source_passage_ids: vec![],
            source_note: String::new(),
            notes: unit["provisional_notice"].as_str().unwrap_or_default().into(),
            created_at: time.clone(),
        };
        let goal = GoalRecord {
            schema_version: 1,
            id: gid.clone(),
            text: plan.goal_text.clone(),
            created_at: time,
            origin: "authored".into(),
            source_ids: vec![],
            passage_ids: vec![],
        };
        {
            let tx = self.conn.unchecked_transaction().map_err(sql)?;
            tx.execute("INSERT INTO goals VALUES (?,?)", params![gid, json(&goal)?])
                .map_err(sql)?;
            tx.execute("INSERT INTO goal_sessions VALUES (?,?)", params![sid, gid])
                .map_err(sql)?;
            insert_plan(&tx, &plan)?;
            tx.commit().map_err(sql)?;
        }
        let goal_text = plan.goal_text.clone();
        self.start_goal_for(sid, self.session(sid)?.revision, &goal_text, oid)
    }

    /// The session in which a plan capability is studied: its existing session, the current session if it is this
    /// goal's still-empty session, or a new session linked to the goal.
    pub fn session_for_capability(&mut self, sid: &str, gid: &str, oid: &str) -> Result<String> {
        if goal_of(&self.conn, sid)?.as_deref() != Some(gid) {
            return Err("That plan belongs to another session.".into());
        }
        let plan = latest_plan(&self.conn, gid)?;
        let cap = plan
            .capabilities
            .iter()
            .find(|c| c.objective_id == oid)
            .ok_or("That capability is not in the plan.")?;
        if let Some((existing, _)) = self.capability_session(gid, oid)? {
            return Ok(existing);
        }
        if self.learning_state(sid)?.is_none()
            && crate::curriculum::selected_if_any(&self.conn, sid)?.is_none()
        {
            return Ok(sid.into());
        }
        // Reuse an empty linked session left by an earlier failed start before creating another.
        let empty: Option<String> = self
            .conn
            .query_row(
                "SELECT g.session_id FROM goal_sessions g WHERE g.goal_id=? AND NOT EXISTS(SELECT 1 FROM learning_sessions l WHERE l.session_id=g.session_id) AND NOT EXISTS(SELECT 1 FROM session_objectives o WHERE o.session_id=g.session_id) ORDER BY g.rowid DESC LIMIT 1",
                [gid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(e) = empty {
            return Ok(e);
        }
        let new = self.new_session(&clip(&cap.outcome, 80))?;
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        tx.execute("INSERT INTO goal_sessions VALUES (?,?)", params![new, gid])
            .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(new)
    }

    /// Validates a step that needs a newly written task for a generated capability and reserves the request.
    /// `action` is "start" or "check" for a capability not yet begun in `sid`, otherwise a transition action.
    pub fn prepare_task(
        &mut self,
        sid: &str,
        revision: i32,
        action: &str,
        objective: Option<&str>,
        note: &str,
    ) -> Result<(TaskInput, TaskPlan)> {
        if self.session(sid)?.revision != revision {
            return Err("Session changed. Refresh before continuing.".into());
        }
        if note.chars().count() > 1000 {
            return Err("Keep the problem report under 1,000 characters.".into());
        }
        let gid = goal_of(&self.conn, sid)?.ok_or("This session has no study plan.")?;
        let plan = latest_plan(&self.conn, &gid)?;
        let (oid, stage) = match action {
            "start" | "check" => {
                if self.learning_state(sid)?.is_some() {
                    return Err("This capability has already started here.".into());
                }
                if self.pending_transcript(sid)?.is_some()
                    || !self.session(sid)?.draft.trim().is_empty()
                {
                    return Err("Save or discard the current draft first.".into());
                }
                let oid = objective.ok_or("Choose a capability.")?.to_owned();
                let cap = plan
                    .capabilities
                    .iter()
                    .find(|c| c.objective_id == oid)
                    .ok_or("That capability is not in the plan.")?;
                if cap.status == "retired" {
                    return Err("That capability was removed from the plan; its earlier work is kept in its session.".into());
                }
                (
                    oid,
                    if action == "check" {
                        "independent".to_owned()
                    } else {
                        "diagnosis".to_owned()
                    },
                )
            }
            _ => {
                let stage = self
                    .transition_target(sid, action)?
                    .ok_or("That action does not need a new task.")?;
                (crate::curriculum::selected(&self.conn, sid)?, stage)
            }
        };
        if !is_generated(&self.conn, &oid)? {
            return Err("Authored capabilities derive their tasks locally.".into());
        }
        let cap_data = objective_data(&self.conn, &oid)?;
        let goal = goal_record(&self.conn, &gid)?;
        let prior_tasks: Vec<String> = {
            let mut st = self
                .conn
                .prepare("SELECT json_extract(data,'$.prompt') FROM activities WHERE objective_id=? ORDER BY rowid DESC LIMIT 10")
                .map_err(sql)?;
            st.query_map([&oid], |r| r.get::<_, String>(0))
                .map_err(sql)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(sql)?
        };
        // Teaching stages see the learner's recent work; fresh checks and reviews are written without it.
        let learner = if ["independent", "review"].contains(&stage.as_str())
            || action == "start"
            || action == "check"
        {
            vec![]
        } else {
            self.teaching_context(sid)?
        };
        let visual = capability_visual(&self.conn, &oid)
            .filter(|_| ["diagnosis", "exploration", "explanation"].contains(&stage.as_str()));
        let visual_json = visual.as_ref().and_then(|v| {
            crate::visualizer::planner_catalog()
                .as_array()
                .and_then(|t| {
                    t.iter()
                        .find(|e| e["template_id"] == v.template_id.as_str())
                        .cloned()
                })
                .map(|mut e| {
                    e["plan_parameters"] = j!(v.parameters);
                    e["plan_purpose"] = j!(v.purpose);
                    e
                })
        });
        let passages = goal_passages(
            &self.conn,
            &goal,
            cap_data["outcome"].as_str().unwrap_or_default(),
            4,
        )?;
        let plan_json = j!({"title":plan.title,"level":plan.level,"background":plan.background,"capabilities":plan.capabilities.iter().filter(|c| c.status == "planned").map(|c| j!({"outcome":c.outcome,"this_one":c.objective_id == oid})).collect::<Vec<_>>()});
        let capability = j!({"outcome":cap_data["outcome"],"why":cap_data["why"],"assessment":cap_data["assessment"],"prerequisites":plan.capabilities.iter().find(|c| c.objective_id == oid).map(|c| c.prerequisites.iter().map(|p| outcome(&self.conn, p)).collect::<Vec<_>>()).unwrap_or_default()});
        let mut input = TaskInput {
            request: Request {
                schema_version: 1,
                id: String::new(),
                session_id: sid.into(),
                attempt_id: None,
                session_revision: revision,
                kind: "task".into(),
                status: "pending".into(),
                reserved_microusd: 0,
                created_at: self.time(),
            },
            choice: self.model_choice()?,
            stage: stage.clone(),
            goal_text: plan.goal_text.clone(),
            plan: plan_json,
            capability,
            note: note.trim().into(),
            prior_tasks: prior_tasks.iter().map(|t| clip(t, 400)).collect(),
            learner,
            passages,
            visual: visual_json,
        };
        let amount = generation::reservation(
            &generation::task_payload(&input),
            &input.choice,
            Purpose::Task,
        )?;
        input.request = self.reserve_with(
            sid,
            None,
            "task",
            if self.offline { 0 } else { amount },
            &input.passages,
            &input.choice,
        )?;
        let model = input.choice.model.id;
        Ok((
            input,
            TaskPlan {
                session_id: sid.into(),
                goal_id: gid.clone(),
                objective_id: oid,
                action: action.into(),
                stage,
                note: note.trim().into(),
                content_version: format!("generated:{gid}:plan-r{}", plan.revision),
                prior_tasks,
                model,
            },
        ))
    }

    /// The learner's latest answers, their resolved feedback and questions in this session, for teaching stages.
    fn teaching_context(&self, sid: &str) -> Result<Vec<Value>> {
        let mut out = vec![];
        let attempts = self.attempts(sid)?;
        for at in attempts
            .iter()
            .rev()
            .filter(|a| {
                !attempts
                    .iter()
                    .any(|b| b.revision_of.as_deref() == Some(a.id.as_str()))
            })
            .take(3)
        {
            let kind = crate::learning::intent(&self.conn, &at.id)?;
            let grade = self
                .conn
                .query_row(
                    "SELECT data FROM assessments WHERE attempt_id=? ORDER BY rowid DESC LIMIT 1",
                    [&at.id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(sql)?
                .map(parse::<Assessment>)
                .transpose()?
                .filter(|g| !g.stale && g.dispute_state != "disputed");
            out.push(j!({"kind":kind,"text":clip(&at.response, 700),"verdict":grade.as_ref().map(|g| g.verdict.clone()),"error_kind":grade.as_ref().map(|g| g.error_kind.clone()),"findings":grade.as_ref().map(|g| g.findings.iter().map(|f| format!("{} {}", f.dimension, f.result)).collect::<Vec<_>>())}));
        }
        out.reverse();
        Ok(out)
    }

    /// Settles a task request. When the proposal passes validation and every deterministic check, the task is
    /// frozen and the step taken in the same transaction as the request's usage. A failed check shows nothing.
    pub fn commit_task(
        &mut self,
        plan: &TaskPlan,
        r: &Request,
        status: &str,
        usage: Usage,
        proposal: Option<TaskProposal>,
    ) -> Result<()> {
        let Some(p) = proposal else {
            return self.close_only(r, status, usage);
        };
        let ctx = generation::TaskContext {
            objective_id: &plan.objective_id,
            stage: &plan.stage,
            prior_tasks: &plan.prior_tasks,
            request: r,
            model: plan.model,
            content_version: plan.content_version.clone(),
            visual: capability_visual(&self.conn, &plan.objective_id),
        };
        let material = match generation::check_task(p, &ctx) {
            Ok(m) => m,
            Err(e) => {
                self.close_only(r, "rejected", usage)?;
                return Err(e);
            }
        };
        if self.session(&plan.session_id)?.revision != r.session_revision {
            self.close_only(r, "stale", usage)?;
            return Err("The session changed while the task was being written. Nothing was changed; try the step again.".into());
        }
        let time = self.time();
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        crate::db::close_request(&tx, r, "completed", &usage)?;
        if !plan.note.is_empty() {
            tx.execute(
                "INSERT INTO learning_events VALUES (?,?,?,?,?)",
                params![
                    id(),
                    plan.session_id,
                    "task_reported",
                    time,
                    j!({"note":plan.note,"request_id":r.id}).to_string()
                ],
            )
            .map_err(sql)?;
        }
        let applied = match plan.action.as_str() {
            "start" | "check" => self.begin_generated(&tx, plan, material, &time),
            action => self.apply_step(&tx, &plan.session_id, action, Some(material), &time),
        };
        match applied {
            Ok(()) => tx.commit().map_err(sql),
            Err(e) => {
                drop(tx);
                self.close_only(r, "rejected", usage)?;
                Err(e)
            }
        }
    }

    /// Whether `action` in `sid` needs a newly written task (a transition for a generated capability).
    pub fn needs_generation(&self, sid: &str, action: &str) -> Result<bool> {
        if self.learning_state(sid)?.is_none() {
            return Ok(false);
        }
        Ok(match self.transition_target(sid, action)? {
            Some(target) => {
                target != "summary"
                    && is_generated(&self.conn, &crate::curriculum::selected(&self.conn, sid)?)?
            }
            None => false,
        })
    }

    /// Starts an authored capability of a plan locally (no model call), optionally straight into a fresh check.
    pub fn begin_authored(&mut self, sid: &str, gid: &str, oid: &str, mode: &str) -> Result<()> {
        let goal_text = clip(&latest_plan(&self.conn, gid)?.goal_text, 600);
        if !self.session(sid)?.draft.trim().is_empty() && self.pending_transcript(sid)?.is_none() {
            self.save_draft(sid, "")?;
        }
        self.start_goal_for(sid, self.session(sid)?.revision, &goal_text, oid)?;
        if mode == "check" {
            self.learning_action(sid, self.session(sid)?.revision, "fresh_check")?;
        }
        Ok(())
    }

    fn begin_generated(
        &self,
        tx: &Connection,
        plan: &TaskPlan,
        m: crate::curriculum::Material,
        time: &str,
    ) -> Result<()> {
        tx.execute(
            "INSERT INTO session_objectives VALUES (?,?,?)",
            params![plan.session_id, plan.objective_id, plan.content_version],
        )
        .map_err(sql)?;
        let stage = if plan.action == "check" {
            "independent"
        } else {
            "diagnosis"
        };
        let aid = crate::learning::freeze_task(tx, &plan.session_id, stage, time, Some(m))?;
        let goal_text = latest_plan(tx, &plan.goal_id)?.goal_text;
        let mut s: Session = parse(
            tx.query_row(
                "SELECT data FROM sessions WHERE id=?",
                [&plan.session_id],
                |r| r.get(0),
            )
            .map_err(sql)?,
        )?;
        s.title = clip(&outcome(tx, &plan.objective_id), 80);
        tx.execute(
            "UPDATE sessions SET data=? WHERE id=?",
            params![json(&s)?, s.id],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO learning_sessions VALUES (?,?)",
            params![
                plan.session_id,
                json(&LearningState {
                    schema_version: 1,
                    session_id: plan.session_id.clone(),
                    goal: clip(&goal_text, 600),
                    stage: stage.into(),
                    active_activity_id: aid,
                    quiet: false,
                    stopped: false,
                    diagnosis_count: 0,
                    policy_version: crate::learning::POLICY.into(),
                })?
            ],
        )
        .map_err(sql)?;
        if plan.action == "check" {
            tx.execute(
                "INSERT INTO learning_events VALUES (?,?,?,?,?)",
                params![
                    id(),
                    plan.session_id,
                    "check_without_teaching",
                    time,
                    j!({"objective_id":plan.objective_id}).to_string()
                ],
            )
            .map_err(sql)?;
        }
        bump(tx, &plan.session_id)
    }
}

/// Removes goals whose sessions were all deleted, with their plans and any generated capabilities nothing refers to.
pub fn delete_orphans(c: &Connection) -> Result<()> {
    c.execute(
        "DELETE FROM goals WHERE id NOT IN (SELECT goal_id FROM goal_sessions)",
        [],
    )
    .map_err(sql)?;
    c.execute(
        "DELETE FROM objectives WHERE json_extract(data,'$.origin')='generated' AND json_extract(data,'$.goal_id') NOT IN (SELECT id FROM goals) AND id NOT IN (SELECT objective_id FROM activities) AND id NOT IN (SELECT objective_id FROM session_objectives) AND id NOT IN (SELECT objective_id FROM reviews)",
        [],
    )
    .map_err(sql)?;
    Ok(())
}
