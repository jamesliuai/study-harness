//! Functional integration tests: real provider parser, Rust policy, transactions and file-backed SQLite.
use crate::{
    clock::{Clock, TestClock},
    contracts::*,
    db::{Store, id},
    offline,
};
use chrono::{Duration, TimeZone, Utc};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn fixture() -> (tempfile::TempDir, Store, String, Arc<TestClock>) {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(TestClock(Mutex::new(
        Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap(),
    )));
    let mut d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock.clone()).unwrap();
    let sid = d.new_session("Offline integration fixture").unwrap();
    d.start_goal(&sid, 0, "Understand oscillation and convergence")
        .unwrap();
    (dir, d, sid, clock)
}
fn action(d: &mut Store, sid: &str, name: &str) {
    let rev = d.session(sid).unwrap().revision;
    d.learning_action(sid, rev, name).unwrap();
}
fn submit(d: &mut Store, sid: &str, text: &str) -> Attempt {
    d.save_attempt(&SaveAttempt {
        session_id: sid.into(),
        expected_revision: d.session(sid).unwrap().revision,
        submission_id: id(),
        response: text.into(),
        transcript_id: None,
    })
    .unwrap()
}
async fn assess(d: &mut Store, sid: &str, at: &Attempt, scenario: &str) {
    let r = d.reserve(sid, Some(&at.id), "tutor", 0).unwrap();
    let input = d.tutor_input(r.clone()).unwrap();
    match offline::respond(input, scenario, CancellationToken::new()).await {
        Ok(out) => match out.proposal {
            Ok(p) => d.settle(&r, "completed", out.usage, Some(&p)).unwrap(),
            Err(_) => d.settle(&r, "rejected", out.usage, None).unwrap(),
        },
        Err(_) => d
            .settle(&r, "cancelled", Store::estimated(&r), None)
            .unwrap(),
    }
}
fn fresh(d: &mut Store, sid: &str) -> Attempt {
    action(d, sid, "fresh_check");
    submit(
        d,
        sid,
        "The multiplier is 1−5η. Its magnitude must be below one: 0<η<2/5. At η=2/5 it is −1, so it alternates without shrinking. At η=0.3 it is −0.5 and converges.",
    )
}
fn view(d: &Store, sid: &str) -> LearningView {
    d.learning_view(sid).unwrap().unwrap()
}

#[tokio::test]
async fn entire_learning_return_loop_survives_restart() {
    let (dir, mut d, sid, clock) = fixture();
    let prediction = submit(
        &mut d,
        &sid,
        "Larger positive steps always go downhill faster.",
    );
    assess(&mut d, &sid, &prediction, "misconception").await;
    for expected in [
        "instruction",
        "worked_example",
        "completion",
        "guided",
        "exploration",
    ] {
        action(&mut d, &sid, "next");
        assert_eq!(view(&d, &sid).state.stage, expected);
        if ["completion", "guided"].contains(&expected) {
            let a = submit(
                &mut d,
                &sid,
                "q is negative and its magnitude shrinks; therefore it alternates toward zero.",
            );
            assess(&mut d, &sid, &a, "partial").await;
        }
    }
    let a = submit(
        &mut d,
        &sid,
        "At .25 it converges, .75 alternates toward zero, 1.25 alternates outward.",
    );
    let sc = d.scene(&a.activity_id).unwrap();
    d.save_scene(&sid, sc.parameters, true).unwrap();
    action(&mut d, &sid, "next");
    assert_eq!(view(&d, &sid).state.stage, "explanation");
    submit(
        &mut d,
        &sid,
        "The sign causes crossing; magnitude below one causes shrinking.",
    );
    action(&mut d, &sid, "next");
    assert_eq!(view(&d, &sid).state.stage, "independent");
    let a = submit(
        &mut d,
        &sid,
        "q=1−5η; 0<η<0.4; at .4 q=−1 and magnitude is constant. At .3 q=−.5.",
    );
    assess(&mut d, &sid, &a, "correct").await;
    action(&mut d, &sid, "next");
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 1);
    let r = view(&d, &sid)
        .reviews
        .into_iter()
        .find(|r| r.status == "scheduled")
        .unwrap();
    assert_eq!(r.interval_days, 1);
    drop(d);
    let mut d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock.clone()).unwrap();
    assert_eq!(view(&d, &sid).state.stage, "summary");
    assert_eq!(d.attempts(&sid).unwrap().len(), 6);
    *clock.0.lock().unwrap() += Duration::days(1);
    action(&mut d, &sid, "review");
    assert!(d.activity(&sid).unwrap().prompt.contains("4/5"));
    let a = submit(
        &mut d,
        &sid,
        "u_n=(−4/5)^n u_0 tends to zero, alternating. Factor −1 alternates with constant magnitude.",
    );
    assess(&mut d, &sid, &a, "alternative").await;
    action(&mut d, &sid, "next");
    let v = view(&d, &sid);
    assert_eq!(v.summary.as_ref().unwrap().independent_count, 2);
    assert_eq!(v.summary.unwrap().delayed_count, 0);
    assert_eq!(
        v.reviews
            .iter()
            .find(|r| r.status == "scheduled")
            .unwrap()
            .interval_days,
        3
    );
    assert!(
        d.usage(&sid)
            .unwrap()
            .iter()
            .all(|u| u.cost_microusd == 0 && u.provider.contains("SIMULATED"))
    );
}

