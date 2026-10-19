//! The local ledger: one directory of append-only JSONL files, one canonical JSON record per line.
//!
//! Every record is sealed before it is written: `record_sha256` is recomputed with the core's
//! `record_sha256` over the canonical JSON without that key, and the line is the core's
//! `canonical_json`. `verify` re-reads every line of every file and reports `file:line` for any
//! line whose hash, canonical form or JSON no longer holds.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use tier_drop_core::{canonical_json, record_sha256};

use crate::CliError;

/// The six ledger files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// `tasks.jsonl`
    Tasks,
    /// `attempts.jsonl`
    Attempts,
    /// `decisions.jsonl`
    Decisions,
    /// `disqualifications.jsonl`
    Disqualifications,
    /// `eval-runs.jsonl`
    EvalRuns,
    /// `eval-splits.jsonl`
    EvalSplits,
}

impl Kind {
    /// Every kind, in the order `verify` and `show` report them.
    pub const ALL: [Kind; 6] = [
        Kind::Tasks,
        Kind::Attempts,
        Kind::Decisions,
        Kind::Disqualifications,
        Kind::EvalRuns,
        Kind::EvalSplits,
    ];

    /// The name used by `--kind` and as the key in `ledger show` output.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Tasks => "tasks",
            Kind::Attempts => "attempts",
            Kind::Decisions => "decisions",
            Kind::Disqualifications => "disqualifications",
            Kind::EvalRuns => "eval-runs",
            Kind::EvalSplits => "eval-splits",
        }
    }

    /// The file name inside the ledger directory.
    pub fn file_name(self) -> String {
        format!("{}.jsonl", self.name())
    }
}

/// A ledger directory.
pub struct Ledger {
    dir: PathBuf,
}

/// Seal a record: serialise it and set `record_sha256` over its canonical JSON without that key.
// decision: a caller-supplied `record_sha256` is always overwritten, so the ledger never stores a
// hash it did not compute itself.
pub fn seal<T: Serialize>(record: &T) -> Result<Value, CliError> {
    let mut value = serde_json::to_value(record)
        .map_err(|e| CliError::new("serialise_failed", e.to_string()))?;
    let hash = record_sha256(&value);
    match value.as_object_mut() {
        Some(map) => {
            map.insert("record_sha256".to_string(), Value::String(hash));
        }
        None => {
            return Err(CliError::new(
                "not_a_record",
                "ledger records must be JSON objects".to_string(),
            ))
        }
    }
    Ok(value)
}

impl Ledger {
    /// A ledger at `dir` (created on the first write).
    pub fn new(dir: &Path) -> Ledger {
        Ledger {
            dir: dir.to_path_buf(),
        }
    }

    fn path(&self, kind: Kind) -> PathBuf {
        self.dir.join(kind.file_name())
    }

