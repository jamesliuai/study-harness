//! Read an explicitly offline database through the same workspace view for UI layout fixtures.
use rusqlite::OptionalExtension;
use std::sync::{Arc, Mutex};
use study_harness::{
    clock::{SystemClock, TestClock},
    contracts::CredentialStatus,
    db::Store,
};
fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Pass the explicitly offline SQLite path")?;
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let mode: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key='data_mode'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if mode != "\"offline-development\"" {
        return Err(
            "Only explicitly offline data can be exported by this development tool.".into(),
        );
    }
    let mut d = Store {
        conn,
        path,
        offline: true,
        clock: Arc::new(SystemClock),
        provider: Default::default(),
    };
    let value: Option<String> = d
        .conn
        .query_row(
            "SELECT value FROM settings WHERE key='simulated_clock'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(value) = value {
        let value: String = serde_json::from_str(&value).map_err(|e| e.to_string())?;
        d.clock = Arc::new(TestClock(Mutex::new(
            chrono::DateTime::parse_from_rfc3339(&value)
                .map_err(|e| e.to_string())?
                .with_timezone(&chrono::Utc),
        )));
    }
    let view = d.workspace_view(
        None,
        CredentialStatus {
            configured: false,
            storage: "disabled in offline development".into(),
            masked: String::new(),
            message: "Layout snapshot from offline Rust workspace; no provider requests.".into(),
        },
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&view).map_err(|e| e.to_string())?
    );
    Ok(())
}
