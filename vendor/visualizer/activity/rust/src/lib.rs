//! Study Harness contract validation for Rust hosts.
//!
//! Validates records against the same JSON Schema files the TypeScript package emits
//! (`packages/activity/schema/*.schema.json`, embedded at compile time), with draft 2020-12 semantics, no format
//! assertions and the linear-time `regex` engine (schema patterns are ASCII-only). Before schema traversal it applies
//! the same data limits as the TypeScript validator (`jsonDataErrors` in `@explorable-models/core/model`): payload bytes
//! counted identically, nesting depth, node count, and rejection of `__proto__`, `prototype` and `constructor` keys.
//! Shared fixtures in `packages/activity/fixtures/contracts` must produce the same decision in both languages.
//!
//! Semantic cross-reference rules (stage graphs, units, disclosure) are TypeScript-only; a host uses this crate for
//! shape, bounds and authority checks on data it receives. Error strings name the location and the failed keyword
//! only; they never echo input values.

pub mod canonical;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use jsonschema::{PatternOptions, Validator};
use serde_json::Value;

macro_rules! embedded {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../../schema/", $name, ".schema.json")))),*]
    };
}

/// Every embedded schema document, by contract or message name.
pub const SCHEMAS: &[(&str, &str)] = embedded![
    "study-harness.activity-template",
    "study-harness.activity-instance",
    "study-harness.host-policy",
    "study-harness.scientific-snapshot",
    "study-harness.presentation-snapshot",
    "study-harness.interaction-event",
    "study-harness.exposure-record",
    "study-harness.interaction-summary",
    "study-harness.activity-artifact",
    "study-harness.review-record",
    "study-harness.exposure-receipt",
    "study-harness.stage-receipt",
    "study-harness.assistance-receipt",
    "study-harness.capability-record",
    "study-harness.activity-request",
    "study-harness.stage-transition-request",
    "study-harness.exposure-intent-request",
    "study-harness.display-ack-request",
    "study-harness.assistance-request",
    "study-harness.append-events-request",
    "study-harness.append-result",
    "study-harness.journal-cursor",
    "study-harness.register-activity-request",
    "study-harness.save-artifact-request",
    "study-harness.activity-lookup-request",
    "study-harness.host-ack",
    "study-harness.activity-records",
    "explorable-models.model-manifest",
];

/// The generated index (file list, digests and per-contract data limits).
pub const INDEX: &str = include_str!("../../schema/index.json");

/// Keys rejected anywhere in a record (prototype pollution vectors in JavaScript hosts).
pub const RESERVED_KEYS: [&str; 3] = ["__proto__", "prototype", "constructor"];

/// Limits applied before schema traversal; values come from `index.json`. `max_payload_bytes` bounds each event
/// payload (an event on its own or inside an append batch) with the same byte accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLimits {
    pub max_bytes: u64,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_payload_bytes: Option<u64>,
}

struct Contract {
    validator: Validator,
    limits: DataLimits,
}

fn contracts() -> &'static BTreeMap<&'static str, Contract> {
    static CONTRACTS: OnceLock<BTreeMap<&'static str, Contract>> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        let index: Value = serde_json::from_str(INDEX).expect("schema/index.json is valid JSON");
        let limits: BTreeMap<String, DataLimits> = index["contracts"]
            .as_array()
            .expect("index.json lists contracts")
            .iter()
            .map(|entry| {
                let limits = &entry["dataLimits"];
                let number = |key: &str| limits[key].as_u64().unwrap_or_else(|| panic!("dataLimits.{key} is an integer"));
                let name = entry["name"].as_str().expect("contract name").to_owned();
                let limits = DataLimits {
                    max_bytes: number("maxBytes"),
                    max_depth: number("maxDepth") as usize,
                    max_nodes: number("maxNodes") as usize,
                    max_payload_bytes: limits.get("maxPayloadBytes").and_then(Value::as_u64),
                };
                (name, limits)
            })
            .collect();
        SCHEMAS
            .iter()
            .map(|(name, text)| {
                let schema: Value = serde_json::from_str(text).unwrap_or_else(|error| panic!("{name} schema is valid JSON: {error}"));
                let validator = jsonschema::draft202012::options()
                    .should_validate_formats(false)
                    .with_pattern_options(PatternOptions::regex())
                    .build(&schema)
                    .unwrap_or_else(|error| panic!("{name} schema compiles: {error}"));
                let limits = *limits.get(*name).unwrap_or_else(|| panic!("{name} has data limits in index.json"));
                (*name, Contract { validator, limits })
            })
            .collect()
    })
}

