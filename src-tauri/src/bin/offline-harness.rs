//! Isolated engineering harness. It calls the actual Store, policy, parser and fixture provider boundary.
//! Never included in either application package. The offline feature is forbidden in release builds.
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::Arc,
};
use study_harness::{
    clock::SystemClock,
    contracts::*,
    db::{Result, Store},
    offline, visualizer,
};
use tokio_util::sync::CancellationToken;
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| format!("Missing harness field {k}"))
}
fn rev(v: &Value) -> Result<i32> {
    v["revision"]
        .as_i64()
        .map(|v| v as i32)
        .ok_or("Missing revision".into())
}
fn view(d: &mut Store, sid: Option<String>) -> Result<Value> {
    serde_json::to_value(d.workspace_view(
        sid,
        CredentialStatus {
            configured: false,
            storage: "offline engineering harness".into(),
            masked: String::new(),
            message: "Simulated boundary; no provider access or charges".into(),
        },
    )?)
    .map_err(|e| e.to_string())
}
fn context(d: &Store, aid: &str) -> Result<Value> {
    let raw: String = d
        .conn
        .query_row(
            "SELECT state FROM visual_activities WHERE id=?",
            [aid],
            |r| r.get(0),
        )
        .map_err(|_| "Unknown visual activity")?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}
