//! Integration tests for the `tier-drop` binary over temporary ledger directories.
//!
//! The walkthrough test reads the `## Walkthrough` block of the repository README and runs it
//! line by line, so the README and the test cannot drift apart.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

const README: &str = include_str!("../../../README.md");
const BIN: &str = env!("CARGO_BIN_EXE_tier-drop");

/// A unique temporary directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "tier-drop-cli-{name}-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run the binary in `dir` with `args`, optionally feeding `stdin_file` (relative to `dir`).
fn run(dir: &Path, args: &[&str], stdin_file: Option<&str>) -> Output {
    let stdin = match stdin_file {
        Some(f) => Stdio::from(std::fs::File::open(dir.join(f)).unwrap()),
        None => Stdio::null(),
    };
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .stdin(stdin)
        .output()
        .unwrap()
}

fn stdout_json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&o.stdout)
        )
    })
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// One step of the README walkthrough.
enum Step {
    /// `cat > FILE <<'EOF'` … `EOF`.
    Write { file: String, body: String },
    /// `tier-drop ARGS [< IN] [> OUT]  # exits N`.
    Run {
        line: String,
        args: Vec<String>,
        stdin: Option<String>,
        stdout: Option<String>,
        expect: i32,
    },
}

/// The shell block under `## Walkthrough`, parsed into steps.
fn walkthrough_steps() -> Vec<Step> {
    let section = README
        .split("\n## Walkthrough\n")
        .nth(1)
        .expect("README has a ## Walkthrough section");
    let block = section
        .split("```sh\n")
        .nth(1)
        .and_then(|b| b.split("\n```").next())
        .expect("the Walkthrough section has a ```sh block");
    let mut steps = Vec::new();
    let mut it = block.lines();
    while let Some(line) = it.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("cat > ") {
            let file = rest
                .strip_suffix(" <<'EOF'")
                .expect("heredoc form `cat > FILE <<'EOF'`")
                .to_string();
            let mut body = String::new();
            for l in it.by_ref() {
                if l == "EOF" {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            steps.push(Step::Write { file, body });
            continue;
        }
        let (command, comment) = match trimmed.split_once("  # ") {
            Some((c, m)) => (c.trim(), Some(m)),
            None => (trimmed, None),
        };
        let expect = comment
            .and_then(|m| m.strip_prefix("exits "))
            .and_then(|m| m.chars().next())
            .and_then(|c| c.to_digit(10))
            .map_or(0, |d| d as i32);
        let mut words = command.split_whitespace();
        assert_eq!(words.next(), Some("tier-drop"), "unsupported line: {line}");
        let (mut args, mut stdin, mut stdout) = (Vec::new(), None, None);
        while let Some(w) = words.next() {
            match w {
                "<" => stdin = words.next().map(str::to_string),
                ">" => stdout = words.next().map(str::to_string),
                _ => args.push(w.to_string()),
            }
        }
        steps.push(Step::Run {
            line: command.to_string(),
            args,
            stdin,
            stdout,
            expect,
        });
    }
    steps
}

/// Run the README walkthrough in `dir`; returns each command line with its output.
fn run_walkthrough(dir: &Path) -> Vec<(String, Output)> {
    let mut outputs = Vec::new();
    for step in walkthrough_steps() {
        match step {
            Step::Write { file, body } => std::fs::write(dir.join(file), body).unwrap(),
            Step::Run {
                line,
                args,
                stdin,
                stdout,
                expect,
            } => {
                let argv: Vec<&str> = args.iter().map(String::as_str).collect();
                let o = run(dir, &argv, stdin.as_deref());
                assert_eq!(
                    code(&o),
                    expect,
                    "`{line}` exited {}; stdout {} stderr {}",
                    code(&o),
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                if let Some(f) = stdout {
                    std::fs::write(dir.join(f), &o.stdout).unwrap();
                }
                outputs.push((line, o));
            }
        }
    }
    outputs
}

/// Only the file-writing steps of the walkthrough plus the registry: a task, two attempts.
fn setup_files(dir: &Path) {
    let o = run(dir, &["registry", "example"], None);
    assert_eq!(code(&o), 0);
    std::fs::write(dir.join("registry.json"), &o.stdout).unwrap();
    for step in walkthrough_steps() {
        if let Step::Write { file, body } = step {
            std::fs::write(dir.join(file), body).unwrap();
        }
    }
}

fn output_of<'a>(outputs: &'a [(String, Output)], prefix: &str) -> &'a Output {
    &outputs
        .iter()
        .rev()
        .find(|(l, _)| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no step starting with `{prefix}`"))
        .1
}

const RECORD: [&str; 8] = [
    "attempt",
    "record",
    "--ledger",
    "ledger",
    "--task",
    "task.json",
    "--registry",
    "registry.json",
];

#[test]
fn walkthrough_from_readme_runs_end_to_end() {
    let tmp = TempDir::new("walkthrough");
    let dir = tmp.path();
    let outputs = run_walkthrough(dir);

    let plan = stdout_json(output_of(&outputs, "tier-drop plan"));
    assert_eq!(plan["kind"], "try");
    assert_eq!(plan["tier"], "system_one");
    assert!(!plan["candidates"].as_array().unwrap().is_empty());

    let decision = stdout_json(output_of(&outputs, "tier-drop decide"));
    assert!(!decision["winner"].is_null(), "winner is null: {decision}");
    assert_eq!(decision["winner"]["candidate"]["model_id"], "jev");

    // The task was appended once although two attempt records and decide each saw it.
    assert_eq!(lines(&dir.join("ledger/tasks.jsonl")).len(), 1);
    assert_eq!(lines(&dir.join("ledger/attempts.jsonl")).len(), 2);
    let decisions = lines(&dir.join("ledger/decisions.jsonl"));
    assert_eq!(decisions.len(), 1);
    let stored: Value = serde_json::from_str(&decisions[0]).unwrap();
    assert!(!stored["winner"].is_null());
    assert_eq!(stored["schema_version"], "tier-drop.routing_decision/1");
    assert_eq!(stored["record_sha256"].as_str().unwrap().len(), 64);

    let verify = stdout_json(output_of(&outputs, "tier-drop ledger verify"));
    assert_eq!(verify["ok"], true);

    // The abstention was accepted and is a dataset row for processing.
    let abstained = output_of(&outputs, "tier-drop attempt record --ledger ledger --task task.json --registry registry.json < attempt-2.json");
    let fr = stdout_json(abstained);
    assert_eq!(fr["floor_result"]["passed"], false);
    let process = run(
        dir,
        &[
            "dataset",
            "export",
            "--ledger",
            "ledger",
            "--purpose",
            "process",
        ],
        None,
    );
    assert_eq!(code(&process), 0);
    let rows: Vec<Value> = String::from_utf8_lossy(&process.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r["outcome"] == "abstained"));
    // Generated ids are 26-character ULIDs.
    assert!(rows
        .iter()
        .all(|r| r["attempt_id"].as_str().unwrap().len() == 26));
}

