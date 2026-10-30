//! Application HostPort. The same SQLite connection/transactions own learning and visual records.
//! Installed content is build-time trusted; artifacts, registrations and journal payloads never grant authority.
use crate::{
    contracts::{Assistance, Attempt, Session},
    db::{Store, id, json, parse, sql},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json as j};
use std::{collections::BTreeSet, sync::OnceLock};
use study_harness_contracts::canonical::{canonical_json, digest};

#[derive(Debug, Serialize)]
pub struct VisualError {
    pub code: &'static str,
    pub message: &'static str,
    pub reason: &'static str,
}
pub type VResult<T> = std::result::Result<T, VisualError>;
fn refusal(reason: &'static str) -> VisualError {
    VisualError {
        code: if reason == "POLICY_DENIED" {
            "POLICY_DENIED"
        } else if reason == "STALE_POLICY_REVISION" {
            reason
        } else {
            "INVALID_INPUT"
        },
        message: "The visual operation was not authorized. Refresh the activity or return to the session.",
        reason,
    }
}
impl From<rusqlite::Error> for VisualError {
    fn from(_: rusqlite::Error) -> Self {
        Self {
            code: "PERSISTENCE_FAILED",
            message: "The visual record could not be saved. Retry or reopen the application.",
            reason: "SQLITE",
        }
    }
}
impl From<String> for VisualError {
    fn from(_: String) -> Self {
        Self {
            code: "PERSISTENCE_FAILED",
            message: "The local record is unavailable. Reopen the application.",
            reason: "LOCAL_RECORD",
        }
    }
}
fn text<'a>(v: &'a Value, k: &str) -> VResult<&'a str> {
    v[k].as_str().ok_or_else(|| refusal("SCHEMA"))
}
fn num(v: &Value, k: &str) -> VResult<i64> {
    v[k].as_i64().ok_or_else(|| refusal("SCHEMA"))
}
fn schema(name: &str, v: &Value) -> VResult<()> {
    study_harness_contracts::validate(name, v).map_err(|_| refusal("SCHEMA"))
}
pub fn installed() -> &'static Value {
    static CONTENT: OnceLock<Value> = OnceLock::new();
    CONTENT.get_or_init(|| {
        serde_json::from_str(include_str!("../visualizer/catalog.json"))
            .expect("installed visualizer catalog")
    })
}
fn entry(tid: &str) -> VResult<&'static Value> {
    installed()["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["template"]["templateId"] == tid)
        .ok_or_else(|| refusal("CONTENT_REVOKED"))
}
fn state(c: &Connection, aid: &str) -> VResult<Value> {
    let raw: Option<String> = c
        .query_row(
            "SELECT state FROM visual_activities WHERE id=?",
            [aid],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(parse)
        .transpose()?
        .ok_or_else(|| refusal("NOT_FOUND"))
}
fn write_state(c: &Connection, aid: &str, v: &Value) -> VResult<()> {
    c.execute(
        "UPDATE visual_activities SET state=? WHERE id=?",
        params![v.to_string(), aid],
    )?;
    Ok(())
}
fn policy(c: &Connection, aid: &str) -> VResult<Value> {
    parse(c.query_row(
        "SELECT data FROM visual_policies WHERE activity_id=? ORDER BY revision DESC LIMIT 1",
        [aid],
        |r| r.get(0),
    )?)
    .map_err(Into::into)
}
fn values(c: &Connection, query: &str, aid: &str) -> VResult<Vec<Value>> {
    let mut s = c.prepare(query)?;
    s.query_map([aid], |r| r.get::<_, String>(0))?
        .map(|r| parse(r?).map_err(Into::into))
        .collect()
}
fn ack(now: &str) -> Value {
    j!({"ok":true,"hostTimestamp":now})
}
fn replay(
    c: &Connection,
    aid: &str,
    key: &str,
    method: &str,
    request: &Value,
) -> VResult<Option<Value>> {
    let old: Option<(String,String,String)> = c.query_row("SELECT method,fingerprint,result FROM visual_operations WHERE activity_id=? AND operation_key=?",params![aid,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    old.map(|(m, d, r)| {
        if m == method && d == digest(request) {
            parse(r).map_err(Into::into)
        } else {
            Err(refusal("OPERATION_CONFLICT"))
        }
    })
    .transpose()
}
fn remember(
    c: &Connection,
    aid: &str,
    key: &str,
    method: &str,
    request: &Value,
    result: &Value,
) -> VResult<()> {
    c.execute(
        "INSERT INTO visual_operations VALUES (?,?,?,?,?)",
        params![aid, key, method, digest(request), result.to_string()],
    )?;
    Ok(())
}
fn bump(c: &Connection, sid: &str) -> VResult<()> {
    let mut s: Session =
        parse(c.query_row("SELECT data FROM sessions WHERE id=?", [sid], |r| r.get(0))?)?;
    s.revision += 1;
    c.execute(
        "UPDATE sessions SET revision=?,data=? WHERE id=?",
        params![s.revision, json(&s)?, sid],
    )?;
    Ok(())
}
fn issue_policy(c: &Connection, aid: &str, s: &Value, all: bool, now: &str) -> VResult<Value> {
    let e = entry(text(s, "templateId")?)?;
    let independent = s["context"]["kind"] == "independent";
    let revision: i64 = c.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM visual_policies WHERE activity_id=?",
        [aid],
        |r| r.get(0),
    )?;
    let groups = e["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| !independent && (all || g["solutionBearing"] == false))
        .map(|g| g["groupId"].clone())
        .collect::<Vec<_>>();
    let controls = if independent {
        vec![]
    } else {
        e["parameters"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| j!(k))
            .collect()
    };
    let p = j!({"contract":"study-harness.host-policy","schemaVersion":"1.0","policyId":format!("policy-{aid}"),"revision":revision,"issuedAt":now,"expiresAt":null,"revoked":false,"mode":"approved","approved":[{"template":e["template"],"model":e["model"]}],"allowedTools":["discover","prepare","inspect.learner","request-reveal","operate.parameters","operate.navigation","operate.playback","operate.presentation","summarize","save","restore","close"],"visibleGroups":groups,"permittedControls":controls,"export":{"learnerSnapshot":true,"capture":true,"backup":true},"inspection":{"developer":false,"tutor":"learner-visible","teacher":false},"independentTask":{"active":independent,"taskRef":s["context"]["taskRef"],"permittedTools":if independent {vec!["inspect.learner"]} else {vec![]},"neutralScaffold":[]},"sampling":{"allowed":false,"maxPositions":0},"exploratory":{"allowed":false,"maxParametersOverride":0}});
    schema("study-harness.host-policy", &p)?;
    c.execute(
        "INSERT INTO visual_policies VALUES (?,?,?)",
        params![aid, revision, p.to_string()],
    )?;
    Ok(p)
}
fn cursor(c: &Connection, aid: &str) -> VResult<Value> {
    let s = state(c, aid)?;
    let last:Option<(i64,String)>=c.query_row("SELECT sequence,event_id FROM visual_events WHERE activity_id=? ORDER BY sequence DESC LIMIT 1",[aid],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (n, event) = last.map(|(n, e)| (n, Some(e))).unwrap_or((0, None));
    Ok(
        j!({"activityId":aid,"lastSequence":n,"lastEventId":event,"checkpointDigest":s["checkpointDigest"]}),
    )
}
pub fn records(c: &Connection, aid: &str) -> VResult<Value> {
    let s = state(c, aid)?;
    let exposures = values(
        c,
        "SELECT data FROM visual_exposures WHERE activity_id=? ORDER BY rowid",
        aid,
    )?;
    let help = values(
        c,
        "SELECT result FROM visual_operations WHERE activity_id=? AND method='recordAssistance' ORDER BY rowid",
        aid,
    )?;
    let last = cursor(c, aid)?["lastSequence"].as_i64().unwrap().max(1);
    let mut assistance = vec![];
    for r in help {
        let seq:Option<i64>=c.query_row("SELECT sequence FROM visual_events WHERE activity_id=? AND json_extract(data,'$.type')='assistance.recorded' AND json_extract(data,'$.payload.receiptId')=? ORDER BY sequence LIMIT 1",params![aid,r["receiptId"].as_str()],|r|r.get(0)).optional()?;
        let grant = if r["grant"].is_object() {
            j!({"exposureId":r["grant"]["exposureId"],"groups":r["grant"]["groups"].as_array().unwrap().iter().map(|g|g["groupId"].clone()).collect::<Vec<_>>()})
        } else {
            Value::Null
        };
        assistance.push(j!({"receiptId":r["receiptId"],"kind":r["kind"],"sequence":seq.unwrap_or(last),"grant":grant}));
    }
    let mut activity = j!({"stage":s["stage"],"activityRevision":s["activityRevision"],"closed":s["closed"],"lineage":{"context":s["context"],"fromActivityRevision":s["lineageFrom"]}});
    if !s["stageReceiptId"].is_null() {
        activity["stageReceiptId"] = s["stageReceiptId"].clone();
    }
    let r =
        j!({"activityId":aid,"exposures":exposures,"assistance":assistance,"activity":activity});
    schema("study-harness.activity-records", &r)?;
    Ok(r)
}
pub fn recover(c: &Connection, now: &str) -> crate::db::Result<()> {
    let pending=values(c,"SELECT data FROM visual_exposures WHERE json_extract(data,'$.displayStatus')='authorized' AND ? IS NOT NULL","").map_err(|_|"Cannot recover visual exposures")?;
    let tx = c.unchecked_transaction().map_err(sql)?;
    for mut e in pending {
        e["displayStatus"] = j!("uncertain");
        e["uncertainty"] = j!({"code":"INTERRUPTED_BEFORE_ACK","reason":"The application reopened before a display acknowledgement was saved; this content may have been seen."});
        tx.execute(
            "UPDATE visual_exposures SET data=? WHERE id=?",
            params![e.to_string(), e["exposureId"].as_str()],
        )
        .map_err(sql)?;
        tx.execute(
            "INSERT INTO visual_recoveries(exposure_id,data) VALUES (?,?)",
            params![
                e["exposureId"].as_str(),
                j!({"at":now,"from":"authorized","to":"uncertain"}).to_string()
            ],
        )
        .map_err(sql)?;
    }
    tx.commit().map_err(sql)
}

impl Store {
    pub fn visual_open(&mut self, sid: &str, tid: &str) -> VResult<Value> {
        let e = entry(tid)?;
        let session = self.session(sid)?;
        let learning = self.activity(sid)?;
        // A generated capability opens only the visual planned with its current task; nothing is a default.
        if crate::goals::is_generated(&self.conn, &learning.objective_id)
            .map_err(VisualError::from)?
            && self
                .visual_plan_for(&learning.id)
                .map_err(VisualError::from)?
                .is_none_or(|p| p.template_id != tid)
        {
            return Err(refusal("POLICY_DENIED"));
        }
        let independent = self
            .learning_state(sid)?
            .is_some_and(|f| ["independent", "review"].contains(&f.stage.as_str()));
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM visual_activities WHERE learning_activity_id=? AND template_id=?",
                params![learning.id, tid],
                |r| r.get(0),
            )
            .optional()?;
        let aid = if let Some(aid) = existing {
            aid
        } else {
            let aid = format!("visual-{}", id());
            let now = self.time();
            // The host already presents orientation and captures the learner prediction in its composer.
            // Start at the public prediction stage, with no solution-bearing groups disclosed.
            let stage = e["stages"]
                .as_array()
                .and_then(|stages| stages.iter().find(|s| s["kind"] == "predict"))
                .map(|s| s["id"].clone())
                .unwrap_or(e["initialStage"].clone());
            // Parameters fixed with the task (see curriculum::visual_plan); legacy tasks keep the former defaults.
            let plan = self
                .visual_plan_for(&learning.id)
                .map_err(VisualError::from)?
                .filter(|p| p.template_id == tid);
            let parameters = match (plan, tid) {
                (Some(plan), _) => Value::Object(
                    plan.parameters
                        .iter()
                        .map(|p| (p.name.clone(), j!(p.value)))
                        .collect(),
                ),
                (None, "quadratic-gradient-descent") => j!({"a":2.,"eta":0.75,"x0":4.}),
                (None, "function-derivative") => j!({"A":1.5,"B":1.,"C":3.,"p":2.,"h":0.5}),
                (None, "quadratic-surface-gradient") => j!({"a":3.,"b":1.,"c":3.,"px":1.,"py":-2.}),
                _ => j!({}),
            };
            let s = j!({"templateId":tid,"sessionId":sid,"learningActivityId":learning.id,"template":e["template"],"model":e["model"],"activityRevision":0,"stage":stage,"initialParameters":parameters,"closed":false,"registered":false,"context":if independent{j!({"kind":"independent","taskRef":learning.id})}else{j!({"kind":"instructional"})},"lineageFrom":if independent{1}else{0},"stageReceiptId":null,"checkpointDigest":null});
            let tx = self.conn.transaction()?;
            tx.execute(
                "INSERT INTO visual_activities VALUES (?,?,?,?,?)",
                params![aid, sid, learning.id, tid, s.to_string()],
            )?;
            issue_policy(&tx, &aid, &s, false, &now)?;
            tx.commit()?;
            aid
        };
        let s = state(&self.conn, &aid)?;
        let p = policy(&self.conn, &aid)?;
        let artifact:Option<String>=self.conn.query_row("SELECT data FROM visual_artifacts WHERE activity_id=? AND json_extract(data,'$.kind')='backup' ORDER BY rowid DESC LIMIT 1",[&aid],|r|r.get(0)).optional()?;
        if !s["checkpointDigest"].is_null()
            && artifact.as_ref().is_none_or(|a| {
                serde_json::from_str::<Value>(a).is_ok_and(|v| digest(&v) != s["checkpointDigest"])
            })
        {
            return Err(VisualError {
                code: "PERSISTENCE_FAILED",
                message: "The saved visual checkpoint is missing or damaged. Keep your study record and restore a complete backup; a new scene cannot stand in for the saved state.",
                reason: "MISSING_OR_DAMAGED_ARTIFACT",
            });
        }
        let mut request = j!({"activityId":aid,"hostSessionId":sid,"hostSessionRevision":session.revision,"objectiveIds":[learning.objective_id],"template":{"templateId":tid,"version":e["template"]["version"]},"mode":"approved"});
        if let Some(parameters) = s.get("initialParameters") {
            request["parameters"] = parameters.clone();
            request["stageId"] = s["stage"].clone();
        }
        Ok(
            j!({"request":request,"policy":p,"artifact":artifact.map(parse::<Value>).transpose()?,"hostState":s,"provisional":true}),
        )
    }
    /// Raw native envelope. Method allowlist, bytes, shape and semantic authority all precede mutation.
    pub fn visualizer_rpc(&mut self, bytes: &[u8]) -> VResult<Value> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(refusal("RESOURCE_LIMIT"));
        }
        let v: Value = serde_json::from_slice(bytes).map_err(|_| refusal("SCHEMA"))?;
        if v.as_object()
            .is_none_or(|o| o.len() != 2 || !o.contains_key("request") || !o.contains_key("method"))
        {
            return Err(refusal("SCHEMA"));
        }
        self.visual_call(text(&v, "method")?, &v["request"])
    }
    pub fn visual_call(&mut self, method: &str, q: &Value) -> VResult<Value> {
        let contract = match method {
            "getPolicy" | "getJournalCursor" | "getActivityRecords" | "checkpoint"
            | "authorizeReveal" => "activity-lookup-request",
            "registerActivity" => "register-activity-request",
            "requestStageTransition" => "stage-transition-request",
            "recordExposureIntent" => "exposure-intent-request",
            "recordDisplayAck" => "display-ack-request",
            "recordAssistance" => "assistance-request",
            "appendEvents" => "append-events-request",
            "saveArtifact" => "save-artifact-request",
            "open" => {
                if q.as_object().is_none_or(|o| o.len() != 2) {
                    return Err(refusal("SCHEMA"));
                }
                return self.visual_open(text(q, "sessionId")?, text(q, "templateId")?);
            }
            _ => return Err(refusal("POLICY_DENIED")),
        };
        schema(&format!("study-harness.{contract}"), q)?;
        let aid = if method == "registerActivity" {
            text(&q["instance"], "activityId")?
        } else if method == "saveArtifact" {
            text(&q["artifact"]["instance"], "activityId")?
        } else {
            text(q, "activityId")?
        };
        let mut s = state(&self.conn, aid)?;
        let e = entry(text(&s, "templateId")?)?;
        let p = policy(&self.conn, aid)?;
        let now = self.time();
        if method == "getPolicy" {
            return Ok(p);
        }
        if method == "getActivityRecords" {
            return records(&self.conn, aid);
        }
        if method == "getJournalCursor" {
            return cursor(&self.conn, aid);
        }
        if method == "authorizeReveal" {
            return self.visual_allow_reveal(aid);
        }
        if method == "checkpoint" {
            return self.visual_open(text(&s, "sessionId")?, text(&s, "templateId")?);
        }
        if method == "appendEvents" {
            return self.visual_append(aid, q, &now);
        }
        let key = if method == "recordDisplayAck" {
            text(q, "exposureId")?
        } else if ["registerActivity", "saveArtifact"].contains(&method) {
            text(q, "operationId")?
        } else {
            text(q, "idempotencyKey")?
        };
        // Separate namespaces for display acknowledgements and registration, but receipts share idempotency keys.
        let operation_key = if method == "recordDisplayAck" || method == "registerActivity" {
            format!("{method}:{key}")
        } else {
            key.to_owned()
        };
        if let Some(result) = replay(&self.conn, aid, &operation_key, method, q)? {
            return Ok(result);
        }
        if method == "registerActivity" {
            let i = &q["instance"];
            if i["template"] != s["template"]
                || i["model"] != s["model"]
                || i["hostSessionId"] != s["sessionId"]
                || i["hostSessionRevision"] != j!(self.session(text(&s, "sessionId")?)?.revision)
                || i["mode"] != "approved"
            {
                return Err(refusal("INVALID_REFERENCE"));
            }
            s["registered"] = j!(true);
            let result = ack(&now);
            let tx = self.conn.transaction()?;
            write_state(&tx, aid, &s)?;
            remember(&tx, aid, &operation_key, method, q, &result)?;
            tx.commit()?;
            return Ok(result);
        }
        if method == "saveArtifact" {
            return self.visual_save(aid, q, &operation_key, &s, &now);
        }
        if method == "recordDisplayAck" {
            let mut record: Value = parse(
                self.conn
                    .query_row(
                        "SELECT data FROM visual_exposures WHERE id=? AND activity_id=?",
                        params![key, aid],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| refusal("NOT_FOUND"))?,
            )?;
            if record["receiptId"] != q["receiptId"] {
                return Err(refusal("RECEIPT_MISMATCH"));
            }
            record["displayStatus"] = j!("displayed");
            record["displayed"] = j!({"projectionDigest":q["projectionDigest"],"frameId":q["frameId"],"monotonicMs":q["monotonicMs"]});
            record.as_object_mut().unwrap().remove("uncertainty");
            schema("study-harness.exposure-record", &record)?;
            let result = ack(&now);
            let tx = self.conn.transaction()?;
            tx.execute(
                "UPDATE visual_exposures SET data=? WHERE id=?",
                params![record.to_string(), key],
            )?;
            tx.execute(
                "INSERT INTO visual_display_acks VALUES (?,?,?)",
                params![key, digest(q), q.to_string()],
            )?;
            remember(&tx, aid, &operation_key, method, q, &result)?;
            self_test_fault("display-ack-before-commit")?;
            tx.commit()?;
            return Ok(result);
        }
        // These are grants. Frozen registrations or saved artifacts never get to choose this state.
        if s["registered"] != true || s["closed"] == true {
            return Err(refusal("POLICY_DENIED"));
        }
        if q["expectedPolicyRevision"] != p["revision"] {
            return Err(refusal("STALE_POLICY_REVISION"));
        }
        if q["basedOnActivityRevision"] != s["activityRevision"] {
            return Err(refusal("STALE_ACTIVITY_REVISION"));
        }
        if q["template"] != s["template"] || q["model"] != s["model"] {
            return Err(refusal("INVALID_REFERENCE"));
        }
        let sid = text(&s, "sessionId")?.to_owned();
        let la = text(&s, "learningActivityId")?.to_owned();
        let current = self.activity(&sid)?;
        if current.id != la
            || self
                .learning_state(&sid)?
                .is_some_and(|f| f.stopped || f.quiet)
        {
            return Err(refusal("POLICY_DENIED"));
        }
        let independent = s["context"]["kind"] == "independent";
        let groups = q["groups"].as_array().ok_or_else(|| refusal("SCHEMA"))?;
        let mut next_stage = text(&s, "stage")?.to_owned();
        let mut contract = "study-harness.exposure-receipt";
        let scope = if method == "requestStageTransition" {
            if q["fromStage"] != s["stage"]
                || !e["transitions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["from"] == q["fromStage"] && t["to"] == q["toStage"])
            {
                return Err(refusal("INVALID_REFERENCE"));
            }
            next_stage = text(q, "toStage")?.to_owned();
            contract = "study-harness.stage-receipt";
            "stage"
        } else if method == "recordAssistance" {
            contract = "study-harness.assistance-receipt";
            "reveal"
        } else {
            if q["intendedStage"] != s["stage"] {
                return Err(refusal("STALE_ACTIVITY_REVISION"));
            }
            text(q, "scope")?
        };
        if independent
            && (!groups.is_empty()
                || method == "recordExposureIntent"
                || q["kind"] == "reopen-instructional")
        {
            return Err(refusal("POLICY_DENIED"));
        }
        let target = e["stages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == next_stage)
            .ok_or_else(|| refusal("INVALID_REFERENCE"))?;
        let mut unique = BTreeSet::new();
        for g in groups {
            let group = e["groups"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["groupId"] == g["groupId"] && v["contentDigest"] == g["contentDigest"])
                .ok_or_else(|| refusal("INVALID_REFERENCE"))?;
            if !unique.insert(g["groupId"].to_string())
                || !target["visible"]
                    .as_array()
                    .unwrap()
                    .contains(&g["groupId"])
                || !p["visibleGroups"]
                    .as_array()
                    .unwrap()
                    .contains(&g["groupId"])
                || (scope != "exploration" && group["solutionBearing"] != true)
            {
                return Err(refusal("POLICY_DENIED"));
            }
        }
        if let Some(domain) = q.get("parameterDomain") {
            for (k, r) in domain.as_object().ok_or_else(|| refusal("SCHEMA"))? {
                let bounds = &e["parameters"][k];
                if bounds.is_null()
                    || !p["permittedControls"].as_array().unwrap().contains(&j!(k))
                    || r[0].as_f64().unwrap() > r[1].as_f64().unwrap()
                    || r[0].as_f64().unwrap() < bounds[0].as_f64().unwrap()
                    || r[1].as_f64().unwrap() > bounds[1].as_f64().unwrap()
                {
                    return Err(refusal("POLICY_DENIED"));
                }
            }
        }
        let grant = if !groups.is_empty() || method == "recordExposureIntent" {
            let mut g =
                j!({"exposureId":format!("exposure-{}",id()),"scope":scope,"groups":groups});
            if let Some(d) = q.get("parameterDomain") {
                g["parameterDomain"] = d.clone();
            }
            g
        } else {
            Value::Null
        };
        let receipt_id = format!("receipt-{}", id());
        let revision = num(&s, "activityRevision")? + 1;
        let mut receipt = j!({"contract":contract,"schemaVersion":"1.0","receiptId":receipt_id,"activityId":aid,"basedOnActivityRevision":s["activityRevision"],"activityRevision":revision,"policyId":p["policyId"],"policyRevision":p["revision"],"idempotencyKey":q["idempotencyKey"],"template":s["template"],"model":s["model"],"issuedAt":now,"expiresAt":null,"grant":grant});
        if method == "requestStageTransition" {
            receipt["fromStage"] = q["fromStage"].clone();
            receipt["toStage"] = q["toStage"].clone();
            s["stageReceiptId"] = j!(receipt_id);
        } else if method == "recordAssistance" {
            receipt["kind"] = q["kind"].clone();
        } else {
            receipt["intendedStage"] = s["stage"].clone();
        }
        schema(contract, &receipt)?;
        s["activityRevision"] = j!(revision);
        s["stage"] = j!(next_stage);
        if target["kind"] == "closed" {
            s["closed"] = j!(true);
        }
        let tx = self.conn.transaction()?;
        if !grant.is_null() {
            let mut record = j!({"contract":"study-harness.exposure-record","schemaVersion":"1.0","exposureId":grant["exposureId"],"activityId":aid,"receiptId":receipt_id,"scope":scope,"groups":groups,"template":s["template"],"model":s["model"],"activityRevision":revision,"intendedStage":next_stage,"displayStatus":"authorized"});
            if let Some(d) = grant.get("parameterDomain") {
                record["parameterDomain"] = d.clone();
            }
            schema("study-harness.exposure-record", &record)?;
            tx.execute(
                "INSERT INTO visual_exposures VALUES (?,?,?,?)",
                params![
                    grant["exposureId"].as_str(),
                    aid,
                    receipt_id,
                    record.to_string()
                ],
            )?;
        }
        if !groups.is_empty() || method == "recordAssistance" {
            let h = Assistance {
                schema_version: 1,
                id: receipt_id.clone(),
                session_id: sid.clone(),
                activity_id: la.clone(),
                attempt_id: None,
                content: format!(
                    "Visual activity {}: {} authorized; display acknowledgement is recorded separately.",
                    e["title"].as_str().unwrap(),
                    method
                ),
                depth: "visual".into(),
                visible_at: now.clone(),
            };
            tx.execute(
                "INSERT INTO assistance VALUES (?,?,?,?,?)",
                params![h.id, sid, la, h.attempt_id, json(&h)?],
            )?;
            bump(&tx, &sid)?;
        }
        write_state(&tx, aid, &s)?;
        remember(&tx, aid, &operation_key, method, q, &receipt)?;
        if method == "recordExposureIntent" && scope != "exploration" {
            self_test_fault("intent-before-commit")?;
        }
        tx.commit()?;
        if method == "recordExposureIntent" && scope != "exploration" {
            self_test_fault("intent-after-commit")?;
        }
        Ok(receipt)
    }
    pub fn visual_allow_reveal(&mut self, aid: &str) -> VResult<Value> {
        let s = state(&self.conn, aid)?;
        let sid = text(&s, "sessionId")?;
        // A prediction (an answer, not a question) must be saved before solution-bearing groups are shown.
        if s["context"]["kind"] == "independent"
            || self.activity(sid)?.id != text(&s, "learningActivityId")?
            || !self.attempts(sid)?.iter().any(|a| {
                a.activity_id == s["learningActivityId"].as_str().unwrap()
                    && crate::learning::intent(&self.conn, &a.id).is_ok_and(|i| i == "answer")
            })
        {
            return Err(refusal("POLICY_DENIED"));
        }
        issue_policy(&self.conn, aid, &s, true, &self.time())
    }
    /// Validates a tutor-audience interaction summary of this session's current instructional visual and returns its
    /// bounded deterministic text. Independent work never sends one; the tutor receives no hidden content.
    pub fn visual_tutor_summary(&self, sid: &str, v: &Value) -> crate::db::Result<Option<String>> {
        let refuse = || "The visual summary does not belong to the current task.".to_string();
        if v.to_string().len() > 64_000 {
            return Err("The visual summary exceeds its size limit.".into());
        }
        schema("study-harness.interaction-summary", v).map_err(|e| e.message.to_string())?;
        if v["audience"] != "tutor" || v["mode"] == "exploratory" {
            return Err(refuse());
        }
        let aid = v["activityId"].as_str().ok_or_else(refuse)?;
        let owner: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT session_id, learning_activity_id FROM visual_activities WHERE id=?",
                [aid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(crate::db::sql)?;
        let flow = self.learning_state(sid)?;
        if owner.is_none_or(|(s, la)| {
            s != sid || flow.as_ref().is_none_or(|f| f.active_activity_id != la)
        }) || flow.is_some_and(|f| ["independent", "review"].contains(&f.stage.as_str()))
        {
            return Err(refuse());
        }
        Ok(v["text"]
            .as_str()
            .map(|t| crate::learning::clip(t, 1500))
            .filter(|t| !t.is_empty()))
    }
    fn visual_append(&mut self, aid: &str, q: &Value, now: &str) -> VResult<Value> {
        let mut s = state(&self.conn, aid)?;
        if s["registered"] != true {
            return Err(refusal("NOT_FOUND"));
        }
        let tx = self.conn.transaction()?;
        let mut last = cursor(&tx, aid)?["lastSequence"].as_i64().unwrap();
        let mut acknowledged = vec![];
        let mut duplicates = vec![];
        for e in q["events"].as_array().unwrap() {
            if e["activityId"] != aid {
                return Err(refusal("INVALID_REFERENCE"));
            }
            let eid = text(e, "eventId")?;
            let fp = digest(e);
            let known: Option<String> = tx
                .query_row(
                    "SELECT fingerprint FROM visual_events WHERE event_id=?",
                    [eid],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(k) = known {
                if k != fp {
                    return Err(refusal("OPERATION_CONFLICT"));
                }
                duplicates.push(j!(eid));
                continue;
            }
            if num(e, "sequence")? != last + 1 {
                return Err(refusal("EVENT_GAP"));
            }
            last += 1;
            tx.execute(
                "INSERT INTO visual_events VALUES (?,?,?,?,?,?)",
                params![eid, aid, last, fp, e.to_string(), now],
            )?;
            let durable = tx.last_insert_rowid();
            acknowledged.push(j!({"eventId":eid,"durableSequence":durable,"hostTimestamp":now}));
            if e["type"] == "activity.closed" {
                s["closed"] = j!(true);
            }
        }
        write_state(&tx, aid, &s)?;
        let result =
            j!({"acknowledged":acknowledged,"duplicates":duplicates,"cursor":cursor(&tx,aid)?});
        schema("study-harness.append-result", &result)?;
        tx.commit()?;
        Ok(result)
    }
    fn visual_save(
        &mut self,
        aid: &str,
        q: &Value,
        key: &str,
        s: &Value,
        now: &str,
    ) -> VResult<Value> {
        let a = &q["artifact"];
        const FIELDS: &[&str] = &[
            "contract",
            "schemaVersion",
            "artifactId",
            "kind",
            "schemas",
            "runtime",
            "template",
            "model",
            "policyRef",
            "instance",
            "scientific",
            "presentation",
            "exposures",
            "journal",
            "pending",
            "contentRefs",
            "reviewRefs",
            "migratedFrom",
            "assistance",
            "lineage",
        ];
        let entries = FIELDS
            .iter()
            .filter_map(|f| a.get(*f).map(|v| j!({"path":f,"digest":digest(v)})))
            .collect::<Vec<_>>();
        if a["integrity"]["algorithm"] != "sha256"
            || a["integrity"]["entries"] != j!(entries)
            || a["integrity"]["digest"] != j!(digest(&j!(entries)))
            || a["template"] != s["template"]
            || a["model"] != s["model"]
        {
            return Err(refusal("INTEGRITY_FAILURE"));
        }
        let hash = digest(a);
        let aid_art = text(a, "artifactId")?;
        let prior: Option<String> = self
            .conn
            .query_row(
                "SELECT digest FROM visual_artifacts WHERE id=?",
                [aid_art],
                |r| r.get(0),
            )
            .optional()?;
        if prior.is_some_and(|h| h != hash) {
            return Err(refusal("OPERATION_CONFLICT"));
        }
        let mut result = ack(now);
        result["artifactDigest"] = j!(hash);
        let mut next = s.clone();
        if a["kind"] == "backup" {
            next["checkpointDigest"] = j!(hash);
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO visual_artifacts VALUES (?,?,?,?)",
            params![aid_art, aid, hash, canonical_json(a)],
        )?;
        write_state(&tx, aid, &next)?;
        remember(&tx, aid, key, "saveArtifact", q, &result)?;
        self_test_fault("save-before-commit")?;
        tx.commit()?;
        Ok(result)
    }
}

/// Snapshot captures all host records, even if a renderer is interrupted or omitted its optional snapshot.
/// Independent tasks inherit dated teaching exposure as history; only their own help contaminates the task.
pub fn attempt_snapshot(
    c: &Connection,
    attempt: &Attempt,
    provided: Option<&Value>,
) -> crate::db::Result<Value> {
    let ids: Vec<Value> = values(
        c,
        "SELECT json_object('id',id) FROM visual_activities WHERE session_id=? ORDER BY rowid",
        &attempt.session_id,
    )
    .map_err(|_| "Cannot read visual submission conditions")?;
    let mut host = vec![];
    for id in ids {
        host.push(
            records(c, id["id"].as_str().unwrap())
                .map_err(|_| "Cannot read visual submission conditions")?,
        );
    }
    if let Some(v) = provided {
        let aid = v["activityId"].as_str().ok_or("Missing visual activity")?;
        let frozen = &v["snapshot"];
        let own = state(c, aid).map_err(|_| "Visual activity unavailable")?;
        if own["sessionId"] != attempt.session_id
            || own["learningActivityId"] != attempt.activity_id
            || frozen["responseId"] != attempt.id
            || !frozen["complete"].is_boolean()
            || !frozen["limitations"].is_array()
            || frozen["limitations"].as_array().unwrap().len() > 16
        {
            return Err("Invalid visual submission snapshot".into());
        }
        let held = records(c, aid).map_err(|_| "Visual history unavailable")?;
        for key in ["exposures", "assistance"] {
            let items = frozen[key]
                .as_array()
                .ok_or("Missing frozen visual history")?;
            if items.len() > 256
                || items.iter().any(|e| {
                    !held[key].as_array().unwrap().iter().any(|h| {
                        if key == "exposures" {
                            h["exposureId"] == e["exposureId"]
                                && h["receiptId"] == e["receiptId"]
                                && h["groups"] == e["groups"]
                        } else {
                            h["receiptId"] == e["receiptId"]
                                && h["kind"] == e["kind"]
                                && h["grant"] == e["grant"]
                        }
                    })
                })
            {
                return Err("Visual snapshot claims an unissued grant".into());
            }
        }
        if frozen["complete"] == true
            && (frozen["exposures"].as_array().unwrap().len()
                != held["exposures"].as_array().unwrap().len()
                || frozen["assistance"].as_array().unwrap().len()
                    != held["assistance"].as_array().unwrap().len())
        {
            return Err("Visual snapshot omits authoritative history".into());
        }
    }
    Ok(
        j!({"schema":"study-harness.visual-attempt-conditions/1","attemptId":attempt.id,"adapterSnapshot":provided,"hostRecords":host}),
    )
}

// Test harness alone may arm process crashes; no setting, environment input or renderer command can arm these.
#[cfg(any(test, feature = "dev-tools"))]
thread_local! {pub static FAULT:std::cell::RefCell<Option<(String,bool)>>=const{std::cell::RefCell::new(None)};}
fn self_test_fault(_point: &str) -> VResult<()> {
    #[cfg(any(test, feature = "dev-tools"))]
    {
        let hit = FAULT.with(|f| {
            let mut f = f.borrow_mut();
            if f.as_ref().is_some_and(|(p, _)| p == _point) {
                f.take()
            } else {
                None
            }
        });
        if let Some((_, abort)) = hit {
            if abort {
                std::process::abort();
            }
            return Err(VisualError {
                code: "PERSISTENCE_FAILED",
                message: "Injected interrupted persistence.",
                reason: "TEST_INTERRUPT",
            });
        }
    }
    Ok(())
}

/// The visuals a planner may choose from: exact template identities, purposes, parameter ranges and constraints.
/// Nothing else can be opened, and generated scenes or code are never accepted.
pub fn planner_catalog() -> Value {
    j!(installed()["templates"]
        .as_array()
        .map(|templates| templates
            .iter()
            .map(|e| {
                let template = &e["content"]["template"];
                let manifest = &e["content"]["manifest"];
                let parameters: Vec<Value> = e["parameters"]
                    .as_object()
                    .map(|p| p
                        .iter()
                        .map(|(name, range)| {
                            let m = &manifest["parameters"][name];
                            j!({"name":name,"label":m["label"],"description":m["description"],"min":range[0],"max":range[1],"default":m["default"]})
                        })
                        .collect())
                    .unwrap_or_default();
                let constraints: Vec<Value> = manifest["constraints"]
                    .as_array()
                    .map(|c| c.iter().map(|c| j!({"expression":c["expression"],"min":c["min"],"max":c["max"],"rule":c["message"]})).collect())
                    .unwrap_or_default();
                j!({"template_id":e["template"]["templateId"],"title":template["title"],"purpose":template["purpose"],"dimension":template["dimension"],"parameters":parameters,"constraints":constraints})
            })
            .collect::<Vec<_>>())
        .unwrap_or_default())
}

/// Validates proposed visual parameters against an installed template: known names only, finite values inside the
/// model's ranges, and the model's own constraints. Missing parameters take the model's defaults.
pub fn validate_visual(
    template_id: &str,
    proposed: &[crate::contracts::VisualParameter],
) -> std::result::Result<Vec<crate::contracts::VisualParameter>, String> {
    let e = entry(template_id).map_err(|_| "that visual is not installed.".to_string())?;
    let ranges = e["parameters"]
        .as_object()
        .ok_or("the visual has no parameters.")?;
    let manifest = &e["content"]["manifest"];
    if proposed.len() > ranges.len() {
        return Err("too many parameters.".into());
    }
    let mut out = vec![];
    for (name, range) in ranges {
        let value = match proposed.iter().find(|p| &p.name == name) {
            Some(p) => p.value,
            None => manifest["parameters"][name]["default"]
                .as_f64()
                .ok_or("a parameter has no default.")?,
        };
        let (lo, hi) = (
            range[0].as_f64().unwrap_or(f64::NAN),
            range[1].as_f64().unwrap_or(f64::NAN),
        );
        if !value.is_finite() || value < lo || value > hi {
            return Err(format!("{name} = {value} is outside {lo}…{hi}."));
        }
        out.push(crate::contracts::VisualParameter {
            name: name.clone(),
            value,
        });
    }
    if let Some(unknown) = proposed.iter().find(|p| !ranges.contains_key(&p.name)) {
        return Err(format!("unknown parameter {}.", unknown.name));
    }
    let values: Vec<(String, f64)> = out.iter().map(|p| (p.name.clone(), p.value)).collect();
    for c in manifest["constraints"].as_array().into_iter().flatten() {
        let Some(v) = c["expression"]
            .as_str()
            .and_then(|x| crate::checks::constraint_value(x, &values))
        else {
            return Err("a model constraint could not be evaluated.".into());
        };
        let below = c["min"].as_f64().is_some_and(|m| {
            if c["minInclusive"] == false {
                v <= m
            } else {
                v < m
            }
        });
        let above = c["max"].as_f64().is_some_and(|m| {
            if c["maxInclusive"] == false {
                v >= m
            } else {
                v > m
            }
        });
        if below || above {
            return Err(c["message"]
                .as_str()
                .unwrap_or("a model constraint is not met.")
                .to_owned());
        }
    }
    Ok(out)
}
