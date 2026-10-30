use schemars::schema_for;
use study_harness::contracts::*;
use ts_rs::{Config, TS};
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut types =
        String::from("// Generated from Rust contracts. Run npm run contracts; do not edit.\n");
    let config = Config::default();
    macro_rules! emit {($($t:ty),*)=>{$({types.push_str("export ");types.push_str(&<$t>::decl(&config));types.push('\n');let schema=schema_for!($t);std::fs::write(root.join("contracts").join(format!("{}.v{}.schema.json",stringify!($t),match stringify!($t) {"Workspace"=>11,"TutorProposal"=>5,"Assessment"|"LearningView"|"PrivacySettings"=>3,"Usage"|"Review"|"AttemptConditions"|"TaskSpec"|"WrapUp"|"GradeProposal"|"RequestVersions"=>2,_=>1})),serde_json::to_string_pretty(&schema).unwrap()).unwrap();})*};}
    emit!(
        GoalRecord,
        PlanRevision,
        PlanCapability,
        PlanVisual,
        Clarification,
        GoalView,
        GoalSummary,
        GoalSource,
        CapabilityProgress,
        CheckResult,
        AnswerKey,
        Given,
        DomainVariable,
        TaskSupport,
        FinalAnswer,
        LearningState,
        TaskSpec,
        AttemptConditions,
        LearningSummary,
        LearningView,
        StepOption,
        VisualParameter,
        VisualPlan,
        WrapUp,
        TimelineEntry,
        ProviderStatus,
        RubricFinding,
        GradeProposal,
        Session,
        Activity,
        Attempt,
        Assistance,
        Assessment,
        Review,
        Source,
        SourceDocument,
        SourcePassage,
        SourceCitation,
        SourceView,
        PrivacySettings,
        ModelOption,
        DeletionPreview,
        CitationProposal,
        Request,
        RequestVersions,
        Usage,
        Limits,
        Budget,
        CredentialStatus,
        SceneParameters,
        SceneSpec,
        TutorProposal,
        Workspace,
        UnitObjective,
        UnitView,
        RecordingTicket,
        Transcript,
        SaveAttempt,
        Diagnostics
    );
    std::fs::write(root.join("src/generated/contracts.ts"), types).unwrap();
}