#[test]
fn ledger_verify_names_the_tampered_attempt_line() {
    let tmp = TempDir::new("tamper");
    let dir = tmp.path();
    run_walkthrough(dir);
    let path = dir.join("ledger/attempts.jsonl");
    let mut bytes = std::fs::read(&path).unwrap();
    // Change one digit of the second line's latency (55 -> 65).
    let second_start = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    let needle = b"\"latency_ms\":55";
    let at = bytes[second_start..]
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
        + second_start
        + needle.len()
        - 2;
    bytes[at] = b'6';
    std::fs::write(&path, bytes).unwrap();

    let o = run(dir, &["ledger", "verify", "--ledger", "ledger"], None);
    assert_eq!(code(&o), 1);
    let report = stdout_json(&o);
    assert_eq!(report["ok"], false);
    let ms = report["mismatches"].as_array().unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0]["location"], "attempts.jsonl:2");
    assert!(String::from_utf8_lossy(&o.stderr).contains("attempts.jsonl:2"));
}

#[test]
fn schemas_lists_eleven_and_names_routing_decision() {
    let tmp = TempDir::new("schemas");
    let dir = tmp.path();
    let one = run(dir, &["schemas", "routing_decision"], None);
    assert_eq!(code(&one), 0);
    assert_eq!(stdout_json(&one)["title"], "RoutingDecision");
    let adjacent = run(dir, &["schemas", "adjacent_moves"], None);
    assert_eq!(code(&adjacent), 0);
    assert_eq!(stdout_json(&adjacent)["title"], "AdjacentMoves");
    let all = run(dir, &["schemas"], None);
    assert_eq!(code(&all), 0);
    let keys: Vec<String> = stdout_json(&all)
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        vec![
            "adjacent_moves",
            "attempt",
            "disqualification",
            "eval_item",
            "eval_run",
            "eval_split",
            "plan",
            "policy",
            "registry",
            "routing_decision",
            "task"
        ]
    );
    assert_eq!(keys.len(), 11);
    assert_eq!(code(&run(dir, &["schemas", "nope"], None)), 2);
}