#[tokio::test]
async fn all_eight_objectives_freeze_distinct_tasks_and_keep_evidence_scoped() {
    let (dir, mut d, _sid, clock) = fixture();
    for o in crate::curriculum::unit()["objectives"].as_array().unwrap() {
        let oid = o["id"].as_str().unwrap();
        let sid = d.new_session("Checked unit scenario").unwrap();
        d.start_goal_for(&sid, 0, o["outcome"].as_str().unwrap(), oid)
            .unwrap();
        assert_eq!(d.activity(&sid).unwrap().objective_id, oid);
        let diagnostic = d.activity(&sid).unwrap();
        let diagnostic_spec = d.task_spec(&diagnostic.id).unwrap().unwrap();
        assert!(!diagnostic_spec.reference_solution.is_empty());
        action(&mut d, &sid, "teach");
        action(&mut d, &sid, "next");
        action(&mut d, &sid, "next");
        assert_eq!(view(&d, &sid).state.stage, "completion");
        assert!(!d.assistance(&sid).unwrap().is_empty());
        action(&mut d, &sid, "fresh_check");
        let fresh = d.activity(&sid).unwrap();
        assert_ne!(fresh.prompt, diagnostic.prompt);
        let a = submit(
            &mut d,
            &sid,
            "Explicit external-boundary fixture response; the scenario determines its grade.",
        );
        assert!(a.assistance_ids.is_empty());
        assess(&mut d, &sid, &a, "correct").await;
        action(&mut d, &sid, "next");
        let summary = view(&d, &sid).summary.unwrap();
        assert_eq!(summary.independent_count, 1);
        assert!(summary.simulated);
        assert_eq!(summary.delayed_count, 0);
        let r = view(&d, &sid)
            .reviews
            .into_iter()
            .find(|r| r.status == "scheduled")
            .unwrap();
        assert_eq!(r.objective_id, oid);
        let u = d.unit_view(&sid).unwrap();
        assert_eq!(u.objectives.len(), 8);
        assert!(u.objectives.iter().all(|o| o.independent_count == 0));
    }
    drop(d);
    let d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock).unwrap();
    for o in crate::curriculum::unit()["objectives"].as_array().unwrap() {
        let count: i64 = d
            .conn
            .query_row(
                "SELECT COUNT(*) FROM session_objectives WHERE objective_id=?",
                [o["id"].as_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count > 0);
    }
}

#[tokio::test]
async fn successive_intervals_are_one_three_seven_fourteen_from_each_success() {
    let (_dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    let origin = clock.now();
    for (days, cumulative) in [(1, 1), (3, 4), (7, 11), (14, 25)] {
        let r = view(&d, &sid)
            .reviews
            .into_iter()
            .find(|r| r.status == "scheduled")
            .unwrap();
        assert_eq!(r.interval_days, days);
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(&r.due_at)
                .unwrap()
                .with_timezone(&Utc),
            origin + Duration::days(cumulative)
        );
        *clock.0.lock().unwrap() = origin + Duration::days(cumulative);
        action(&mut d, &sid, "review");
        let a = submit(
            &mut d,
            &sid,
            "Geometric magnitude below one tends to zero; the negative sign alternates.",
        );
        assess(&mut d, &sid, &a, "correct").await;
    }
    assert_eq!(
        view(&d, &sid).summary.unwrap().delayed_count,
        0,
        "clock simulation never proves real delayed learning"
    );
}

#[tokio::test]
async fn supported_and_overdue_reviews_do_not_imply_forgetting_or_extend_success() {
    let (_dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    *clock.0.lock().unwrap() += Duration::days(10);
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 1);
    action(&mut d, &sid, "review");
    action(&mut d, &sid, "hint");
    let a = submit(&mut d, &sid, "I can finish with this cue.");
    assess(&mut d, &sid, &a, "correct").await;
    let v = view(&d, &sid);
    assert_eq!(v.summary.unwrap().independent_count, 1);
    let r = v.reviews.iter().find(|r| r.status == "scheduled").unwrap();
    assert_eq!(r.interval_days, 1);
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&r.due_at)
            .unwrap()
            .with_timezone(&Utc),
        clock.now() + Duration::days(1)
    );
}

#[tokio::test]
async fn help_is_durable_and_submission_snapshots_never_change_after_feedback() {
    let (dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    assert!(a.assistance_ids.is_empty());
    assess(&mut d, &sid, &a, "correct").await;
    assert!(
        d.attempts(&sid)
            .unwrap()
            .iter()
            .find(|x| x.id == a.id)
            .unwrap()
            .assistance_ids
            .is_empty()
    );
    let b = submit(&mut d, &sid, "A second answer after feedback.");
    assert!(!b.assistance_ids.is_empty());
    action(&mut d, &sid, "worked_example");
    let help = d.assistance(&sid).unwrap();
    assert!(
        d.conn
            .execute("DELETE FROM assistance WHERE id=?", [&help[0].id])
            .is_err()
    );
    drop(d);
    let mut d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock).unwrap();
    assert_eq!(d.assistance(&sid).unwrap().len(), help.len());
    let c = submit(&mut d, &sid, "Now I can repeat the solution.");
    assess(&mut d, &sid, &c, "correct").await;
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 1);
}

#[tokio::test]
async fn corrections_retain_speech_and_invalidate_all_dependent_views() {
    let (_dir, mut d, sid, clock) = fixture();
    action(&mut d, &sid, "fresh_check");
    let t = Transcript {
        id: id(),
        recognized_text: offline::transcript().into(),
        audio_seconds: 0.,
        format: "simulated".into(),
    };
    d.save_transcript(&sid, &t).unwrap();
    let input = SaveAttempt {
        session_id: sid.clone(),
        expected_revision: d.session(&sid).unwrap().revision,
        submission_id: id(),
        response: "Negative half; the signs alternate and the magnitude shrinks.".into(),
        transcript_id: Some(t.id),
    };
    let a = d.save_attempt(&input).unwrap();
    assess(&mut d, &sid, &a, "correct").await;
    *clock.0.lock().unwrap() += Duration::seconds(1);
    let key = id();
    let rev = d.session(&sid).unwrap().revision;
    let b = d
        .correct_attempt(
            &sid,
            rev,
            &a.id,
            &key,
            "My confirmed wording should say negative one half.",
        )
        .unwrap();
    assert_eq!(
        d.correct_attempt(&sid, rev, &a.id, &key, &b.response)
            .unwrap()
            .id,
        b.id
    );
    assert_eq!(b.recognized_transcript, a.recognized_transcript);
    assert_eq!(b.revision_of, Some(a.id.clone()));
    assert!(!b.assistance_ids.is_empty());
    let v = view(&d, &sid);
    assert!(v.summary.unwrap().stale);
    assert!(v.assessments.iter().all(|x| x.stale));
    assert!(!v.reviews.iter().any(|r| r.status == "scheduled"));
    assess(&mut d, &sid, &b, "correct").await;
    assert_eq!(
        view(&d, &sid).summary.unwrap().independent_count,
        0,
        "post-solution correction cannot become independent success"
    );
    assert_eq!(
        d.attempts(&sid)
            .unwrap()
            .iter()
            .find(|x| x.id == a.id)
            .unwrap()
            .response,
        input.response
    );
}

