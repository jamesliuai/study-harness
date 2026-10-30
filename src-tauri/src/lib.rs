#[cfg(all(feature = "offline-provider", not(debug_assertions)))]
compile_error!(
    "offline-provider is development-only; use a debug build. Production builds must omit it."
);
pub mod audio;
pub mod checks;
pub mod clock;
pub mod contracts;
mod credentials;
pub mod curriculum;
pub mod db;
pub mod evaluation;
pub mod generation;
pub mod goals;
pub mod learning;
#[cfg(any(test, feature = "offline-provider"))]
pub mod offline;
pub mod ownership;
pub mod providers;
pub mod registry;
pub mod scene;
pub mod sources;
pub mod visualizer;
use contracts::*;
use credentials::Credentials;
use db::{Result, Store};
use providers::{GenerationProvider, OpenAi, TranscriptionProvider, TutorProvider};
use registry::OPENAI;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;
struct Recording {
    id: String,
    session_id: String,
    started: Instant,
    stopped: bool,
}
struct Core {
    _profile_lock: std::fs::File,
    started: Instant,
    measure_startup: bool,
    no_keychain: bool,
    db: Mutex<Store>,
    credentials: Mutex<Credentials>,
    provider: Option<OpenAi>,
    job: Mutex<Option<CancellationToken>>,
    mic: Arc<AtomicBool>,
    recording: Mutex<Option<Recording>>,
}
#[tauri::command]
fn record_startup(core: State<Core>) -> Result<()> {
    if !core.measure_startup {
        return Ok(());
    }
    let d = lock(&core.db)?;
    let path = std::path::Path::new(&d.path)
        .parent()
        .ok_or("Missing profile folder")?
        .join("startup-metrics.json");
    let data = serde_json::json!({"kind":"native process entry to first usable workspace after two animation frames","elapsed_ms":core.started.elapsed().as_secs_f64()*1000.,"offline":d.offline,"app_version":env!("CARGO_PKG_VERSION"),"recorded_at":d.time(),"sessions":d.sessions()?.len(),"provider_latency":"none; no provider request"});
    ownership::write_selected_export(
        &path,
        &serde_json::to_vec_pretty(&data).map_err(|_| "Cannot encode local startup measurement")?,
    )
}
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    m.lock()
        .map_err(|_| "Internal state unavailable. Restart the application.".into())
}
struct JobGuard<'a>(&'a Mutex<Option<CancellationToken>>);
impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut j) = self.0.lock() {
            *j = None;
        }
    }
}
fn job(core: &Core) -> Result<(CancellationToken, JobGuard<'_>)> {
    let mut j = lock(&core.job)?;
    if j.is_some() {
        return Err("Another provider request is running. Wait or cancel it.".into());
    }
    let token = CancellationToken::new();
    *j = Some(token.clone());
    Ok((token, JobGuard(&core.job)))
}
fn view(c: &Core, sid: Option<String>) -> Result<Workspace> {
    let offline = lock(&c.db)?.offline;
    let credential = if offline {
        CredentialStatus {
            configured: false,
            storage: "disabled in offline development".into(),
            masked: String::new(),
            message: "Simulated responses; no credentials, outbound provider calls or billing."
                .into(),
        }
    } else {
        let mut status = lock(&c.credentials)?.status();
        if c.no_keychain && !status.configured {
            status.storage = "session only (Keychain disabled)".into();
            status.message="This launch uses --no-keychain. No saved credential is read or written; choose session-only setup if needed.".into();
        }
        status
    };
    lock(&c.db)?.workspace_view(sid, credential)
}