#[test]
fn adjacent_lists_moves_over_the_example_registry() {
    let tmp = TempDir::new("adjacent");
    let dir = tmp.path();
    setup_files(dir);
    std::fs::write(
        dir.join("from.json"),
        r#"{ "model_id": "jev", "reasoning_level": "none" }"#,
    )
    .unwrap();
    let o = run(
        dir,
        &[
            "adjacent",
            "--from",
            "from.json",
            "--task",
            "task.json",
            "--registry",
            "registry.json",
            "--ledger",
            "ledger",
        ],
        None,
    );
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let a = stdout_json(&o);
    assert_eq!(a["schema_version"], "tier-drop.adjacent_moves/1");
    assert_eq!(a["task_id"], "ticket-4711");
    assert_eq!(a["family"], "ticket.priority");
    assert_eq!(a["from"]["tier"], "system_one");
    assert_eq!(
        a["ladder"],
        serde_json::json!(["system_one", "cheap_generalist"])
    );
    assert_eq!(a["issues"], serde_json::json!([]));
    let moves = a["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 3);
    // another lineage at the same tier comes before the tier above
    assert_eq!(moves[0]["kind"], "same_tier_other_lineage");
    assert_eq!(moves[0]["to"]["model_id"], "laya");
    assert_eq!(moves[1]["kind"], "one_tier_up");
    assert_eq!(moves[1]["to"]["model_id"], "qwen-3.8-27b");
    assert_eq!(moves[1]["to"]["reasoning_level"], "none");
    assert_eq!(
        moves[1]["passed_over"],
        serde_json::json!([{ "tier": "specialised", "reason": "no_fitting_candidates" }])
    );
    assert_eq!(moves[2]["to"]["reasoning_level"], "thinking");
    // the walkthrough task forbids remote calls, so no remote model is a target
    for m in moves {
        assert_ne!(m["to"]["model_id"], "deepseek-4.1-flash");
        assert_ne!(m["to"]["model_id"], "opus-5.5");
    }

    // an unknown model is a validation failure: exit 1 with the issue described
    std::fs::write(
        dir.join("from-unknown.json"),
        r#"{ "model_id": "nope", "reasoning_level": "none" }"#,
    )
    .unwrap();
    let bad = run(
        dir,
        &[
            "adjacent",
            "--from",
            "from-unknown.json",
            "--task",
            "task.json",
            "--registry",
            "registry.json",
        ],
        None,
    );
    assert_eq!(code(&bad), 1);
    let b = stdout_json(&bad);
    assert_eq!(b["issues"][0]["code"], "unknown_model");
    assert_eq!(b["moves"], serde_json::json!([]));
}

#[test]
fn registry_validate_accepts_the_example_with_its_lineages() {
    let tmp = TempDir::new("registry-lineage");
    let dir = tmp.path();
    let o = run(dir, &["registry", "example"], None);
    assert_eq!(code(&o), 0);
    std::fs::write(dir.join("registry.json"), &o.stdout).unwrap();
    let v = run(dir, &["registry", "validate", "registry.json"], None);
    assert_eq!(code(&v), 0, "{}", String::from_utf8_lossy(&v.stdout));
    assert_eq!(stdout_json(&v)["valid"], true);
    let registry = stdout_json(&o);
    for m in registry["models"].as_array().unwrap() {
        assert!(
            m["lineage"].as_str().is_some_and(|l| !l.is_empty()),
            "model {} has no lineage",
            m["id"]
        );
    }
}

#[test]
fn exit_codes_ok_validation_and_usage() {
    let tmp = TempDir::new("exit");
    let dir = tmp.path();
    setup_files(dir);
    assert_eq!(
        code(&run(dir, &["registry", "validate", "registry.json"], None)),
        0
    );

    // An attempt recorded before the task was created: exit 1, issues, nothing written.
    let a: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("attempt-1.json")).unwrap())
            .unwrap();
    let mut early = a.clone();
    early["recorded_at"] = Value::String("2026-10-19T08:59:59Z".into());
    std::fs::write(dir.join("early.json"), early.to_string()).unwrap();
    let o = run(dir, &RECORD, Some("early.json"));
    assert_eq!(code(&o), 1);
    let out = stdout_json(&o);
    assert_eq!(out["ok"], false);
    assert!(out["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["code"] == "recorded_before_task_created"));
    assert!(!dir.join("ledger/attempts.jsonl").exists());
    assert!(!dir.join("ledger/tasks.jsonl").exists());

    // A passing attempt: exit 0. Recording the same attempt id again: exit 1.
    let mut ok = a.clone();
    ok["attempt_id"] = Value::String("a-1".into());
    std::fs::write(dir.join("ok.json"), ok.to_string()).unwrap();
    assert_eq!(code(&run(dir, &RECORD, Some("ok.json"))), 0);
    let dup = run(dir, &RECORD, Some("ok.json"));
    assert_eq!(code(&dup), 1);
    assert!(stdout_json(&dup)["issues"][0]["code"] == "duplicate_attempt_id");

    // Usage errors: exit 2.
    assert_eq!(
        code(&run(dir, &["plan", "--bogus", "--task", "task.json"], None)),
        2
    );
    assert_eq!(code(&run(dir, &["plan"], None)), 2);
    assert_eq!(
        code(&run(
            dir,
            &[
                "dataset",
                "export",
                "--ledger",
                "ledger",
                "--purpose",
                "sell"
            ],
            None
        )),
        2
    );
}

#[test]
fn dataset_export_train_omits_attempt_without_train_right() {
    let tmp = TempDir::new("dataset");
    let dir = tmp.path();
    setup_files(dir);
    let a: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("attempt-1.json")).unwrap())
            .unwrap();
    let mut no_train = a.clone();
    no_train["attempt_id"] = Value::String("a-no-train".into());
    no_train["rights"]["train"] = Value::Bool(false);
    let mut yes_train = a;
    yes_train["attempt_id"] = Value::String("a-train".into());
    yes_train["candidate"]["model_id"] = Value::String("laya".into());
    std::fs::write(dir.join("no-train.json"), no_train.to_string()).unwrap();
    std::fs::write(dir.join("train.json"), yes_train.to_string()).unwrap();
    assert_eq!(code(&run(dir, &RECORD, Some("no-train.json"))), 0);
    assert_eq!(code(&run(dir, &RECORD, Some("train.json"))), 0);

    let ids = |purpose: &str| -> Vec<String> {
        let o = run(
            dir,
            &[
                "dataset",
                "export",
                "--ledger",
                "ledger",
                "--purpose",
                purpose,
            ],
            None,
        );
        assert_eq!(code(&o), 0);
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["attempt_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    assert_eq!(ids("train"), vec!["a-train"]);
    assert_eq!(ids("process"), vec!["a-no-train", "a-train"]);
}

#[test]
fn expired_disqualification_does_not_block_plan_and_future_one_does() {
    let disqualify = |dir: &Path, expires: &str| {
        let o = run(
            dir,
            &[
                "disqualify",
                "--ledger",
                "ledger",
                "--registry",
                "registry.json",
                "--family",
                "ticket.priority",
                "--tier",
                "system_one",
                "--reason",
                "operator",
                "--evidence",
                "a-1,a-0",
                "--detail",
                "operator review",
                "--expires-at",
                expires,
            ],
            None,
        );
        assert_eq!(
            code(&o),
            0,
            "disqualify failed: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        stdout_json(&o)
    };
    let plan = |dir: &Path| {
        let o = run(
            dir,
            &[
                "plan",
                "--task",
                "task.json",
                "--registry",
                "registry.json",
                "--ledger",
                "ledger",
            ],
            None,
        );
        assert_eq!(code(&o), 0);
        stdout_json(&o)
    };

    let expired = TempDir::new("dq-expired");
    setup_files(expired.path());
    let record = disqualify(expired.path(), "2000-01-01T00:00:00Z");
    assert_eq!(record["evidence"], serde_json::json!(["a-0", "a-1"]));
    assert_eq!(record["registry_as_of"], "2026-10-19");
    assert!(record["disqualification_id"]
        .as_str()
        .unwrap()
        .starts_with("dq-"));
    assert_eq!(plan(expired.path())["tier"], "system_one");

    let future = TempDir::new("dq-future");
    setup_files(future.path());
    let record = disqualify(future.path(), "2999-01-01T00:00:00Z");
    let p = plan(future.path());
    assert_ne!(p["tier"], "system_one");
    let skipped = p["skipped"].as_array().unwrap();
    assert!(skipped.iter().any(|s| s["tier"] == "system_one"
        && s["reason"] == "disqualified"
        && s["detail"]
            .as_str()
            .unwrap()
            .contains(record["disqualification_id"].as_str().unwrap())));

    // A malformed expiry is a usage error.
    let o = run(
        future.path(),
        &[
            "disqualify",
            "--ledger",
            "ledger",
            "--registry",
            "registry.json",
            "--family",
            "f",
            "--tier",
            "frontier",
            "--reason",
            "operator",
            "--evidence",
            "x",
            "--detail",
            "d",
            "--expires-at",
            "tomorrow",
        ],
        None,
    );
    assert_eq!(code(&o), 2);
}

#[test]
fn eval_split_and_summarise_append_sealed_records() {
    let tmp = TempDir::new("eval");
    let dir = tmp.path();
    run_walkthrough(dir);
    let items = serde_json::json!([
        { "item_id": "i1", "family": "ticket.priority", "source_family": "helpdesk-a",
          "input_sha256": "00", "expected_sha256": null, "oracle": "exact" },
        { "item_id": "i2", "family": "ticket.priority", "source_family": "helpdesk-b",
          "input_sha256": "01", "expected_sha256": null, "oracle": "exact" }
    ]);
    std::fs::write(dir.join("items.json"), items.to_string()).unwrap();
    let split = run(
        dir,
        &[
            "eval",
            "split",
            "--items",
            "items.json",
            "--ledger",
            "ledger",
        ],
        None,
    );
    assert_eq!(code(&split), 0);
    assert_eq!(stdout_json(&split)["items"].as_array().unwrap().len(), 2);
    assert_eq!(lines(&dir.join("ledger/eval-splits.jsonl")).len(), 1);

    std::fs::write(
        dir.join("candidate.json"),
        r#"{ "kind": "single", "model_id": "jev", "reasoning_level": "none" }"#,
    )
    .unwrap();
    let run_out = run(
        dir,
        &[
            "eval",
            "summarise",
            "--ledger",
            "ledger",
            "--family",
            "ticket.priority",
            "--tier",
            "system_one",
            "--candidate",
            "candidate.json",
            "--task",
            "task.json",
        ],
        None,
    );
    assert_eq!(code(&run_out), 0);
    let er = stdout_json(&run_out);
    assert_eq!(er["n"], 1);
    assert_eq!(er["pass_rate"], 1.0);
    assert_eq!(lines(&dir.join("ledger/eval-runs.jsonl")).len(), 1);

    let show = run(
        dir,
        &[
            "ledger",
            "show",
            "--ledger",
            "ledger",
            "--kind",
            "eval-runs",
            "--family",
            "ticket.priority",
        ],
        None,
    );
    assert_eq!(code(&show), 0);
    let shown = stdout_json(&show);
    assert_eq!(shown["eval-runs"].as_array().unwrap().len(), 1);
    assert!(shown.get("attempts").is_none());
    assert_eq!(
        code(&run(dir, &["ledger", "verify", "--ledger", "ledger"], None)),
        0
    );
}