#[tokio::test]
async fn dispute_requires_explicit_resolution_after_reassessment() {
    let (_dir, mut d, sid, _clock) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    let g = view(&d, &sid).assessments.pop().unwrap();
    d.dispute_grade(
        &sid,
        d.session(&sid).unwrap().revision,
        &g.id,
        "The assessment misread my alternative proof.",
    )
    .unwrap();
    assert!(view(&d, &sid).summary.unwrap().stale);
    action(&mut d, &sid, "recompute");
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 0);
    assert!(
        d.resolve_dispute(&sid, d.session(&sid).unwrap().revision, &g.id)
            .is_err()
    );
    // Reassessment may share the same instant on an injected clock; identity establishes order.
    assess(&mut d, &sid, &a, "alternative").await;
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 0);
    let g = view(&d, &sid).assessments.pop().unwrap();
    d.resolve_dispute(&sid, d.session(&sid).unwrap().revision, &g.id)
        .unwrap();
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 1);
    let unresolved: i64 = d
        .conn
        .query_row(
            "SELECT COUNT(*) FROM disputes WHERE resolved_by IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unresolved, 0);
}

#[tokio::test]
async fn every_scenario_passes_through_parser_and_preserves_usage() {
    for scenario in offline::SCENARIOS {
        if *scenario == "slow" {
            continue;
        }
        let (_dir, mut d, sid, _) = fixture();
        let a = fresh(&mut d, &sid);
        assess(&mut d, &sid, &a, scenario).await;
        assert_eq!(d.requests(&sid).unwrap().len(), 1, "{scenario}");
        assert_eq!(d.usage(&sid).unwrap().len(), 1, "{scenario}");
        let rejected = [
            "malformed",
            "invalid_reference",
            "invalid_action",
            "stale",
            "cancelled",
        ]
        .contains(scenario);
        if rejected {
            assert!(d.assistance(&sid).unwrap().is_empty(), "{scenario}");
            assert!(view(&d, &sid).assessments.is_empty(), "{scenario}");
        }
        if ["ambiguous", "dispute"].contains(scenario) {
            assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 0);
        }
    }
}

#[tokio::test]
async fn actual_cancellation_and_changed_revision_discard_response_but_keep_request() {
    let (_dir, mut d, sid, _) = fixture();
    let a = fresh(&mut d, &sid);
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let i = d.tutor_input(r.clone()).unwrap();
    let token = CancellationToken::new();
    token.cancel();
    assert!(offline::respond(i, "slow", token).await.is_err());
    d.settle(&r, "cancelled", Store::estimated(&r), None)
        .unwrap();
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let i = d.tutor_input(r.clone()).unwrap();
    let out = offline::respond(i, "correct", CancellationToken::new())
        .await
        .unwrap();
    action(&mut d, &sid, "stop");
    d.settle(&r, "completed", out.usage, Some(&out.proposal.unwrap()))
        .unwrap();
    assert_eq!(d.requests(&sid).unwrap()[0].status, "stale");
    assert!(view(&d, &sid).assessments.is_empty());
    assert_eq!(d.usage(&sid).unwrap().len(), 2);
}

#[test]
fn duplicate_submission_and_interrupted_request_recover_on_disk() {
    let (dir, mut d, sid, clock) = fixture();
    let i = SaveAttempt {
        session_id: sid.clone(),
        expected_revision: d.session(&sid).unwrap().revision,
        submission_id: id(),
        response: "It alternates and shrinks.".into(),
        transcript_id: None,
    };
    let a = d.save_attempt(&i).unwrap();
    d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    drop(d);
    let mut d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock).unwrap();
    assert_eq!(d.save_attempt(&i).unwrap().id, a.id);
    assert_eq!(d.attempts(&sid).unwrap().len(), 1);
    assert_eq!(d.requests(&sid).unwrap()[0].status, "interrupted");
    assert_eq!(d.usage(&sid).unwrap()[0].cost_microusd, 0);
    let mut altered = i;
    altered.response = "Changed".into();
    assert!(d.save_attempt(&altered).is_err());
    assert!(Store::open(&dir.path().join("offline.sqlite3")).is_err());
}

#[test]
fn frozen_tasks_diagnosis_limit_quiet_stop_and_visual_policy_are_core_enforced() {
    let (_dir, mut d, sid, _) = fixture();
    let a = d.activity(&sid).unwrap();
    assert!(d.task_spec(&a.id).unwrap().is_some());
    assert!(
        d.conn
            .execute(
                "UPDATE activities SET reference_solution='changed' WHERE id=?",
                [&a.id]
            )
            .is_err()
    );
    assert!(
        d.conn
            .execute(
                "UPDATE task_specs SET data='{}' WHERE activity_id=?",
                [&a.id]
            )
            .is_err()
    );
    let sc = d.scene(&a.id).unwrap();
    assert!(d.save_scene(&sid, sc.parameters, true).is_err());
    action(&mut d, &sid, "quiet");
    assert!(d.allowed_actions(&sid).unwrap().is_empty());
    submit(&mut d, &sid, "A quiet prediction is still valid.");
    action(&mut d, &sid, "resume");
    submit(&mut d, &sid, "Second probe");
    submit(&mut d, &sid, "Third probe");
    assert!(d.validate_submission(&sid).is_err());
    action(&mut d, &sid, "teach");
    action(&mut d, &sid, "stop");
    assert!(d.validate_submission(&sid).is_err());
    action(&mut d, &sid, "resume");
    action(&mut d, &sid, "fresh_check");
    let sc = d.scene(&d.activity(&sid).unwrap().id).unwrap();
    assert!(d.save_scene(&sid, sc.parameters, true).is_err());
}