    /// Seal `record` and append it as one canonical line; returns the sealed value.
    pub fn append<T: Serialize>(&self, kind: Kind, record: &T) -> Result<Value, CliError> {
        let sealed = seal(record)?;
        let mut line = canonical_json(&sealed);
        line.push('\n');
        fs::create_dir_all(&self.dir).map_err(|e| {
            CliError::new(
                "ledger_write_failed",
                format!("{}: {e}", self.dir.display()),
            )
        })?;
        let path = self.path(kind);
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| {
                CliError::new("ledger_write_failed", format!("{}: {e}", path.display()))
            })?;
        // One write call per record so concurrent appenders interleave whole lines.
        file.write_all(line.as_bytes()).map_err(|e| {
            CliError::new("ledger_write_failed", format!("{}: {e}", path.display()))
        })?;
        Ok(sealed)
    }

    /// Every non-empty line of a file with its 1-based line number; a missing file has none.
    fn lines(&self, kind: Kind) -> Result<Vec<(usize, String)>, CliError> {
        let path = self.path(kind);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(CliError::new(
                    "ledger_read_failed",
                    format!("{}: {e}", path.display()),
                ))
            }
        };
        Ok(text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, l)| (i + 1, l.to_string()))
            .collect())
    }

    /// Every record of a file as JSON values.
    pub fn values(&self, kind: Kind) -> Result<Vec<Value>, CliError> {
        self.lines(kind)?
            .into_iter()
            .map(|(n, l)| {
                serde_json::from_str(&l).map_err(|e| {
                    CliError::new(
                        "ledger_line_unreadable",
                        format!("{}:{n}: {e}", kind.file_name()),
                    )
                })
            })
            .collect()
    }

    /// Every record of a file as typed records.
    pub fn read<T: DeserializeOwned>(&self, kind: Kind) -> Result<Vec<T>, CliError> {
        self.lines(kind)?
            .into_iter()
            .map(|(n, l)| {
                serde_json::from_str(&l).map_err(|e| {
                    CliError::new(
                        "ledger_line_unreadable",
                        format!("{}:{n}: {e}", kind.file_name()),
                    )
                })
            })
            .collect()
    }

    /// Whether any record of `kind` has `field == value`.
    pub fn contains(&self, kind: Kind, field: &str, value: &str) -> Result<bool, CliError> {
        Ok(self
            .values(kind)?
            .iter()
            .any(|v| v.get(field).and_then(Value::as_str) == Some(value)))
    }

    /// Re-read every line of every file and report each line that no longer verifies.
    pub fn verify(&self) -> Result<Value, CliError> {
        if !self.dir.is_dir() {
            return Err(CliError::new(
                "ledger_not_found",
                format!("{} is not a directory", self.dir.display()),
            ));
        }
        let mut files = Vec::new();
        let mut mismatches = Vec::new();
        for kind in Kind::ALL {
            let lines = self.lines(kind)?;
            files.push(json!({ "file": kind.file_name(), "lines": lines.len() }));
            for (n, line) in lines {
                if let Some((reason, detail)) = check_line(&line) {
                    mismatches.push(json!({
                        "location": format!("{}:{n}", kind.file_name()),
                        "file": kind.file_name(),
                        "line": n,
                        "reason": reason,
                        "detail": detail,
                    }));
                }
            }
        }
        Ok(json!({ "ok": mismatches.is_empty(), "files": files, "mismatches": mismatches }))
    }

    /// Records of the selected kinds, optionally only those of one family, keyed by kind name.
    pub fn show(&self, kind: Option<Kind>, family: Option<&str>) -> Result<Value, CliError> {
        let mut out = Map::new();
        let kinds: Vec<Kind> = match kind {
            Some(k) => vec![k],
            None => Kind::ALL.to_vec(),
        };
        for k in kinds {
            let records: Vec<Value> = self
                .values(k)?
                .into_iter()
                .filter(|v| family.is_none_or(|f| family_matches(v, f)))
                .collect();
            out.insert(k.name().to_string(), Value::Array(records));
        }
        Ok(Value::Object(out))
    }
}

/// Whether a record belongs to `family`. An eval split over mixed families stores them joined
/// with `,`, so any member matches.
fn family_matches(record: &Value, family: &str) -> bool {
    record
        .get("family")
        .and_then(Value::as_str)
        .is_some_and(|f| f == family || f.split(',').any(|p| p == family))
}

/// The reason a ledger line fails verification, or None when it verifies.
fn check_line(line: &str) -> Option<(&'static str, String)> {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(("unparseable", e.to_string())),
    };
    let Some(map) = value.as_object() else {
        return Some(("not_a_record", "line is not a JSON object".to_string()));
    };
    if !map.get("schema_version").is_some_and(Value::is_string) {
        return Some(("missing_schema_version", String::new()));
    }
    let Some(stored) = map.get("record_sha256").and_then(Value::as_str) else {
        return Some(("missing_record_sha256", String::new()));
    };
    let computed = record_sha256(&value);
    if stored != computed {
        return Some((
            "record_sha256_mismatch",
            format!("stored {stored}, computed {computed}"),
        ));
    }
    // decision: a line that hashes correctly but is not in canonical form was still edited
    // (the ledger only writes canonical lines), so it is reported too.
    if canonical_json(&value) != line {
        return Some((
            "not_canonical",
            "line differs from its canonical JSON".to_string(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_check_line_detects_edits() {
        let sealed = seal(&json!({"schema_version": "x/1", "a": 1})).unwrap();
        let line = canonical_json(&sealed);
        assert_eq!(check_line(&line), None);
        let edited = line.replace("\"a\":1", "\"a\":2");
        assert_eq!(check_line(&edited).unwrap().0, "record_sha256_mismatch");
        let spaced = line.replacen(',', ", ", 1);
        assert_eq!(check_line(&spaced).unwrap().0, "not_canonical");
        assert_eq!(check_line("{").unwrap().0, "unparseable");
    }

    #[test]
    fn ledger_lines_with_computed_floats_verify_after_rereading() {
        // Means of metrics are the values the default float parser can misread by one ulp.
        for i in 0..2000u32 {
            let q = (f64::from(i) * 0.37 + 0.92 + 1.0) / 3.0;
            let sealed = seal(&json!({"schema_version": "x/1", "quality": q})).unwrap();
            assert_eq!(check_line(&canonical_json(&sealed)), None, "value {q}");
        }
    }

    #[test]
    fn ledger_family_match_includes_mixed_eval_splits() {
        assert!(family_matches(&json!({"family": "a,b"}), "b"));
        assert!(!family_matches(&json!({"family": "ab"}), "a"));
        assert!(!family_matches(&json!({}), "a"));
    }
}