/// Names of every embedded contract.
pub fn contract_names() -> Vec<&'static str> {
    SCHEMAS.iter().map(|(name, _)| *name).collect()
}

/// Data limits for a contract, if it is known.
pub fn data_limits(contract: &str) -> Option<DataLimits> {
    contracts().get(contract).map(|entry| entry.limits)
}

/// Mirrors `jsonDataErrors` in `@explorable-models/core/model`: identical byte accounting (strings: UTF-8 length + 2;
/// numbers: 24; null/booleans: 5; object entries: UTF-8 key length + 3; array entries: 1), depth counted from the root
/// at 0, every visited value counted as a node, and reserved keys rejected.
pub fn data_errors(value: &Value, limits: DataLimits) -> Option<String> {
    struct Walk {
        limits: DataLimits,
        bytes: u64,
        nodes: usize,
    }
    impl Walk {
        fn visit(&mut self, value: &Value, path: &str, depth: usize) -> Result<(), String> {
            self.nodes += 1;
            if depth > self.limits.max_depth || self.nodes > self.limits.max_nodes {
                return Err(format!("{path}: nesting or node limit exceeded"));
            }
            match value {
                Value::Null | Value::Bool(_) => self.bytes += 5,
                Value::Number(_) => self.bytes += 24,
                Value::String(text) => {
                    self.bytes += text.len() as u64 + 2;
                    if self.bytes > self.limits.max_bytes {
                        return Err(format!("{path}: payload exceeds {} bytes", self.limits.max_bytes));
                    }
                }
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        self.bytes += 1;
                        self.visit(item, &format!("{path}/{index}"), depth + 1)?;
                    }
                    self.check_bytes(path)?;
                }
                Value::Object(map) => {
                    for (key, item) in map {
                        if RESERVED_KEYS.contains(&key.as_str()) {
                            return Err(format!("{path}: reserved object key"));
                        }
                        self.bytes += key.len() as u64 + 3;
                        self.visit(item, &format!("{path}/{}", escape(key)), depth + 1)?;
                    }
                    self.check_bytes(path)?;
                }
            }
            Ok(())
        }
        fn check_bytes(&self, path: &str) -> Result<(), String> {
            if self.bytes > self.limits.max_bytes {
                Err(format!("{path}: payload exceeds {} bytes", self.limits.max_bytes))
            } else {
                Ok(())
            }
        }
    }
    let mut walk = Walk { limits, bytes: 0, nodes: 0 };
    walk.visit(value, "", 0).err()
}

/// Event payload byte bound for `study-harness.interaction-event` and each event of an append batch.
fn payload_errors(contract: &str, value: &Value, limits: DataLimits) -> Option<String> {
    let max_bytes = limits.max_payload_bytes?;
    let payload_limits = DataLimits { max_bytes, max_payload_bytes: None, ..limits };
    let payloads: Vec<(String, &Value)> = if contract == "study-harness.interaction-event" {
        value.get("payload").map(|payload| ("/payload".to_owned(), payload)).into_iter().collect()
    } else {
        value
            .get("events")
            .and_then(Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| event.get("payload").map(|payload| (format!("/events/{index}/payload"), payload)))
                    .collect()
            })
            .unwrap_or_default()
    };
    payloads.into_iter().find_map(|(path, payload)| data_errors(payload, payload_limits).map(|error| format!("{path}{error}")))
}

