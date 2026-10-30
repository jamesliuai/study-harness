//! Personal comparison records. These held-out items are never joined into tutor retrieval or graded by the tutor.
use crate::{
    contracts::*,
    db::{Result, Store, id, parse, sql},
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json as j};
pub const VERSION: &str = "personal-comparison-v1";
pub fn dependent_pairs(
    c: &Connection,
    sessions: &[String],
    source: Option<&str>,
) -> Result<Vec<String>> {
    let mut ids = std::collections::BTreeSet::new();
    for sid in sessions {
        let mut q=c.prepare("SELECT t.pair_id FROM evaluation_links k JOIN evaluation_topics t ON t.id=k.topic_id WHERE k.session_id=?").map_err(sql)?;
        for id in q.query_map([sid], |r| r.get::<_, String>(0)).map_err(sql)? {
            ids.insert(id.map_err(sql)?);
        }
    }
    if let Some(source) = source {
        let mut q=c.prepare("SELECT DISTINCT t.pair_id FROM evaluation_topics t,json_each(t.data,'$.source_ids') s WHERE s.value=?").map_err(sql)?;
        for id in q
            .query_map([source], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            ids.insert(id.map_err(sql)?);
        }
    }
    Ok(ids.into_iter().collect())
}
pub fn validate_archive(c: &Connection) -> Result<()> {
    let mut p = c
        .prepare("SELECT id,data FROM evaluation_pairs")
        .map_err(sql)?;
    for row in p
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql)?
    {
        let (pid, raw) = row.map_err(sql)?;
        let p: Value = parse(raw)?;
        if p["id"] != pid
            || p["versions"]["protocol"] != VERSION
            || p["simulated"].as_bool().is_none()
        {
            return Err("Unsupported comparison registration".into());
        }
        let r: Registration = serde_json::from_value(p["registration"].clone())
            .map_err(|_| "Invalid comparison material")?;
        validate_registration(c, &r)?;
        stamp(required(&p, "registered_at")?)?;
        let topics = rows(c, "evaluation_topics", "pair_id", &pid)?;
        if topics.len() != 2 {
            return Err("A comparison pair must retain both topic records".into());
        }
        for t in topics {
            let tid = required(&t, "id")?;
            if !["Study Harness", "baseline"].contains(&required(&t, "condition")?) {
                return Err("Invalid comparison condition".into());
            }
            let mut authored = t.clone();
            for k in ["id", "condition", "simulated"] {
                authored.as_object_mut().unwrap().remove(k);
            }
            for item in authored["items"]
                .as_array_mut()
                .ok_or("Missing held-out items")?
            {
                if item["id"].as_str().is_none() {
                    return Err("Invalid held-out item identity".into());
                }
                item.as_object_mut().unwrap().remove("id");
            }
            let _: Topic =
                serde_json::from_value(authored).map_err(|_| "Invalid held-out topic")?;
            if let Some(l) = one(c, "evaluation_lessons", "topic_id", tid)? {
                let at = stamp(required(&l, "completed_at")?)?;
                if stamp(required(&l, "assessment_due_at")?)? != at + Duration::days(7)
                    || stamp(required(&l, "review_exclusion_at")?)? != at + Duration::days(5)
                {
                    return Err(
                        "Comparison timing does not match the seven-day / 48-hour protocol".into(),
                    );
                }
            }
            if let Some(r) = one(c, "evaluation_responses", "topic_id", tid)? {
                if one(c, "evaluation_exams", "topic_id", tid)?.is_none()
                    || one(c, "evaluation_lessons", "topic_id", tid)?.is_none()
                {
                    return Err(
                        "Held-out responses lack their registered assessment context".into(),
                    );
                }
                let answers: Vec<Response> = serde_json::from_value(r["responses"].clone())
                    .map_err(|_| "Invalid held-out response")?;
                if answers.len() != 2
                    || answers.iter().any(|a| {
                        a.response.trim().is_empty()
                            || a.confidence > 100
                            || !t["items"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|i| i["id"] == a.item_id)
                    })
                {
                    return Err("Held-out responses disagree with their items".into());
                }
            }
        }
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub form: String,
    pub prompt: String,
    pub rubric: String,
    pub reference_solution: String,
    pub max_points: u32,
    pub review_provenance: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub name: String,
    pub prior_knowledge: String,
    pub source_ids: Vec<String>,
    pub items: Vec<Item>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub topic_a: Topic,
    pub topic_b: Topic,
    pub baseline_method: String,
    pub baseline_model: String,
    pub matched_rationale: String,
    pub decision_criterion: String,
    pub time_allowance_minutes: u32,
    pub confirm_distinct_topics: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    item_id: String,
    response: String,
    confidence: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Score {
    item_id: String,
    points: f64,
    notes: String,
}
fn one(c: &Connection, table: &str, column: &str, key: &str) -> Result<Option<Value>> {
    let raw: Option<String> = c
        .query_row(
            &format!("SELECT data FROM {table} WHERE {column}=?"),
            [key],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    raw.map(parse).transpose()
}
fn rows(c: &Connection, table: &str, column: &str, key: &str) -> Result<Vec<Value>> {
    let mut s = c
        .prepare(&format!(
            "SELECT data FROM {table} WHERE {column}=? ORDER BY rowid"
        ))
        .map_err(sql)?;
    s.query_map([key], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .map(|v| parse(v.map_err(sql)?))
        .collect()
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Missing evaluation field {key}"))
}
fn stamp(raw: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(raw)
        .map_err(|_| "Invalid evaluation timestamp")?
        .with_timezone(&Utc))
}
fn topic(c: &Connection, tid: &str) -> Result<Value> {
    one(c, "evaluation_topics", "id", tid)?.ok_or("Comparison topic not found".into())
}
pub fn review_exclusion(c: &Connection, sid: &str, time: &str) -> Result<Option<String>> {
    let raw:Option<String>=c.query_row("SELECT l.data FROM evaluation_links k JOIN evaluation_lessons l ON l.topic_id=k.topic_id LEFT JOIN evaluation_responses r ON r.topic_id=k.topic_id WHERE k.session_id=? AND r.topic_id IS NULL",[sid],|r|r.get(0)).optional().map_err(sql)?;
    if let Some(raw) = raw {
        let lesson: Value = parse(raw)?;
        if stamp(time)? >= stamp(required(&lesson, "review_exclusion_at")?)? {
            return Ok(Some(format!(
                "Planned review is excluded before the seven-day held-out assessment, due {}. Record any outside exposure in the comparison record.",
                lesson["assessment_due_at"]
                    .as_str()
                    .unwrap_or("at the registered time")
            )));
        }
    }
    Ok(None)
}
fn validate_registration(c: &Connection, r: &Registration) -> Result<()> {
    if !r.confirm_distinct_topics || r.topic_a.name.trim() == r.topic_b.name.trim() {
        return Err("Register two distinct matched topics. Multiple objectives from one lesson are not independent topic pairs.".into());
    }
    if !(5..=180).contains(&r.time_allowance_minutes) {
        return Err("Use an equal total time allowance of 5–180 minutes per topic".into());
    }
    for text in [
        &r.baseline_method,
        &r.baseline_model,
        &r.matched_rationale,
        &r.decision_criterion,
    ] {
        valid_text(text, 3000)?;
    }
    for t in [&r.topic_a, &r.topic_b] {
        valid_text(&t.name, 300)?;
        valid_text(&t.prior_knowledge, 2000)?;
        if t.items.len() != 2
            || !t.items.iter().any(|i| i.form == "familiar")
            || !t.items.iter().any(|i| i.form == "changed")
        {
            return Err("Prepare exactly one familiar-form and one changed-context held-out item for each topic before tutoring.".into());
        }
        if t.source_ids.len() > 10 {
            return Err("Choose at most ten source identities per topic".into());
        }
        for source in &t.source_ids {
            let present: bool = c
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sources WHERE id=?)",
                    [source],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            if !present {
                return Err("Unknown source in comparison registration".into());
            }
        }
        for i in &t.items {
            for text in [
                &i.prompt,
                &i.rubric,
                &i.reference_solution,
                &i.review_provenance,
            ] {
                valid_text(text, 10000)?;
            }
            if !(1..=20).contains(&i.max_points) {
                return Err("Held-out item maximum must be 1–20 points".into());
            }
        }
    }
    Ok(())
}
impl Store {
    pub fn evaluation_register(&mut self, r: Registration) -> Result<String> {
        validate_registration(&self.conn, &r)?;
        let pid = id();
        let assignment = id();
        let a_tool = assignment.as_bytes()[0].is_multiple_of(2);
        let tool_first = assignment.as_bytes()[1].is_multiple_of(2);
        let versions = j!({"app":env!("CARGO_PKG_VERSION"),"build_source":env!("STUDY_HARNESS_BUILD_SOURCE"),"model":self.model_choice()?.model.id,"baseline_model":r.baseline_model,"tutor_prompt_sha256":crate::sources::sha(crate::providers::PROMPT.as_bytes()),"content_version":crate::curriculum::CONTENT_VERSION,"content_sha256":crate::sources::sha(include_bytes!("../../content/unit-v1.json")),"scheduler":crate::learning::SCHEDULER,"policy":crate::learning::POLICY,"visualizer_manifest_sha256":crate::sources::sha(include_bytes!("../../vendor/visualizer/manifest.json")),"protocol":VERSION});
        let pair = j!({"id":pid,"registered_at":self.time(),"simulated":self.offline||self.clock.simulated(),"registration":r,"assignment_draw":assignment,"order":if tool_first{"Study Harness first"}else{"baseline first"},"versions":versions,"claim":"Registration and records only; no learning benefit or independent topic equivalence established."});
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO evaluation_pairs VALUES (?,?)",
            params![pid, pair.to_string()],
        )
        .map_err(sql)?;
        for (which, condition) in [
            ("topic_a", if a_tool { "Study Harness" } else { "baseline" }),
            ("topic_b", if a_tool { "baseline" } else { "Study Harness" }),
        ] {
            let tid = id();
            let mut t = pair["registration"][which].clone();
            t["id"] = j!(tid);
            t["condition"] = j!(condition);
            t["simulated"] = pair["simulated"].clone();
            for item in t["items"].as_array_mut().unwrap() {
                item["id"] = j!(id());
            }
            tx.execute(
                "INSERT INTO evaluation_topics VALUES (?,?,?)",
                params![tid, pid, t.to_string()],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)?;
        Ok(pid)
    }
    fn evaluation_evidence(&self, tid: &str) -> Result<Value> {
        let sid: Option<String> = self
            .conn
            .query_row(
                "SELECT session_id FROM evaluation_links WHERE topic_id=?",
                [tid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if let Some(sid) = sid {
            let attempts = self.attempts(&sid)?;
            let assistance = self.assistance(&sid)?;
            let requests = self.requests(&sid)?;
            let usage = self.usage(&sid)?;
            Ok(
                j!({"session_id":sid,"attempts":attempts,"assistance":assistance,"requests":requests,"usage":usage,"request_versions":self.request_versions(&sid)?,"source_citations":self.source_view(&sid)?.citations,"conditions":self.learning_view(&sid)?.map(|v|v.conditions)}),
            )
        } else {
            Ok(
                j!({"session_id":null,"note":"External baseline study and costs are manually recorded; no tool logs are inferred."}),
            )
        }
    }
    pub fn evaluation_view(&self, full: bool) -> Result<Value> {
        let mut s = self
            .conn
            .prepare("SELECT id,data FROM evaluation_pairs ORDER BY rowid DESC")
            .map_err(sql)?;
        let pairs = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql)?;
        let mut result = vec![];
        for (pid, raw) in pairs {
            let mut pair: Value = parse(raw)?;
            // Registration contains solutions, so only the explicit full research export includes it.
            if !full {
                pair["registration"]
                    .as_object_mut()
                    .unwrap()
                    .remove("topic_a");
                pair["registration"]
                    .as_object_mut()
                    .unwrap()
                    .remove("topic_b");
            }
            let mut topics = vec![];
            for mut t in rows(&self.conn, "evaluation_topics", "pair_id", &pid)? {
                let tid = required(&t, "id")?.to_owned();
                let mut source_documents = vec![];
                for source in t["source_ids"]
                    .as_array()
                    .ok_or("Missing registered sources")?
                {
                    if let Some(id) = source.as_str()
                        && let Some(raw) = one(&self.conn, "sources", "id", id)?
                    {
                        source_documents.push(raw);
                    }
                }
                t["source_documents"] = j!(source_documents);
                let responses = one(&self.conn, "evaluation_responses", "topic_id", &tid)?;
                let exam = one(&self.conn, "evaluation_exams", "topic_id", &tid)?;
                if !full && responses.is_none() {
                    if exam.is_none() {
                        t.as_object_mut().unwrap().remove("items");
                    } else {
                        for item in t["items"].as_array_mut().unwrap() {
                            item.as_object_mut().unwrap().remove("reference_solution");
                            item.as_object_mut().unwrap().remove("rubric");
                        }
                    }
                }
                t["lesson"] = j!(one(&self.conn, "evaluation_lessons", "topic_id", &tid)?);
                t["timer"] = j!(one(&self.conn, "evaluation_timers", "topic_id", &tid)?);
                t["events"] = j!(rows(&self.conn, "evaluation_events", "topic_id", &tid)?);
                t["exam"] = j!(exam);
                t["responses"] = j!(responses);
                t["scores"] = j!(rows(&self.conn, "evaluation_scores", "topic_id", &tid)?);
                t["evidence"] = self.evaluation_evidence(&tid)?;
                let due = t["lesson"]["assessment_due_at"]
                    .as_str()
                    .and_then(|s| stamp(s).ok());
                t["assessment_ready"] =
                    j!(due.is_some_and(|at| self.clock.now() >= at) && t["responses"].is_null());
                t["planned_review_excluded"] = j!(t["lesson"]["review_exclusion_at"]
                    .as_str()
                    .and_then(|s| stamp(s).ok())
                    .is_some_and(|at| self.clock.now() >= at)
                    && t["responses"].is_null());
                // Exposures within the window remain visible as protocol deviations, never silently excluded.
                let from = t["lesson"]["review_exclusion_at"]
                    .as_str()
                    .and_then(|s| stamp(s).ok());
                let until = t["responses"]["submitted_at"]
                    .as_str()
                    .and_then(|s| stamp(s).ok())
                    .unwrap_or(self.clock.now());
                let automatic = t["evidence"]["assistance"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|a| {
                                a["visible_at"]
                                    .as_str()
                                    .and_then(|s| stamp(s).ok())
                                    .is_some_and(|at| from.is_some_and(|f| at >= f) && at <= until)
                            })
                            .count()
                    })
                    .unwrap_or(0);
                t["recorded_exposures_in_exclusion_window"] = j!(automatic);
                topics.push(t);
            }
            pair["topics"] = j!(topics);
            result.push(pair);
        }
        Ok(
            j!({"protocol":VERSION,"now":self.time(),"simulated":self.offline||self.clock.simulated(),"pairs":result,"notice":"Six to ten genuinely comparable topic pairs over several weeks are a planning target. Registration is not proof of comparability. Do not count several objectives from one lesson as independent pairs. Actual benefit remains untested."}),
        )
    }
    pub fn evaluation_rpc(&mut self, bytes: &[u8]) -> Result<Value> {
        if bytes.len() > 500_000 {
            return Err("Evaluation request exceeds its bound".into());
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            method: String,
            request: Value,
        }
        let e: Envelope =
            serde_json::from_slice(bytes).map_err(|_| "Invalid evaluation request")?;
        let q = &e.request;
        let fields: &[&str] = match e.method.as_str() {
            "view" => &[],
            "register" => &[
                "topic_a",
                "topic_b",
                "baseline_method",
                "baseline_model",
                "matched_rationale",
                "decision_criterion",
                "time_allowance_minutes",
                "confirm_distinct_topics",
            ],
            "link" => &["topicId", "sessionId"],
            "timer_start" => &["topicId", "kind"],
            "timer_stop" | "finish_lesson" | "begin_exam" => &["topicId"],
            "event" => &["topicId", "kind", "note", "seconds", "cost_microusd"],
            "submit_exam" => &["topicId", "responses", "outside_help"],
            "score" => &["topicId", "reviewer", "blinded", "scores"],
            _ => return Err("Unsupported evaluation operation".into()),
        };
        if q.as_object().is_none_or(|o| {
            o.len() != fields.len() || o.keys().any(|k| !fields.contains(&k.as_str()))
        }) {
            return Err("Invalid evaluation operation fields".into());
        }
        if e.method == "view" {
            return self.evaluation_view(false);
        }
        if e.method == "register" {
            let pid = self.evaluation_register(
                serde_json::from_value(q.clone()).map_err(|_| "Invalid comparison registration")?,
            )?;
            return Ok(j!({"pair_id":pid}));
        }
        let tid = required(q, "topicId")?;
        let t = topic(&self.conn, tid)?;
        let time = self.time();
        match e.method.as_str() {
            "link" => {
                let sid = required(q, "sessionId")?;
                self.session(sid)?;
                if t["condition"] != "Study Harness" {
                    return Err("Baseline work is recorded externally; it cannot be linked to a tutoring session.".into());
                }
                if one(&self.conn, "evaluation_lessons", "topic_id", tid)?.is_some() {
                    return Err(
                        "Link the study session before completing its initial lesson".into(),
                    );
                }
                self.conn
                    .execute(
                        "INSERT INTO evaluation_links VALUES (?,?)",
                        params![tid, sid],
                    )
                    .map_err(sql)?;
            }
            "timer_start" => {
                let kind = required(q, "kind")?;
                if !["study", "review", "setup", "transcript_correction"].contains(&kind) {
                    return Err("Unknown time category".into());
                }
                if kind == "review" && self.topic_excluded(tid)? {
                    return Err(
                        "Planned review is excluded during the 48-hour window before assessment."
                            .into(),
                    );
                }
                let timer = j!({"kind":kind,"started_at":time,"simulated":self.offline||self.clock.simulated()});
                self.conn
                    .execute(
                        "INSERT INTO evaluation_timers VALUES (?,?)",
                        params![tid, timer.to_string()],
                    )
                    .map_err(sql)?;
            }
            "timer_stop" => {
                let timer = one(&self.conn, "evaluation_timers", "topic_id", tid)?
                    .ok_or("No timer is running")?;
                let seconds =
                    (self.clock.now() - stamp(required(&timer, "started_at")?)?).num_seconds();
                if seconds < 0 {
                    return Err("Clock moved backwards; retain and inspect the timer instead of inventing elapsed time.".into());
                }
                let event = j!({"id":id(),"kind":timer["kind"],"started_at":timer["started_at"],"ended_at":time,"seconds":seconds,"timing":"native elapsed wall clock; includes any time while the app was closed","simulated":self.offline||self.clock.simulated()});
                let tx = self.conn.transaction().map_err(sql)?;
                tx.execute(
                    "INSERT INTO evaluation_events VALUES (?,?,?)",
                    params![event["id"].as_str(), tid, event.to_string()],
                )
                .map_err(sql)?;
                tx.execute("DELETE FROM evaluation_timers WHERE topic_id=?", [tid])
                    .map_err(sql)?;
                tx.commit().map_err(sql)?;
            }
            "event" => {
                let kind = required(q, "kind")?;
                if ![
                    "outside_exposure",
                    "waiting",
                    "unwanted_help",
                    "repeated_question",
                    "abandoned",
                    "voluntary_return",
                    "manual_time_and_cost",
                ]
                .contains(&kind)
                {
                    return Err("Unknown comparison observation".into());
                }
                let note = required(q, "note")?;
                valid_text(note, 5000)?;
                let seconds = q["seconds"]
                    .as_u64()
                    .filter(|n| *n <= 86400)
                    .ok_or("Use 0–86400 seconds for a manual observation")?;
                let cost = q["cost_microusd"]
                    .as_u64()
                    .filter(|n| *n <= 1_000_000_000)
                    .ok_or("Invalid manually reported cost")?;
                let event = j!({"id":id(),"kind":kind,"note":note,"seconds":seconds,"cost_microusd":cost,"recorded_at":time,"timing":"manually reported; not inferred from app activity","simulated":self.offline||self.clock.simulated()});
                self.conn
                    .execute(
                        "INSERT INTO evaluation_events VALUES (?,?,?)",
                        params![event["id"].as_str(), tid, event.to_string()],
                    )
                    .map_err(sql)?;
            }
            "finish_lesson" => {
                if one(&self.conn, "evaluation_timers", "topic_id", tid)?.is_some() {
                    return Err("Stop the active timer before completing the lesson".into());
                }
                let at = self.clock.now();
                let lesson = j!({"completed_at":time,"assessment_due_at":(at+Duration::days(7)).to_rfc3339(),"review_exclusion_at":(at+Duration::days(5)).to_rfc3339(),"timing_definition":"seven days after initial lesson; planned review excluded for preceding 48 hours; later tests retain actual delay","simulated":self.offline||self.clock.simulated()});
                self.conn
                    .execute(
                        "INSERT INTO evaluation_lessons VALUES (?,?)",
                        params![tid, lesson.to_string()],
                    )
                    .map_err(sql)?;
            }
            "begin_exam" => {
                let lesson = one(&self.conn, "evaluation_lessons", "topic_id", tid)?
                    .ok_or("Complete the initial lesson first")?;
                if self.clock.now() < stamp(required(&lesson, "assessment_due_at")?)? {
                    return Err("The held-out assessment is available seven days after the initial lesson. Its questions remain outside tutoring retrieval.".into());
                }
                if one(&self.conn, "evaluation_timers", "topic_id", tid)?.is_some() {
                    return Err(
                        "Stop the active timer before beginning the held-out assessment".into(),
                    );
                }
                self.conn.execute("INSERT OR IGNORE INTO evaluation_exams VALUES (?,?)",params![tid,j!({"started_at":time,"simulated":self.offline||self.clock.simulated()}).to_string()]).map_err(sql)?;
            }
            "submit_exam" => {
                let exam = one(&self.conn, "evaluation_exams", "topic_id", tid)?
                    .ok_or("Begin the held-out assessment first")?;
                let responses: Vec<Response> = serde_json::from_value(q["responses"].clone())
                    .map_err(|_| "Invalid held-out responses")?;
                if responses.len() != 2 {
                    return Err("Respond to both held-out items".into());
                }
                let mut seen = std::collections::BTreeSet::new();
                for r in &responses {
                    valid_text(&r.response, 12000)?;
                    if r.confidence > 100
                        || !seen.insert(&r.item_id)
                        || !t["items"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|i| i["id"] == r.item_id)
                    {
                        return Err("Response does not match its frozen held-out item".into());
                    }
                }
                let outside = q["outside_help"]
                    .as_bool()
                    .ok_or("Record whether any help was used")?;
                let seconds =
                    (self.clock.now() - stamp(required(&exam, "started_at")?)?).num_seconds();
                if seconds < 0 {
                    return Err("Assessment clock moved backwards".into());
                }
                let record = j!({"submitted_at":time,"responses":q["responses"],"outside_help":outside,"elapsed_seconds":seconds,"simulated":self.offline||self.clock.simulated(),"grading":"awaiting separately recorded self-score or subject review; no tutor assessment"});
                self.conn
                    .execute(
                        "INSERT INTO evaluation_responses VALUES (?,?)",
                        params![tid, record.to_string()],
                    )
                    .map_err(sql)?;
            }
            "score" => {
                if one(&self.conn, "evaluation_responses", "topic_id", tid)?.is_none() {
                    return Err("Save the unaided responses before opening scoring material".into());
                }
                let reviewer = required(q, "reviewer")?;
                valid_text(reviewer, 300)?;
                let scores: Vec<Score> = serde_json::from_value(q["scores"].clone())
                    .map_err(|_| "Invalid manual scores")?;
                if scores.len() != 2 {
                    return Err("Record each item’s score separately".into());
                }
                let mut seen = std::collections::BTreeSet::new();
                for s in &scores {
                    let item = t["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|i| i["id"] == s.item_id)
                        .ok_or("Unknown score item")?;
                    if !s.points.is_finite()
                        || s.points < 0.
                        || s.points > item["max_points"].as_f64().unwrap()
                        || !seen.insert(&s.item_id)
                    {
                        return Err("Score is outside the frozen rubric scale".into());
                    }
                    valid_text(&s.notes, 3000)?;
                }
                let record = j!({"id":id(),"recorded_at":time,"reviewer":reviewer,"blinded":q["blinded"].as_bool().ok_or("Record whether the reviewer was blinded")?,"scores":q["scores"],"provenance":"user-entered review or self-score; not verified by the app","simulated":self.offline||self.clock.simulated()});
                self.conn
                    .execute(
                        "INSERT INTO evaluation_scores VALUES (?,?,?)",
                        params![record["id"].as_str(), tid, record.to_string()],
                    )
                    .map_err(sql)?;
            }
            _ => return Err("Unsupported evaluation operation".into()),
        }
        self.evaluation_view(false)
    }
    fn topic_excluded(&self, tid: &str) -> Result<bool> {
        let lesson = one(&self.conn, "evaluation_lessons", "topic_id", tid)?;
        Ok(lesson
            .as_ref()
            .and_then(|v| v["review_exclusion_at"].as_str())
            .and_then(|s| stamp(s).ok())
            .is_some_and(|at| self.clock.now() >= at)
            && one(&self.conn, "evaluation_responses", "topic_id", tid)?.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, TestClock};
    use std::sync::{Arc, Mutex};
    fn registration() -> Registration {
        let t = |name: &str| Topic {
            name: name.into(),
            prior_knowledge: "No prior knowledge; synthetic engineering fixture only".into(),
            source_ids: vec![],
            items: ["familiar", "changed"]
                .iter()
                .map(|f| Item {
                    form: (*f).into(),
                    prompt: format!("SECRET-HELD-OUT {name} {f}"),
                    rubric: "Award five for a complete justified answer; fixture only".into(),
                    reference_solution:
                        "Known synthetic reference, not a reviewed subject assessment".into(),
                    max_points: 5,
                    review_provenance: "Pending subject review; engineering fixture".into(),
                })
                .collect(),
        };
        Registration {
            topic_a: t("Separate synthetic topic A"),
            topic_b: t("Separate synthetic topic B"),
            baseline_method: "Strong current study method".into(),
            baseline_model: "Same model if used; otherwise no model".into(),
            matched_rationale: "Two independent synthetic fixtures, not objectives in one lesson"
                .into(),
            decision_criterion: "Pre-registered comparison; no expected benefit claimed".into(),
            time_allowance_minutes: 30,
            confirm_distinct_topics: true,
        }
    }
    fn rpc(d: &mut Store, method: &str, request: Value) -> Value {
        d.evaluation_rpc(&serde_json::to_vec(&j!({"method":method,"request":request})).unwrap())
            .unwrap()
    }
    #[test]
    fn held_out_collection_timing_and_manual_review_are_separate_from_tutoring() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestClock(Mutex::new(
            "2026-01-01T00:00:00Z".parse().unwrap(),
        )));
        let mut d = Store::open_with(&dir.path().join("study.db"), true, clock.clone()).unwrap();
        let sid = d.new_session("Synthetic comparison").unwrap();
        d.start_goal(&sid, 0, "Convergence").unwrap();
        d.evaluation_register(registration()).unwrap();
        let view = d.evaluation_view(false).unwrap();
        assert!(!view.to_string().contains("SECRET-HELD-OUT"));
        let topics = view["pairs"][0]["topics"].as_array().unwrap();
        let tid = topics
            .iter()
            .find(|t| t["condition"] == "Study Harness")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        rpc(&mut d, "link", j!({"topicId":tid,"sessionId":sid}));
        rpc(&mut d, "timer_start", j!({"topicId":tid,"kind":"study"}));
        *clock.0.lock().unwrap() += Duration::minutes(30);
        rpc(&mut d, "timer_stop", j!({"topicId":tid}));
        rpc(&mut d, "finish_lesson", j!({"topicId":tid}));
        assert!(
            d.evaluation_rpc(
                &serde_json::to_vec(&j!({"method":"begin_exam","request":{"topicId":tid}}))
                    .unwrap()
            )
            .is_err()
        );
        assert!(d.source_search("SECRET-HELD-OUT").unwrap().is_empty());
        *clock.0.lock().unwrap() += Duration::days(5);
        assert!(
            review_exclusion(&d.conn, &sid, &d.time())
                .unwrap()
                .is_some()
        );
        assert!(
            d.learning_action(&sid, d.session(&sid).unwrap().revision, "fresh_check")
                .is_err()
        );
        assert!(
            d.evaluation_rpc(
                &serde_json::to_vec(
                    &j!({"method":"timer_start","request":{"topicId":tid,"kind":"review"}})
                )
                .unwrap()
            )
            .is_err()
        );
        rpc(
            &mut d,
            "event",
            j!({"topicId":tid,"kind":"outside_exposure","note":"Synthetic exposure must remain visible as a protocol deviation","seconds":60,"cost_microusd":0}),
        );
        *clock.0.lock().unwrap() += Duration::days(2);
        rpc(&mut d, "begin_exam", j!({"topicId":tid}));
        let t = topic(&d.conn, &tid).unwrap();
        let responses = t["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| j!({"item_id":i["id"],"response":"Synthetic held-out answer","confidence":50}))
            .collect::<Vec<_>>();
        rpc(
            &mut d,
            "submit_exam",
            j!({"topicId":tid,"responses":responses,"outside_help":false}),
        );
        assert!(
            review_exclusion(&d.conn, &sid, &d.time())
                .unwrap()
                .is_none()
        );
        let scores=t["items"].as_array().unwrap().iter().map(|i|j!({"item_id":i["id"],"points":3,"notes":"Manual synthetic score, not real learning evidence"})).collect::<Vec<_>>();
        rpc(
            &mut d,
            "score",
            j!({"topicId":tid,"scores":scores,"reviewer":"Self-score fixture","blinded":false}),
        );
        assert!(
            d.learning_view(&sid)
                .unwrap()
                .unwrap()
                .assessments
                .is_empty()
        );
        let export = d.evaluation_view(true).unwrap();
        assert!(export.to_string().contains("SECRET-HELD-OUT"));
        assert_eq!(export["pairs"][0]["simulated"], true);
        let bytes = d.export_archive().unwrap();
        let mut fresh =
            Store::open_with(&dir.path().join("restored.db"), true, clock.clone()).unwrap();
        fresh.restore_archive(&bytes, None).unwrap();
        assert_eq!(
            fresh.evaluation_view(true).unwrap()["pairs"],
            export["pairs"]
        );
        fresh.delete_owned("session", &sid, "DELETE").unwrap();
        assert!(
            fresh.evaluation_view(true).unwrap()["pairs"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fresh
                .conn
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        assert_eq!(
            clock
                .now()
                .signed_duration_since("2026-01-01T00:30:00Z".parse::<DateTime<Utc>>().unwrap())
                .num_days(),
            7
        );
    }
    #[test]
    fn registration_refuses_pseudo_replication_and_heldout_mutation() {
        let mut d = Store::open_with(
            std::path::Path::new(":memory:"),
            true,
            Arc::new(crate::clock::SystemClock),
        )
        .unwrap();
        let mut r = registration();
        r.confirm_distinct_topics = false;
        assert!(d.evaluation_register(r).is_err());
        let mut r = registration();
        r.topic_b.name = r.topic_a.name.clone();
        assert!(d.evaluation_register(r).is_err());
        d.evaluation_register(registration()).unwrap();
        assert!(
            d.conn
                .execute("UPDATE evaluation_topics SET data='{}'", [])
                .is_err()
        );
        assert!(
            d.evaluation_rpc(br#"{"method":"view","request":{"human_approved":true}}"#)
                .is_err()
        );
    }
}