#[tokio::test]
async fn semantic_validation_is_repeated_at_commit_and_cannot_be_bypassed() {
    let (_dir, mut d, sid, _) = fixture();
    let a = fresh(&mut d, &sid);
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let i = d.tutor_input(r.clone()).unwrap();
    let out = offline::respond(i, "correct", CancellationToken::new())
        .await
        .unwrap();
    let mut p = out.proposal.unwrap();
    p.grade.as_mut().unwrap().findings[0].excerpt = "invented evidence".into();
    d.settle(&r, "completed", out.usage, Some(&p)).unwrap();
    assert_eq!(d.requests(&sid).unwrap()[0].status, "rejected");
    assert!(view(&d, &sid).assessments.is_empty());
    assert_eq!(d.usage(&sid).unwrap().len(), 1);
    assert!(
        d.settle(&r, "completed", Store::estimated(&r), Some(&p))
            .is_err()
    );
}

#[tokio::test]
async fn early_practice_does_not_advance_the_review_interval_and_outside_help_is_retained() {
    let (_dir, mut d, sid, _) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    let before = view(&d, &sid)
        .reviews
        .into_iter()
        .find(|r| r.status == "scheduled")
        .unwrap();
    assert!(
        d.learning_action(&sid, d.session(&sid).unwrap().revision, "review")
            .is_err()
    );
    let b = fresh(&mut d, &sid);
    assess(&mut d, &sid, &b, "correct").await;
    let after = view(&d, &sid)
        .reviews
        .into_iter()
        .find(|r| r.status == "scheduled")
        .unwrap();
    assert_eq!(before.due_at, after.due_at);
    assert_eq!(after.interval_days, 1);
    action(&mut d, &sid, "fresh_check");
    action(&mut d, &sid, "outside_help");
    let a = submit(&mut d, &sid, "With a notebook I derived the result.");
    assess(&mut d, &sid, &a, "correct").await;
    assert!(
        view(&d, &sid)
            .conditions
            .iter()
            .find(|c| c.attempt_id == a.id)
            .unwrap()
            .outside_help
    );
}

#[test]
fn checked_quadratic_cases_use_independent_closed_form_and_limits() {
    for (eta, q, expected) in [
        (0.25_f64, 0.5_f64, 0.5_f64),
        (0.75, -0.5, -0.5),
        (1.25, -1.5, -13.5),
        (0.5, 0., 0.),
        (1., -1., -4.),
        (0., 1., 4.),
    ] {
        let mut x = 4.;
        for _ in 0..3 {
            x -= eta * (2. * x);
        }
        assert!((x - expected).abs() < 1e-12);
        assert!((x - 4. * q.powi(3)).abs() < 1e-12);
    }
    for a in [2.0_f64, 4., 5., 7., 9.] {
        let boundary = 2. / a;
        assert_eq!(1. - boundary * a, -1.);
        assert!((1. - 1.5 / a * a).abs() < 1.);
    }
}

#[test]
fn offline_mode_rejects_live_data_before_migrating_or_changing_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy-live.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../migrations/001_foundation.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../migrations/002_transcript_recovery.sql"))
        .unwrap();
    conn.execute("INSERT INTO sessions VALUES ('real',0,'{}')", [])
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open_with(&path, true, Arc::new(crate::clock::SystemClock)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn interrupted_assessment_transaction_publishes_no_partial_help_and_recovers_usage() {
    let (dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let input = d.tutor_input(r.clone()).unwrap();
    let out = offline::respond(input, "correct", CancellationToken::new())
        .await
        .unwrap();
    d.conn.execute_batch("CREATE TRIGGER injected_failure BEFORE INSERT ON assessments BEGIN SELECT RAISE(ABORT,'simulated interrupted save'); END;").unwrap();
    assert!(
        d.settle(&r, "completed", out.usage, Some(&out.proposal.unwrap()))
            .is_err()
    );
    assert!(d.assistance(&sid).unwrap().is_empty());
    assert!(view(&d, &sid).assessments.is_empty());
    assert_eq!(d.requests(&sid).unwrap()[0].status, "pending");
    drop(d);
    let d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock).unwrap();
    assert_eq!(d.requests(&sid).unwrap()[0].status, "interrupted");
    assert_eq!(d.usage(&sid).unwrap().len(), 1);
}

#[tokio::test]
async fn early_success_reanchors_due_date_without_skipping_a_spacing_step() {
    let (_dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    *clock.0.lock().unwrap() += Duration::hours(12);
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    let r = view(&d, &sid)
        .reviews
        .into_iter()
        .find(|r| r.status == "scheduled")
        .unwrap();
    assert_eq!(r.interval_days, 1);
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&r.due_at)
            .unwrap()
            .with_timezone(&Utc),
        clock.now() + Duration::days(1)
    );
}

#[test]
fn fresh_tasks_do_not_repeat_across_sessions_and_keep_the_same_family() {
    let (_dir, mut d, sid, _) = fixture();
    action(&mut d, &sid, "fresh_check");
    let first = d.activity(&sid).unwrap();
    let other = d.new_session("Another lesson").unwrap();
    d.start_goal(&other, 0, "Check the same objective again")
        .unwrap();
    action(&mut d, &other, "fresh_check");
    let second = d.activity(&other).unwrap();
    assert_ne!(first.prompt, second.prompt);
    assert_eq!(first.task_family, second.task_family);
    assert!(first.prompt.contains("(5/2)"));
    assert!(second.prompt.contains("(7/2)"));
}

