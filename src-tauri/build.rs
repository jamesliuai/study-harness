fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or("unrecorded source".into());
    let dirty = git(&["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
    println!(
        "cargo:rustc-env=STUDY_HARNESS_BUILD_SOURCE={commit}{}",
        if dirty {
            " (working copy modified)"
        } else {
            " (clean)"
        }
    );
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "workspace",
            "plan_goal",
            "revise_plan",
            "plan_decision",
            "start_sample",
            "begin_capability",
            "advance",
            "record_startup",
            "start_goal",
            "learning_action",
            "correct_attempt",
            "dispute_grade",
            "resolve_dispute",
            "development_action",
            "development_cases",
            "new_session",
            "save_attempt",
            "visualizer_rpc",
            "source_rpc",
            "evaluation_rpc",
            "export_evaluation",
            "save_draft",
            "ask_tutor",
            "cancel_request",
            "set_credential",
            "remove_credential",
            "check_connection",
            "update_limits",
            "update_privacy",
            "deletion_preview",
            "delete_records",
            "export_data",
            "restore_data",
            "begin_recording",
            "end_recording",
            "transcribe",
            "discard_recording",
            "discard_transcript",
            "diagnostics",
        ]),
    ))
    .expect("failed to build Tauri application");
}
