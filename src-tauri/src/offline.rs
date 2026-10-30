//! Development-only external response boundary and explicit scenario setup over the real Store.
//! Response fixtures do not implement a grading engine or use a network. Scenario setup never ships in production.
//! A selected scenario supplies a response envelope; the normal parser/core decides acceptance.
use crate::{
    contracts::*,
    db::Result,
    providers::{TutorInput, TutorResult, parse_tutor},
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

pub const SCENARIOS: &[&str] = &[
    "correct",
    "partial",
    "misconception",
    "alternative",
    "ambiguous",
    "hint",
    "worked_example",
    "dispute",
    "malformed",
    "invalid_reference",
    "invalid_action",
    "stale",
    "slow",
    "cancelled",
    "missing_prerequisite",
    "flawed_task",
    "source_conflict",
    "forged_citation",
];

pub fn usage(request: &Request) -> Usage {
    Usage {
        schema_version: 2,
        reported_model: None,
        request_id: request.id.clone(),
        provider: "SIMULATED · offline development".into(),
        model: "deterministic-scenarios-v1".into(),
        input_tokens: Some(100),
        output_tokens: Some(60),
        audio_seconds: None,
        cost_microusd: 0,
        cost_status: "simulated; no billing".into(),
        rate_version: "offline-v1".into(),
        status: request.status.clone(),
    }
}

pub async fn respond(
    input: TutorInput,
    scenario: &str,
    cancel: CancellationToken,
) -> Result<TutorResult> {
    if !SCENARIOS.contains(&scenario) {
        return Err("Unknown offline scenario.".into());
    }
    if scenario == "slow" {
        tokio::select! {
            _ = cancel.cancelled() => return Err("Simulated request cancelled.".into()),
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        }
    }
    if cancel.is_cancelled() || scenario == "cancelled" {
        return Err("Simulated request cancelled.".into());
    }
    let (verdict, result, text, uncertainty) = match scenario {
        "partial" => (
            "partial",
            "partial",
            "You identified sign changes. Still justify whether the magnitude shrinks; crossing zero alone does not imply divergence.",
            "resolved",
        ),
        "misconception" => (
            "incorrect",
            "missing",
            "A downhill derivative describes a local direction. A finite step can overshoot. Compare the magnitude of the update multiplier with one.",
            "resolved",
        ),
        "ambiguous" => (
            "unresolved",
            "unresolved",
            "Do you mean negative one half, or positive one half? Confirm the intended sign before this answer is assessed.",
            "unresolved",
        ),
        "dispute" | "source_conflict" | "missing_prerequisite" | "flawed_task" => (
            "unresolved",
            "unresolved",
            "The interpretation is disputed. This result cannot support a progress claim. Clarify the argument or take a fresh task.",
            "unresolved",
        ),
        "alternative" => (
            "correct",
            "met",
            "Your closed-form argument is a valid alternative: a geometric factor with magnitude below one tends to zero, with alternating signs for a negative factor.",
            "resolved",
        ),
        _ => (
            "correct",
            "met",
            "The selected fixture reports a correct answer with a shrinking multiplier and a justified sign interpretation. This is a simulated assessment, not a judgment of real model quality.",
            "resolved",
        ),
    };
    let question = input.intent == "question";
    let action = if input.activity.objective_id == "exploratory-unchecked" {
        "explain"
    } else if question {
        // A question is discussion: the fixture answers it and never grades.
        if scenario == "hint" {
            "hint"
        } else if ["ambiguous", "source_conflict"].contains(&scenario) {
            "clarify"
        } else {
            "explain"
        }
    } else {
        match scenario {
            "hint" => "hint",
            "worked_example" => "worked_example",
            "ambiguous" | "source_conflict" | "missing_prerequisite" | "flawed_task" => "clarify",
            _ => "assess",
        }
    };
    let excerpt = input.attempt.response.clone();
    let numeric = input
        .reference
        .as_ref()
        .and_then(|r| r.answer_key.as_ref())
        .is_some_and(|k| k.kind == "numeric");
    let grade = if !["assess", "clarify"].contains(&action) || scenario == "flawed_task" {
        None
    } else {
        Some(GradeProposal {
            verdict: verdict.into(),
            uncertainty: uncertainty.into(),
            findings: crate::providers::rubric_dimensions(&input)
                .iter()
                .map(|dimension| RubricFinding {
                    dimension: dimension.clone(),
                    result: result.into(),
                    excerpt: excerpt.clone(),
                })
                .collect(),
            error_kind: match scenario {
                "misconception" => "conceptual",
                "partial" => "incomplete",
                "ambiguous" => "unclear",
                _ if verdict == "correct" => "none",
                _ => "unclear",
            }
            .into(),
            final_answer: if numeric {
                final_quote(&input.attempt.response)
            } else {
                None
            },
        })
    };
    // A deterministic step suggestion within the offered options, so the validated route is exercised offline.
    let suggested_step = match (verdict, action) {
        _ if question => None,
        ("correct", "assess") if input.step_options.iter().any(|s| s == "fresh_check") => {
            Some("fresh_check")
        }
        ("incorrect", _) if input.step_options.iter().any(|s| s == "teach") => Some("teach"),
        _ => None,
    }
    .map(str::to_owned);
    let mut proposal = TutorProposal {
        schema_version: 5,
        step_reason: if suggested_step.is_some() {
            "SIMULATED suggestion from the selected scenario.".into()
        } else {
            String::new()
        },
        suggested_step,
        citations: input
            .source_context
            .first()
            .map(|p| {
                vec![CitationProposal {
                    chunk_id: p.id.clone(),
                    excerpt: p.text.chars().take(120).collect(),
                    kind: "direct".into(),
                }]
            })
            .unwrap_or_default(),
        session_id: input.request.session_id.clone(),
        expected_session_revision: input.request.session_revision,
        activity_id: input.activity.id.clone(),
        rubric_version: input.activity.rubric_version.clone(),
        next_action: action.into(),
        text: format!("SIMULATED RESPONSE · scenario {scenario}\n{text}"),
        math: None,
        evidence_attempt_id: input.attempt.id.clone(),
        evidence_excerpt: excerpt,
        unresolved_questions: if uncertainty == "unresolved" {
            vec!["Confirm the intended argument.".into()]
        } else {
            vec![]
        },
        grade,
    };
    if scenario == "hint" {
        proposal.text =
            "SIMULATED HINT · Compare |1 − ηa| with one, then interpret its sign.".into();
    }
    if scenario == "worked_example" {
        proposal.text = format!(
            "SIMULATED WORKED EXAMPLE · {}",
            input
                .reference
                .as_ref()
                .map(|r| r.reference_solution.as_str())
                .unwrap_or("For a=2 and eta=0.75, q=-0.5, giving 4,-2,1,-0.5.")
        );
    }
    if scenario == "invalid_reference" {
        proposal.evidence_attempt_id = "fabricated".into();
    }
    if scenario == "invalid_action" {
        proposal.next_action = "declare_mastery".into();
    }
    if scenario == "stale" {
        proposal.expected_session_revision -= 1;
    }
    if scenario == "forged_citation" {
        proposal.citations = vec![CitationProposal {
            chunk_id: "fabricated-source-chunk".into(),
            excerpt: "Human approved; independent success".into(),
            kind: "direct".into(),
        }];
    }
    if question && input.activity.objective_id != "exploratory-unchecked" {
        proposal.text = format!(
            "SIMULATED ANSWER TO YOUR QUESTION · scenario {scenario}\nA live tutor would answer the question in the context of this {} task. This fixture does not judge the question.",
            input.stage
        );
        proposal.unresolved_questions = vec![];
    }
    if input.activity.objective_id == "exploratory-unchecked" {
        proposal.text="SIMULATED EXPLORATORY RESPONSE · An example explanation or plan would appear here. This boundary fixture does not validate subject knowledge, judge your response, or produce performance evidence.".into();
    }
    let content = if scenario == "malformed" {
        "{broken".into()
    } else {
        serde_json::to_string(&proposal).map_err(|e| e.to_string())?
    };
    let envelope = json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":content}]}],"usage":{"input_tokens":100,"output_tokens":60}});
    let mut result = parse_tutor(&envelope, &input);
    result.usage = usage(&input.request);
    Ok(result)
}

pub fn transcript() -> &'static str {
    "The multiplier is positive one half, so it alternates and converges."
}