#[tokio::test]
async fn exploratory_planning_and_explanations_use_the_real_pipeline_without_grades_or_reviews() {
    let (_dir, mut d, _sid, _clock) = fixture();
    let sid = d.new_session("Exploratory fixture").unwrap();
    d.start_goal_for(
        &sid,
        0,
        "Plan study of integration by parts",
        "exploratory-unchecked",
    )
    .unwrap();
    assert!(d.unit_view(&sid).unwrap().plan.is_empty());
    let a = d
        .save_attempt(&SaveAttempt {
            session_id: sid.clone(),
            expected_revision: d.session(&sid).unwrap().revision,
            submission_id: crate::db::id(),
            response:
                "Please propose a short plan and explain when integration by parts is useful."
                    .into(),
            transcript_id: None,
        })
        .unwrap();
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let input = d.tutor_input(r.clone()).unwrap();
    assert!(!input.allowed_actions.contains(&"assess".into()));
    let result = offline::respond(input, "correct", CancellationToken::new())
        .await
        .unwrap();
    d.settle(
        &r,
        "completed",
        result.usage,
        Some(&result.proposal.unwrap()),
    )
    .unwrap();
    let v = d.learning_view(&sid).unwrap().unwrap();
    assert!(v.assessments.is_empty());
    assert!(v.reviews.is_empty());
    assert!(v.summary.is_none());
    assert_eq!(d.assistance(&sid).unwrap().len(), 1);
    assert!(
        d.learning_action(&sid, d.session(&sid).unwrap().revision, "fresh_check")
            .is_err()
    );
    action(&mut d, &sid, "quiet");
    assert!(d.allowed_actions(&sid).unwrap().is_empty());
    action(&mut d, &sid, "resume");
    action(&mut d, &sid, "stop");
    assert!(d.validate_submission(&sid).is_err());
}

#[tokio::test]
async fn forty_versioned_diagnostics_exercise_the_actual_store_parser_and_policy() {
    let cases = offline::diagnostic_cases();
    assert_eq!(cases["cases"].as_array().unwrap().len(), 40);
    assert!(cases["human_subject_review"].is_null());
    let mut ids = std::collections::BTreeSet::new();
    for case in cases["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = Store::open_with(
            &dir.path().join("diagnostic.sqlite3"),
            true,
            Arc::new(crate::clock::SystemClock),
        )
        .unwrap();
        let cid = case["id"].as_str().unwrap();
        assert!(ids.insert(cid));
        let sid = d.load_diagnostic(cid).unwrap();
        let a = d.attempts(&sid).unwrap().last().unwrap().clone();
        let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
        let input = d.tutor_input(r.clone()).unwrap();
        let payload = crate::providers::tutor_payload(&input);
        assert_eq!(payload["store"], false);
        let out = offline::respond(
            input,
            case["scenario"].as_str().unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        match out.proposal {
            Ok(p) => d.settle(&r, "completed", out.usage, Some(&p)).unwrap(),
            Err(_) => d.settle(&r, "rejected", out.usage, None).unwrap(),
        };
        assert_eq!(
            d.requests(&sid).unwrap()[0].status,
            case["expected_status"],
            "{cid}"
        );
        assert_eq!(d.usage(&sid).unwrap().len(), 1);
        let view = d.learning_view(&sid).unwrap().unwrap();
        let verdict = view
            .assessments
            .last()
            .map(|a| serde_json::json!(a.verdict))
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(verdict, case["expected_verdict"], "{cid}");
        assert_eq!(
            view.summary
                .as_ref()
                .map(|s| s.independent_count)
                .unwrap_or(0),
            case["expected_independent_count"].as_u64().unwrap() as u32,
            "{cid}"
        );
        assert!(
            view.assessments
                .iter()
                .all(|a| a.grader_version.contains("simulated")),
            "{cid}"
        );
        if case["prior_help"] == true {
            assert!(!a.assistance_ids.is_empty());
            assert_eq!(view.summary.unwrap().supported_count, 1);
        }
        drop(d);
        let restored = Store::open_with(
            &dir.path().join("diagnostic.sqlite3"),
            true,
            Arc::new(crate::clock::SystemClock),
        )
        .unwrap();
        assert_eq!(
            restored.attempts(&sid).unwrap()[0].response,
            case["response"]
        );
    }
}

