//! Portable ownership: coordinated SQLite snapshots, authenticated protected backups, vetted fresh-profile restore,
//! and intentional scoped deletion. Archive bytes never supply SQL, file paths, permissions or executable modules.
use crate::{
    contracts::*,
    db::{Result, Store, id, json, parse, sql},
    sources::sha,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
pub const CURRENT_SCHEMA: i32 = 9;
pub const MAX_ARCHIVE: usize = 128 * 1024 * 1024;
pub const MAX_PROTECTED: usize = MAX_ARCHIVE * 4 / 3 + 4096;
const ITERATIONS: u32 = 600_000;
const AAD: &[u8] = b"study-harness.protected-backup/1";
const PACKAGES: &[(&str, &[u8])] = &[
    (
        "explorable-models-core-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-core-0.3.0-preview.1.tgz"
        ),
    ),
    (
        "explorable-models-activity-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-activity-0.3.0-preview.1.tgz"
        ),
    ),
    (
        "explorable-models-catalog-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-catalog-0.3.0-preview.1.tgz"
        ),
    ),
    (
        "explorable-models-player-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-player-0.3.0-preview.1.tgz"
        ),
    ),
    (
        "explorable-models-player-3d-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-player-3d-0.3.0-preview.1.tgz"
        ),
    ),
    (
        "explorable-models-agent-0.3.0-preview.1.tgz",
        include_bytes!(
            "../../vendor/visualizer/packages/explorable-models-agent-0.3.0-preview.1.tgz"
        ),
    ),
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveFile {
    path: String,
    sha256: String,
    bytes: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    format: String,
    schema_version: u32,
    database_schema: i32,
    mode: String,
    created_at: String,
    app_version: String,
    files: Vec<ArchiveFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Protected {
    format: String,
    kdf: String,
    iterations: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Cannot create the private archive staging file")?;
    file.write_all(bytes)
        .map_err(|_| "Cannot write the archive")?;
    file.sync_all()
        .map_err(|_| "Cannot synchronize the archive")?;
    Ok(())
}
fn staging(parent: &Path) -> Result<Temporary> {
    let path = parent.join(format!(".study-harness-staging-{}.sqlite3", id()));
    private_file(&path, &[])?;
    Ok(Temporary(path))
}
pub fn write_selected_export(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or("Selected export directory unavailable")?;
    let temp = Temporary(parent.join(format!(".study-harness-export-{}", id())));
    private_file(&temp.0, bytes)?;
    std::fs::rename(&temp.0, path).map_err(|_| "Cannot save the selected archive")?;
    Ok(())
}
pub fn cleanup_staging(parent: &Path) -> Result<()> {
    for entry in std::fs::read_dir(parent).map_err(|_| "Cannot inspect archive staging files")? {
        let p = entry
            .map_err(|_| "Cannot inspect archive staging file")?
            .path();
        if p.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            [
                ".study-harness-staging-",
                ".study-harness-export-",
                ".before-restore-",
            ]
            .iter()
            .any(|prefix| s.starts_with(prefix))
        }) {
            std::fs::remove_file(p).map_err(|_| "Cannot remove a private archive staging file")?;
        }
    }
    Ok(())
}
pub fn schema_sql(version: i32) -> Result<Vec<&'static str>> {
    let migrations = [
        include_str!("../migrations/001_foundation.sql"),
        include_str!("../migrations/002_transcript_recovery.sql"),
        include_str!("../migrations/003_learning.sql"),
        include_str!("../migrations/004_visualizer.sql"),
        include_str!("../migrations/005_unit.sql"),
        include_str!("../migrations/006_sources.sql"),
        include_str!("../migrations/007_evaluation.sql"),
        include_str!("../migrations/008_request_versions.sql"),
        include_str!("../migrations/009_goals.sql"),
    ];
    if !(1..=CURRENT_SCHEMA).contains(&version) {
        return Err(
            "Unsupported database schema; keep the archive for its original application version."
                .into(),
        );
    }
    Ok(migrations[..version as usize].to_vec())
}
fn expected_schema(version: i32) -> Result<BTreeMap<String, (String, String)>> {
    let c = Connection::open_in_memory().map_err(sql)?;
    for m in schema_sql(version)? {
        c.execute_batch(m).map_err(sql)?;
    }
    schema_objects(&c)
}
fn schema_objects(c: &Connection) -> Result<BTreeMap<String, (String, String)>> {
    let mut s=c.prepare("SELECT name,type,COALESCE(sql,'') FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql)?;
    s.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))
        .map_err(sql)?
        .collect::<std::result::Result<_, _>>()
        .map_err(sql)
}
fn file(path: &str, bytes: &[u8]) -> ArchiveFile {
    ArchiveFile {
        path: path.into(),
        sha256: sha(bytes),
        bytes: STANDARD.encode(bytes),
    }
}
pub fn protect(bytes: &[u8], password: &str) -> Result<Vec<u8>> {
    if password.chars().count() < 12 || password.len() > 1024 {
        return Err(
            "Use a backup passphrase of at least 12 characters and at most 1024 bytes.".into(),
        );
    }
    let rng = SystemRandom::new();
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    rng.fill(&mut salt).map_err(|_| "Randomness unavailable")?;
    rng.fill(&mut nonce).map_err(|_| "Randomness unavailable")?;
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ITERATIONS).unwrap(),
        &salt,
        password.as_bytes(),
        key.as_mut(),
    );
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref())
            .map_err(|_| "Cannot protect backup")?,
    );
    let mut encrypted = bytes.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(AAD),
        &mut encrypted,
    )
    .map_err(|_| "Cannot protect backup")?;
    serde_json::to_vec(&Protected {
        format: "study-harness.protected-backup/1".into(),
        kdf: "PBKDF2-HMAC-SHA256/AES-256-GCM".into(),
        iterations: ITERATIONS,
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(encrypted),
    })
    .map_err(|_| "Cannot encode protected backup".into())
}
pub fn unprotect(bytes: &[u8], password: Option<&str>) -> Result<Vec<u8>> {
    if bytes.len() > MAX_PROTECTED {
        return Err("Archive exceeds the import limit.".into());
    }
    let header: Value = serde_json::from_slice(bytes).map_err(|_| "Malformed archive")?;
    if header["format"] != "study-harness.protected-backup/1" {
        if bytes.len() > MAX_ARCHIVE {
            return Err("Archive exceeds the 128 MiB import limit".into());
        }
        return Ok(bytes.to_vec());
    }
    let p: Protected =
        serde_json::from_value(header).map_err(|_| "Invalid protected backup fields")?;
    if p.iterations != ITERATIONS || p.kdf != "PBKDF2-HMAC-SHA256/AES-256-GCM" {
        return Err("Unsupported backup protection version.".into());
    }
    let password = password.ok_or("This backup requires its passphrase")?;
    if password.len() > 1024 {
        return Err("Invalid backup passphrase".into());
    }
    let salt = STANDARD.decode(p.salt).map_err(|_| "Invalid backup salt")?;
    let nonce = STANDARD
        .decode(p.nonce)
        .map_err(|_| "Invalid backup nonce")?;
    if salt.len() != 16 || nonce.len() != 12 {
        return Err("Invalid backup protection parameters".into());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ITERATIONS).unwrap(),
        &salt,
        password.as_bytes(),
        key.as_mut(),
    );
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref())
            .map_err(|_| "Cannot open backup")?,
    );
    let mut ciphertext = STANDARD
        .decode(p.ciphertext)
        .map_err(|_| "Invalid protected backup bytes")?;
    let opened = key
        .open_in_place(
            aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| "Invalid backup nonce")?,
            aead::Aad::from(AAD),
            &mut ciphertext,
        )
        .map_err(|_| "Wrong passphrase or altered protected backup")?;
    Ok(opened.to_vec())
}
pub fn migration_backup(c: &Connection, path: &Path, version: i32) -> Result<()> {
    if version == 0 || version == CURRENT_SCHEMA || !path.is_file() {
        return Ok(());
    }
    let folder = path
        .parent()
        .ok_or("Database directory unavailable")?
        .join("backups");
    std::fs::create_dir_all(&folder).map_err(|_| "Cannot create a pre-migration backup")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect backup directory")?;
    }
    let dest = folder.join(format!("pre-schema-{version}-{}.sqlite3", id()));
    private_file(&dest, &[])?;
    c.backup(
        rusqlite::MAIN_DB,
        &dest,
        None::<fn(rusqlite::backup::Progress)>,
    )
    .map_err(|_| "Pre-migration backup failed; the database was not upgraded")?;
    Ok(())
}
fn validate_database(path: &Path, version: i32, mode: &str) -> Result<()> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "Invalid SQLite snapshot")?;
    c.pragma_update(None, "trusted_schema", false)
        .map_err(sql)?;
    let actual: i32 = c
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql)?;
    if actual != version || schema_objects(&c)? != expected_schema(version)? {
        return Err("Archive database schema contains unsupported or changed objects.".into());
    }
    let integrity: String = c
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(sql)?;
    if integrity != "ok" {
        return Err("Archive database integrity check failed".into());
    }
    let foreign: i64 = c
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    if foreign != 0 {
        return Err("Archive contains broken record references".into());
    }
    let stored: String = c
        .query_row(
            "SELECT value FROM settings WHERE key='data_mode'",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if parse::<String>(stored)? != mode {
        return Err("Archive data mode does not match its manifest".into());
    }
    let allowed = [
        "data_mode",
        "limits",
        "simulated_clock",
        "review_minutes",
        "privacy",
    ];
    let mut s = c.prepare("SELECT key FROM settings").map_err(sql)?;
    for key in s.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
        if !allowed.contains(&key.map_err(sql)?.as_str()) {
            return Err("Archive contains unsupported private settings".into());
        }
    }
    let setting = |key: &str| -> Result<Option<String>> {
        c.query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sql)
    };
    if let Some(raw) = setting("limits")? {
        let l: Limits = parse(raw)?;
        if l.session_microusd < 0
            || l.session_microusd > 100_000_000
            || l.monthly_microusd < 0
            || l.monthly_microusd > 1_000_000_000
        {
            return Err("Invalid archive spending limits".into());
        }
    }
    if let Some(raw) = setting("privacy")? {
        let p: PrivacySettings = parse(raw)?;
        if !(3..=60).contains(&p.review_minutes)
            || !(100..=200).contains(&p.text_scale_percent)
            || !crate::registry::valid_model(&p.tutor_model)
            || !crate::registry::valid_effort(&p.reasoning_effort)
        {
            return Err("Invalid archive privacy or presentation settings".into());
        }
    }
    if let Some(raw) = setting("simulated_clock")? {
        if mode != "offline-development" {
            return Err("A production archive cannot install a simulated clock".into());
        }
        let at: String = parse(raw)?;
        chrono::DateTime::parse_from_rfc3339(&at)
            .map_err(|_| "Invalid archive simulation clock")?;
    }
    // Validate every primary record with the actual native readers before adoption.
    macro_rules! rows {
        ($table:literal,$ty:ty) => {{
            let mut st = c
                .prepare(concat!("SELECT data FROM ", $table))
                .map_err(sql)?;
            for row in st.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
                let _: $ty = parse(row.map_err(sql)?)
                    .map_err(|_| concat!("Invalid archive record in ", $table))?;
            }
        }};
    }
    rows!("sessions", Session);
    rows!("activities", Activity);
    rows!("attempts", Attempt);
    rows!("assistance", Assistance);
    rows!("requests", Request);
    rows!("usage", Usage);
    if version >= 9 {
        rows!("goals", GoalRecord);
        rows!("plans", PlanRevision);
    }
    for (table, key) in [
        ("sessions", "id"),
        ("activities", "id"),
        ("attempts", "id"),
        ("assistance", "id"),
        ("requests", "id"),
        ("usage", "request_id"),
    ] {
        let count: i64 = c
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {table} WHERE json_extract(data,'$.{key}') IS NOT {key}"
                ),
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if count != 0 {
            return Err("Archive record identity disagrees with its database key".into());
        }
    }
    for query in [
        "SELECT COUNT(*) FROM attempts a JOIN activities t ON t.id=a.activity_id WHERE a.session_id IS NOT t.session_id OR json_extract(a.data,'$.session_id') IS NOT a.session_id OR json_extract(a.data,'$.activity_id') IS NOT a.activity_id OR json_extract(a.data,'$.task_version') IS NOT t.version",
        "SELECT COUNT(*) FROM assistance h JOIN activities a ON a.id=h.activity_id WHERE h.session_id IS NOT a.session_id OR json_extract(h.data,'$.session_id') IS NOT h.session_id OR json_extract(h.data,'$.activity_id') IS NOT h.activity_id",
        "SELECT COUNT(*) FROM requests WHERE json_extract(data,'$.status') IS NOT status OR json_extract(data,'$.session_id') IS NOT session_id OR json_extract(data,'$.kind') IS NOT kind OR json_extract(data,'$.reserved_microusd') IS NOT reserved_microusd",
        "SELECT COUNT(*) FROM usage WHERE json_extract(data,'$.cost_microusd') IS NOT cost_microusd",
        "SELECT COUNT(*) FROM sessions WHERE json_extract(data,'$.revision') IS NOT revision",
        "SELECT COUNT(*) FROM activities WHERE json_extract(data,'$.session_id') IS NOT session_id OR json_extract(data,'$.objective_id') IS NOT objective_id OR json_extract(data,'$.version') IS NOT version",
        "SELECT COUNT(*) FROM assistance WHERE json_extract(data,'$.attempt_id') IS NOT attempt_id",
        "SELECT COUNT(*) FROM requests WHERE json_extract(data,'$.attempt_id') IS NOT attempt_id OR json_extract(data,'$.created_at') IS NOT created_at",
    ] {
        let count: i64 = c.query_row(query, [], |r| r.get(0)).map_err(sql)?;
        if count != 0 {
            return Err("Archive contains inconsistent evidence references".into());
        }
    }
    if version >= 3 {
        rows!("task_specs", TaskSpec);
        rows!("attempt_conditions", AttemptConditions);
        rows!("learning_sessions", LearningState);
        rows!("learning_summaries", LearningSummary);
        rows!("assessments", Assessment);
        rows!("reviews", Review);
        for query in [
            "SELECT COUNT(*) FROM task_specs WHERE json_extract(data,'$.activity_id') IS NOT activity_id",
            "SELECT COUNT(*) FROM attempt_conditions WHERE json_extract(data,'$.attempt_id') IS NOT attempt_id",
            "SELECT COUNT(*) FROM learning_sessions s LEFT JOIN activities a ON a.id=json_extract(s.data,'$.active_activity_id') WHERE a.id IS NULL OR a.session_id IS NOT s.session_id OR json_extract(s.data,'$.session_id') IS NOT s.session_id",
            "SELECT COUNT(*) FROM assessments s JOIN attempts a ON a.id=s.attempt_id JOIN activities t ON t.id=a.activity_id LEFT JOIN requests r ON r.id=json_extract(s.data,'$.request_id') WHERE json_extract(s.data,'$.id') IS NOT s.id OR json_extract(s.data,'$.attempt_id') IS NOT s.attempt_id OR json_extract(s.data,'$.rubric_version') IS NOT json_extract(t.data,'$.rubric_version') OR r.attempt_id IS NOT a.id",
            "SELECT COUNT(*) FROM reviews v LEFT JOIN sessions s ON s.id=json_extract(v.data,'$.session_id') WHERE s.id IS NULL OR json_extract(v.data,'$.id') IS NOT v.id OR json_extract(v.data,'$.objective_id') IS NOT v.objective_id",
        ] {
            let bad: i64 = c.query_row(query, [], |r| r.get(0)).map_err(sql)?;
            if bad != 0 {
                return Err("Archive learning references disagree".into());
            }
        }
        let mut at_rows = c.prepare("SELECT data FROM attempts").map_err(sql)?;
        for row in at_rows
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let at: Attempt = parse(row.map_err(sql)?)?;
            for help in at.assistance_ids {
                let owned:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM assistance WHERE id=? AND activity_id=? AND session_id=?)",rusqlite::params![help,at.activity_id,at.session_id],|r|r.get(0)).map_err(sql)?;
                if !owned {
                    return Err("Archive submission refers to unrelated assistance".into());
                }
            }
        }
        if mode == "live" {
            let count:i64=c.query_row("SELECT (SELECT COUNT(*) FROM attempt_conditions WHERE json_extract(data,'$.simulated')=1)+(SELECT COUNT(*) FROM assessments WHERE json_extract(data,'$.grader_version') LIKE '%simulated%')+(SELECT COUNT(*) FROM learning_summaries WHERE json_extract(data,'$.simulated')=1)",[],|r|r.get(0)).map_err(sql)?;
            if count != 0 {
                return Err(
                    "Simulated evidence cannot be restored into a production profile".into(),
                );
            }
        }
    }
    if version >= 4 {
        let missing:i64=c.query_row("SELECT COUNT(*) FROM visual_activities v WHERE json_extract(v.state,'$.checkpointDigest') IS NOT NULL AND NOT EXISTS (SELECT 1 FROM visual_artifacts a WHERE a.activity_id=v.id AND a.digest=json_extract(v.state,'$.checkpointDigest'))",[],|r|r.get(0)).map_err(sql)?;
        if missing != 0 {
            return Err("Archive is missing a referenced visual checkpoint".into());
        }
        for (table, contract) in [
            ("visual_policies", "study-harness.host-policy"),
            ("visual_exposures", "study-harness.exposure-record"),
            ("visual_events", "study-harness.interaction-event"),
            ("visual_artifacts", "study-harness.activity-artifact"),
        ] {
            let mut st = c
                .prepare(&format!("SELECT data FROM {table}"))
                .map_err(sql)?;
            for row in st.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
                let v: Value = parse(row.map_err(sql)?)?;
                study_harness_contracts::validate(contract, &v)
                    .map_err(|_| "Invalid visualizer record in archive")?;
            }
        }
        let missing:i64=c.query_row("SELECT COUNT(*) FROM visual_artifacts a JOIN visual_activities v ON a.activity_id=v.id WHERE json_extract(a.data,'$.instance.activityId')!=v.id OR json_extract(a.data,'$.template')!=json_extract(v.state,'$.template') OR json_extract(a.data,'$.model')!=json_extract(v.state,'$.model')",[],|r|r.get(0)).map_err(sql)?;
        if missing != 0 {
            return Err("Archive visualizer identities disagree".into());
        }
        let mut st = c
            .prepare("SELECT digest,data FROM visual_artifacts")
            .map_err(sql)?;
        for row in st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?
        {
            let (hash, data) = row.map_err(sql)?;
            if hash != study_harness_contracts::canonical::digest(&parse::<Value>(data)?) {
                return Err("Archive visual artifact hash mismatch".into());
            }
        }
    }
    if version >= 6 {
        rows!("sources", SourceDocument);
        rows!("source_chunks", SourcePassage);
        let mut st = c
            .prepare("SELECT s.data,b.bytes FROM sources s JOIN source_blobs b ON b.source_id=s.id")
            .map_err(sql)?;
        for row in st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(sql)?
        {
            let (raw, bytes) = row.map_err(sql)?;
            let doc: SourceDocument = parse(raw)?;
            if doc.sha256.len() != 64
                || doc.id != format!("source-{}", &sha(&bytes)[..32])
                || bytes.len() > crate::sources::MAX_SOURCE_BYTES
                || doc.page_count > 100
                || sha(&bytes) != doc.sha256
            {
                return Err("Archive source hash mismatch".into());
            }
        }
        let mut st = c.prepare("SELECT data FROM source_chunks").map_err(sql)?;
        for row in st.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            let p: SourcePassage = parse(row.map_err(sql)?)?;
            if p.text.chars().count() > 1500 || sha(p.text.as_bytes()) != p.sha256 {
                return Err("Archive passage hash mismatch".into());
            }
        }
        let missing:i64=c.query_row("SELECT COUNT(*) FROM sources s LEFT JOIN source_blobs b ON b.source_id=s.id WHERE b.source_id IS NULL",[],|r|r.get(0)).map_err(sql)?;
        if missing != 0 {
            return Err("Archive is missing source bytes".into());
        }
        let guard: i64 = c
            .query_row("SELECT COUNT(*) FROM deletion_context", [], |r| r.get(0))
            .map_err(sql)?;
        if guard != 0 {
            return Err("Archive contains an unfinished deletion".into());
        }
        let mut contexts = c
            .prepare("SELECT data FROM request_source_context")
            .map_err(sql)?;
        for row in contexts
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let context: Vec<SourcePassage> = parse(row.map_err(sql)?)?;
            if context.len() > 5 {
                return Err("Archive source context exceeds its bound".into());
            }
            for p in context {
                let raw: String = c
                    .query_row("SELECT data FROM source_chunks WHERE id=?", [&p.id], |r| {
                        r.get(0)
                    })
                    .map_err(sql)?;
                let original: SourcePassage = parse(raw)?;
                if p.source_id != original.source_id
                    || p.title != original.title
                    || p.anchor != original.anchor
                    || !original.text.contains(&p.text)
                    || sha(p.text.as_bytes()) != p.sha256
                {
                    return Err("Archive captured source context is inconsistent".into());
                }
            }
        }
        let mut st = c
            .prepare("SELECT data FROM source_citations")
            .map_err(sql)?;
        for row in st.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            let cite: SourceCitation = parse(row.map_err(sql)?)?;
            let context: String = c
                .query_row(
                    "SELECT data FROM request_source_context WHERE request_id=?",
                    [&cite.request_id],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            let context: Vec<SourcePassage> = parse(context)?;
            if !context.iter().any(|p| {
                p.id == cite.chunk_id
                    && p.source_id == cite.source_id
                    && p.title == cite.title
                    && p.anchor == cite.anchor
                    && p.text.contains(&cite.excerpt)
                    && !cite.excerpt.trim().is_empty()
            }) {
                return Err(
                    "Archive citation is not grounded in its captured source context".into(),
                );
            }
        }
    }
    if version >= 7 {
        crate::evaluation::validate_archive(&c)?;
        if mode == "live" {
            let count:i64=c.query_row("SELECT COUNT(*) FROM evaluation_pairs WHERE json_extract(data,'$.simulated')=1",[],|r|r.get(0)).map_err(sql)?;
            if count != 0 {
                return Err(
                    "Simulated comparison records cannot enter a production profile".into(),
                );
            }
        }
    }
    if version >= 8 {
        rows!("request_versions", RequestVersions);
        let count:i64=c.query_row("SELECT COUNT(*) FROM request_versions WHERE json_extract(data,'$.request_id') IS NOT request_id",[],|r|r.get(0)).map_err(sql)?;
        if count != 0 {
            return Err("Archive request version identity is inconsistent".into());
        }
    }
    Ok(())
}
fn decode_archive(
    bytes: &[u8],
    parent: &Path,
    mode: &str,
) -> Result<(Archive, Temporary, BTreeMap<String, Vec<u8>>)> {
    if bytes.len() > MAX_ARCHIVE {
        return Err("Archive exceeds the 128 MiB import limit".into());
    }
    let archive: Archive = serde_json::from_slice(bytes).map_err(|_| "Invalid archive manifest")?;
    if archive.format != "study-harness.portable/1"
        || archive.schema_version != 1
        || archive.mode != mode
        || archive.files.len() != 10
    {
        return Err("Archive format, data mode, or complete file set is incompatible".into());
    }
    schema_sql(archive.database_schema)?;
    let mut files = BTreeMap::new();
    let mut total = 0;
    for f in &archive.files {
        let safe = [
            "database.sqlite3",
            "content/visualizer.json",
            "content/unit-v1.json",
            "packages/manifest.json",
        ]
        .contains(&f.path.as_str())
            || f.path.strip_prefix("packages/").is_some_and(|name| {
                name.ends_with(".tgz")
                    && name.len() <= 120
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
            });
        if !safe || files.contains_key(&f.path) {
            return Err("Archive contains an unexpected or repeated path".into());
        }
        let data = STANDARD
            .decode(&f.bytes)
            .map_err(|_| "Invalid archive file encoding")?;
        total += data.len();
        if total > MAX_ARCHIVE || sha(&data) != f.sha256 {
            return Err("Archive content hash or size validation failed".into());
        }
        files.insert(f.path.clone(), data);
    }
    for required in [
        "database.sqlite3",
        "content/visualizer.json",
        "content/unit-v1.json",
        "packages/manifest.json",
    ] {
        if !files.contains_key(required) {
            return Err("Archive is missing required content".into());
        }
    }
    let package: Value = serde_json::from_slice(&files["packages/manifest.json"])
        .map_err(|_| "Invalid package manifest")?;
    let entries = package["packages"]
        .as_array()
        .filter(|p| p.len() == 6)
        .ok_or("Archive must include all six exact visualizer packages")?;
    let mut names = BTreeSet::new();
    for p in entries {
        let name = p["file"].as_str().ok_or("Invalid package identity")?;
        if !names.insert(name) {
            return Err("Repeated package identity".into());
        }
        let data = files
            .get(&format!("packages/{name}"))
            .ok_or("Archive is missing an exact visualizer package")?;
        if p["sha256"] != sha(data) {
            return Err("Visual package does not match its recorded source manifest".into());
        }
    }
    // Only the installed, reference-checked public content set may be adopted. Imported executable
    // packages are never loaded. Unknown versions remain portable for their original application.
    for (name, known) in [
        (
            "content/visualizer.json",
            include_bytes!("../visualizer/catalog.json").as_slice(),
        ),
        (
            "content/unit-v1.json",
            include_bytes!("../../content/unit-v1.json").as_slice(),
        ),
    ] {
        if sha(&files[name]) != sha(known) {
            return Err("Archive uses an unsupported content/package version. Keep it for its original application version.".into());
        }
    }
    let installed_manifest: Value =
        serde_json::from_slice(include_bytes!("../../vendor/visualizer/manifest.json"))
            .expect("installed package manifest");
    if package["source"]["dirty"] != false
        || package["source"]["commit"]
            .as_str()
            .is_none_or(|s| s.len() != 40 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        || package["lockSha256"] != installed_manifest["lockSha256"]
    {
        return Err("Archive visualizer source identity is unsupported".into());
    }
    for p in entries {
        let known = installed_manifest["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["file"] == p["file"])
            .ok_or("Unknown archive package")?;
        for key in ["name", "version", "file", "sha256"] {
            if p[key] != known[key] {
                return Err("Archive package identity is inconsistent".into());
            }
        }
    }
    for (name, known) in PACKAGES {
        if sha(&files[&format!("packages/{name}")]) != sha(known) {
            return Err("Archive package differs from the installed content version".into());
        }
    }
    let temp = staging(parent)?;
    std::fs::write(&temp.0, &files["database.sqlite3"])
        .map_err(|_| "Cannot stage the database snapshot")?;
    validate_database(&temp.0, archive.database_schema, mode)?;
    Ok((archive, temp, files))
}
impl Store {
    pub fn export_archive(&self) -> Result<Vec<u8>> {
        let parent = Path::new(&self.path)
            .parent()
            .ok_or("Database directory unavailable")?;
        let temp = staging(parent)?;
        self.conn
            .backup(
                rusqlite::MAIN_DB,
                &temp.0,
                None::<fn(rusqlite::backup::Progress)>,
            )
            .map_err(sql)?;
        let bytes = std::fs::read(&temp.0).map_err(|_| "Cannot read the coordinated snapshot")?;
        let mut files = vec![
            file("database.sqlite3", &bytes),
            file(
                "content/visualizer.json",
                include_bytes!("../visualizer/catalog.json"),
            ),
            file(
                "content/unit-v1.json",
                include_bytes!("../../content/unit-v1.json"),
            ),
            file(
                "packages/manifest.json",
                include_bytes!("../../vendor/visualizer/manifest.json"),
            ),
        ];
        for (name, bytes) in PACKAGES {
            files.push(file(&format!("packages/{name}"), bytes));
        }
        let archive = Archive {
            format: "study-harness.portable/1".into(),
            schema_version: 1,
            database_schema: self
                .conn
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .map_err(sql)?,
            mode: if self.offline {
                "offline-development"
            } else {
                "live"
            }
            .into(),
            created_at: self.time(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            files,
        };
        let bytes = serde_json::to_vec(&archive).map_err(|_| "Cannot encode portable archive")?;
        if bytes.len() > MAX_ARCHIVE {
            return Err("Portable archive exceeds 128 MiB. Split or delete unnecessary source material before exporting.".into());
        }
        Ok(bytes)
    }
    pub fn restore_archive(&mut self, bytes: &[u8], password: Option<&str>) -> Result<()> {
        let bytes = Zeroizing::new(unprotect(bytes, password)?);
        let parent = Path::new(&self.path)
            .parent()
            .ok_or("Database directory unavailable")?
            .to_path_buf();
        let mode = if self.offline {
            "offline-development"
        } else {
            "live"
        };
        let (_archive, staged, _files) = decode_archive(&bytes, &parent, mode)?;
        let used:i64=self.conn.query_row("SELECT (SELECT COUNT(*) FROM attempts)+(SELECT COUNT(*) FROM learning_sessions)+(SELECT COUNT(*) FROM sources)+(SELECT COUNT(*) FROM requests)+(SELECT COUNT(*) FROM pending_transcripts)+(SELECT COUNT(*) FROM evaluation_pairs)+(SELECT COUNT(*) FROM sessions WHERE json_extract(data,'$.draft')!='')",[],|r|r.get(0)).map_err(sql)?;
        if used != 0 {
            return Err("Restore requires a fresh profile with no study work, sources, requests or drafts. Export this profile first and choose a fresh profile at launch.".into());
        }
        let old = std::mem::replace(&mut self.conn, Connection::open_in_memory().map_err(sql)?);
        if let Err((connection, error)) = old.close() {
            self.conn = connection;
            return Err(sql(error));
        }
        let previous = parent.join(format!(".before-restore-{}.sqlite3", id()));
        if std::fs::rename(&self.path, &previous).is_err() {
            self.conn = Connection::open(&self.path).map_err(sql)?;
            self.conn
                .pragma_update(None, "foreign_keys", true)
                .map_err(sql)?;
            return Err("Cannot preserve the fresh profile during restore".into());
        }
        if std::fs::rename(&staged.0, &self.path).is_err() {
            let _ = std::fs::rename(&previous, &self.path);
            self.conn = Connection::open(&self.path).map_err(sql)?;
            self.conn
                .execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
                .map_err(sql)?;
            return Err("Atomic restore failed; the earlier profile was preserved".into());
        }
        match Store::open_with(Path::new(&self.path), self.offline, self.clock.clone()).and_then(
            |mut restored| {
                restored.rebuild_restored_derivatives()?;
                Ok(restored)
            },
        ) {
            Ok(restored) => {
                *self = restored;
                let _ = std::fs::remove_file(previous);
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&self.path);
                let _ = std::fs::rename(previous, &self.path);
                self.conn = Connection::open(&self.path).map_err(sql)?;
                self.conn
                    .execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
                    .map_err(sql)?;
                Err(format!("Restore refused; earlier profile preserved. {e}"))
            }
        }
    }
    fn rebuild_restored_derivatives(&mut self) -> Result<()> {
        let ids = {
            let mut s = self
                .conn
                .prepare("SELECT session_id FROM learning_summaries")
                .map_err(sql)?;
            s.query_map([], |r| r.get::<_, String>(0))
                .map_err(sql)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(sql)?
        };
        let time = self.time();
        let simulated = self.offline || self.clock.simulated();
        let tx = self.conn.transaction().map_err(sql)?;
        for sid in ids {
            crate::learning::recompute(&tx, &sid, &time, simulated)?;
        }
        tx.commit().map_err(sql)
    }
    pub fn privacy_settings(&self) -> Result<PrivacySettings> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key='privacy'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sql)?;
        raw.map(parse).unwrap_or(Ok(PrivacySettings {
            retain_failed_audio: true,
            review_minutes: 10,
            text_scale_percent: 100,
            tutor_model: default_tutor_model(),
            reasoning_effort: default_reasoning_effort(),
        }))
    }
    pub fn set_privacy(&self, p: &PrivacySettings) -> Result<()> {
        if !(100..=200).contains(&p.text_scale_percent) {
            return Err("Text size must be 100–200 percent".into());
        }
        if !(3..=60).contains(&p.review_minutes) {
            return Err("Choose 3–60 minutes of planned review per day".into());
        }
        if !crate::registry::valid_model(&p.tutor_model) {
            return Err("Choose one of the listed tutor models".into());
        }
        if !crate::registry::valid_effort(&p.reasoning_effort) {
            return Err("Choose low, medium or high reasoning effort".into());
        }
        self.conn
            .execute(
                "INSERT OR REPLACE INTO settings VALUES ('privacy',?)",
                [json(p)?],
            )
            .map_err(sql)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO settings VALUES ('review_minutes',?)",
                [json(&p.review_minutes)?],
            )
            .map_err(sql)?;
        Ok(())
    }
    pub fn restore_clock(&mut self) -> Result<()> {
        if self.offline {
            let raw: Option<String> = self
                .conn
                .query_row(
                    "SELECT value FROM settings WHERE key='simulated_clock'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            if let Some(raw) = raw {
                let value: String = parse(raw)?;
                self.clock = std::sync::Arc::new(crate::clock::TestClock(std::sync::Mutex::new(
                    chrono::DateTime::parse_from_rfc3339(&value)
                        .map_err(|_| "Invalid simulation clock")?
                        .with_timezone(&chrono::Utc),
                )));
            }
        }
        Ok(())
    }
    pub fn deletion_preview(&self, scope: &str, target: &str) -> Result<DeletionPreview> {
        let sessions = if scope == "session" {
            self.session(target)?;
            vec![target.to_owned()]
        } else if scope == "source" {
            let exists: bool = self
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sources WHERE id=?)",
                    [target],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            if !exists {
                return Err("Source not found".into());
            }
            // Every capability shares its goal's source-derived plan, even an empty session left by a failed
            // start. Keep the preview and deletion scope aligned so no surviving session retains that plan.
            let mut s = self.conn.prepare(
                "SELECT session_id FROM source_uses WHERE source_id=?1
                 UNION SELECT s.session_id FROM source_selections s JOIN source_chunks c ON c.id=s.chunk_id WHERE c.source_id=?1
                 UNION SELECT r.session_id FROM requests r JOIN request_source_context c ON c.request_id=r.id,json_each(c.data) item WHERE json_extract(item.value,'$.source_id')=?1
                 UNION SELECT gs.session_id FROM goal_sessions gs JOIN goals g ON g.id=gs.goal_id,json_each(g.data,'$.source_ids') source WHERE source.value=?1",
            ).map_err(sql)?;
            s.query_map([target], |r| r.get(0))
                .map_err(sql)?
                .collect::<std::result::Result<Vec<String>, _>>()
                .map_err(sql)?
        } else if scope == "comparison" {
            let exists: bool = self
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM evaluation_pairs WHERE id=?)",
                    [target],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            if !exists {
                return Err("Comparison pair not found".into());
            }
            vec![]
        } else {
            return Err("Unsupported deletion scope".into());
        };
        let mut attempts = 0;
        for sid in &sessions {
            attempts += self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM attempts WHERE session_id=?",
                    [sid],
                    |r| r.get::<_, u32>(0),
                )
                .map_err(sql)?;
        }
        let pairs = if scope == "comparison" {
            vec![target.into()]
        } else {
            crate::evaluation::dependent_pairs(
                &self.conn,
                &sessions,
                if scope == "source" {
                    Some(target)
                } else {
                    None
                },
            )?
        };
        Ok(DeletionPreview {
            scope: scope.into(),
            target_id: target.into(),
            session_ids: sessions,
            attempt_count: attempts,
            notice: format!(
                "{} {} associated comparison pairs (both conditions and all held-out responses/scores) will also be deleted to avoid incomplete comparisons.",
                if scope == "source" {
                    "Deleting this source also deletes sessions that viewed or used it and every session in a goal with this source attached, including their plans, attempts, grades, reviews and visual artifacts. This prevents retained quotations and misleading evidence. Other sources are retained."
                } else {
                    "Deleting this session removes its attempts, grades, reviews, requests, visual artifacts and derived records. Local raw audio is memory-only and is released when its composer closes."
                },
                pairs.len()
            ),
        })
    }
    pub fn delete_owned(&mut self, scope: &str, target: &str, confirmation: &str) -> Result<()> {
        if confirmation != "DELETE" {
            return Err("Type DELETE to intentionally remove these records".into());
        }
        let preview = self.deletion_preview(scope, target)?;
        let pairs = if scope == "comparison" {
            vec![target.into()]
        } else {
            crate::evaluation::dependent_pairs(
                &self.conn,
                &preview.session_ids,
                if scope == "source" {
                    Some(target)
                } else {
                    None
                },
            )?
        };
        let pending: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM requests WHERE status='pending'",
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if pending > 0 {
            return Err("Finish or cancel the running request before deleting data".into());
        }
        // App-managed migration backups contain previous records, so intentional deletion also removes them.
        if let Some(parent) = Path::new(&self.path).parent() {
            cleanup_staging(parent)?;
            let backups = parent.join("backups");
            if backups.is_dir() {
                for entry in std::fs::read_dir(backups)
                    .map_err(|_| "Cannot inspect local migration backups")?
                {
                    let path = entry.map_err(|_| "Cannot inspect migration backup")?.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("pre-schema-") && n.ends_with(".sqlite3"))
                    {
                        std::fs::remove_file(path).map_err(
                            |_| "Cannot remove a migration backup; no study records were deleted",
                        )?;
                    }
                }
            }
        }
        self.conn
            .pragma_update(None, "secure_delete", true)
            .map_err(sql)?;
        let tx = self.conn.transaction().map_err(sql)?;
        for pair in pairs {
            tx.execute("DELETE FROM evaluation_pairs WHERE id=?", [pair])
                .map_err(sql)?;
        }
        for sid in preview.session_ids {
            tx.execute("INSERT INTO deletion_context VALUES (?)", [&sid])
                .map_err(sql)?;
            for query in [
                "DELETE FROM disputes WHERE session_id=?",
                "DELETE FROM learning_summaries WHERE session_id=?",
                "DELETE FROM reviews WHERE json_extract(data,'$.session_id')=?",
                "DELETE FROM assistance WHERE session_id=?",
                "DELETE FROM usage WHERE request_id IN (SELECT id FROM requests WHERE session_id=?)",
                "DELETE FROM requests WHERE session_id=?",
                "DELETE FROM attempt_conditions WHERE attempt_id IN (SELECT id FROM attempts WHERE session_id=?)",
                "DELETE FROM attempts WHERE session_id=?",
                "DELETE FROM task_specs WHERE activity_id IN (SELECT id FROM activities WHERE session_id=?)",
                "DELETE FROM learning_events WHERE session_id=?",
                "DELETE FROM learning_sessions WHERE session_id=?",
                "DELETE FROM sessions WHERE id=?",
            ] {
                tx.execute(query, [&sid]).map_err(sql)?;
            }
            tx.execute("DELETE FROM deletion_context WHERE session_id=?", [sid])
                .map_err(sql)?;
        }
        // A goal whose sessions are all gone takes its plans and unreferenced generated capabilities with it.
        crate::goals::delete_orphans(&tx)?;
        if scope == "source" {
            tx.execute("DELETE FROM source_search WHERE source_id IN (SELECT id FROM source_chunks WHERE source_id=?)",[target]).map_err(sql)?;
            tx.execute("DELETE FROM sources WHERE id=?", [target])
                .map_err(sql)?;
        }
        tx.commit().map_err(sql)?;
        self.conn.execute_batch("VACUUM;").map_err(sql)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as j;
    use std::sync::Arc;
    fn fixture(path: &Path) -> Store {
        Store::open_with(path, true, Arc::new(crate::clock::SystemClock)).unwrap()
    }
    #[test]
    fn coordinated_archive_restores_fresh_profile_and_rejects_changed_content() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = fixture(&dir.path().join("original.sqlite3"));
        let sid = d.new_session("Portable fixture").unwrap();
        d.start_goal(&sid, 0, "Convergence").unwrap();
        let archive = d.export_archive().unwrap();
        let mut fresh = fixture(&dir.path().join("fresh.sqlite3"));
        fresh.restore_archive(&archive, None).unwrap();
        assert_eq!(fresh.session(&sid).unwrap().revision, 1);
        assert!(fresh.restore_archive(&archive, None).is_err());
        let mut damaged: Value = serde_json::from_slice(&archive).unwrap();
        damaged["files"][0]["bytes"] = j!("YQ==");
        let mut other = fixture(&dir.path().join("other.sqlite3"));
        let before = std::fs::read(&other.path).unwrap();
        assert!(
            other
                .restore_archive(&serde_json::to_vec(&damaged).unwrap(), None)
                .is_err()
        );
        assert_eq!(before, std::fs::read(&other.path).unwrap());
    }
    #[test]
    fn protected_backups_authenticate_password_metadata_and_ciphertext() {
        let p = "a deliberate test-only passphrase";
        let bytes = b"private fixture content";
        let a = protect(bytes, p).unwrap();
        let b = protect(bytes, p).unwrap();
        assert_ne!(a, b);
        assert_eq!(unprotect(&a, Some(p)).unwrap(), bytes);
        assert!(unprotect(&a, Some("wrong")).is_err());
        let mut v: Value = serde_json::from_slice(&a).unwrap();
        v["iterations"] = j!(1);
        assert!(unprotect(&serde_json::to_vec(&v).unwrap(), Some(p)).is_err());
        v["iterations"] = j!(ITERATIONS);
        v["ciphertext"] = j!("AAAA");
        assert!(unprotect(&serde_json::to_vec(&v).unwrap(), Some(p)).is_err());
    }
    #[test]
    fn intentional_deletion_removes_evidence_but_sql_cannot_erase_assistance() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = fixture(&dir.path().join("delete.sqlite3"));
        let sid = d.new_session("Delete fixture").unwrap();
        d.start_goal(&sid, 0, "Convergence").unwrap();
        d.learning_action(&sid, d.session(&sid).unwrap().revision, "hint")
            .unwrap();
        assert!(d.conn.execute("DELETE FROM assistance", []).is_err());
        d.delete_owned("session", &sid, "DELETE").unwrap();
        assert!(d.sessions().unwrap().is_empty());
        assert_eq!(
            d.conn
                .query_row("SELECT COUNT(*) FROM assistance", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            d.conn
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
    }
    #[test]
    fn archive_schema_reference_source_clock_and_path_attacks_are_rejected_before_adoption() {
        let dir = tempfile::tempdir().unwrap();
        let mut original = fixture(&dir.path().join("original.db"));
        let sid = original.new_session("Archive adversarial fixture").unwrap();
        original.start_goal(&sid, 0, "Convergence").unwrap();
        let attempt = original
            .save_attempt(&SaveAttempt {
                session_id: sid.clone(),
                expected_revision: original.session(&sid).unwrap().revision,
                submission_id: id(),
                response: "A synthetic private response".into(),
                transcript_id: None,
            })
            .unwrap();
        original
            .reserve(&sid, Some(&attempt.id), "tutor", 0)
            .unwrap();
        original
            .import_source(crate::sources::ImportSource {
                title: "Archive reference fixture".into(),
                kind: "text".into(),
                raw_base64: STANDARD.encode(b"A shrinking multiplier converges."),
                pages: vec![],
                extraction_version: crate::sources::TEXT_EXTRACTION.into(),
            })
            .unwrap();
        let archive: Value = serde_json::from_slice(&original.export_archive().unwrap()).unwrap();
        for (n, attack) in [
            "extra_sql",
            "record_reference",
            "source_bytes",
            "source_identity",
            "clock",
            "credential_setting",
            "future_schema",
            "path",
            "package",
        ]
        .iter()
        .enumerate()
        {
            let mut altered = archive.clone();
            if *attack == "path" {
                altered["files"][0]["path"] = j!("../../outside.sqlite3");
            } else if *attack == "package" {
                altered["files"][2]["bytes"] = j!(STANDARD.encode(b"unsupported content"));
                altered["files"][2]["sha256"] = j!(sha(b"unsupported content"));
            } else {
                let snapshot = dir.path().join(format!("attack-{n}.db"));
                std::fs::write(
                    &snapshot,
                    STANDARD
                        .decode(altered["files"][0]["bytes"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                let c = Connection::open(&snapshot).unwrap();
                match *attack {
                    "extra_sql" => {
                        c.execute_batch("CREATE TRIGGER untrusted_trigger BEFORE INSERT ON sessions BEGIN SELECT load_extension('never-executed'); END;").unwrap();
                    }
                    "record_reference" => {
                        c.execute(
                            "UPDATE requests SET data=json_set(data,'$.session_id','unrelated')",
                            [],
                        )
                        .unwrap();
                    }
                    "source_bytes" => {
                        c.execute("UPDATE source_blobs SET bytes=?", [b"altered".as_slice()])
                            .unwrap();
                    }
                    "source_identity" => {
                        c.execute(
                            "UPDATE sources SET data=json_set(data,'$.sha256',?)",
                            [format!("{}€{}", "a".repeat(31), "b".repeat(30))],
                        )
                        .unwrap();
                    }
                    "clock" => {
                        c.execute(
                            "INSERT INTO settings VALUES ('simulated_clock',?)",
                            [json(&"not a timestamp").unwrap()],
                        )
                        .unwrap();
                    }
                    "credential_setting" => {
                        c.execute(
                            "INSERT INTO settings VALUES ('api_key',?)",
                            [json(&"synthetic marker; not a credential").unwrap()],
                        )
                        .unwrap();
                    }
                    "future_schema" => {
                        c.execute_batch("PRAGMA user_version=999;").unwrap();
                    }
                    _ => unreachable!(),
                };
                drop(c);
                let raw = std::fs::read(snapshot).unwrap();
                altered["files"][0]["bytes"] = j!(STANDARD.encode(&raw));
                altered["files"][0]["sha256"] = j!(sha(&raw));
            }
            let mut fresh = fixture(&dir.path().join(format!("fresh-{n}.db")));
            let before = std::fs::read(&fresh.path).unwrap();
            assert!(
                fresh
                    .restore_archive(&serde_json::to_vec(&altered).unwrap(), None)
                    .is_err(),
                "{attack}"
            );
            assert_eq!(before, std::fs::read(&fresh.path).unwrap(), "{attack}");
            assert_eq!(
                fresh
                    .conn
                    .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i32>(0))
                    .unwrap(),
                1
            );
        }
    }
    #[test]
    fn restore_rebuilds_derived_counts_and_removes_abandoned_staging_copies() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = fixture(&dir.path().join("original.db"));
        let sid = d.load_diagnostic("quadratic-update-help_leakage").unwrap();
        let a = d.attempts(&sid).unwrap().pop().unwrap();
        let input = d.prepare_tutor_request(&sid, &a.id).unwrap();
        let r = input.request.clone();
        let out = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(crate::offline::respond(
                input,
                "correct",
                tokio_util::sync::CancellationToken::new(),
            ))
            .unwrap();
        d.settle(&r, "completed", out.usage, Some(&out.proposal.unwrap()))
            .unwrap();
        d.conn.execute("UPDATE learning_summaries SET data=json_set(data,'$.independent_count',999,'$.text','untrusted inflated count')",[]).unwrap();
        let archive = d.export_archive().unwrap();
        let mut fresh = fixture(&dir.path().join("restored.db"));
        fresh.restore_archive(&archive, None).unwrap();
        let summary = fresh.learning_view(&sid).unwrap().unwrap().summary.unwrap();
        assert_eq!(summary.independent_count, 0);
        assert_eq!(summary.supported_count, 1);
        assert!(!summary.text.contains("inflated"));
        let abandoned = dir.path().join(".study-harness-staging-disposable.sqlite3");
        private_file(&abandoned, b"synthetic private staging bytes").unwrap();
        cleanup_staging(dir.path()).unwrap();
        assert!(!abandoned.exists());
        assert!(Path::new(&fresh.path).exists());
    }
}