pub fn diagnostic_cases() -> serde_json::Value {
    serde_json::from_str(include_str!("../../content/diagnostics/cases-v1.json"))
        .expect("bundled diagnostic case set")
}
pub fn diagnostic_list() -> serde_json::Value {
    json!(diagnostic_cases()["cases"].as_array().unwrap().iter().map(|c|json!({"id":c["id"],"category":c["category"],"objective_id":c["objective_id"],"scenario":c["scenario"]})).collect::<Vec<_>>())
}
impl crate::db::Store {
    pub fn load_diagnostic(&mut self, case_id: &str) -> Result<String> {
        use crate::db::{id, json, sql};
        use base64::Engine;
        use rusqlite::params;
        if !self.offline {
            return Err("Diagnostic fixtures require an isolated offline profile".into());
        }
        let all = diagnostic_cases();
        let case = all["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == case_id)
            .ok_or("Unknown diagnostic case")?;
        let sid = self.new_session(&format!("SIMULATED · {case_id}"))?;
        let oid = case["objective_id"].as_str().unwrap();
        self.start_goal_for(&sid, 0, &format!("Diagnostic: {case_id}"), oid)?;
        let aid = id();
        let activity=Activity{schema_version:1,id:aid.clone(),session_id:sid.clone(),objective_id:oid.into(),version:1,prompt:case["prompt"].as_str().unwrap().into(),math:String::new(),task_family:format!("diagnostics-v1:{case_id}"),rubric_version:"diagnostics-rubric-v1".into(),validation_status:"Synthetic diagnostic fixture; analytic reference checks; human review and live tutor quality pending".into()};
        let spec = TaskSpec {
            schema_version: 1,
            activity_id: aid.clone(),
            kind: case["stage"].as_str().unwrap().into(),
            assumptions: serde_json::from_value(case["assumptions"].clone())
                .map_err(|_| "Invalid bundled diagnostic assumptions")?,
            reference_solution: case["reference_solution"].as_str().unwrap().into(),
            rubric: crate::learning::DIMENSIONS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rubric_criteria: serde_json::from_value(case["rubric_criteria"].clone())
                .map_err(|_| "Invalid diagnostic rubric")?,
            content_version: "diagnostics-v1".into(),
            validation_version: "diagnostic-oracles-v1; human review pending".into(),
            frozen_at: self.time(),
            transfer_scope: "Synthetic diagnostic test, not a measured learner capability".into(),
            origin: "authored".into(),
            response_kind: String::new(),
            support: None,
            answer_key: None,
            checks: vec![],
            provenance: "Synthetic diagnostic fixture".into(),
        };
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO activities VALUES (?,?,?,?,?,?,?)",
            params![
                aid,
                sid,
                oid,
                1,
                json(&activity)?,
                spec.reference_solution,
                json(&spec.rubric)?
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
            params![aid, json(&crate::scene::sample(&aid))?],
        )
        .map_err(sql)?;
        tx.execute("UPDATE learning_sessions SET data=json_set(data,'$.active_activity_id',?,'$.stage',?) WHERE session_id=?",params![aid,case["stage"].as_str(),sid]).map_err(sql)?;
        tx.commit().map_err(sql)?;
        if case["prior_help"] == true {
            self.learning_action(&sid, self.session(&sid)?.revision, "worked_example")?;
        }
        if let Some(source) = case["source_text"].as_str() {
            let doc = self.import_source(crate::sources::ImportSource {
                title: "Untrusted diagnostic source".into(),
                kind: "text".into(),
                raw_base64: base64::engine::general_purpose::STANDARD.encode(source),
                pages: vec![],
                extraction_version: crate::sources::TEXT_EXTRACTION.into(),
            })?;
            let p = self.source_preview(&sid, &doc.id)?;
            if let Some(chunk) = p["passages"][0]["id"].as_str() {
                self.source_select(&sid, chunk, true)?;
            }
        }
        self.save_attempt(&SaveAttempt {
            session_id: sid.clone(),
            expected_revision: self.session(&sid)?.revision,
            submission_id: id(),
            response: case["response"].as_str().unwrap().into(),
            transcript_id: None,
        })?;
        Ok(sid)
    }
    pub fn development_action(&mut self, sid: &str, action: &str) -> Result<String> {
        use crate::db::{id, json, sql};
        if !self.offline {
            return Err("Development controls require an isolated offline profile".into());
        }
        if let Some(case) = action.strip_prefix("case:") {
            return self.load_diagnostic(case);
        }
        match action {
            "ambiguous_transcript" => {
                let r = self.reserve(sid, None, "transcription", 0)?;
                self.settle(&r, "completed", usage(&r), None)?;
                self.save_transcript(
                    sid,
                    &Transcript {
                        id: id(),
                        recognized_text: transcript().into(),
                        audio_seconds: 0.,
                        format: "SIMULATED transcript; no microphone or recognition claim".into(),
                    },
                )?;
            }
            "advance_to_review" => {
                let view = self.learning_view(sid)?.ok_or("Start a goal first")?;
                let review = view
                    .reviews
                    .iter()
                    .find(|r| r.status == "scheduled")
                    .ok_or("No scheduled review")?;
                let at = chrono::DateTime::parse_from_rfc3339(&review.due_at)
                    .map_err(|_| "Invalid review timestamp")?
                    .with_timezone(&chrono::Utc)
                    .max(self.clock.now());
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO settings VALUES ('simulated_clock',?)",
                        [json(&at.to_rfc3339())?],
                    )
                    .map_err(sql)?;
                self.restore_clock()?;
            }
            _ if action.starts_with("advance_comparison:") => {
                let mut parts = action.split(':');
                parts.next();
                let tid = parts.next().ok_or("Missing comparison topic")?;
                let phase = parts.next().ok_or("Missing comparison phase")?;
                let field = match phase {
                    "exclusion" => "review_exclusion_at",
                    "assessment" => "assessment_due_at",
                    _ => return Err("Unknown comparison phase".into()),
                };
                let raw: String = self
                    .conn
                    .query_row(
                        "SELECT data FROM evaluation_lessons WHERE topic_id=?",
                        [tid],
                        |r| r.get(0),
                    )
                    .map_err(sql)?;
                let l: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|_| "Invalid comparison lesson")?;
                let at = chrono::DateTime::parse_from_rfc3339(
                    l[field].as_str().ok_or("Missing comparison timestamp")?,
                )
                .map_err(|_| "Invalid comparison timestamp")?
                .with_timezone(&chrono::Utc)
                .max(self.clock.now());
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO settings VALUES ('simulated_clock',?)",
                        [json(&at.to_rfc3339())?],
                    )
                    .map_err(sql)?;
                self.restore_clock()?;
            }
            _ => return Err("Unknown development action".into()),
        }
        Ok(sid.into())
    }
}