#[tokio::test]
async fn delayed_evidence_accounts_for_the_same_objective_in_other_sessions() {
    let (_dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    assess(&mut d, &sid, &a, "correct").await;
    *clock.0.lock().unwrap() += Duration::days(7);
    let other = d
        .new_session("Another session for the same objective")
        .unwrap();
    d.start_goal(&other, 0, "Revisit convergence").unwrap();
    action(&mut d, &other, "worked_example");
    let last_exposure = d.time();
    *clock.0.lock().unwrap() += Duration::hours(1);
    let review = fresh(&mut d, &sid);
    let v = d.learning_view(&sid).unwrap().unwrap();
    assert_eq!(
        v.conditions
            .iter()
            .find(|c| c.attempt_id == review.id)
            .unwrap()
            .last_relevant_exposure_at
            .as_deref(),
        Some(last_exposure.as_str())
    );
    assess(&mut d, &sid, &review, "correct").await;
    assert_eq!(
        d.learning_view(&sid)
            .unwrap()
            .unwrap()
            .summary
            .unwrap()
            .delayed_count,
        0
    );
}
#[test]
fn missing_visual_checkpoint_does_not_silently_create_a_replacement_scene() {
    let (_dir, mut d, sid, _) = fixture();
    let opened = d.visual_open(&sid, "quadratic-gradient-descent").unwrap();
    let aid = opened["request"]["activityId"].as_str().unwrap();
    d.conn.execute("UPDATE visual_activities SET state=json_set(state,'$.checkpointDigest','sha256:missing') WHERE id=?",[aid]).unwrap();
    let error = d
        .visual_open(&sid, "quadratic-gradient-descent")
        .unwrap_err();
    assert_eq!(error.reason, "MISSING_OR_DAMAGED_ARTIFACT");
}

#[tokio::test]
async fn each_request_freezes_versions_and_retains_a_reported_identifier_without_certifying_it() {
    let (dir, mut d, sid, clock) = fixture();
    let a = fresh(&mut d, &sid);
    let input = d.prepare_tutor_request(&sid, &a.id).unwrap();
    let request = input.request.clone();
    let version = d.request_versions(&sid).unwrap().pop().unwrap();
    assert_eq!(version.request_id, request.id);
    assert_eq!(
        version.prompt_sha256,
        crate::sources::sha(crate::providers::PROMPT.as_bytes())
    );
    assert!(!version.build_source.is_empty());
    assert_eq!(
        version.content_version,
        d.task_spec(&a.activity_id)
            .unwrap()
            .unwrap()
            .content_version
    );
    assert!(version.provenance.contains("Synthetic"));
    let mut out = offline::respond(input, "correct", CancellationToken::new())
        .await
        .unwrap();
    out.usage.reported_model = Some("SIMULATED-reported-identifier-fixture".into());
    d.settle(
        &request,
        "completed",
        out.usage,
        Some(&out.proposal.unwrap()),
    )
    .unwrap();
    drop(d);
    let d = Store::open_with(&dir.path().join("offline.sqlite3"), true, clock).unwrap();
    let reopened = d.request_versions(&sid).unwrap().pop().unwrap();
    assert_eq!(reopened.prompt_sha256, version.prompt_sha256);
    assert_eq!(
        reopened.reported_model.as_deref(),
        Some("SIMULATED-reported-identifier-fixture")
    );
    assert_eq!(d.usage(&sid).unwrap()[0].schema_version, 2);
}

fn ask(d: &mut Store, sid: &str, text: &str) -> Attempt {
    d.save_submission(
        &SaveAttempt {
            session_id: sid.into(),
            expected_revision: d.session(sid).unwrap().revision,
            submission_id: id(),
            response: text.into(),
            transcript_id: None,
        },
        None,
        "question",
    )
    .unwrap()
}

#[tokio::test]
async fn questions_are_discussion_never_graded_counted_or_advancing() {
    let (_dir, mut d, sid, _) = fixture();
    let q = ask(&mut d, &sid, "What does the step size multiply?");
    assert_eq!(
        view(&d, &sid).state.diagnosis_count,
        0,
        "a question is not a diagnostic attempt"
    );
    assert!(
        !d.allowed_for(&sid, &q.id)
            .unwrap()
            .contains(&"assess".to_string())
    );
    let rev = d.session(&sid).unwrap().revision;
    assert!(
        d.learning_action(&sid, rev, "next").is_err(),
        "a question cannot satisfy the answer requirement"
    );
    assess(&mut d, &sid, &q, "correct").await;
    assert!(
        view(&d, &sid).assessments.is_empty(),
        "the fixture answers a question without a grade"
    );
    assert_eq!(d.assistance(&sid).unwrap().len(), 1);
    // Questions stay open while reading, unlike answers after a summary.
    action(&mut d, &sid, "teach");
    ask(&mut d, &sid, "Why does a negative multiplier cross zero?");
    action(&mut d, &sid, "next");
    assert_eq!(view(&d, &sid).state.stage, "worked_example");
    ask(&mut d, &sid, "Why is the first step the consequential one?");
    let v = view(&d, &sid);
    assert!(
        v.conditions
            .iter()
            .filter(|c| c.intent == "question")
            .count()
            == 3
    );
    d.recompute_for_test(&sid);
    assert_eq!(view(&d, &sid).summary.unwrap().independent_count, 0);
}

#[test]
fn worked_example_stage_shows_the_authored_related_example_not_the_task_solution() {
    let (_dir, mut d, sid, _) = fixture();
    action(&mut d, &sid, "teach");
    action(&mut d, &sid, "next");
    let aid = d.activity(&sid).unwrap().id;
    let help: Vec<Assistance> = d
        .assistance(&sid)
        .unwrap()
        .into_iter()
        .filter(|h| h.activity_id == aid)
        .collect();
    let authored = crate::curriculum::objective("quadratic-convergence").unwrap();
    assert_eq!(help.len(), 1);
    assert_eq!(help[0].depth, "example");
    assert_eq!(
        help[0].content,
        authored["worked_example"].as_str().unwrap()
    );
    assert_ne!(help[0].content, authored["explanation"].as_str().unwrap());
    action(&mut d, &sid, "worked_example");
    let solution = d.assistance(&sid).unwrap().pop().unwrap();
    assert!(solution.content.starts_with("Solution to this task:"));
    assert!(
        view(&d, &sid).visual.is_none(),
        "no visual during the worked example"
    );
}

#[tokio::test]
async fn checks_withhold_earlier_teaching_from_timeline_payload_and_tutor() {
    let (_dir, mut d, sid, _) = fixture();
    let p = submit(&mut d, &sid, "It might alternate.");
    assess(&mut d, &sid, &p, "partial").await;
    action(&mut d, &sid, "next");
    assert!(d.timeline(&sid).unwrap().iter().any(|e| e.kind == "help"));
    let a = fresh(&mut d, &sid);
    let line = d.timeline(&sid).unwrap();
    assert_eq!(line[0].kind, "hidden");
    assert!(line.iter().skip(1).all(|e| e.activity_id == a.activity_id));
    let w = d
        .workspace_view(
            Some(sid.clone()),
            CredentialStatus {
                configured: false,
                storage: String::new(),
                masked: String::new(),
                message: String::new(),
            },
        )
        .unwrap();
    assert!(w.assistance.iter().all(|h| h.activity_id == a.activity_id));
    assert!(w.attempts.iter().all(|x| x.activity_id == a.activity_id));
    let r = d.reserve(&sid, Some(&a.id), "tutor", 0).unwrap();
    let input = d.tutor_input(r).unwrap();
    assert!(
        input.conversation.is_empty(),
        "no earlier instruction reaches an independent check"
    );
    assert!(input.evidence.is_empty());
    assert!(input.visual_summary.is_none());
}

#[tokio::test]
async fn step_policy_teaches_after_unsure_and_tutor_skip_needs_full_evidence() {
    let (_dir, mut d, sid, _) = fixture();
    submit(&mut d, &sid, "I don't know");
    let (steps, note) = d.steps(&sid).unwrap();
    assert!(
        steps.iter().any(|s| s.id == "teach" && s.recommended),
        "{steps:?}"
    );
    assert!(note.unwrap().contains("not sure"));
    let a = submit(
        &mut d,
        &sid,
        "q=1−0.75·2=−0.5, so it alternates and halves each step: 4, −2, 1. It converges to 0.",
    );
    assess(&mut d, &sid, &a, "correct").await;
    let (steps, note) = d.steps(&sid).unwrap();
    assert!(
        steps.iter().any(|s| s.id == "fresh_check" && s.recommended),
        "{steps:?}"
    );
    assert!(note.unwrap().starts_with("Tutor suggestion"));
    let b = submit(&mut d, &sid, "Larger steps always converge faster.");
    assess(&mut d, &sid, &b, "misconception").await;
    let (steps, _) = d.steps(&sid).unwrap();
    assert!(
        steps.iter().any(|s| s.id == "teach" && s.recommended),
        "{steps:?}"
    );
    assert!(!steps.iter().any(|s| s.id == "fresh_check" && s.recommended));
}

/// A goal outside the authored unit drives the same engine: generated plan → frozen, checked tasks → assessment
/// reconciled with the deterministic answer check → teaching → fresh check → summary → review after a restart.
#[tokio::test]
async fn generated_goal_runs_the_full_learning_and_return_loop() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(TestClock(Mutex::new(
        Utc.with_ymd_and_hms(2026, 10, 29, 9, 0, 0).unwrap(),
    )));
    let path = dir.path().join("offline.sqlite3");
    let mut d = Store::open_with(&path, true, clock.clone()).unwrap();
    let sid = d.new_session("New session").unwrap();
    offline::plan_offline(
        &mut d,
        &sid,
        0,
        "Average speed in kinematics problems",
        &[],
        "",
        None,
        "correct",
    )
    .await
    .unwrap();
    let goal = d.goal_view(&sid).unwrap().unwrap();
    assert_eq!(goal.capabilities.len(), 3);
    assert!(goal.provenance.contains("Not reviewed"));
    let numeric = goal.capabilities[1].objective_id.clone();
    // Nothing is assessed before the capability starts, and no quadratic default appears.
    assert!(d.learning_state(&sid).unwrap().is_none());
    let rev = d.session(&sid).unwrap().revision;
    offline::task_offline(&mut d, &sid, rev, "start", Some(&numeric), "", "correct")
        .await
        .unwrap();
    let v = view(&d, &sid);
    assert_eq!(v.state.stage, "diagnosis");
    assert!(v.generated && v.task_origin == "generated");
    assert!(
        v.task_checks
            .iter()
            .any(|c| c.id == "arithmetic" && c.outcome == "passed")
    );
    assert!(
        v.task_checks
            .iter()
            .any(|c| c.id == "units" && c.outcome == "passed")
    );
    assert_eq!(v.target_dimensions, vec!["setup", "execution"]);
    assert!(v.visual.is_none(), "an unrelated goal never gets a visual");
    // A wrong final value cannot become a correct verdict: the deterministic check holds it.
    let wrong = submit(
        &mut d,
        &sid,
        "Speed is distance over time, so it is 99 km/h",
    );
    assess(&mut d, &sid, &wrong, "correct").await;
    let held = view(&d, &sid).assessments.last().cloned().unwrap();
    assert_eq!(held.verdict, "unresolved");
    assert!(held.checks.iter().any(|c| c.outcome == "failed") && !held.note.is_empty());
    // The core asks for teaching; the generated explanation is recorded before it is shown.
    let rev = d.session(&sid).unwrap().revision;
    assert!(d.needs_generation(&sid, "teach").unwrap());
    offline::task_offline(&mut d, &sid, rev, "teach", None, "", "correct")
        .await
        .unwrap();
    assert_eq!(view(&d, &sid).state.stage, "instruction");
    assert!(
        d.assistance(&sid)
            .unwrap()
            .iter()
            .any(|h| h.depth == "explain")
    );
    // A rejected task is never shown and the step does not happen.
    let rev = d.session(&sid).unwrap().revision;
    assert!(
        offline::task_offline(&mut d, &sid, rev, "fresh_check", None, "", "flawed_task")
            .await
            .is_err()
    );
    assert_eq!(view(&d, &sid).state.stage, "instruction");
    let rev = d.session(&sid).unwrap().revision;
    offline::task_offline(&mut d, &sid, rev, "fresh_check", None, "", "correct")
        .await
        .unwrap();
    assert_eq!(view(&d, &sid).state.stage, "independent");
    let task = d.activity(&sid).unwrap().prompt;
    let numbers = crate::checks::numbers(&task);
    let speed = numbers.iter().find(|n| **n >= 6.0).unwrap()
        / numbers.iter().find(|n| **n < 6.0 && **n > 0.0).unwrap();
    let right = submit(
        &mut d,
        &sid,
        &format!(
            "Average speed is distance over time: {} km/h",
            crate::checks::short(speed)
        ),
    );
    assess(&mut d, &sid, &right, "correct").await;
    let graded = view(&d, &sid).assessments.last().cloned().unwrap();
    assert_eq!(graded.verdict, "correct", "{:?}", graded.checks);
    assert!(graded.checks.iter().any(|c| c.outcome == "passed"));
    action(&mut d, &sid, "next");
    let summary = view(&d, &sid);
    assert_eq!(summary.state.stage, "summary");
    assert!(summary.reviews.iter().any(|r| r.status == "scheduled"));
    assert!(!summary.wrap_up.explained.is_empty());
    drop(d);
    // Restart, travel to the review, and get a fresh generated review task with the same family.
    let mut d = Store::open_with(&path, true, clock.clone()).unwrap();
    *clock.0.lock().unwrap() += Duration::days(2);
    assert!(d.needs_generation(&sid, "review").unwrap());
    let rev = d.session(&sid).unwrap().revision;
    offline::task_offline(&mut d, &sid, rev, "review", None, "", "correct")
        .await
        .unwrap();
    assert_eq!(view(&d, &sid).state.stage, "review");
    assert_ne!(
        d.activity(&sid).unwrap().prompt,
        task,
        "review tasks are fresh"
    );
    // The plan shows progress, and the next capability opens in its own session.
    let goal = d.goal_view(&sid).unwrap().unwrap();
    assert!(goal.capabilities[1].label != "Not started");
    let next = d
        .session_for_capability(&sid, &goal.id, &goal.capabilities[2].objective_id)
        .unwrap();
    assert_ne!(next, sid);
    // Skipping is a new plan revision, never evidence.
    let rev = d.session(&sid).unwrap().revision;
    d.plan_decision(
        &sid,
        rev,
        &goal.id,
        &goal.capabilities[0].objective_id,
        "skip",
    )
    .unwrap();
    let after = d.goal_view(&sid).unwrap().unwrap();
    assert_eq!(after.revision, goal.revision + 1);
    assert_eq!(after.capabilities[0].label, "Skipped");
}

