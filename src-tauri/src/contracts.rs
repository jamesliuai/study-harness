use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const VERSION: u32 = 1;
macro_rules! contract {
    ($name:ident {$($(#[$meta:meta])* $field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$meta])* pub $field: $ty),* }
    };
}
contract!(Session {
    schema_version: u32,
    id: String,
    title: String,
    revision: i32,
    created_at: String,
    draft: String
});
contract!(Activity {
    schema_version: u32,
    id: String,
    session_id: String,
    objective_id: String,
    version: u32,
    prompt: String,
    math: String,
    task_family: String,
    rubric_version: String,
    validation_status: String
});
contract!(Attempt {schema_version:u32,id:String,session_id:String,activity_id:String,task_version:u32,response:String,recognized_transcript:Option<String>,modality:String,created_at:String,assistance_ids:Vec<String>,exposure_ids:Vec<String>,revision_of:Option<String>});
contract!(Assistance {schema_version:u32,id:String,session_id:String,activity_id:String,attempt_id:Option<String>,content:String,depth:String,visible_at:String});
contract!(RubricFinding {
    dimension: String,
    result: String,
    excerpt: String
});
// The learner's own final value or expression, quoted exactly; the core checks it against a frozen answer key.
contract!(FinalAnswer {
    excerpt: String,
    value: Option<f64>,
    unit: String,
    expression: String
});
// `error_kind` separates conceptual errors, slips, notation/transcription ambiguity and flawed tasks.
contract!(GradeProposal {verdict:String,findings:Vec<RubricFinding>,uncertainty:String,#[serde(default)] error_kind:String,#[serde(default)] final_answer:Option<FinalAnswer>});
// One deterministic check and its outcome (passed, failed, inconclusive, not_applicable). Never a model opinion.
contract!(CheckResult {
    id: String,
    label: String,
    outcome: String,
    detail: String
});
// Version 3 records deterministic answer checks separately from the model's proposed verdict, plus any reconciliation.
contract!(Assessment {schema_version:u32,id:String,attempt_id:String,rubric_version:String,findings:Vec<RubricFinding>,uncertainty:String,dispute_state:String,grader_version:String,stale:bool,verdict:String,assessed_at:String,request_id:String,#[serde(default)] error_kind:String,#[serde(default)] checks:Vec<CheckResult>,#[serde(default)] note:String});
contract!(Review {schema_version:u32,id:String,session_id:String,objective_id:String,task_family:String,due_at:String,reason:String,scheduler_version:String,supporting_attempts:Vec<String>,assessment_ids:Vec<String>,interval_days:u32,status:String,simulated:bool});
contract!(Source {
    schema_version: u32,
    id: String,
    sha256: String,
    title: String,
    anchor: String,
    extraction_version: String,
    import_scope: String
});
contract!(Request {schema_version:u32,id:String,session_id:String,attempt_id:Option<String>,session_revision:i32,kind:String,status:String,reserved_microusd:i32,created_at:String});
contract!(Usage {schema_version:u32,request_id:String,provider:String,model:String,#[serde(default)] reported_model:Option<String>,input_tokens:Option<u32>,output_tokens:Option<u32>,audio_seconds:Option<f64>,cost_microusd:i32,cost_status:String,rate_version:String,status:String});
// Version 2 freezes the reasoning effort sent with the request (empty for transcription and older records).
contract!(RequestVersions {request_id:String,app_version:String,#[serde(default)] build_source:String,requested_model:String,#[serde(default)] reasoning_effort:String,reported_model:Option<String>,prompt_version:String,prompt_sha256:String,content_version:String,task_version:u32,scheduler_version:String,policy_version:String,visualizer_manifest_sha256:String,recorded_at:String,provenance:String});
contract!(Limits {
    session_microusd: i32,
    monthly_microusd: i32
});
contract!(Budget {
    session_spent_microusd: i32,
    month_spent_microusd: i32,
    unresolved_count: u32,
    limits: Limits,
    month: String
});
contract!(CredentialStatus {
    configured: bool,
    storage: String,
    masked: String,
    message: String
});
contract!(SceneParameters {
    curvature: f64,
    step_size: f64,
    start: f64,
    steps: u32
});
contract!(SceneSpec {schema_version:u32,capability_version:String,model_id:String,model_version:String,objective_id:String,activity_id:String,parameters:SceneParameters,prediction:String,follow_up:String,assumptions:Vec<String>,validation:String,accessibility:String,revealed:bool});
contract!(CitationProposal {
    chunk_id: String,
    excerpt: String,
    kind: String
});
contract!(SourceDocument {schema_version:u32,id:String,sha256:String,title:String,kind:String,extraction_version:String,page_count:u32,warnings:Vec<String>,imported_at:String});
contract!(SourcePassage {
    id: String,
    source_id: String,
    title: String,
    anchor: String,
    text: String,
    sha256: String
});
contract!(SourceCitation {
    request_id: String,
    assistance_id: String,
    source_id: String,
    chunk_id: String,
    title: String,
    anchor: String,
    excerpt: String,
    kind: String
});
contract!(SourceView {documents:Vec<SourceDocument>,selected_passage_ids:Vec<String>,citations:Vec<SourceCitation>});
pub fn default_text_scale() -> u32 {
    100
}
pub fn default_tutor_model() -> String {
    crate::registry::DEFAULT_TUTOR_MODEL.into()
}
pub fn default_reasoning_effort() -> String {
    crate::registry::DEFAULT_EFFORT.into()
}
// Version 3 adds the learner's tutor model and reasoning effort; records saved earlier read as Sol at low effort.
contract!(PrivacySettings {
    retain_failed_audio: bool,
    review_minutes: u32,
    #[serde(default = "default_text_scale")]
    text_scale_percent: u32,
    #[serde(default = "default_tutor_model")]
    tutor_model: String,
    #[serde(default = "default_reasoning_effort")]
    reasoning_effort: String
});
// A selectable tutor model with its list prices in USD per million tokens.
contract!(ModelOption {
    id: String,
    label: String,
    note: String,
    input_usd_per_million: f64,
    output_usd_per_million: f64
});
contract!(DeletionPreview {scope:String,target_id:String,session_ids:Vec<String>,attempt_count:u32,notice:String});
// Version 4 adds a core-validated next-step suggestion; the provider never moves the stage itself.
contract!(TutorProposal {schema_version:u32,session_id:String,expected_session_revision:i32,activity_id:String,rubric_version:String,next_action:String,text:String,math:Option<String>,evidence_attempt_id:String,evidence_excerpt:String,unresolved_questions:Vec<String>,grade:Option<GradeProposal>,citations:Vec<CitationProposal>,#[serde(default)] suggested_step:Option<String>,#[serde(default)] step_reason:String});
contract!(UnitObjective {id:String,outcome:String,prerequisites:Vec<String>,visual_template:String,validation_status:String,coverage_count:u32,independent_count:u32,simulated_independent_count:u32,delayed_count:u32});
contract!(UnitView {version:String,title:String,notice:String,active_objective_id:String,plan:Vec<String>,objectives:Vec<UnitObjective>,continue_session_id:Option<String>,due_reviews:Vec<Review>,due_review_count:u32,available_review_minutes:u32});
// Plain-language provider state. `verified_at` comes from durable request records: a response that reached the
// application (accepted, rejected or stale) proves the key and model were usable then; nothing is probed silently.
contract!(ProviderStatus {state:String,verified_at:Option<String>,last_error_kind:Option<String>,last_error:Option<String>,last_error_at:Option<String>});
// Version 11 lists the selectable tutor models; `provider_model` is the learner's current choice.
contract!(Workspace {schema_version:u32,goal:Option<GoalView>,goals:Vec<GoalSummary>,unit:UnitView,sources:SourceView,privacy:PrivacySettings,learning:Option<LearningView>,offline:bool,scenarios:Vec<String>,pending_transcript:Option<Transcript>,sessions:Vec<Session>,active_session_id:String,activity:Activity,attempts:Vec<Attempt>,assistance:Vec<Assistance>,requests:Vec<Request>,request_versions:Vec<RequestVersions>,usage:Vec<Usage>,budget:Budget,credential:CredentialStatus,provider:ProviderStatus,scene:SceneSpec,provider_model:String,models:Vec<ModelOption>,transcription_model:String,rate_version:String,timeline:Vec<TimelineEntry>});
contract!(RecordingTicket {
    id: String,
    max_seconds: u32
});
contract!(Transcript {
    id: String,
    recognized_text: String,
    audio_seconds: f64,
    format: String
});
contract!(SaveAttempt {session_id:String,expected_revision:i32,submission_id:String,response:String,transcript_id:Option<String>});
contract!(LearningState {
    schema_version: u32,
    session_id: String,
    goal: String,
    stage: String,
    active_activity_id: String,
    quiet: bool,
    stopped: bool,
    diagnosis_count: u32,
    policy_version: String
});
// Generated support frozen with its task. It stays hidden until the learner asks; each reveal is recorded as help first.
contract!(TaskSupport {
    explanation: String,
    hints: Vec<String>,
    worked_example: String,
    prerequisite: String
});
contract!(Given {
    name: String,
    value: f64,
    unit: String
});
contract!(DomainVariable {
    name: String,
    min: f64,
    max: f64
});
// A machine-checkable reference answer: `numeric` (a formula over the givens, its value and unit) or `expression`
// (in the listed variables over a stated domain). `none` for proofs, explanations and other judged work.
contract!(AnswerKey {kind:String,givens:Vec<Given>,expression:String,value:Option<f64>,unit:String,tolerance:f64,variables:Vec<DomainVariable>});
// Version 2 adds the content origin, generated support, an optional answer key and the deterministic checks that ran
// before the task was shown. Authored and legacy tasks leave them empty.
contract!(TaskSpec {schema_version:u32,activity_id:String,kind:String,assumptions:Vec<String>,reference_solution:String,rubric:Vec<String>,#[serde(default)] rubric_criteria:Vec<String>,content_version:String,validation_version:String,frozen_at:String,transfer_scope:String,#[serde(default)] origin:String,#[serde(default)] response_kind:String,#[serde(default)] support:Option<TaskSupport>,#[serde(default)] answer_key:Option<AnswerKey>,#[serde(default)] checks:Vec<CheckResult>,#[serde(default)] provenance:String});
pub fn default_intent() -> String {
    "answer".into()
}
// Version 2 freezes whether the learner submitted an answer or asked a question. Questions are discussion: they are
// never graded, never count as diagnostic attempts and never advance a stage. Version 1 rows read as answers.
contract!(AttemptConditions {schema_version:u32,attempt_id:String,submitted_at:String,last_relevant_exposure_at:Option<String>,simulated:bool,outside_help:bool,task_kind:String,#[serde(default = "default_intent")] intent:String});
contract!(LearningSummary {schema_version:u32,session_id:String,text:String,assessment_ids:Vec<String>,stale:bool,generated_at:String,independent_count:u32,supported_count:u32,delayed_count:u32,simulated:bool});
// One learner-selectable next step. Rust computes eligibility from durable evidence; `recommended` marks the
// policy's (or a validated tutor) suggestion, and the learner may still choose any eligible option.
contract!(StepOption {
    id: String,
    label: String,
    reason: String,
    kind: String,
    recommended: bool
});
// Visual parameters fixed with the task, and how they relate to it in learner language.
contract!(VisualParameter {
    name: String,
    value: f64
});
contract!(VisualPlan {template_id:String,parameters:Vec<VisualParameter>,relation:String,purpose:String,matches_task:bool});
// Concise end-of-session account derived from primary records (never a mastery score): what was explained, what was
// done with help, what was shown independently and what remains unchecked.
contract!(WrapUp {explained:Vec<String>,demonstrated:Vec<String>,needed_help:Vec<String>,unchecked:Vec<String>});
contract!(LearningView {state:LearningState,assumptions:Vec<String>,target_dimensions:Vec<String>,transfer_scope:String,assessments:Vec<Assessment>,conditions:Vec<AttemptConditions>,summary:Option<LearningSummary>,reviews:Vec<Review>,allowed_actions:Vec<String>,now:String,steps:Vec<StepOption>,step_note:Option<String>,visual:Option<VisualPlan>,wrap_up:WrapUp,objective_outcome:String,stage_title:String,task_origin:String,task_provenance:String,task_checks:Vec<CheckResult>,rubric_lines:Vec<String>,response_kind:String,goal_id:Option<String>,generated:bool});
// One clarifying question the planner asks only when the answer changes the plan. `blocking` means planning
// cannot usefully proceed without it.
contract!(Clarification {question:String,options:Vec<String>,blocking:bool});
contract!(PlanVisual {template_id:String,parameters:Vec<VisualParameter>,purpose:String});
// A capability inside one plan revision. `objective_id` is the stable capability identity; `status` is planned,
// skipped or retired (kept for its evidence after a revision dropped it).
contract!(PlanCapability {objective_id:String,key:String,outcome:String,why:String,status:String,minutes:u32,assessment:String,origin:String,prerequisites:Vec<String>,visual:Option<PlanVisual>,note:String});
contract!(GoalRecord {schema_version:u32,id:String,text:String,created_at:String,origin:String,source_ids:Vec<String>,passage_ids:Vec<String>});
contract!(PlanRevision {schema_version:u32,goal_id:String,revision:u32,title:String,goal_text:String,summary:String,level:String,background:Vec<String>,capabilities:Vec<PlanCapability>,clarification:Option<Clarification>,reason:String,origin:String,provenance:String,request_id:Option<String>,source_passage_ids:Vec<String>,source_note:String,notes:String,created_at:String});
// Plan capability with progress derived from its sessions and evidence (never stored, never a score).
contract!(CapabilityProgress {objective_id:String,outcome:String,why:String,status:String,minutes:u32,assessment:String,origin:String,prerequisites:Vec<String>,visual:Option<String>,session_id:Option<String>,label:String,detail:String});
contract!(GoalSource {
    id: String,
    title: String,
    kind: String,
    warnings: Vec<String>,
    excerpt: String
});
contract!(GoalView {id:String,title:String,goal_text:String,summary:String,level:String,background:Vec<String>,revision:u32,origin:String,provenance:String,clarification:Option<Clarification>,capabilities:Vec<CapabilityProgress>,sources:Vec<GoalSource>,source_note:String,notes:String,reason:String,next_objective_id:Option<String>,minutes:u32});
contract!(GoalSummary {id:String,title:String,created_at:String,session_ids:Vec<String>,capabilities:u32,checked:u32});
// Chronological projection of durable records for the conversation surface. `hidden` entries carry no content:
// during an independent check or review, earlier teaching is withheld here as well as in the tutor context.
contract!(TimelineEntry {id:String,kind:String,at:String,activity_id:String,stage:String,text:String,detail:String,attempt_id:Option<String>,assistance_id:Option<String>,hidden:bool});
contract!(Diagnostics {
    app_version: String,
    schema_version: u32,
    sqlite_version: String,
    database_path: String,
    provider_endpoint: String,
    allowed_audio_format: String
});

pub fn valid_text(text: &str, max: usize) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > max {
        Err(format!("Enter between 1 and {max} bytes of text."))
    } else {
        Ok(())
    }
}
