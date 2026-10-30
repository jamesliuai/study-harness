use crate::{
    contracts::*,
    registry::{self, ModelChoice, OPENAI, Purpose},
    scene,
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use uuid::Uuid;
pub type Result<T> = std::result::Result<T, String>;
pub fn now() -> String {
    Utc::now().to_rfc3339()
}
pub fn id() -> String {
    Uuid::new_v4().to_string()
}
pub(crate) fn json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}
pub(crate) fn parse<T: DeserializeOwned>(s: String) -> Result<T> {
    serde_json::from_str(&s).map_err(|e| e.to_string())
}
pub(crate) fn sql(e: rusqlite::Error) -> String {
    format!("Local database error: {e}")
}
/// Records a request's final status and its usage (actual or estimated) in `tx`. Every request keeps its billing
/// record whether its proposal was accepted, rejected, stale or cancelled.
pub(crate) fn close_request(
    tx: &Connection,
    r: &Request,
    status: &str,
    usage: &Usage,
) -> Result<()> {
    if usage.request_id != r.id
        || usage.cost_microusd < 0
        || (r.kind.starts_with("offline:") && usage.cost_microusd != 0)
    {
        return Err("Usage does not match request mode.".into());
    }
    let stored: Request = parse(
        tx.query_row("SELECT data FROM requests WHERE id=?", [&r.id], |row| {
            row.get(0)
        })
        .map_err(sql)?,
    )?;
    if stored.status != "pending" {
        return Err("Request already settled.".into());
    }
    let mut record = stored;
    record.status = status.into();
    let mut usage = usage.clone();
    usage.status = status.into();
    tx.execute(
        "UPDATE requests SET status=?,data=? WHERE id=?",
        params![record.status, json(&record)?, r.id],
    )
    .map_err(sql)?;
    if let Some(model) = &usage.reported_model {
        tx.execute("UPDATE request_versions SET data=json_set(data,'$.reported_model',?) WHERE request_id=?",params![model,r.id]).map_err(sql)?;
    }
    tx.execute(
        "INSERT INTO usage VALUES (?,?,?)",
        params![r.id, usage.cost_microusd, json(&usage)?],
    )
    .map_err(sql)?;
    Ok(())
}
pub struct Store {
    pub conn: Connection,
    pub path: String,
    pub offline: bool,
    pub clock: std::sync::Arc<dyn crate::clock::Clock>,
    /// Provider status since launch; never persisted, exported or sent anywhere.
    pub provider: ProviderMemory,
}
/// Plain-language provider outcomes since launch: when the key last changed, when an opt-in access check passed,
/// and the latest failure (category, message, time). Contains no secret, prompt or learner text.
#[derive(Default, Clone)]
pub struct ProviderMemory {
    pub changed_at: Option<String>,
    pub checked_at: Option<String>,
    pub error: Option<(String, String, String)>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, false, std::sync::Arc::new(crate::clock::SystemClock))
    }
    pub fn time(&self) -> String {
        self.clock.now().to_rfc3339()
    }
    pub fn open_with(
        path: &Path,
        offline: bool,
        clock: std::sync::Arc<dyn crate::clock::Clock>,
    ) -> Result<Self> {
        let conn = Connection::open(path).map_err(sql)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA busy_timeout=5000;",
        )
        .map_err(sql)?;
        let v: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(sql)?;
        if v > crate::ownership::CURRENT_SCHEMA {
            return Err("Database created by a newer Study Harness. Keep it unchanged and use the newer application.".into());
        }
        let mode = if offline {
            "offline-development"
        } else {
            "live"
        };
        let stored: Option<String> = if v == 0 {
            None
        } else {
            conn.query_row(
                "SELECT value FROM settings WHERE key='data_mode'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?
        };
        if stored.as_deref().is_some_and(|s| s != json(&mode).unwrap()) {
            return Err(
                "Data mode mismatch: offline scenarios cannot open real learner data.".into(),
            );
        }
        if v > 0 && offline && stored.is_none() {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .map_err(sql)?;
            if count > 0 {
                return Err(
                    "Offline mode requires its own empty or explicitly offline database.".into(),
                );
            }
        }
        crate::ownership::migration_backup(&conn, path, v)?;
        if v == 0 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/001_foundation.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 2 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/002_transcript_recovery.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 3 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/003_learning.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 4 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/004_visualizer.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 5 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/005_unit.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 6 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/006_sources.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        crate::curriculum::install(&conn)?;
        if v < 7 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/007_evaluation.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 8 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/008_request_versions.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        if v < 9 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../migrations/009_goals.sql"),
                "COMMIT;"
            ))
            .map_err(sql)?;
        }
        conn.execute(
            "INSERT OR IGNORE INTO settings VALUES ('data_mode',?)",
            [json(&mode)?],
        )
        .map_err(sql)?;
        let mut s = Self {
            offline,
            clock,
            conn,
            path: path.to_string_lossy().into(),
            provider: ProviderMemory::default(),
        };
        s.restore_clock()?;
        s.recover()?;
        crate::visualizer::recover(&s.conn, &s.time())?;
        Ok(s)
    }
    fn all<T: DeserializeOwned>(&self, query: &str, arg: &str) -> Result<Vec<T>> {
        let mut st = self.conn.prepare(query).map_err(sql)?;
        let rows = st
            .query_map([arg], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| parse(r.map_err(sql)?)).collect()
    }
    pub fn sessions(&self) -> Result<Vec<Session>> {
        self.all(
            "SELECT data FROM sessions WHERE ?1 IS NOT NULL ORDER BY rowid DESC",
            "",
        )
    }
    pub fn session(&self, id: &str) -> Result<Session> {
        parse(
            self.conn
                .query_row("SELECT data FROM sessions WHERE id=?", [id], |r| r.get(0))
                .map_err(sql)?,
        )
    }
    pub fn activity(&self, sid: &str) -> Result<Activity> {
        let v: i32 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(sql)?;
        if v >= 3
            && let Some(flow) = self.learning_state(sid)?
        {
            return parse(
                self.conn
                    .query_row(
                        "SELECT data FROM activities WHERE id=?",
                        [flow.active_activity_id],
                        |r| r.get(0),
                    )
                    .map_err(sql)?,
            );
        }
        parse(
            self.conn
                .query_row(
                    "SELECT data FROM activities WHERE session_id=? ORDER BY rowid LIMIT 1",
                    [sid],
                    |r| r.get(0),
                )
                .map_err(sql)?,
        )
    }
    pub fn scene(&self, aid: &str) -> Result<SceneSpec> {
        parse(
            self.conn
                .query_row("SELECT data FROM scenes WHERE activity_id=?", [aid], |r| {
                    r.get(0)
                })
                .map_err(sql)?,
        )
    }
    pub fn new_session(&mut self, title: &str) -> Result<String> {
        valid_text(title, 120)?;
        let sid = id();
        let aid = id();
        let s = Session {
            schema_version: 1,
            id: sid.clone(),
            title: title.trim().into(),
            revision: 0,
            created_at: self.time(),
            draft: String::new(),
        };
        // A session starts with a neutral placeholder until a goal or capability gives it a real task.
        let a = Activity {
            schema_version: 1,
            id: aid.clone(),
            session_id: sid.clone(),
            objective_id: "unstarted".into(),
            version: 1,
            prompt: "No study goal yet.".into(),
            math: String::new(),
            task_family: "unstarted-v1".into(),
            rubric_version: "none".into(),
            validation_status: "Placeholder before a goal; never assessed".into(),
        };
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT OR IGNORE INTO objectives VALUES ('unstarted', ?)",
            [r#"{"schema_version":1,"outcome":"No study goal yet","origin":"placeholder"}"#],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO sessions VALUES (?,0,?)",
            params![sid, json(&s)?],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO activities VALUES (?,?,?,?,?,?,?)",
            params![
                aid,
                sid,
                a.objective_id,
                1,
                json(&a)?,
                "No reference: placeholder before a goal.",
                "none"
            ],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO scenes VALUES (?,?)",
            params![aid, json(&scene::sample(&aid))?],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(sid)
    }
    pub fn attempts(&self, sid: &str) -> Result<Vec<Attempt>> {
        self.all(
            "SELECT data FROM attempts WHERE session_id=? ORDER BY rowid",
            sid,
        )
    }
    pub fn assistance(&self, sid: &str) -> Result<Vec<Assistance>> {
        self.all(
            "SELECT data FROM assistance WHERE session_id=? ORDER BY rowid",
            sid,
        )
    }
    pub fn requests(&self, sid: &str) -> Result<Vec<Request>> {
        self.all(
            "SELECT data FROM requests WHERE session_id=? ORDER BY rowid DESC LIMIT 30",
            sid,
        )
    }
    pub fn usage(&self, sid: &str) -> Result<Vec<Usage>> {
        self.all("SELECT u.data FROM usage u JOIN requests r ON u.request_id=r.id WHERE r.session_id=? ORDER BY r.rowid DESC LIMIT 30",sid)
    }
    pub fn save_draft(&self, sid: &str, text: &str) -> Result<()> {
        if text.len() > 12000 {
            return Err("Draft is too long.".into());
        }
        let mut s = self.session(sid)?;
        s.draft = text.into();
        self.conn
            .execute(
                "UPDATE sessions SET data=? WHERE id=?",
                params![json(&s)?, sid],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn pending_transcript(&self, sid: &str) -> Result<Option<Transcript>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM pending_transcripts WHERE session_id=?",
                [sid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        raw.map(parse).transpose()
    }
    pub fn save_transcript(&mut self, sid: &str, value: &Transcript) -> Result<()> {
        valid_text(&value.recognized_text, 12000)?;
        let mut session = self.session(sid)?;
        session.draft = value.recognized_text.clone();
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO pending_transcripts VALUES (?,?,?)",
            params![sid, value.id, json(value)?],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE sessions SET data=? WHERE id=?",
            params![json(&session)?, sid],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)
    }
    pub fn discard_transcript(&mut self, sid: &str, tid: &str) -> Result<()> {
        let mut session = self.session(sid)?;
        let Some(t) = self.pending_transcript(sid)? else {
            return Ok(());
        };
        if t.id != tid {
            return Err("Transcript does not belong to this session.".into());
        }
        session.draft.clear();
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "DELETE FROM pending_transcripts WHERE session_id=? AND transcript_id=?",
            params![sid, tid],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE sessions SET data=? WHERE id=?",
            params![json(&session)?, sid],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)
    }
    pub fn save_attempt(&mut self, input: &SaveAttempt) -> Result<Attempt> {
        self.save_attempt_with_visual(input, None)
    }
    pub fn save_attempt_with_visual(
        &mut self,
        input: &SaveAttempt,
        visual: Option<&serde_json::Value>,
    ) -> Result<Attempt> {
        self.save_submission(input, visual, "answer")
    }
    /// Saves an answer or a question. The intent is frozen with the submission conditions; a question never
    /// counts as a diagnostic attempt, is never graded and never satisfies a stage's answer requirement.
    pub fn save_submission(
        &mut self,
        input: &SaveAttempt,
        visual: Option<&serde_json::Value>,
        intent: &str,
    ) -> Result<Attempt> {
        valid_text(&input.response, 12000)?;
        Uuid::parse_str(&input.submission_id).map_err(|_| "Invalid submission ID.")?;
        let existing: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT data,transcript_id FROM attempts WHERE submission_id=?",
                [&input.submission_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        if let Some((raw, tid)) = existing {
            let a: Attempt = parse(raw)?;
            if a.session_id != input.session_id
                || a.response != input.response.trim()
                || tid != input.transcript_id
                || crate::learning::intent(&self.conn, &a.id)? != intent
            {
                return Err("Submission ID already belongs to a different response.".into());
            }
            return Ok(a);
        }
        self.validate_submission_for(&input.session_id, intent)?;
        let recognized = match (
            self.pending_transcript(&input.session_id)?,
            &input.transcript_id,
        ) {
            (Some(t), Some(tid)) if &t.id == tid => Some(t.recognized_text),
            (None, None) => None,
            (Some(_), None) => {
                return Err(
                    "Confirm or discard the pending transcript before saving a typed response."
                        .into(),
                );
            }
            _ => {
                return Err("Transcript is unavailable or does not belong to this session.".into());
            }
        };
        let mut s = self.session(&input.session_id)?;
        if s.revision != input.expected_revision {
            return Err("Session changed. Refresh before saving.".into());
        }
        let a = self.activity(&s.id)?;
        let help = self
            .assistance(&s.id)?
            .into_iter()
            .filter(|h| h.activity_id == a.id)
            .collect::<Vec<_>>();
        let at = Attempt {
            schema_version: 1,
            id: input.submission_id.clone(),
            session_id: s.id.clone(),
            activity_id: a.id,
            task_version: a.version,
            response: input.response.trim().into(),
            recognized_transcript: recognized.clone(),
            modality: if recognized.is_some() {
                "recorded_speech"
            } else {
                "text"
            }
            .into(),
            created_at: self.time(),
            assistance_ids: help.iter().map(|h| h.id.clone()).collect(),
            exposure_ids: help
                .iter()
                .filter(|h| h.depth == "visual")
                .map(|h| h.id.clone())
                .collect(),
            revision_of: None,
        };
        let conditions = self.submission_conditions_as(&at, intent)?;
        let visual_conditions = crate::visualizer::attempt_snapshot(&self.conn, &at, visual)?;
        let mut flow = self.learning_state(&s.id)?;
        if let Some(f) = flow.as_mut()
            && f.stage == "diagnosis"
            && intent == "answer"
        {
            f.diagnosis_count += 1;
        }
        s.revision += 1;
        s.draft.clear();
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO attempts (id,session_id,activity_id,submission_id,data,transcript_id) VALUES (?,?,?,?,?,?)",
            params![at.id, s.id, at.activity_id, input.submission_id, json(&at)?, input.transcript_id],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE sessions SET revision=?,data=? WHERE id=?",
            params![s.revision, json(&s)?, s.id],
        )
        .map_err(sql)?;
        tx.execute(
            "DELETE FROM pending_transcripts WHERE session_id=?",
            [&s.id],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO attempt_conditions VALUES (?,?)",
            params![at.id, json(&conditions)?],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO visual_attempt_snapshots VALUES (?,?)",
            params![at.id, json(&visual_conditions)?],
        )
        .map_err(sql)?;
        if let Some(f) = flow {
            tx.execute(
                "UPDATE learning_sessions SET data=? WHERE session_id=?",
                params![json(&f)?, s.id],
            )
            .map_err(sql)?;
        }
        crate::learning::invalidate(&tx, &s.id)?;
        tx.commit().map_err(sql)?;
        Ok(at)
    }
    pub fn limits(&self) -> Result<Limits> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key='limits'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)?;
        match v {
            Some(s) => parse(s),
            None => Ok(Limits {
                session_microusd: 1_000_000,
                monthly_microusd: 10_000_000,
            }),
        }
    }
    pub fn set_limits(&self, l: &Limits) -> Result<()> {
        if !(0..=100_000_000).contains(&l.session_microusd)
            || !(0..=1_000_000_000).contains(&l.monthly_microusd)
        {
            return Err("Limits must be $0–100 per session and $0–1000 monthly.".into());
        }
        self.conn.execute("INSERT INTO settings VALUES ('limits',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json(l)?]).map_err(sql)?;
        Ok(())
    }
    pub fn budget(&self, sid: &str) -> Result<Budget> {
        let month = self.clock.now().format("%Y-%m").to_string();
        let sum = |query: &str, arg: &str| -> Result<i32> {
            self.conn.query_row(query, [arg], |r| r.get(0)).map_err(sql)
        };
        Ok(Budget {
            session_spent_microusd: sum(
                "SELECT COALESCE(SUM(COALESCE(u.cost_microusd,r.reserved_microusd)),0) FROM requests r LEFT JOIN usage u ON r.id=u.request_id WHERE r.session_id=?",
                sid,
            )?,
            month_spent_microusd: sum(
                "SELECT COALESCE(SUM(COALESCE(u.cost_microusd,r.reserved_microusd)),0) FROM requests r LEFT JOIN usage u ON r.id=u.request_id WHERE substr(r.created_at,1,7)=?",
                &month,
            )?,
            unresolved_count: sum(
                "SELECT COUNT(*) FROM requests r LEFT JOIN usage u ON r.id=u.request_id WHERE r.session_id=? AND (u.request_id IS NULL OR json_extract(u.data,'$.cost_status')='estimated')",
                sid,
            )? as u32,
            limits: self.limits()?,
            month,
        })
    }
    /// Reserves with the learner's current model preference.
    pub fn reserve(
        &mut self,
        sid: &str,
        attempt: Option<&str>,
        kind: &str,
        amount: i32,
    ) -> Result<Request> {
        let choice = self.model_choice()?;
        self.reserve_for(sid, attempt, kind, amount, &choice)
    }
    /// Reserves with the model choice the caller already used to build the payload and price the reservation.
    pub fn reserve_for(
        &mut self,
        sid: &str,
        attempt: Option<&str>,
        kind: &str,
        amount: i32,
        choice: &ModelChoice,
    ) -> Result<Request> {
        let context = match (kind, attempt) {
            ("tutor", Some(attempt_id)) => {
                let at = self
                    .attempts(sid)?
                    .into_iter()
                    .find(|a| a.id == attempt_id)
                    .ok_or("Attempt not found")?;
                self.source_context(sid, &at.response)?
            }
            _ => vec![],
        };
        self.reserve_with(sid, attempt, kind, amount, &context, choice)
    }
    /// Reserves a request with the exact source passages it will carry, so citations stay traceable, and freezes the
    /// model and effort it is sent with.
    pub fn reserve_with(
        &mut self,
        sid: &str,
        attempt: Option<&str>,
        kind: &str,
        amount: i32,
        source_context: &[SourcePassage],
        choice: &ModelChoice,
    ) -> Result<Request> {
        if (amount <= 0 && !self.offline) || amount < 0 || (self.offline && amount != 0) {
            return Err("Invalid reservation.".into());
        }
        let b = self.budget(sid)?;
        if b.session_spent_microusd.saturating_add(amount) > b.limits.session_microusd
            || b.month_spent_microusd.saturating_add(amount) > b.limits.monthly_microusd
        {
            return Err(
                "Request stopped: the conservative reservation exceeds your local spending limit."
                    .into(),
            );
        }
        if let Some(at) = attempt
            && !self.attempts(sid)?.iter().any(|a| a.id == at)
        {
            return Err("Attempt belongs to another session.".into());
        }
        let s = self.session(sid)?;
        let r = Request {
            schema_version: 1,
            id: id(),
            session_id: sid.into(),
            attempt_id: attempt.map(str::to_string),
            session_revision: s.revision,
            kind: if self.offline {
                format!("offline:{kind}")
            } else {
                kind.into()
            },
            status: "pending".into(),
            reserved_microusd: amount,
            created_at: self.time(),
        };
        let activity = self.activity(sid)?;
        let task = self.task_spec(&activity.id)?;
        let flow = self.learning_state(sid)?;
        let (prompt_version, prompt) = match kind {
            "tutor" => (crate::providers::PROMPT_VERSION, crate::providers::PROMPT),
            "plan" => (
                crate::generation::PLAN_PROMPT_VERSION,
                crate::generation::PLAN_PROMPT,
            ),
            "task" => (
                crate::generation::TASK_PROMPT_VERSION,
                crate::generation::TASK_PROMPT,
            ),
            _ => ("not applicable: audio transcription", ""),
        };
        let versions=RequestVersions{request_id:r.id.clone(),app_version:env!("CARGO_PKG_VERSION").into(),build_source:env!("STUDY_HARNESS_BUILD_SOURCE").into(),requested_model:if kind=="transcription"{OPENAI.transcription_model}else{choice.model.id}.into(),reasoning_effort:Purpose::of(kind).map(|p|choice.effort_for(p)).unwrap_or_default().into(),reported_model:None,prompt_version:prompt_version.into(),prompt_sha256:if prompt.is_empty(){String::new()}else{crate::sources::sha(prompt.as_bytes())},content_version:if ["plan","task"].contains(&kind){format!("{kind} generation")}else{task.as_ref().map(|t|t.content_version.clone()).unwrap_or("legacy reference-1".into())},task_version:activity.version,scheduler_version:crate::learning::SCHEDULER.into(),policy_version:flow.map(|s|s.policy_version).unwrap_or("legacy reference-1".into()),visualizer_manifest_sha256:crate::sources::sha(include_bytes!("../../vendor/visualizer/manifest.json")),recorded_at:r.created_at.clone(),provenance:if self.offline{"Synthetic boundary; no actual model was called"}else{"Native request reservation; reported model is populated only if the provider returns an identifier"}.into()};
        let tx = self.conn.transaction().map_err(sql)?;
        tx.execute(
            "INSERT INTO requests VALUES (?,?,?,?,?,?,?,?)",
            params![
                r.id,
                sid,
                attempt,
                r.kind,
                r.status,
                amount,
                r.created_at,
                json(&r)?
            ],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO request_source_context VALUES (?,?)",
            params![r.id, json(&source_context.to_vec())?],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO request_versions VALUES (?,?)",
            params![r.id, json(&versions)?],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(r)
    }
    pub fn settle(
        &mut self,
        r: &Request,
        status: &str,
        mut usage: Usage,
        proposal: Option<&TutorProposal>,
    ) -> Result<()> {
        let stored: Request = parse(
            self.conn
                .query_row("SELECT data FROM requests WHERE id=?", [&r.id], |row| {
                    row.get(0)
                })
                .map_err(sql)?,
        )?;
        if json(&stored)? != json(r)? {
            return Err("Request does not match its durable reservation.".into());
        }
        if stored.status != "pending" {
            return Err("Request already settled.".into());
        }
        if usage.request_id != r.id
            || usage.cost_microusd < 0
            || (self.offline && usage.cost_microusd != 0)
        {
            return Err("Usage does not match request mode.".into());
        }
        let mut record = stored;
        record.status = status.into();
        usage.status = status.into();
        let mut s = self.session(&r.session_id)?;
        let valid = proposal.is_some_and(|p| {
            self.tutor_input(r.clone())
                .is_ok_and(|i| crate::providers::validate_proposal(p, &i).is_ok())
        });
        let accepted = if let Some(p) = proposal {
            valid
                && s.revision == r.session_revision
                && status == "completed"
                && p.expected_session_revision == r.session_revision
        } else {
            false
        };
        if proposal.is_some() && !accepted {
            let rejected = if s.revision != r.session_revision {
                "stale"
            } else {
                "rejected"
            };
            record.status = rejected.into();
            usage.status = rejected.into()
        }
        let time = self.time();
        let simulated = self.offline || self.clock.simulated();
        let learning = self.learning_state(&s.id)?.is_some();
        let source_context: Vec<SourcePassage> = self
            .conn
            .query_row(
                "SELECT data FROM request_source_context WHERE request_id=?",
                [&r.id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(sql)?
            .map(parse)
            .transpose()?
            .unwrap_or_default();
        let tx = self.conn.transaction().map_err(sql)?;
        close_request(&tx, r, &record.status, &usage)?;
        if accepted {
            let p = proposal.unwrap();
            let content = if let Some(m) = &p.math {
                format!("{}\n\n$${}$$", p.text, m)
            } else {
                p.text.clone()
            };
            let h = Assistance {
                schema_version: 1,
                id: id(),
                session_id: s.id.clone(),
                activity_id: p.activity_id.clone(),
                attempt_id: r.attempt_id.clone(),
                content,
                depth: p.next_action.clone(),
                visible_at: time.clone(),
            };
            tx.execute(
                "INSERT INTO assistance VALUES (?,?,?,?,?)",
                params![h.id, s.id, h.activity_id, h.attempt_id, json(&h)?],
            )
            .map_err(sql)?;
            crate::sources::persist_citations(&tx, r, &h, p, &source_context)?;
            if learning {
                crate::learning::persist_assessment(&tx, r, p, &time)?;
                if p.grade.is_some() {
                    crate::learning::recompute(&tx, &s.id, &time, simulated)?;
                }
            }
            if learning && let Some(step) = &p.suggested_step {
                // A skip to a fresh check is kept only when this very answer met every rubric part after the
                // deterministic answer check was applied; otherwise the suggestion is discarded and the rest stands.
                let stored: Option<Assessment> = tx
                    .query_row(
                        "SELECT data FROM assessments WHERE json_extract(data,'$.request_id')=? ORDER BY rowid DESC LIMIT 1",
                        [&r.id],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                    .map(parse)
                    .transpose()?;
                let supported = step != "fresh_check"
                    || stored.as_ref().is_some_and(|g| {
                        g.verdict == "correct"
                            && g.uncertainty == "resolved"
                            && g.findings.iter().all(|f| f.result == "met")
                    });
                if step != "stay" && supported {
                    tx.execute(
                        "INSERT INTO learning_events VALUES (?,?,?,?,?)",
                        params![
                            id(),
                            s.id,
                            "step_suggested",
                            time,
                            serde_json::json!({"attempt_id":r.attempt_id,"activity_id":p.activity_id,"step":step,"reason":p.step_reason,"request_id":r.id}).to_string()
                        ],
                    )
                    .map_err(sql)?;
                }
            }
            s.revision += 1;
            tx.execute(
                "UPDATE sessions SET revision=?,data=? WHERE id=?",
                params![s.revision, json(&s)?, s.id],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)?;
        Ok(())
    }
    /// An estimate at the reservation for a request sent with the default model (tests and legacy records).
    pub fn estimated(r: &Request) -> Usage {
        Self::estimated_for(r, registry::DEFAULT_TUTOR_MODEL)
    }
    /// An estimate at the reservation, naming the model the request was frozen with.
    pub fn estimated_for(r: &Request, model: &str) -> Usage {
        if r.kind.starts_with("offline:") {
            return Usage {
                schema_version: 2,
                reported_model: None,
                request_id: r.id.clone(),
                provider: "SIMULATED · offline development".into(),
                model: "deterministic-scenarios-v1".into(),
                input_tokens: None,
                output_tokens: None,
                audio_seconds: None,
                cost_microusd: 0,
                cost_status: "simulated; no billing".into(),
                rate_version: "offline-v1".into(),
                status: r.status.clone(),
            };
        }
        Usage {
            schema_version: 2,
            reported_model: None,
            request_id: r.id.clone(),
            provider: "OpenAI".into(),
            model: if r.kind == "transcription" {
                OPENAI.transcription_model
            } else {
                model
            }
            .into(),
            input_tokens: None,
            output_tokens: None,
            audio_seconds: None,
            cost_microusd: r.reserved_microusd,
            cost_status: "estimated".into(),
            rate_version: OPENAI.version.into(),
            status: r.status.clone(),
        }
    }
    fn recover(&self) -> Result<()> {
        let requests: Vec<Request> = self.all(
            "SELECT data FROM requests WHERE status='pending' AND ?1 IS NOT NULL",
            "",
        )?;
        let tx = self.conn.unchecked_transaction().map_err(sql)?;
        for mut r in requests {
            r.status = "interrupted".into();
            tx.execute(
                "UPDATE requests SET status='interrupted',data=? WHERE id=?",
                params![json(&r)?, r.id],
            )
            .map_err(sql)?;
            let u = Self::estimated_for(&r, self.request_choice(&r.id)?.model.id);
            tx.execute(
                "INSERT OR IGNORE INTO usage VALUES (?,?,?)",
                params![r.id, u.cost_microusd, json(&u)?],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)
    }
    pub fn save_scene(
        &mut self,
        sid: &str,
        parameters: SceneParameters,
        reveal: bool,
    ) -> Result<()> {
        let mut s = self.session(sid)?;
        let a = self.activity(sid)?;
        if let Some(flow) = self.learning_state(sid)? {
            if flow.stopped
                || flow.quiet
                || !["diagnosis", "exploration"].contains(&flow.stage.as_str())
            {
                return Err("The instructional visual is unavailable in this task.".into());
            }
            if reveal && !self.attempts(sid)?.iter().any(|at| at.activity_id == a.id) {
                return Err("Save a prediction before revealing the trace.".into());
            }
        }
        let mut scene = self.scene(&a.id)?;
        scene.parameters = parameters;
        scene::validate(&scene)?;
        let time = self.time();
        let first_reveal = reveal && !scene.revealed;
        if reveal {
            scene.revealed = true;
        }
        let tx = self.conn.transaction().map_err(sql)?;
        if first_reveal {
            let h = Assistance {
                schema_version: 1,
                id: id(),
                session_id: sid.into(),
                activity_id: a.id.clone(),
                attempt_id: None,
                content: "Revealed the local quadratic iteration trace and numerical behavior."
                    .into(),
                depth: "visual".into(),
                visible_at: time.clone(),
            };
            tx.execute(
                "INSERT INTO assistance VALUES (?,?,?,?,?)",
                params![h.id, sid, a.id, h.attempt_id, json(&h)?],
            )
            .map_err(sql)?;
            s.revision += 1;
            tx.execute(
                "UPDATE sessions SET revision=?,data=? WHERE id=?",
                params![s.revision, json(&s)?, sid],
            )
            .map_err(sql)?;
        }
        if scene.revealed {
            tx.execute(
                "INSERT INTO learning_events VALUES (?,?,?,?,?)",
                params![
                    id(),
                    sid,
                    "visual_interaction",
                    time,
                    json(&scene.parameters)?
                ],
            )
            .map_err(sql)?;
        }
        tx.execute(
            "UPDATE scenes SET data=? WHERE activity_id=?",
            params![json(&scene)?, a.id],
        )
        .map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(())
    }
}

impl Store {
    /// Provider state from durable records plus the latest in-memory failure. A response that reached the app proves
    /// the key and model worked at that time; saving a key alone never does.
    pub fn provider_status(&self, credential: &CredentialStatus) -> Result<ProviderStatus> {
        let changed = self.provider.changed_at.clone();
        let answered: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(created_at) FROM requests WHERE kind IN ('tutor','transcription','plan','task') AND status IN ('completed','rejected','stale')",
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let verified_at = [answered, self.provider.checked_at.clone()]
            .into_iter()
            .flatten()
            .filter(|at| changed.as_ref().is_none_or(|c| at > c))
            .max();
        let error = self.provider.error.clone();
        let state = if self.offline {
            "offline"
        } else if !credential.configured {
            "not_configured"
        } else if error.is_some() {
            "attention"
        } else if verified_at.is_some() {
            "verified"
        } else {
            "saved_unverified"
        };
        Ok(ProviderStatus {
            state: state.into(),
            verified_at,
            last_error_kind: error.as_ref().map(|e| e.0.clone()),
            last_error: error.as_ref().map(|e| e.1.clone()),
            last_error_at: error.map(|e| e.2),
        })
    }
    /// Notes the outcome of a live provider attempt: a failure keeps its category and plain message; a response
    /// that reached the app clears it.
    pub fn note_provider(&mut self, failure: Option<&str>) {
        let Some(message) = failure else {
            self.provider.error = None;
            return;
        };
        match crate::providers::error_kind(message) {
            // Local refusals and cancellations say nothing about the provider.
            "local" | "cancelled" => {}
            // A response arrived; only its content was refused.
            "invalid_response" => self.provider.error = None,
            kind => self.provider.error = Some((kind.into(), message.into(), self.time())),
        }
    }
    /// The learner's current tutor model and reasoning effort.
    pub fn model_choice(&self) -> Result<ModelChoice> {
        let p = self.privacy_settings()?;
        Ok(ModelChoice::resolve(&p.tutor_model, &p.reasoning_effort))
    }
    /// The model and effort a stored request was frozen with; records from before the choice existed read as the
    /// defaults they were sent with.
    pub fn request_choice(&self, request_id: &str) -> Result<ModelChoice> {
        let frozen: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT json_extract(data,'$.requested_model'),COALESCE(json_extract(data,'$.reasoning_effort'),'') FROM request_versions WHERE request_id=?",
                [request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)?;
        Ok(frozen
            .map(|(model, effort)| ModelChoice::resolve(&model, &effort))
            .unwrap_or_default())
    }
    pub fn request_versions(&self, sid: &str) -> Result<Vec<RequestVersions>> {
        self.all("SELECT v.data FROM request_versions v JOIN requests r ON r.id=v.request_id WHERE r.session_id=? ORDER BY v.rowid",sid)
    }
    pub fn workspace_view(
        &mut self,
        sid: Option<String>,
        credential: CredentialStatus,
    ) -> Result<Workspace> {
        let offline = self.offline;
        let d = self;
        let mut sessions = d.sessions()?;
        if sessions.is_empty() {
            d.new_session("New session")?;
            sessions = d.sessions()?;
        }
        let sid = sid
            .filter(|id| sessions.iter().any(|s| &s.id == id))
            .unwrap_or_else(|| sessions[0].id.clone());
        let a = d.activity(&sid)?;
        // During an independent check or review the payload carries only this task's own records; earlier
        // teaching, answers and their citations are withheld until the check is finished.
        let check = d
            .learning_state(&sid)?
            .is_some_and(|f| ["independent", "review"].contains(&f.stage.as_str()));
        let mut attempts = d.attempts(&sid)?;
        let mut assistance = d.assistance(&sid)?;
        let mut sources = d.source_view(&sid)?;
        if check {
            attempts.retain(|x| x.activity_id == a.id);
            assistance.retain(|x| x.activity_id == a.id);
            sources
                .citations
                .retain(|c| assistance.iter().any(|h| h.id == c.assistance_id));
        }
        let provider = d.provider_status(&credential)?;
        let choice = d.model_choice()?;
        Ok(Workspace {
            schema_version: 11,
            goal: d.goal_view(&sid)?,
            goals: d.goals_list()?,
            timeline: d.timeline(&sid)?,
            provider,
            request_versions: d.request_versions(&sid)?,
            privacy: d.privacy_settings()?,
            sources,
            unit: d.unit_view(&sid)?,
            offline,
            scenarios: {
                #[cfg(feature = "offline-provider")]
                {
                    crate::offline::SCENARIOS
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                }
                #[cfg(not(feature = "offline-provider"))]
                {
                    vec![]
                }
            },
            learning: d.learning_view(&sid)?,
            sessions,
            active_session_id: sid.clone(),
            scene: d.scene(&a.id)?,
            pending_transcript: d.pending_transcript(&sid)?,
            activity: a,
            attempts,
            assistance,
            requests: d.requests(&sid)?,
            usage: d.usage(&sid)?,
            budget: d.budget(&sid)?,
            credential,
            provider_model: if offline {
                "SIMULATED · deterministic-scenarios-v1"
            } else {
                choice.model.id
            }
            .into(),
            models: registry::TUTOR_MODELS
                .iter()
                .map(|m| ModelOption {
                    id: m.id.into(),
                    label: m.label.into(),
                    note: m.note.into(),
                    input_usd_per_million: m.input_microusd_per_token,
                    output_usd_per_million: m.output_microusd_per_token,
                })
                .collect(),
            transcription_model: OPENAI.transcription_model.into(),
            rate_version: OPENAI.version.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }
    fn input(sid: &str, rev: i32) -> SaveAttempt {
        SaveAttempt {
            session_id: sid.into(),
            expected_revision: rev,
            submission_id: id(),
            response: "It alternates and converges".into(),
            transcript_id: None,
        }
    }
    #[test]
    fn idempotency_and_help_snapshot() {
        let mut d = store();
        let sid = d.new_session("Test").unwrap();
        let i = input(&sid, 0);
        let a = d.save_attempt(&i).unwrap();
        assert_eq!(a.id, d.save_attempt(&i).unwrap().id);
        assert_eq!(d.attempts(&sid).unwrap().len(), 1);
        assert!(a.assistance_ids.is_empty());
        let sc = d.scene(&d.activity(&sid).unwrap().id).unwrap();
        d.save_scene(&sid, sc.parameters, true).unwrap();
        let b = d.save_attempt(&input(&sid, 2)).unwrap();
        assert_eq!(b.exposure_ids.len(), 1);
        assert!(d.attempts(&sid).unwrap()[0].assistance_ids.is_empty());
    }
    #[test]
    fn transcript_restart_confirmation_retry_and_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice.db");
        let mut d = Store::open(&path).unwrap();
        let sid = d
            .new_session("Speech fixture, not live recognition")
            .unwrap();
        let other = d.new_session("Other session").unwrap();
        let t = Transcript {
            id: id(),
            recognized_text: "The multiplier is positive one half".into(),
            audio_seconds: 3.,
            format: "test fixture".into(),
        };
        d.save_transcript(&sid, &t).unwrap();
        d.save_draft(&sid, "The multiplier is negative one half")
            .unwrap();
        drop(d);
        let mut d = Store::open(&path).unwrap();
        assert_eq!(
            d.pending_transcript(&sid).unwrap().unwrap().recognized_text,
            t.recognized_text
        );
        assert_eq!(
            d.session(&sid).unwrap().draft,
            "The multiplier is negative one half"
        );
        let mut i = input(&sid, 0);
        i.response = d.session(&sid).unwrap().draft;
        assert!(
            d.save_attempt(&i).is_err(),
            "speech must not silently become typed input"
        );
        i.transcript_id = Some(t.id.clone());
        let mut wrong = i.clone();
        wrong.session_id = other;
        assert!(d.save_attempt(&wrong).is_err());
        let at = d.save_attempt(&i).unwrap();
        assert_eq!(at.recognized_transcript, Some(t.recognized_text));
        assert_eq!(at.response, i.response);
        assert_eq!(at.modality, "recorded_speech");
        assert!(d.pending_transcript(&sid).unwrap().is_none());
        drop(d);
        let mut d = Store::open(&path).unwrap();
        assert_eq!(
            d.save_attempt(&i).unwrap().id,
            at.id,
            "retry works after confirmation and process restart"
        );
        assert_eq!(d.attempts(&sid).unwrap().len(), 1);
        i.transcript_id = None;
        assert!(
            d.save_attempt(&i).is_err(),
            "retry cannot change provenance"
        );
    }
    #[test]
    fn discard_transcript_is_scoped_and_restores_text_path() {
        let mut d = store();
        let sid = d.new_session("Test").unwrap();
        let t = Transcript {
            id: id(),
            recognized_text: "Speech fixture".into(),
            audio_seconds: 2.,
            format: "test".into(),
        };
        d.save_transcript(&sid, &t).unwrap();
        assert!(d.discard_transcript(&sid, "wrong-ticket").is_err());
        assert!(d.pending_transcript(&sid).unwrap().is_some());
        d.discard_transcript(&sid, &t.id).unwrap();
        d.discard_transcript(&sid, &t.id).unwrap();
        assert_eq!(d.session(&sid).unwrap().draft, "");
        assert_eq!(d.save_attempt(&input(&sid, 0)).unwrap().modality, "text");
    }
    #[test]
    fn upgrades_v1_without_losing_attempts_and_rejects_future() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../migrations/001_foundation.sql"))
            .unwrap();
        let mut old = Store {
            conn,
            path: String::new(),
            offline: false,
            clock: std::sync::Arc::new(crate::clock::SystemClock),
            provider: ProviderMemory::default(),
        };
        let sid = old.new_session("Before migration").unwrap();
        let a = old.activity(&sid).unwrap();
        let raw = serde_json::json!({"schema_version":1,"id":"old-attempt","session_id":sid,"activity_id":a.id,"task_version":1,"response":"Prior saved reasoning","recognized_transcript":null,"modality":"text","created_at":now(),"assistance_ids":[],"exposure_ids":[],"revision_of":null}).to_string();
        old.conn
            .execute(
                "INSERT INTO attempts VALUES (?,?,?,?,?)",
                params!["old-attempt", sid, a.id, id(), raw],
            )
            .unwrap();
        drop(old);
        let upgraded = Store::open(&path).unwrap();
        assert_eq!(
            upgraded.attempts(&sid).unwrap()[0].response,
            "Prior saved reasoning"
        );
        assert!(upgraded.pending_transcript(&sid).unwrap().is_none());
        upgraded
            .conn
            .pragma_update(None, "user_version", 99)
            .unwrap();
        drop(upgraded);
        assert!(
            Store::open(&path)
                .err()
                .unwrap()
                .contains("newer Study Harness")
        );
    }
    #[test]
    fn budget_reserves_unknown_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let mut d = Store::open(&path).unwrap();
        let sid = d.new_session("Test").unwrap();
        d.set_limits(&Limits {
            session_microusd: 100,
            monthly_microusd: 200,
        })
        .unwrap();
        d.reserve(&sid, None, "transcription", 80).unwrap();
        assert!(d.reserve(&sid, None, "tutor", 21).is_err());
        drop(d);
        let d = Store::open(&path).unwrap();
        assert_eq!(d.budget(&sid).unwrap().session_spent_microusd, 80);
        assert_eq!(d.requests(&sid).unwrap()[0].status, "interrupted");
        assert_eq!(d.usage(&sid).unwrap()[0].cost_status, "estimated");
    }
    #[test]
    fn model_preference_is_validated_and_frozen_per_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.db");
        let mut d = Store::open(&path).unwrap();
        let sid = d.new_session("Test").unwrap();
        // Settings saved before the choice existed read as Sol at low effort.
        d.conn
            .execute(
                "INSERT OR REPLACE INTO settings VALUES ('privacy',?)",
                [r#"{"retain_failed_audio":true,"review_minutes":10,"text_scale_percent":100}"#],
            )
            .unwrap();
        let mut p = d.privacy_settings().unwrap();
        assert_eq!(
            (p.tutor_model.as_str(), p.reasoning_effort.as_str()),
            ("gpt-6.1-sol", "low")
        );
        p.tutor_model = "gpt-4o".into();
        assert!(d.set_privacy(&p).is_err(), "only listed models");
        p.tutor_model = "gpt-6-astra".into();
        p.reasoning_effort = "max".into();
        assert!(d.set_privacy(&p).is_err(), "only low, medium or high");
        p.reasoning_effort = "high".into();
        d.set_privacy(&p).unwrap();
        let r = d.reserve(&sid, None, "plan", 100).unwrap();
        // Changing the preference later never rewrites what a request was sent with.
        p.tutor_model = "gpt-6.1-terra".into();
        p.reasoning_effort = "low".into();
        d.set_privacy(&p).unwrap();
        let frozen = d.request_choice(&r.id).unwrap();
        assert_eq!((frozen.model.id, frozen.effort), ("gpt-6-astra", "high"));
        let v = d.request_versions(&sid).unwrap();
        assert_eq!(v[0].requested_model, "gpt-6-astra");
        assert_eq!(v[0].reasoning_effort, "high");
        let t = d.reserve(&sid, None, "task", 100).unwrap();
        let v = d.request_versions(&sid).unwrap();
        assert_eq!(v[1].requested_model, "gpt-6.1-terra");
        assert_eq!(
            v[1].reasoning_effort, "medium",
            "tasks never go below medium"
        );
        assert_eq!(d.request_choice(&t.id).unwrap().model.id, "gpt-6.1-terra");
        drop(d);
        // An interrupted request keeps its frozen model in the estimated usage.
        let d = Store::open(&path).unwrap();
        let usage = d.usage(&sid).unwrap();
        assert!(
            usage
                .iter()
                .any(|u| u.request_id == r.id && u.model == "gpt-6-astra")
        );
        assert!(
            usage
                .iter()
                .any(|u| u.request_id == t.id && u.model == "gpt-6.1-terra")
        );
    }
    #[test]
    fn stale_response_keeps_charge_discards_help() {
        let mut d = store();
        let sid = d.new_session("Test").unwrap();
        let at = d.save_attempt(&input(&sid, 0)).unwrap();
        let r = d.reserve(&sid, Some(&at.id), "tutor", 100).unwrap();
        let a = d.activity(&sid).unwrap();
        let p = TutorProposal {
            schema_version: 5,
            suggested_step: None,
            step_reason: String::new(),
            citations: vec![],
            session_id: sid.clone(),
            expected_session_revision: r.session_revision,
            activity_id: a.id,
            rubric_version: a.rubric_version,
            next_action: "hint".into(),
            text: "Consider the multiplier".into(),
            math: None,
            evidence_attempt_id: at.id,
            evidence_excerpt: "alternates".into(),
            unresolved_questions: vec![],
            grade: None,
        };
        d.save_attempt(&input(&sid, 1)).unwrap();
        d.settle(&r, "completed", Store::estimated(&r), Some(&p))
            .unwrap();
        assert!(d.assistance(&sid).unwrap().is_empty());
        assert_eq!(d.requests(&sid).unwrap()[0].status, "stale");
        assert_eq!(d.budget(&sid).unwrap().session_spent_microusd, 100);
    }
    #[test]
    fn migrations_reject_future_and_enforce_foreign_keys() {
        let d = store();
        assert!(
            d.conn
                .execute("INSERT INTO reviews VALUES ('r','missing','{}')", [])
                .is_err()
        );
        assert_eq!(
            d.conn
                .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                .unwrap(),
            9
        );
    }
}