/// Revisions keep identities of unchanged capabilities and never silently drop recorded work.
#[tokio::test]
async fn plan_revisions_keep_capability_identity_and_recorded_work() {
    let (_dir, mut d, _, _) = fixture();
    let sid = d.new_session("New session").unwrap();
    offline::plan_offline(
        &mut d,
        &sid,
        0,
        "Eigenvectors of 2×2 matrices",
        &[],
        "",
        None,
        "correct",
    )
    .await
    .unwrap();
    let first = d.goal_view(&sid).unwrap().unwrap();
    assert!(
        first.capabilities[1].visual.is_some(),
        "a matching visual is offered when it serves the capability"
    );
    let rev = d.session(&sid).unwrap().revision;
    offline::plan_offline(
        &mut d,
        &sid,
        rev,
        "",
        &[],
        "Add a capability on diagonalization",
        Some(&first.id),
        "correct",
    )
    .await
    .unwrap();
    let second = d.goal_view(&sid).unwrap().unwrap();
    assert_eq!(second.revision, 2);
    for (a, b) in first.capabilities.iter().zip(&second.capabilities) {
        assert_eq!(a.objective_id, b.objective_id);
    }
    assert_eq!(second.capabilities.len(), 4);
    let rev = d.session(&sid).unwrap().revision;
    assert!(
        offline::plan_offline(
            &mut d,
            &sid,
            rev,
            "",
            &[],
            "x",
            Some(&first.id),
            "malformed"
        )
        .await
        .is_err()
    );
    assert_eq!(
        d.goal_view(&sid).unwrap().unwrap().revision,
        2,
        "a malformed reply changes nothing"
    );
    assert!(
        d.requests(&sid)
            .unwrap()
            .iter()
            .any(|r| r.status == "rejected")
    );
}