async fn dispatch(d: &mut Store, cmd: &str, a: &Value) -> std::result::Result<Value, Value> {
    if cmd == "visualizer_rpc" || cmd == "source_rpc" || cmd == "evaluation_rpc" {
        let bytes = a["body"]
            .as_array()
            .ok_or(json!("Missing raw body"))?
            .iter()
            .map(|n| {
                n.as_u64()
                    .filter(|n| *n < 256)
                    .map(|n| n as u8)
                    .ok_or(json!("Invalid byte"))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return if cmd == "evaluation_rpc" {
            d.evaluation_rpc(&bytes).map_err(|e| json!(e))
        } else if cmd == "source_rpc" {
            d.source_rpc(&bytes).map_err(|e| json!(e))
        } else {
            d.visualizer_rpc(&bytes)
                .map_err(|e| serde_json::to_value(e).unwrap())
        };
    }
    let run = async {
        match cmd {
            "record_startup" => Ok(Value::Null),
            "test.contextMetrics" => {
                let sid = text(a, "sessionId")?;
                let at = d
                    .attempts(sid)?
                    .last()
                    .cloned()
                    .ok_or("Save a response first")?;
                let request = Request {
                    schema_version: 1,
                    id: String::new(),
                    session_id: sid.into(),
                    attempt_id: Some(at.id),
                    session_revision: d.session(sid)?.revision,
                    kind: "tutor".into(),
                    status: "pending".into(),
                    reserved_microusd: 0,
                    created_at: d.time(),
                };
                let input = d.tutor_input(request)?;
                let bytes = study_harness::providers::tutor_payload(&input)
                    .to_string()
                    .len();
                Ok(
                    json!({"encoded_payload_bytes":bytes,"ceiling_bytes":32000,"source_passages":input.source_context.len(),"recent_exchanges":input.conversation.len(),"conservative_reservation_microusd":study_harness::providers::tutor_reservation(&input)?,"note":"Byte bound and pricing reservation, not measured provider tokenization or latency."}),
                )
            }
            "test.seedHistory" => {
                let sid = d.seed_performance_history()?;
                view(d, Some(sid))
            }
            "development_cases" => Ok(offline::diagnostic_list()),
            "development_action" => {
                let sid = d.development_action(text(a, "sessionId")?, text(a, "action")?)?;
                view(d, Some(sid))
            }
            "workspace" => view(d, a["sessionId"].as_str().map(str::to_owned)),
            "new_session" => {
                let sid = d.new_session(text(a, "title")?)?;
                view(d, Some(sid))
            }
            "save_draft" => {
                d.save_draft(text(a, "sessionId")?, text(a, "text")?)?;
                Ok(Value::Null)
            }
            "discard_transcript" => {
                d.discard_transcript(text(a, "sessionId")?, text(a, "transcriptId")?)?;
                Ok(Value::Null)
            }
            "start_goal" => {
                let sid = text(a, "sessionId")?;
                d.start_goal_for(
                    sid,
                    rev(a)?,
                    text(a, "goal")?,
                    a["objectiveId"].as_str().unwrap_or("quadratic-convergence"),
                )?;
                view(d, Some(sid.into()))
            }
            "plan_goal" => {
                let sid = text(a, "sessionId")?;
                let sources: Vec<String> =
                    serde_json::from_value(a["sourceIds"].clone()).unwrap_or_default();
                offline::plan_offline(
                    d,
                    sid,
                    rev(a)?,
                    text(a, "goal")?,
                    &sources,
                    "",
                    None,
                    a["scenario"].as_str().unwrap_or("correct"),
                )
                .await?;
                view(d, Some(sid.into()))
            }
            "revise_plan" => {
                let sid = text(a, "sessionId")?;
                offline::plan_offline(
                    d,
                    sid,
                    rev(a)?,
                    "",
                    &[],
                    text(a, "instruction")?,
                    Some(text(a, "goalId")?),
                    a["scenario"].as_str().unwrap_or("correct"),
                )
                .await?;
                view(d, Some(sid.into()))
            }
            "plan_decision" => {
                let sid = text(a, "sessionId")?;
                d.plan_decision(
                    sid,
                    rev(a)?,
                    text(a, "goalId")?,
                    text(a, "objectiveId")?,
                    text(a, "decision")?,
                )?;
                view(d, Some(sid.into()))
            }
            "start_sample" => {
                let sid = text(a, "sessionId")?;
                d.start_authored_goal(sid, rev(a)?, text(a, "objectiveId")?)?;
                view(d, Some(sid.into()))
            }
            "begin_capability" => {
                let sid = text(a, "sessionId")?;
                if d.session(sid)?.revision != rev(a)? {
                    return Err("Session changed. Refresh first.".into());
                }
                let (gid, oid, mode) = (
                    text(a, "goalId")?,
                    text(a, "objectiveId")?,
                    text(a, "mode")?,
                );
                let target = d.session_for_capability(sid, gid, oid)?;
                if d.learning_state(&target)?.is_none() {
                    if !study_harness::goals::is_generated(&d.conn, oid)? {
                        d.begin_authored(&target, gid, oid, mode)?;
                    } else {
                        let r = d.session(&target)?.revision;
                        offline::task_offline(
                            d,
                            &target,
                            r,
                            if mode == "check" { "check" } else { "start" },
                            Some(oid),
                            "",
                            a["scenario"].as_str().unwrap_or("correct"),
                        )
                        .await?;
                    }
                }
                view(d, Some(target))
            }
            "advance" => {
                let sid = text(a, "sessionId")?;
                let action = text(a, "action")?;
                if d.needs_generation(sid, action)? {
                    offline::task_offline(
                        d,
                        sid,
                        rev(a)?,
                        action,
                        None,
                        a["note"].as_str().unwrap_or(""),
                        a["scenario"].as_str().unwrap_or("correct"),
                    )
                    .await?;
                } else {
                    d.learning_action(sid, rev(a)?, action)?;
                }
                view(d, Some(sid.into()))
            }
            "learning_action" => {
                let sid = text(a, "sessionId")?;
                d.learning_action(sid, rev(a)?, text(a, "action")?)?;
                view(d, Some(sid.into()))
            }
            "update_privacy" => {
                d.set_privacy(
                    &serde_json::from_value(a["privacy"].clone()).map_err(|e| e.to_string())?,
                )?;
                Ok(Value::Null)
            }
            "update_limits" => {
                d.set_limits(
                    &serde_json::from_value(a["limits"].clone()).map_err(|e| e.to_string())?,
                )?;
                view(d, Some(text(a, "sessionId")?.into()))
            }
            "deletion_preview" => {
                serde_json::to_value(d.deletion_preview(text(a, "scope")?, text(a, "targetId")?)?)
                    .map_err(|e| e.to_string())
            }
            "delete_records" => {
                d.delete_owned(
                    text(a, "scope")?,
                    text(a, "targetId")?,
                    text(a, "confirmation")?,
                )?;
                Ok(Value::Null)
            }
            "test.export" => {
                use base64::Engine;
                Ok(json!(
                    base64::engine::general_purpose::STANDARD.encode(d.export_archive()?)
                ))
            }
            "test.restore" => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(text(a, "bytes")?)
                    .map_err(|_| "Invalid test archive")?;
                d.restore_archive(&bytes, None)?;
                Ok(Value::Null)
            }
            "save_attempt" => {
                let input: SaveAttempt =
                    serde_json::from_value(a["input"].clone()).map_err(|e| e.to_string())?;
                d.save_submission(
                    &input,
                    a.get("visualSubmission").filter(|v| !v.is_null()),
                    a["intent"].as_str().unwrap_or("answer"),
                )?;
                view(d, Some(input.session_id))
            }
            "correct_attempt" => {
                let sid = text(a, "sessionId")?;
                d.correct_attempt(
                    sid,
                    rev(a)?,
                    text(a, "attemptId")?,
                    text(a, "submissionId")?,
                    text(a, "text")?,
                )?;
                view(d, Some(sid.into()))
            }
            "dispute_grade" => {
                let sid = text(a, "sessionId")?;
                d.dispute_grade(sid, rev(a)?, text(a, "assessmentId")?, text(a, "reason")?)?;
                view(d, Some(sid.into()))
            }
            "resolve_dispute" => {
                let sid = text(a, "sessionId")?;
                d.resolve_dispute(sid, rev(a)?, text(a, "assessmentId")?)?;
                view(d, Some(sid.into()))
            }
            "ask_tutor" => {
                let sid = text(a, "sessionId")?;
                let attempt = text(a, "attemptId")?;
                let visual = match a.get("visualSummary").filter(|v| !v.is_null()) {
                    Some(v) => d.visual_tutor_summary(sid, v)?,
                    None => None,
                };
                let input = d.prepare_tutor_request_with(sid, attempt, visual)?;
                let r = input.request.clone();
                let result = offline::respond(
                    input,
                    a["scenario"].as_str().unwrap_or("correct"),
                    CancellationToken::new(),
                )
                .await;
                match result {
                    Ok(rsp) => match rsp.proposal {
                        Ok(p) => d.settle(&r, "completed", rsp.usage, Some(&p))?,
                        Err(e) => {
                            d.settle(&r, "rejected", rsp.usage, None)?;
                            return Err(e);
                        }
                    },
                    Err(e) => {
                        d.settle(&r, "failed", Store::estimated(&r), None)?;
                        return Err(e);
                    }
                }
                view(d, Some(sid.into()))
            }
            "test.openActivity" => {
                let sid = d.new_session("HostPort conformance — simulated")?;
                d.start_goal(&sid, 0, "Predict quadratic convergence")?;
                d.save_attempt(&SaveAttempt { session_id: sid.clone(), expected_revision: d.session(&sid)?.revision, submission_id: study_harness::db::id(), response: "Crash-chain preparation fixture: I predict alternating convergence when q=-0.5.".into(), transcript_id: None })?;
                d.visual_open(&sid, text(a, "templateId")?)
                    .map(|v| v["request"].clone())
                    .map_err(|_| "Cannot open visual activity".into())
            }
            "test.hostSessionRevision" => {
                let c = context(d, text(a, "activityId")?)?;
                Ok(json!(d.session(text(&c, "sessionId")?)?.revision))
            }
            "test.respond" => {
                let aid = text(a, "activityId")?;
                let c = context(d, aid)?;
                let sid = text(&c, "sessionId")?;
                let old = d.session(sid)?.revision;
                let input = SaveAttempt {
                    session_id: sid.into(),
                    expected_revision: old,
                    submission_id: study_harness::db::id(),
                    response: "Conformance fixture: q=-0.5, alternates and shrinks.".into(),
                    transcript_id: None,
                };
                let at = d.save_attempt(&input)?;
                let current = d.session(sid)?.revision;
                Ok(
                    json!({"response":{"responseId":at.id,"taskId":a["taskId"],"taskRevision":1,"kind":a["kind"],"stageId":a["stageId"],"modality":"text"},"expectedHostSessionRevision":old,"hostSessionRevision":current}),
                )
            }
            "test.authorizeReveal" => d
                .visual_allow_reveal(text(a, "activityId")?)
                .map_err(|_| "Reveal was not authorized".into()),
            "test.startIndependentTask" => {
                let c = context(d, text(a, "activityId")?)?;
                let sid = text(&c, "sessionId")?;
                d.learning_action(sid, d.session(sid)?.revision, "fresh_check")?;
                let task = d.activity(sid)?.id;
                let v = d
                    .visual_open(sid, text(&c, "templateId")?)
                    .map_err(|_| "Cannot open independent visual")?;
                Ok(json!({"request":v["request"],"taskRef":task,"policy":v["policy"]}))
            }
            "test.armCrash" => {
                visualizer::FAULT.with(|f| {
                    *f.borrow_mut() = Some((
                        text(a, "point").unwrap_or_default().into(),
                        a["terminate"].as_bool().unwrap_or(true),
                    ))
                });
                Ok(Value::Null)
            }
            "test.inspect" => {
                let sid = text(a, "sessionId")?;
                let mut data = view(d, Some(sid.into()))?;
                let mut stmt=d.conn.prepare("SELECT data FROM visual_attempt_snapshots WHERE attempt_id IN (SELECT id FROM attempts WHERE session_id=?)").map_err(|e|e.to_string())?;
                let snapshots = stmt
                    .query_map([sid], |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?
                    .map(|r| {
                        serde_json::from_str::<Value>(&r.map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>>>()?;
                data["visualSnapshots"] = json!(snapshots);
                Ok(data)
            }
            _ => Err("Unsupported offline harness command".into()),
        }
    };
    run.await.map_err(|e: String| json!(e))
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let db = std::env::args()
        .nth(1)
        .ok_or("Pass an isolated offline SQLite path")?;
    let mut d = Store::open_with(Path::new(&db), true, Arc::new(SystemClock))?;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(v) => match dispatch(
                &mut d,
                v["command"].as_str().unwrap_or_default(),
                &v["args"],
            )
            .await
            {
                Ok(result) => json!({"ok":true,"result":result}),
                Err(error) => json!({"ok":false,"error":error}),
            },
            Err(_) => json!({"ok":false,"error":"Invalid test envelope"}),
        };
        println!("{reply}");
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
