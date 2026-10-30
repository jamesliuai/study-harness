//! Canonical JSON identical to `canonicalJSON` in `@explorable-models/core`: object keys sorted by UTF-16 code units,
//! arrays in order, strings escaped like `JSON.stringify`, numbers formatted like ECMAScript `Number::toString`.
//! Artifact integrity entries and `artifactDigest` computed here must equal the TypeScript values; the host verifies
//! every saved artifact's integrity manifest with it, so a formatting difference would be rejected, not hidden.

use serde_json::{Number, Value};
use sha2::{Digest, Sha256};

pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&js_number(number)),
        Value::String(text) => out.push_str(&serde_json::to_string(text).unwrap_or_default()),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push(':');
                write(&map[*key], out);
            }
            out.push('}');
        }
    }
}

/// ECMAScript `Number::toString(10)` for a finite JSON number.
pub fn js_number(number: &Number) -> String {
    if let Some(integer) = number.as_i64() {
        if integer.unsigned_abs() < 1_000_000_000_000_000_000 {
            return integer.to_string();
        }
    }
    if let Some(integer) = number.as_u64() {
        if integer < 1_000_000_000_000_000_000 {
            return integer.to_string();
        }
    }
    js_float(number.as_f64().unwrap_or(0.0))
}

fn js_float(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return "0".to_owned();
    }
    let negative = value < 0.0;
    // Rust's `{:e}` prints the shortest round-trip digits, like the ECMAScript algorithm requires.
    let formatted = format!("{:e}", value.abs());
    let (mantissa, exponent) = formatted.split_once('e').unwrap_or((&formatted, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = exponent + 1;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-n) as usize));
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if n >= 1 { '+' } else { '-' });
        out.push_str(&(n - 1).abs().to_string());
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `'sha256:' + sha256(canonicalJSON(value))`, the identity used by `@explorable-models/activity`.
pub fn digest(value: &Value) -> String {
    format!("sha256:{}", sha256_hex(canonical_json(value).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number(text: &str) -> String {
        let value: Value = serde_json::from_str(text).unwrap();
        canonical_json(&value)
    }

    #[test]
    fn numbers_follow_ecmascript_number_to_string() {
        for (input, expected) in [
            ("0", "0"),
            ("-0.0", "0"),
            ("3", "3"),
            ("-17", "-17"),
            ("0.75", "0.75"),
            ("0.1", "0.1"),
            ("1.5e-7", "1.5e-7"),
            ("0.000001", "0.000001"),
            ("1e21", "1e+21"),
            ("123456789012345680000", "123456789012345680000"),
            ("2.5e300", "2.5e+300"),
            ("5e-324", "5e-324"),
            ("-0.5", "-0.5"),
            ("1234.5678", "1234.5678"),
            ("0.30000000000000004", "0.30000000000000004"),
            ("100", "100"),
            ("1e2", "100"),
        ] {
            assert_eq!(number(input), expected, "{input}");
        }
    }

    #[test]
    fn keys_are_sorted_and_strings_escaped_like_json_stringify() {
        let value = json!({ "b": 1, "a": [true, null, "x\"y\n\u{1}"], "B": { "z": 0.25, "Z": -1 } });
        assert_eq!(canonical_json(&value), r#"{"B":{"Z":-1,"z":0.25},"a":[true,null,"x\"y\n\u0001"],"b":1}"#);
    }

    #[test]
    fn digest_matches_the_typescript_identity_of_a_known_value() {
        // sha256 of the canonical text `{"a":1}`.
        assert_eq!(digest(&json!({ "a": 1 })), "sha256:015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862");
    }
}
