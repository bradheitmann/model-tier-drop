//! Canonical JSON and record hashing.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lower-case hex of a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// `prefix` followed by the first 16 hex characters of the SHA-256 of `material`.
pub(crate) fn short_id(prefix: &str, material: &str) -> String {
    let h = sha256_hex(material.as_bytes());
    format!("{prefix}{}", &h[..16])
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // A string key always serialises.
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(v, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(v, out);
            }
            out.push(']');
        }
        // Scalars: serde_json's compact form.
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// Compact JSON with object keys sorted recursively.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

/// SHA-256 (lower-case hex) of the canonical JSON of `value` without its top-level `record_sha256` key.
pub fn record_sha256(value: &Value) -> String {
    match value {
        Value::Object(map) if map.contains_key("record_sha256") => {
            let mut m = map.clone();
            m.remove("record_sha256");
            sha256_hex(canonical_json(&Value::Object(m)).as_bytes())
        }
        other => sha256_hex(canonical_json(other).as_bytes()),
    }
}

/// `record_sha256` of any serialisable record.
pub(crate) fn seal<T: Serialize>(record: &T) -> String {
    // decision: the record types here contain only strings, numbers, bools, maps with
    // string keys and sequences, so serialisation to a Value cannot fail; Null is a
    // defensive fallback that still yields a stable hash.
    let v = serde_json::to_value(record).unwrap_or(Value::Null);
    record_sha256(&v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_keys_recursively_and_is_compact() {
        let v = json!({"b": 1, "a": {"z": [ {"y": 1, "x": 2} ], "c": null}});
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":null,"z":[{"x":2,"y":1}]},"b":1}"#
        );
    }

    #[test]
    fn record_sha256_ignores_its_own_field() {
        let a = json!({"x": 1, "record_sha256": "anything"});
        let b = json!({"x": 1});
        assert_eq!(record_sha256(&a), record_sha256(&b));
        assert_eq!(record_sha256(&b), sha256_hex(br#"{"x":1}"#));
        assert_ne!(record_sha256(&b), record_sha256(&json!({"x": 2})));
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