#[tauri::command]
fn workspace(core: State<Core>, session_id: Option<String>) -> Result<Workspace> {
    view(&core, session_id)
}
#[tauri::command]
fn new_session(core: State<Core>, title: String) -> Result<Workspace> {
    let sid = lock(&core.db)?.new_session(&title)?;
    view(&core, Some(sid))
}
#[tauri::command]
fn save_draft(core: State<Core>, session_id: String, text: String) -> Result<()> {
    lock(&core.db)?.save_draft(&session_id, &text)
}
#[tauri::command]
fn save_attempt(
    core: State<Core>,
    input: SaveAttempt,
    visual_submission: Option<serde_json::Value>,
    intent: Option<String>,
) -> Result<Workspace> {
    lock(&core.db)?.save_submission(
        &input,
        visual_submission.as_ref(),
        intent.as_deref().unwrap_or("answer"),
    )?;
    view(&core, Some(input.session_id))
}
#[tauri::command]
fn visualizer_rpc(
    core: State<Core>,
    request: tauri::ipc::Request<'_>,
) -> std::result::Result<serde_json::Value, visualizer::VisualError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(visualizer::VisualError {
            code: "INVALID_INPUT",
            message: "A bounded binary request is required.",
            reason: "RAW_BODY_REQUIRED",
        });
    };
    lock(&core.db)
        .map_err(visualizer::VisualError::from)?
        .visualizer_rpc(bytes)
}
#[tauri::command]
fn source_rpc(core: State<Core>, request: tauri::ipc::Request<'_>) -> Result<serde_json::Value> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("A bounded binary source request is required.".into());
    };
    lock(&core.db)?.source_rpc(bytes)
}
#[tauri::command]
fn evaluation_rpc(
    core: State<Core>,
    request: tauri::ipc::Request<'_>,
) -> Result<serde_json::Value> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("A bounded binary evaluation request is required".into());
    };
    lock(&core.db)?.evaluation_rpc(bytes)
}
#[tauri::command]
async fn export_evaluation(core: State<'_, Core>) -> Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Export private personal comparison records")
            .set_file_name("Study-Harness-comparison.json")
            .save_file()
            .await
        else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec_pretty(&lock(&core.db)?.evaluation_view(true)?)
            .map_err(|_| "Cannot encode comparison export")?;
        ownership::write_selected_export(file.path(), &bytes)?;
        Ok(Some(file.path().to_string_lossy().into()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = core;
        Err("Evaluation export dialogs are implemented for macOS.".into())
    }
}
#[tauri::command]
fn set_credential(core: State<Core>, key: String, persist: bool) -> Result<CredentialStatus> {
    if lock(&core.db)?.offline {
        return Err("Credentials are disabled in offline development.".into());
    }
    let status = lock(&core.credentials)?.set(key, persist)?;
    // A new key is unverified until a response or an explicit access check succeeds with it.
    let mut d = lock(&core.db)?;
    d.provider.changed_at = Some(d.time());
    d.provider.error = None;
    Ok(status)
}
#[tauri::command]
fn remove_credential(core: State<Core>) -> Result<CredentialStatus> {
    if lock(&core.db)?.offline {
        return Err("Credentials are disabled in offline development.".into());
    }
    let status = lock(&core.credentials)?.remove()?;
    let mut d = lock(&core.db)?;
    d.provider.changed_at = Some(d.time());
    d.provider.error = None;
    Ok(status)
}
/// Opt-in, learner-initiated access check: two model lookups with the saved key, no learning content, no tokens.
#[tauri::command]
async fn check_connection(core: State<'_, Core>, session_id: String) -> Result<Workspace> {
    let (token, _guard) = job(&core)?;
    if lock(&core.db)?.offline {
        return Err("Offline development makes no provider requests.".into());
    }
    let key = lock(&core.credentials)?.key()?;
    let model = lock(&core.db)?.model_choice()?.model.id;
    let result = core
        .provider
        .as_ref()
        .ok_or("Live provider unavailable")?
        .check_models(key, token, model)
        .await;
    {
        let mut d = lock(&core.db)?;
        match &result {
            Ok(()) => {
                d.provider.checked_at = Some(d.time());
                d.note_provider(None);
            }
            Err(e) => d.note_provider(Some(e)),
        }
    }
    result?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn update_limits(core: State<Core>, limits: Limits, session_id: String) -> Result<Workspace> {
    lock(&core.db)?.set_limits(&limits)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn update_privacy(core: State<Core>, privacy: PrivacySettings) -> Result<()> {
    lock(&core.db)?.set_privacy(&privacy)
}
#[tauri::command]
fn deletion_preview(
    core: State<Core>,
    scope: String,
    target_id: String,
) -> Result<DeletionPreview> {
    lock(&core.db)?.deletion_preview(&scope, &target_id)
}
#[tauri::command]
fn delete_records(
    core: State<Core>,
    scope: String,
    target_id: String,
    confirmation: String,
) -> Result<()> {
    if lock(&core.recording)?.is_some() || lock(&core.job)?.is_some() {
        return Err(
            "Finish or discard recording and running requests before deleting data.".into(),
        );
    }
    lock(&core.db)?.delete_owned(&scope, &target_id, &confirmation)
}
#[tauri::command]
async fn export_data(
    core: State<'_, Core>,
    protected: bool,
    passphrase: String,
) -> Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        let selected = rfd::AsyncFileDialog::new()
            .set_title(if protected {
                "Save protected Study Harness backup"
            } else {
                "Export portable Study Harness archive"
            })
            .set_file_name(if protected {
                "Study-Harness.shbackup"
            } else {
                "Study-Harness.sharchive"
            })
            .save_file()
            .await;
        let Some(file) = selected else {
            return Ok(None);
        };
        let bytes = lock(&core.db)?.export_archive()?;
        let password = zeroize::Zeroizing::new(passphrase);
        let bytes = tokio::task::spawn_blocking(move || {
            if protected {
                ownership::protect(&bytes, &password)
            } else {
                Ok(bytes)
            }
        })
        .await
        .map_err(|_| "Backup task interrupted")??;
        ownership::write_selected_export(file.path(), &bytes)?;
        Ok(Some(file.path().to_string_lossy().into()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (core, protected, passphrase);
        Err("Native archive dialogs have only been implemented for macOS.".into())
    }
}
#[tauri::command]
async fn restore_data(core: State<'_, Core>, passphrase: Option<String>) -> Result<bool> {
    #[cfg(target_os = "macos")]
    {
        if lock(&core.recording)?.is_some() || lock(&core.job)?.is_some() {
            return Err(
                "Finish or discard recording and running requests before restoring.".into(),
            );
        }
        let selected = rfd::AsyncFileDialog::new()
            .set_title("Restore into this fresh Study Harness profile")
            .add_filter(
                "Study Harness archive or backup",
                &["sharchive", "shbackup"],
            )
            .pick_file()
            .await;
        let Some(file) = selected else {
            return Ok(false);
        };
        if std::fs::metadata(file.path())
            .map_err(|_| "Cannot read selected archive")?
            .len()
            > ownership::MAX_PROTECTED as u64
        {
            return Err("Archive exceeds the import limit".into());
        }
        let bytes = std::fs::read(file.path()).map_err(|_| "Cannot read selected archive")?;
        let password = passphrase.map(zeroize::Zeroizing::new);
        let bytes = tokio::task::spawn_blocking(move || {
            ownership::unprotect(&bytes, password.as_ref().map(|s| s.as_str()))
        })
        .await
        .map_err(|_| "Restore task interrupted")??;
        lock(&core.db)?.restore_archive(&bytes, None)?;
        Ok(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (core, passphrase);
        Err("Native archive dialogs have only been implemented for macOS.".into())
    }
}
#[tauri::command]
fn cancel_request(core: State<Core>) -> Result<()> {
    if let Some(j) = lock(&core.job)?.as_ref() {
        j.cancel();
    }
    Ok(())
}
#[tauri::command]
async fn ask_tutor(
    core: State<'_, Core>,
    session_id: String,
    attempt_id: String,
    scenario: Option<String>,
    visual_summary: Option<serde_json::Value>,
) -> Result<Workspace> {
    let outcome = tutor_request(&core, &session_id, &attempt_id, scenario, visual_summary).await;
    {
        let mut d = lock(&core.db)?;
        if !d.offline {
            d.note_provider(outcome.as_ref().err().map(String::as_str));
        }
    }
    outcome?;
    view(&core, Some(session_id))
}
async fn tutor_request(
    core: &Core,
    session_id: &str,
    attempt_id: &str,
    scenario: Option<String>,
    visual_summary: Option<serde_json::Value>,
) -> Result<()> {
    let (token, _guard) = job(core)?;
    let offline = lock(&core.db)?.offline;
    let key = if offline {
        None
    } else {
        Some(lock(&core.credentials)?.key()?)
    };
    let visual = match visual_summary {
        Some(v) => lock(&core.db)?.visual_tutor_summary(session_id, &v)?,
        None => None,
    };
    let input = lock(&core.db)?.prepare_tutor_request_with(session_id, attempt_id, visual)?;
    let request = input.request.clone();
    let model = input.choice.model.id;
    let result = if offline {
        #[cfg(feature = "offline-provider")]
        {
            offline::respond(
                input,
                scenario.as_deref().unwrap_or("correct"),
                token.clone(),
            )
            .await
        }
        #[cfg(not(feature = "offline-provider"))]
        {
            let _ = scenario;
            Err("Offline provider is not compiled into this build.".into())
        }
    } else {
        core.provider
            .as_ref()
            .ok_or("Live provider unavailable")?
            .tutor(key.ok_or("Missing credential")?, input, token.clone())
            .await
    };
    match result {
        Ok(result) if token.is_cancelled() => {
            lock(&core.db)?.settle(&request, "cancelled", result.usage, None)?;
            return Err("Request cancelled. Usage retained.".into());
        }
        Ok(result) => match result.proposal {
            Ok(p) => lock(&core.db)?.settle(&request, "completed", result.usage, Some(&p))?,
            Err(e) => {
                lock(&core.db)?.settle(&request, "rejected", result.usage, None)?;
                return Err(e);
            }
        },
        Err(e) => {
            lock(&core.db)?.settle(
                &request,
                if token.is_cancelled() || e.contains("cancelled") {
                    "cancelled"
                } else {
                    "failed"
                },
                Store::estimated_for(&request, model),
                None,
            )?;
            return Err(e);
        }
    }
    Ok(())
}
/// A raw generation reply: the live provider, or the labelled offline fixture in development builds only.
async fn generate(
    core: &Core,
    payload: serde_json::Value,
    fixture: impl std::future::Future<Output = Result<serde_json::Value>>,
    key: Option<zeroize::Zeroizing<String>>,
    token: CancellationToken,
) -> Result<serde_json::Value> {
    if lock(&core.db)?.offline {
        #[cfg(feature = "offline-provider")]
        {
            let _ = payload;
            return fixture.await;
        }
        #[cfg(not(feature = "offline-provider"))]
        {
            drop(fixture);
            return Err("Offline provider is not compiled into this build.".into());
        }
    }
    drop(fixture);
    core.provider
        .as_ref()
        .ok_or("Live provider unavailable")?
        .generate(key.ok_or("Missing credential")?, payload, token)
        .await
}
fn key_for(core: &Core) -> Result<Option<zeroize::Zeroizing<String>>> {
    if lock(&core.db)?.offline {
        Ok(None)
    } else {
        Ok(Some(lock(&core.credentials)?.key()?))
    }
}
#[allow(clippy::too_many_arguments)]
async fn plan_request(
    core: &Core,
    session_id: &str,
    revision: i32,
    goal: &str,
    source_ids: &[String],
    instruction: &str,
    goal_id: Option<&str>,
    scenario: Option<String>,
) -> Result<()> {
    let (token, _guard) = job(core)?;
    let key = key_for(core)?;
    let (input, ctx) = lock(&core.db)?.prepare_plan(
        session_id,
        revision,
        goal,
        source_ids,
        instruction,
        goal_id,
    )?;
    let request = input.request.clone();
    #[cfg(feature = "offline-provider")]
    let fixture = offline::plan_reply(
        &input,
        scenario.as_deref().unwrap_or("correct"),
        token.clone(),
    );
    #[cfg(not(feature = "offline-provider"))]
    let fixture = {
        let _ = &scenario;
        async { Err::<serde_json::Value, String>("No fixture".into()) }
    };
    let reply = generate(
        core,
        generation::plan_payload(&input),
        fixture,
        key,
        token.clone(),
    )
    .await;
    let mut d = lock(&core.db)?;
    match reply {
        Ok(v) if token.is_cancelled() => {
            let (_, usage) = generation::parse_plan(&v, &request, input.choice.model);
            d.commit_plan(&ctx, &request, "cancelled", usage, None)?;
            Err("Request cancelled. Usage retained.".into())
        }
        Ok(v) => {
            let (proposal, usage) = generation::parse_plan(&v, &request, input.choice.model);
            match proposal {
                Ok(p) => d.commit_plan(&ctx, &request, "completed", usage, Some(&p)),
                Err(e) => {
                    d.commit_plan(&ctx, &request, "rejected", usage, None)?;
                    Err(e)
                }
            }
        }
        Err(e) => {
            let status = if token.is_cancelled() || e.contains("cancelled") {
                "cancelled"
            } else {
                "failed"
            };
            d.commit_plan(
                &ctx,
                &request,
                status,
                Store::estimated_for(&request, input.choice.model.id),
                None,
            )?;
            Err(e)
        }
    }
}
#[allow(clippy::too_many_arguments)]
async fn task_request(
    core: &Core,
    session_id: &str,
    revision: i32,
    action: &str,
    objective: Option<&str>,
    note: &str,
    scenario: Option<String>,
) -> Result<()> {
    let (token, _guard) = job(core)?;
    let key = key_for(core)?;
    let (input, plan) =
        lock(&core.db)?.prepare_task(session_id, revision, action, objective, note)?;
    let request = input.request.clone();
    #[cfg(feature = "offline-provider")]
    let fixture = offline::task_reply(
        &input,
        scenario.as_deref().unwrap_or("correct"),
        token.clone(),
    );
    #[cfg(not(feature = "offline-provider"))]
    let fixture = {
        let _ = &scenario;
        async { Err::<serde_json::Value, String>("No fixture".into()) }
    };
    let reply = generate(
        core,
        generation::task_payload(&input),
        fixture,
        key,
        token.clone(),
    )
    .await;
    let mut d = lock(&core.db)?;
    match reply {
        Ok(v) if token.is_cancelled() => {
            let (_, usage) = generation::parse_task(&v, &request, input.choice.model);
            d.commit_task(&plan, &request, "cancelled", usage, None)?;
            Err("Request cancelled. Usage retained.".into())
        }
        Ok(v) => {
            let (proposal, usage) = generation::parse_task(&v, &request, input.choice.model);
            match proposal {
                Ok(p) => d.commit_task(&plan, &request, "completed", usage, Some(p)),
                Err(e) => {
                    d.commit_task(&plan, &request, "rejected", usage, None)?;
                    Err(e)
                }
            }
        }
        Err(e) => {
            let status = if token.is_cancelled() || e.contains("cancelled") {
                "cancelled"
            } else {
                "failed"
            };
            d.commit_task(
                &plan,
                &request,
                status,
                Store::estimated_for(&request, input.choice.model.id),
                None,
            )?;
            Err(e)
        }
    }
}
fn note_outcome(core: &Core, outcome: &Result<()>) -> Result<()> {
    let mut d = lock(&core.db)?;
    if !d.offline {
        d.note_provider(outcome.as_ref().err().map(String::as_str));
    }
    Ok(())
}
/// Plans a new goal from the learner's words and any attached sources. Saving work never needs this call.
#[tauri::command]
async fn plan_goal(
    core: State<'_, Core>,
    session_id: String,
    revision: i32,
    goal: String,
    source_ids: Vec<String>,
    scenario: Option<String>,
) -> Result<Workspace> {
    let outcome = plan_request(
        &core,
        &session_id,
        revision,
        &goal,
        &source_ids,
        "",
        None,
        scenario,
    )
    .await;
    note_outcome(&core, &outcome)?;
    outcome?;
    view(&core, Some(session_id))
}
/// Revises the plan with the learner's instruction or clarification answer; recorded work is never dropped silently.
#[tauri::command]
async fn revise_plan(
    core: State<'_, Core>,
    session_id: String,
    revision: i32,
    goal_id: String,
    instruction: String,
    scenario: Option<String>,
) -> Result<Workspace> {
    let outcome = plan_request(
        &core,
        &session_id,
        revision,
        "",
        &[],
        &instruction,
        Some(&goal_id),
        scenario,
    )
    .await;
    note_outcome(&core, &outcome)?;
    outcome?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn plan_decision(
    core: State<Core>,
    session_id: String,
    revision: i32,
    goal_id: String,
    objective_id: String,
    decision: String,
) -> Result<Workspace> {
    lock(&core.db)?.plan_decision(&session_id, revision, &goal_id, &objective_id, &decision)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn start_sample(
    core: State<Core>,
    session_id: String,
    revision: i32,
    objective_id: String,
) -> Result<Workspace> {
    lock(&core.db)?.start_authored_goal(&session_id, revision, &objective_id)?;
    view(&core, Some(session_id))
}
/// Begins (or reopens) a plan capability in its own session: "learn" starts with a diagnostic question, "check"
/// goes straight to a fresh check. Generated capabilities first receive a newly written, checked task.
#[tauri::command]
async fn begin_capability(
    core: State<'_, Core>,
    session_id: String,
    revision: i32,
    goal_id: String,
    objective_id: String,
    mode: String,
    scenario: Option<String>,
) -> Result<Workspace> {
    if !["learn", "check"].contains(&mode.as_str()) {
        return Err("Choose to learn the capability or check it.".into());
    }
    let target = {
        let mut d = lock(&core.db)?;
        if d.session(&session_id)?.revision != revision {
            return Err("Session changed. Refresh first.".into());
        }
        d.session_for_capability(&session_id, &goal_id, &objective_id)?
    };
    {
        let mut d = lock(&core.db)?;
        if d.learning_state(&target)?.is_some() {
            drop(d);
            return view(&core, Some(target));
        }
        if !goals::is_generated(&d.conn, &objective_id)? {
            d.begin_authored(&target, &goal_id, &objective_id, &mode)?;
            drop(d);
            return view(&core, Some(target));
        }
    }
    let rev = lock(&core.db)?.session(&target)?.revision;
    let action = if mode == "check" { "check" } else { "start" };
    let outcome = task_request(
        &core,
        &target,
        rev,
        action,
        Some(&objective_id),
        "",
        scenario,
    )
    .await;
    note_outcome(&core, &outcome)?;
    outcome?;
    view(&core, Some(target))
}
/// Takes a lesson step. Authored capabilities and steps without a new task act at once; a generated capability's
/// next task is written, checked and frozen first. `note` reports a problem with the current task.
#[tauri::command]
async fn advance(
    core: State<'_, Core>,
    session_id: String,
    revision: i32,
    action: String,
    note: Option<String>,
    scenario: Option<String>,
) -> Result<Workspace> {
    let needs = lock(&core.db)?.needs_generation(&session_id, &action)?;
    if !needs {
        lock(&core.db)?.learning_action(&session_id, revision, &action)?;
        return view(&core, Some(session_id));
    }
    let outcome = task_request(
        &core,
        &session_id,
        revision,
        &action,
        None,
        note.as_deref().unwrap_or(""),
        scenario,
    )
    .await;
    note_outcome(&core, &outcome)?;
    outcome?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn start_goal(
    core: State<Core>,
    session_id: String,
    revision: i32,
    goal: String,
    objective_id: Option<String>,
) -> Result<Workspace> {
    lock(&core.db)?.start_goal_for(
        &session_id,
        revision,
        &goal,
        objective_id.as_deref().unwrap_or("quadratic-convergence"),
    )?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn learning_action(
    core: State<Core>,
    session_id: String,
    revision: i32,
    action: String,
) -> Result<Workspace> {
    lock(&core.db)?.learning_action(&session_id, revision, &action)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn correct_attempt(
    core: State<Core>,
    session_id: String,
    revision: i32,
    attempt_id: String,
    submission_id: String,
    text: String,
) -> Result<Workspace> {
    lock(&core.db)?.correct_attempt(&session_id, revision, &attempt_id, &submission_id, &text)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn dispute_grade(
    core: State<Core>,
    session_id: String,
    revision: i32,
    assessment_id: String,
    reason: String,
) -> Result<Workspace> {
    lock(&core.db)?.dispute_grade(&session_id, revision, &assessment_id, &reason)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn resolve_dispute(
    core: State<Core>,
    session_id: String,
    revision: i32,
    assessment_id: String,
) -> Result<Workspace> {
    lock(&core.db)?.resolve_dispute(&session_id, revision, &assessment_id)?;
    view(&core, Some(session_id))
}
#[tauri::command]
fn development_action(core: State<Core>, session_id: String, action: String) -> Result<Workspace> {
    if !lock(&core.db)?.offline {
        return Err("Development controls are disabled in this build.".into());
    }
    #[cfg(feature = "offline-provider")]
    {
        let sid = lock(&core.db)?.development_action(&session_id, &action)?;
        view(&core, Some(sid))
    }
    #[cfg(not(feature = "offline-provider"))]
    {
        let _ = (session_id, action);
        Err("Development fixtures are not compiled into this build.".into())
    }
}
#[tauri::command]
fn development_cases(core: State<Core>) -> Result<serde_json::Value> {
    if !lock(&core.db)?.offline {
        return Err("Development controls are disabled in this build.".into());
    }
    #[cfg(feature = "offline-provider")]
    {
        Ok(offline::diagnostic_list())
    }
    #[cfg(not(feature = "offline-provider"))]
    {
        Err("Development fixtures are not compiled into this build.".into())
    }
}
#[tauri::command]
fn begin_recording(core: State<Core>, session_id: String) -> Result<RecordingTicket> {
    {
        let db = lock(&core.db)?;
        db.session(&session_id)?;
        if db.pending_transcript(&session_id)?.is_some() {
            return Err(
                "Confirm or discard this session's transcript before recording again.".into(),
            );
        }
    }
    let mut r = lock(&core.recording)?;
    if r.is_some() {
        return Err("A recording is already active. Stop or discard it first.".into());
    }
    let id = db::id();
    *r = Some(Recording {
        id: id.clone(),
        session_id,
        started: Instant::now(),
        stopped: false,
    });
    core.mic.store(true, Ordering::SeqCst);
    Ok(RecordingTicket {
        id,
        max_seconds: 120,
    })
}
#[tauri::command]
fn end_recording(core: State<Core>, recording_id: String) -> Result<()> {
    let mut r = lock(&core.recording)?;
    let r = r.as_mut().ok_or("No recording is active.")?;
    if r.id != recording_id {
        return Err("Recording ticket mismatch.".into());
    }
    core.mic.store(false, Ordering::SeqCst);
    r.stopped = true;
    Ok(())
}
#[tauri::command]
fn discard_recording(core: State<Core>, recording_id: String) -> Result<()> {
    let mut r = lock(&core.recording)?;
    if r.as_ref().is_some_and(|r| r.id == recording_id) {
        core.mic.store(false, Ordering::SeqCst);
        *r = None;
    }
    Ok(())
}
#[tauri::command]
fn discard_transcript(core: State<Core>, session_id: String, transcript_id: String) -> Result<()> {
    lock(&core.db)?.discard_transcript(&session_id, &transcript_id)
}
#[tauri::command]
async fn transcribe(
    core: State<'_, Core>,
    session_id: String,
    recording_id: String,
    wav: Vec<u8>,
) -> Result<Transcript> {
    let (token, _guard) = job(&core)?;
    if lock(&core.db)?.offline {
        return Err("Live transcription is disabled in offline development. Use the labeled transcript scenario or discard this recording.".into());
    }
    let key = lock(&core.credentials)?.key()?;
    let seconds = audio::validate_wav(&wav)?;
    {
        let r = lock(&core.recording)?;
        let r = r
            .as_ref()
            .ok_or("Record audio with Start recording first.")?;
        if r.id != recording_id
            || r.session_id != session_id
            || !r.stopped
            || r.started.elapsed().as_secs() > 600
        {
            return Err("Recording ticket is invalid or expired. Record again.".into());
        }
    }
    let request = lock(&core.db)?.reserve(
        &session_id,
        None,
        "transcription",
        (seconds * OPENAI.audio_microusd_per_second).ceil() as i32,
    )?;
    let result = core
        .provider
        .as_ref()
        .ok_or("Live provider unavailable")?
        .transcribe(key, request.clone(), wav, seconds, token.clone())
        .await;
    lock(&core.db)?.note_provider(result.as_ref().err().map(String::as_str));
    match result {
        Ok(result) if token.is_cancelled() => {
            lock(&core.db)?.settle(&request, "cancelled", result.usage, None)?;
            Err("Transcription cancelled. Usage retained.".into())
        }
        Ok(result) => {
            let mut recording = lock(&core.recording)?;
            let mut d = lock(&core.db)?;
            let stale = d.session(&session_id)?.revision != request.session_revision;
            d.settle(
                &request,
                if stale { "stale" } else { "completed" },
                result.usage,
                None,
            )?;
            if stale {
                return Err("Session changed during transcription. Usage retained; transcript was not installed in a different activity.".into());
            }
            if !recording
                .as_ref()
                .is_some_and(|r| r.id == recording_id && r.session_id == session_id)
            {
                return Err(
                    "Recording was discarded while transcription was running. Usage was retained."
                        .into(),
                );
            }
            let transcript = Transcript {
                id: db::id(),
                recognized_text: result.text,
                audio_seconds: seconds,
                format: "mono PCM WAV / 16 kHz / 16 bit".into(),
            };
            d.save_transcript(&session_id, &transcript)?;
            *recording = None;
            Ok(transcript)
        }
        Err(e) => {
            lock(&core.db)?.settle(&request, "failed", Store::estimated(&request), None)?;
            Err(e)
        }
    }
}
#[tauri::command]
fn diagnostics(core: State<Core>) -> Result<Diagnostics> {
    let d = lock(&core.db)?;
    Ok(Diagnostics {
        app_version: env!("CARGO_PKG_VERSION").into(),
        schema_version: ownership::CURRENT_SCHEMA as u32,
        sqlite_version: rusqlite::version().into(),
        database_path: d.path.clone(),
        provider_endpoint: if d.offline {
            "disabled · offline development"
        } else {
            OPENAI.endpoint
        }
        .into(),
        allowed_audio_format: "mono PCM WAV / 16000 Hz / 16 bit; max 120 seconds".into(),
    })
}
pub fn run() {
    let started = Instant::now();
    #[cfg(all(debug_assertions, not(feature = "offline-provider")))]
    if std::env::args().any(|a| a == "--credential-smoke") {
        match credentials::native_smoke() {
            Ok(result) => println!("{}", result),
            Err(e) => {
                eprintln!("Credential smoke failed: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }
    tauri::Builder::default()
        .setup(move |app| {
            let offline = cfg!(feature = "offline-provider");
            let base_dir = if offline {
                app.path().app_data_dir()?.join("offline-development")
            } else {
                app.path().app_data_dir()?
            };
            // A named profile is selected only at process launch, never by a renderer path.
            let args: Vec<String> = std::env::args().collect();
            let profile = args.windows(2).find(|v|v[0]=="--profile").map(|v|v[1].clone());
            let dir = if let Some(name)=profile {
                if name.is_empty() || name.len()>64 || !name.bytes().all(|b|b.is_ascii_alphanumeric() || b"-_".contains(&b)) {return Err("Use a profile name containing 1–64 letters, numbers, hyphens or underscores".into());}
                base_dir.join("profiles").join(name)
            }else{base_dir};
            std::fs::create_dir_all(&dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let mut lock_options=std::fs::OpenOptions::new();lock_options.create(true).truncate(false).read(true).write(true);
            #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;lock_options.mode(0o600);}
            let profile_lock=lock_options.open(dir.join(".study-harness.lock"))?;
            if profile_lock.try_lock().is_err() {
                // Tauri panics when a setup hook returns an error. A duplicate
                // launch should exit normally, leaving the existing writer alone.
                eprintln!("This Study Harness profile is already open. Use the existing window, quit it, or launch with a different --profile name.");
                // No database, credentials or window has been opened yet. A
                // synchronous exit also avoids queuing an exit from Ready.
                std::process::exit(1);
            }
            ownership::cleanup_staging(&dir)?;
            let mut d = Store::open_with(
                &dir.join("study-harness.sqlite3"),
                offline,
                Arc::new(clock::SystemClock),
            )?;
            if offline {
                use rusqlite::OptionalExtension;
                let saved: Option<String> = d
                    .conn
                    .query_row(
                        "SELECT value FROM settings WHERE key='simulated_clock'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(raw) = saved {
                    let value: String = serde_json::from_str(&raw)?;
                    d.clock = Arc::new(clock::TestClock(Mutex::new(
                        chrono::DateTime::parse_from_rfc3339(&value)?.with_timezone(&chrono::Utc),
                    )));
                }
            }
            let mic = Arc::new(AtomicBool::new(false));
            app.manage(Core {
                _profile_lock:profile_lock,
                started,
                measure_startup:args.iter().any(|a|a=="--measure-startup"),
                no_keychain:args.iter().any(|a|a=="--no-keychain"),
                db: Mutex::new(d),
                credentials: Mutex::new(if offline||args.iter().any(|a|a=="--no-keychain"){Credentials::without_native()}else{Credentials::new()}),
                provider: if offline { None } else { Some(OpenAi::new()?) },
                job: Mutex::new(None),
                mic: mic.clone(),
                recording: Mutex::new(None),
            });
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(if offline {
                "Study Harness · OFFLINE DEVELOPMENT"
            } else {
                "Study Harness"
            })
            .inner_size(1280., 860.)
            .min_inner_size(820., 640.)
            .on_web_resource_request(|_request, response| {
                if response
                    .headers()
                    .get("content-type")
                    .is_some_and(|v| v.to_str().is_ok_and(|v| v.contains("javascript")))
                {
                    response.headers_mut().insert(
                        "Content-Security-Policy",
                        tauri::http::HeaderValue::from_static(
                            "default-src 'none'; script-src 'self'; connect-src 'self'",
                        ),
                    );
                }
            })
            .on_navigation(|url| {
                url.scheme() == "tauri" && url.host_str() == Some("localhost")
                    || cfg!(debug_assertions)
                        && url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.port() == Some(1420)
            })
            .on_permission_request(move |_webview, kind| {
                use tauri::webview::{PermissionKind, PermissionResponse};
                if kind == PermissionKind::Microphone && mic.load(Ordering::SeqCst) {
                    PermissionResponse::Default
                } else {
                    PermissionResponse::Deny
                }
            })
            .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace,
            plan_goal,
            revise_plan,
            plan_decision,
            start_sample,
            begin_capability,
            advance,
            record_startup,
            start_goal,
            learning_action,
            correct_attempt,
            dispute_grade,
            resolve_dispute,
            development_action,
            development_cases,
            new_session,
            save_draft,
            save_attempt,
            visualizer_rpc,
            source_rpc,
            evaluation_rpc,
            export_evaluation,
            set_credential,
            remove_credential,
            check_connection,
            update_limits,
            update_privacy,
            deletion_preview,
            delete_records,
            export_data,
            restore_data,
            cancel_request,
            ask_tutor,
            begin_recording,
            end_recording,
            discard_recording,
            discard_transcript,
            transcribe,
            diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("Study Harness could not start");
}

#[cfg(test)]
mod learning_tests;
