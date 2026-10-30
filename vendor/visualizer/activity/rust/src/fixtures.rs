//! Shared-fixture runner used by the `validate-fixtures` binary and the integration test (not part of the library).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One fixture decision.
pub struct Decision {
    /// Path relative to the fixture root, with `/` separators (for example `valid/template/quadratic-2d.json`).
    pub file: String,
    pub contract: String,
    pub expect: String,
    pub valid: bool,
}

/// Default fixture root: `packages/activity/fixtures/contracts`.
pub fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/contracts")
}

fn walk(directory: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "json") {
            found.push(path);
        }
    }
    Ok(())
}

/// Validates every fixture under `root` with the Rust validator, sorted by relative path.
pub fn decide_all(root: &Path) -> Result<Vec<Decision>, String> {
    let mut files = Vec::new();
    walk(root, &mut files).map_err(|error| format!("cannot read fixtures at {}: {error}", root.display()))?;
    files.sort();
    files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let text = fs::read_to_string(path).map_err(|error| format!("{relative}: {error}"))?;
            let fixture: Value = serde_json::from_str(&text).map_err(|error| format!("{relative}: invalid JSON: {error}"))?;
            let contract = fixture["contract"].as_str().ok_or_else(|| format!("{relative}: missing contract"))?.to_owned();
            let expect = fixture["expect"].as_str().ok_or_else(|| format!("{relative}: missing expect"))?.to_owned();
            let valid = study_harness_contracts::validate(&contract, &fixture["value"]).is_ok();
            Ok(Decision { file: relative, contract, expect, valid })
        })
        .collect()
}