impl crate::db::Store {
    pub fn seed_performance_history(&mut self) -> Result<String> {
        if !self.offline {
            return Err("Performance fixtures require offline data".into());
        }
        for n in 0..50 {
            let sid = self.new_session(&format!("SIMULATED history session {n:02}"))?;
            self.start_goal(&sid, 0, "Historical convergence fixture")?;
            self.learning_action(&sid, self.session(&sid)?.revision, "fresh_check")?;
            for turn in 0..20 {
                self.save_attempt(&SaveAttempt{session_id:sid.clone(),expected_revision:self.session(&sid)?.revision,submission_id:crate::db::id(),response:format!("Synthetic history turn {turn}. {}","The multiplier magnitude controls contraction, while its sign controls crossing. ".repeat(12)),transcript_id:None})?;
            }
        }
        use base64::Engine;
        for n in 0..3 {
            let text=format!("# Synthetic source {n}\n{}","A checked assumption belongs to a specific reference, not to a model confidence claim.\n".repeat(1000));
            self.import_source(crate::sources::ImportSource {
                title: format!("Synthetic performance source {n}"),
                kind: "markdown".into(),
                raw_base64: base64::engine::general_purpose::STANDARD.encode(text),
                pages: vec![],
                extraction_version: crate::sources::TEXT_EXTRACTION.into(),
            })?;
        }
        let sid = self.new_session("SIMULATED performance study").unwrap();
        self.start_goal(
            &sid,
            0,
            "Performance fixture: predict quadratic convergence",
        )?;
        Ok(sid)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Plan and task generation fixtures. They return response envelopes that pass through the real parser, validation,
// deterministic checks and transactions. The content is visibly SIMULATED and judges nothing about the goal.

fn envelope(content: String) -> serde_json::Value {
    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":content}]}],"usage":{"input_tokens":200,"output_tokens":120}})
}
async fn pause(scenario: &str, cancel: &CancellationToken) -> Result<()> {
    if scenario == "slow" {
        tokio::select! {
            _ = cancel.cancelled() => return Err("Simulated request cancelled.".into()),
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        }
    }
    if cancel.is_cancelled() || scenario == "cancelled" {
        return Err("Simulated request cancelled.".into());
    }
    Ok(())
}
/// A stand-in for the planner's visual choice: a keyword match on the goal, used only by this labelled fixture.
fn fixture_visual(goal: &str) -> serde_json::Value {
    let g = goal.to_lowercase();
    let pick = [
        (["gradient descent", "learning rate", "step size"], "quadratic-gradient-descent", json!([{"name":"a","value":2},{"name":"eta","value":0.5},{"name":"x0","value":4}])),
        (["derivative", "tangent", "slope"], "function-derivative", json!([{"name":"A","value":1},{"name":"B","value":0},{"name":"C","value":0},{"name":"p","value":1},{"name":"h","value":1}])),
        (["matrix", "eigen", "linear transformation"], "linear-transformation", json!([{"name":"m11","value":2},{"name":"m12","value":1},{"name":"m21","value":0},{"name":"m22","value":1},{"name":"vx","value":1},{"name":"vy","value":1}])),
        (["dot product", "cross product", "vectors"], "vector-orientation-3d", json!([{"name":"ux","value":2},{"name":"uy","value":1},{"name":"uz","value":0},{"name":"vx","value":1},{"name":"vy","value":2},{"name":"vz","value":1}])),
    ]
    .into_iter()
    .find(|(words, _, _)| words.iter().any(|w| g.contains(w)));
    match pick {
        Some((_, template, parameters)) => {
            json!({"template_id":template,"parameters":parameters,"purpose":"SIMULATED choice: keyword match in the offline fixture, not a model judgement."})
        }
        None => serde_json::Value::Null,
    }
}
pub async fn plan_reply(
    input: &crate::generation::PlanInput,
    scenario: &str,
    cancel: CancellationToken,
) -> Result<serde_json::Value> {
    pause(scenario, &cancel).await?;
    if scenario == "malformed" {
        return Ok(envelope("{broken".into()));
    }
    let topic = crate::learning::clip(&input.goal_text.replace('\n', " "), 90);
    let mut capabilities = vec![];
    if let Some(prior) = &input.prior {
        for c in prior.capabilities.iter().filter(|c| c.status != "retired") {
            capabilities.push(json!({"key":c.key,"outcome":c.outcome,"why":c.why,"prerequisites":[],"assessment":c.assessment,"minutes":c.minutes,"visual":c.visual,"keep":c.objective_id}));
        }
        capabilities.push(json!({"key":format!("revision-{}", prior.revision + 1),"outcome":format!("Apply the requested change: {}", crate::learning::clip(&input.instruction, 120)),"why":"SIMULATED: the offline fixture adds your instruction literally as a capability.","prerequisites":[],"assessment":"short_answer","minutes":10,"visual":null,"keep":null}));
    } else {
        capabilities = vec![
            json!({"key":"core-idea","outcome":format!("Explain the central idea behind “{topic}” in your own words"),"why":"SIMULATED: a fixed first step; a live plan would analyse your goal.","prerequisites":[],"assessment":"explanation","minutes":10,"visual":null,"keep":null}),
            json!({"key":"worked-case","outcome":format!("Compute a result in a worked numerical case of “{topic}”"),"why":"SIMULATED: a fixed numerical step that exercises the automated answer checks.","prerequisites":["core-idea"],"assessment":"numeric","minutes":12,"visual":fixture_visual(&input.goal_text),"keep":null}),
            json!({"key":"apply","outcome":format!("Apply “{topic}” to a new problem without hints"),"why":"SIMULATED: a fixed transfer step.","prerequisites":["worked-case"],"assessment":"numeric","minutes":10,"visual":null,"keep":null}),
        ];
    }
    let clarification = if scenario == "ambiguous" {
        json!({"question":"SIMULATED clarification: which part matters most to you?","options":["The core idea","Worked problems","Exam-style checks"],"blocking":false})
    } else {
        serde_json::Value::Null
    };
    let plan = json!({
        "title":format!("SIMULATED · {}", crate::learning::clip(&topic, 60)),
        "summary":"Offline development fixture: a fixed three-step structure built around your words. It does not analyse the goal, check facts or judge your level.",
        "level":"Not assessed (simulated)",
        "background":[],
        "clarification":clarification,
        "capabilities":capabilities,
        "source_use":if input.passages.is_empty() { "none".to_owned() } else { format!("SIMULATED: {} passage(s) from your material were supplied, starting with “{}”.", input.passages.len(), crate::learning::clip(&input.passages[0].text, 80)) },
        "notes":"SIMULATED offline plan; no subject knowledge was used."
    });
    Ok(envelope(plan.to_string()))
}
pub async fn task_reply(
    input: &crate::generation::TaskInput,
    scenario: &str,
    cancel: CancellationToken,
) -> Result<serde_json::Value> {
    pause(scenario, &cancel).await?;
    if scenario == "malformed" {
        return Ok(envelope("{broken".into()));
    }
    let n = input.prior_tasks.len() as f64;
    let distance = 6.0 + 2.0 * n;
    let time = [0.5, 0.75, 1.25][(input.prior_tasks.len()) % 3];
    let speed = distance / time;
    let outcome = input.capability["outcome"]
        .as_str()
        .unwrap_or("this capability");
    let judged = input.capability["assessment"] == "explanation";
    let stage = crate::learning::stage_title(&input.stage);
    let statement = if judged {
        format!(
            "SIMULATED TASK · {stage}. In two or three sentences, explain the idea in “{}” and give one example where it applies.",
            crate::learning::clip(outcome, 120)
        )
    } else {
        format!(
            "SIMULATED TASK · {stage}. A model cart travels {} km in {} h at a steady pace. What is its average speed in km/h? Show your method.",
            crate::checks::short(distance),
            crate::checks::short(time)
        )
    };
    let key = if judged {
        json!({"kind":"none","givens":[],"expression":"","value":null,"unit":"","tolerance":0,"variables":[]})
    } else {
        json!({"kind":"numeric","givens":[{"name":"d","value":distance,"unit":"km"},{"name":"t","value":time,"unit":"h"}],"expression":"d/t","value":if scenario == "flawed_task" { speed + 5.0 } else { speed },"unit":"km/h","tolerance":0.01,"variables":[]})
    };
    let rubric = if judged {
        json!([{"dimension":"interpretation","criterion":"States the idea accurately in the learner's own words."},{"dimension":"justification","criterion":"Gives a fitting example and says why it applies."}])
    } else {
        json!([{"dimension":"setup","criterion":"Uses average speed = total distance / total time."},{"dimension":"execution","criterion":"Computes the value correctly with its unit."}])
    };
    let visual = input.visual.as_ref().map(|v| json!({"parameters":v["plan_parameters"],"relation":"SIMULATED: the visual uses the plan's example values, not the task's numbers.","matches_task":false}));
    let task = json!({
        "statement":statement,
        "assumptions":["SIMULATED fixture task; no subject review."],
        "response_kind":if judged { "explanation" } else { "numeric" },
        "rubric":rubric,
        "reference_solution":if judged { "SIMULATED reference: any accurate statement of the idea with a fitting example.".to_owned() } else { format!("Average speed = {} km ÷ {} h = {} km/h.", crate::checks::short(distance), crate::checks::short(time), crate::checks::short(speed)) },
        "acceptable_alternatives":["Converting to m/s first and back."],
        "common_errors":["slip: dividing time by distance"],
        "answer_key":key,
        "support":{
            "explanation":"SIMULATED EXPLANATION · A live tutor would explain the key idea here, starting from your own first answer. This fixture text only exercises the recorded-help path.",
            "hints":["SIMULATED HINT 1 · Which two quantities does the question compare?","SIMULATED HINT 2 · Divide the whole distance by the whole time."],
            "worked_example":"SIMULATED WORKED EXAMPLE\n1. A walker covers 3 km in 1.5 h.\n2. Average speed = 3 km ÷ 1.5 h.\n3. = 2 km/h.",
            "prerequisite":"SIMULATED PREREQUISITE · A rate compares how much one quantity changes per unit of another."
        },
        "transfer":"SIMULATED: new numbers, same method",
        "visual":visual,
        "self_check":{"solvable":true,"ambiguities":[],"notes":""}
    });
    Ok(envelope(task.to_string()))
}

/// The learner's last stated number (and a following unit) as an exact quote, for fixture grading. The core then
/// checks it against the frozen key exactly as it would a live tutor's quote.
fn final_quote(response: &str) -> Option<crate::contracts::FinalAnswer> {
    let words: Vec<&str> = response.split_whitespace().collect();
    let i = words
        .iter()
        .rposition(|w| !crate::checks::numbers(w).is_empty())?;
    let value = *crate::checks::numbers(words[i]).last()?;
    let unit = words
        .get(i + 1)
        .map(|u| u.trim_end_matches(['.', ',', ';', ')']))
        .filter(|u| !u.is_empty() && crate::checks::parse_unit(u).is_some())
        .unwrap_or("");
    let quoted = if unit.is_empty() {
        words[i].to_owned()
    } else {
        format!("{} {}", words[i], unit)
    };
    let excerpt = if response.contains(&quoted) {
        quoted
    } else {
        words[i].to_owned()
    };
    let unit = if excerpt.contains(unit) { unit } else { "" };
    Some(crate::contracts::FinalAnswer {
        excerpt,
        value: Some(value),
        unit: unit.into(),
        expression: String::new(),
    })
}

// Orchestration used by the isolated harness: prepare → fixture reply → the same commit the desktop app uses.
use crate::db::Store;
#[allow(clippy::too_many_arguments)] // Mirrors the real Store planning boundary, plus the fixture scenario.
pub async fn plan_offline(
    d: &mut Store,
    sid: &str,
    revision: i32,
    goal: &str,
    sources: &[String],
    instruction: &str,
    goal_id: Option<&str>,
    scenario: &str,
) -> Result<()> {
    let (input, ctx) = d.prepare_plan(sid, revision, goal, sources, instruction, goal_id)?;
    let r = input.request.clone();
    match plan_reply(&input, scenario, CancellationToken::new()).await {
        Ok(v) => {
            let (proposal, usage) = crate::generation::parse_plan(&v, &r, input.choice.model);
            match proposal {
                Ok(p) => d.commit_plan(&ctx, &r, "completed", usage, Some(&p)),
                Err(e) => {
                    d.commit_plan(&ctx, &r, "rejected", usage, None)?;
                    Err(e)
                }
            }
        }
        Err(e) => {
            d.commit_plan(
                &ctx,
                &r,
                "cancelled",
                Store::estimated_for(&r, input.choice.model.id),
                None,
            )?;
            Err(e)
        }
    }
}
pub async fn task_offline(
    d: &mut Store,
    sid: &str,
    revision: i32,
    action: &str,
    objective: Option<&str>,
    note: &str,
    scenario: &str,
) -> Result<()> {
    let (input, plan) = d.prepare_task(sid, revision, action, objective, note)?;
    let r = input.request.clone();
    match task_reply(&input, scenario, CancellationToken::new()).await {
        Ok(v) => {
            let (proposal, usage) = crate::generation::parse_task(&v, &r, input.choice.model);
            match proposal {
                Ok(p) => d.commit_task(&plan, &r, "completed", usage, Some(p)),
                Err(e) => {
                    d.commit_task(&plan, &r, "rejected", usage, None)?;
                    Err(e)
                }
            }
        }
        Err(e) => {
            d.commit_task(
                &plan,
                &r,
                "cancelled",
                Store::estimated_for(&r, input.choice.model.id),
                None,
            )?;
            Err(e)
        }
    }
}