/// JSON Pointer escaping for one path segment.
fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Validates `value` as `contract`. Errors name the JSON Pointer location and the failed keyword only.
pub fn validate(contract: &str, value: &Value) -> Result<(), Vec<String>> {
    let Some(entry) = contracts().get(contract) else {
        return Err(vec!["unknown contract name".to_owned()]);
    };
    if let Some(error) = data_errors(value, entry.limits) {
        return Err(vec![error]);
    }
    if let Some(error) = payload_errors(contract, value, entry.limits) {
        return Err(vec![error]);
    }
    let errors: Vec<String> =
        entry.validator.iter_errors(value).take(32).map(|error| format!("{}: {}", error.instance_path().as_str(), error.kind().keyword())).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validates that `bytes` are JSON and a valid `contract`.
pub fn validate_bytes(contract: &str, bytes: &[u8]) -> Result<Value, Vec<String>> {
    // Bound raw IPC bytes before JSON allocation/traversal. Whitespace and escaped strings count too.
    let limits = data_limits(contract).ok_or_else(|| vec!["unknown contract name".to_owned()])?;
    if bytes.len() as u64 > limits.max_bytes {
        return Err(vec!["input exceeds raw byte limit".to_owned()]);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| vec!["input is not valid JSON".to_owned()])?;
    validate(contract, &value).map(|()| value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_schema_compiles_and_matches_the_index() {
        let index: Value = serde_json::from_str(INDEX).unwrap();
        let indexed: Vec<&str> = index["contracts"].as_array().unwrap().iter().map(|entry| entry["name"].as_str().unwrap()).collect();
        let mut embedded = contract_names();
        let mut expected = indexed.clone();
        embedded.sort_unstable();
        expected.sort_unstable();
        assert_eq!(embedded, expected, "embedded schema list must equal schema/index.json");
        for name in indexed {
            assert!(data_limits(name).is_some());
        }
    }

    #[test]
    fn oversized_raw_bytes_are_refused_before_parsing() {
        let limit = data_limits("study-harness.activity-lookup-request").unwrap().max_bytes;
        let bytes = vec![b' '; limit as usize + 1];
        assert_eq!(validate_bytes("study-harness.activity-lookup-request", &bytes).unwrap_err(), vec!["input exceeds raw byte limit"]);
        assert_eq!(validate_bytes("unknown", b"{}").unwrap_err(), vec!["unknown contract name"]);
    }

    #[test]
    fn unknown_contract_is_rejected() {
        assert!(validate("study-harness.unknown", &json!({})).is_err());
    }

    #[test]
    fn reserved_keys_and_depth_are_rejected_before_schema() {
        let limits = DataLimits { max_bytes: 1024, max_depth: 2, max_nodes: 100, max_payload_bytes: None };
        assert!(data_errors(&json!({"constructor": 1}), limits).is_some());
        assert!(data_errors(&json!({"a": {"b": {"c": 1}}}), limits).is_some());
        assert!(data_errors(&json!({"a": {"b": 1}}), limits).is_none());
        let bytes = DataLimits { max_bytes: 10, max_depth: 8, max_nodes: 100, max_payload_bytes: None };
        assert!(data_errors(&json!("12345678"), bytes).is_none());
        assert!(data_errors(&json!("123456789"), bytes).is_some());
    }

    #[test]
    fn errors_do_not_echo_values() {
        let secret = "hidden-solution-text-42";
        let errors = validate(
            "study-harness.journal-cursor",
            &json!({"activityId": secret, "lastSequence": secret, "lastEventId": null, "checkpointDigest": null}),
        )
        .unwrap_err();
        assert!(errors.iter().all(|error| !error.contains(secret)), "{errors:?}");
    }
}
