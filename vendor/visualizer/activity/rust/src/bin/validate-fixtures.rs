//! Prints the Rust decision for every shared contract fixture as JSON: `[{"file", "contract", "expect", "valid"}]`.
//! Usage: `validate-fixtures [fixture-root]` (default `packages/activity/fixtures/contracts`).

#[path = "../fixtures.rs"]
mod fixtures;

use serde_json::json;

fn main() {
    let root = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(fixtures::default_root);
    match fixtures::decide_all(&root) {
        Ok(decisions) => {
            let output: Vec<_> = decisions
                .iter()
                .map(|decision| json!({ "file": decision.file, "contract": decision.contract, "expect": decision.expect, "valid": decision.valid }))
                .collect();
            println!("{}", serde_json::to_string_pretty(&output).expect("decisions serialize"));
            let mismatched = decisions.iter().filter(|decision| (decision.expect == "valid") != decision.valid).count();
            eprintln!("{} fixtures, {} differ from their declared expectation", decisions.len(), mismatched);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