#[tokio::test]
async fn deleting_attached_source_removes_shared_plan_and_empty_capability_sessions() {
    use base64::Engine;
    let (_dir, mut d, unrelated, _) = fixture();
    let source = d
        .import_source(crate::sources::ImportSource {
            title: "Private kinematics notes".into(),
            kind: "text".into(),
            raw_base64: base64::engine::general_purpose::STANDARD
                .encode("Private example: average speed compares total distance with total time."),
            pages: vec![],
            extraction_version: crate::sources::TEXT_EXTRACTION.into(),
        })
        .unwrap();
    let sid = d.new_session("New session").unwrap();
    offline::plan_offline(
        &mut d,
        &sid,
        0,
        "Average speed",
        std::slice::from_ref(&source.id),
        "",
        None,
        "correct",
    )
    .await
    .unwrap();
    let goal = d.goal_view(&sid).unwrap().unwrap();
    assert!(goal.source_note.contains("Private example"));
    let revision = d.session(&sid).unwrap().revision;
    offline::task_offline(
        &mut d,
        &sid,
        revision,
        "start",
        Some(&goal.capabilities[0].objective_id),
        "",
        "correct",
    )
    .await
    .unwrap();
    // A capability start can allocate its session before generation fails or is cancelled.
    // It has no source-use request of its own, but still exposes the shared source-derived plan.
    let empty = d
        .session_for_capability(&sid, &goal.id, &goal.capabilities[1].objective_id)
        .unwrap();
    assert_ne!(sid, empty);
    assert!(d.requests(&empty).unwrap().is_empty());
    let preview = d.deletion_preview("source", &source.id).unwrap();
    assert_eq!(preview.session_ids.len(), 2);
    assert!(preview.session_ids.contains(&sid));
    assert!(preview.session_ids.contains(&empty));
    assert!(!preview.session_ids.contains(&unrelated));
    d.delete_owned("source", &source.id, "DELETE").unwrap();
    assert!(d.session(&sid).is_err());
    assert!(d.session(&empty).is_err());
    assert!(d.session(&unrelated).is_ok());
    for query in [
        "SELECT COUNT(*) FROM goals",
        "SELECT COUNT(*) FROM plans",
        "SELECT COUNT(*) FROM objectives WHERE json_extract(data,'$.origin')='generated'",
        "SELECT COUNT(*) FROM sources",
    ] {
        let count: i64 = d.conn.query_row(query, [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "{query}");
    }
}
