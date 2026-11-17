//! `tier-drop`: the command line over `tier-drop-core` with a local append-only JSONL ledger.
//!
//! JSON in, JSON out: results are pretty-printed to stdout, diagnostics go to stderr. Exit codes:
//! 0 ok; 1 validation or floor failure (the JSON output still describes it); 2 usage error.
//! Every routing rule lives in the core; this binary reads files, owns the clock, generates ids
//! and writes the ledger.

mod clock;
mod ledger;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tier_drop_core::summarise_eval as summarise_run;
use tier_drop_core::{
    adjacent, assess_disqualification, check_floors, dataset_filter, decide, plan_next, qos_gate,
    schemas, sha256_hex, split_holdout, validate_attempt, validate_registry, validate_task,
    Attempt, Candidate, Disqualification, DisqualificationReason, EvalItem, Floor, Issue, ModelRef,
    Policy, Purpose, Registry, Task, Tier, DISQUALIFICATION_SCHEMA_VERSION,
};

use crate::ledger::{Kind, Ledger};

/// A failure that exits 1 with a JSON description on stdout.
#[derive(Debug)]
pub enum CliError {
    /// A single error with a code (unreadable file, malformed JSON, ledger I/O).
    Error {
        /// Short snake_case code.
        code: String,
        /// What went wrong.
        message: String,
    },
    /// Validation issues reported by the core.
    Issues(Vec<Issue>),
}

impl CliError {
    /// A coded error.
    pub fn new(code: &str, message: String) -> CliError {
        CliError::Error {
            code: code.to_string(),
            message,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            CliError::Error { code, message } => {
                json!({ "ok": false, "error": { "code": code, "message": message } })
            }
            CliError::Issues(issues) => json!({ "ok": false, "issues": issues }),
        }
    }

    fn describe(&self) -> String {
        match self {
            CliError::Error { code, message } => format!("{code}: {message}"),
            CliError::Issues(issues) => issues
                .iter()
                .map(|i| format!("{}: {}", i.code, i.message))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// What a command printed and whether it counts as success.
struct Done {
    output: Output,
    ok: bool,
}

enum Output {
    /// Pretty-printed JSON.
    Json(Value),
    /// Raw text printed as is (embedded files, JSONL exports).
    Text(String),
}

impl Done {
    fn ok(value: Value) -> Done {
        Done {
            output: Output::Json(value),
            ok: true,
        }
    }

    fn text(text: String) -> Done {
        Done {
            output: Output::Text(text),
            ok: true,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "tier-drop",
    version,
    about = "Tiered model routing with evidence-based disqualification, over a local JSONL ledger."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a registry or print the example registry.
    #[command(subcommand)]
    Registry(RegistryCmd),
    /// Print the embedded default policy.
    #[command(subcommand)]
    Policy(PolicyCmd),
    /// Print the next plan for a task: the lowest open tier and its untried candidates.
    Plan {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        ledger: Option<PathBuf>,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Print the lawful adjacent moves after a recommended candidate failed.
    Adjacent {
        /// A ModelRef: `{ "model_id": …, "reasoning_level": … }`.
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        ledger: Option<PathBuf>,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Print the QoS gate result for a task.
    Qos {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Record attempts.
    #[command(subcommand)]
    Attempt(AttemptCmd),
    /// Decide the routing for a task from the ledger, append and print the decision.
    Decide {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Append an operator-authored disqualification.
    Disqualify {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        family: String,
        #[arg(long, value_parser = parse_tier)]
        tier: Tier,
        #[arg(long, value_parser = parse_reason)]
        reason: DisqualificationReason,
        /// Attempt ids or eval run ids, comma-separated.
        #[arg(long, value_delimiter = ',', required = true)]
        evidence: Vec<String>,
        #[arg(long)]
        detail: String,
        /// Expiry as YYYY-MM-DDTHH:MM:SSZ.
        #[arg(long, value_parser = parse_timestamp)]
        expires_at: Option<String>,
    },
    /// Verify or show the ledger.
    #[command(subcommand)]
    Ledger(LedgerCmd),
    /// Held-out splits and eval-run summaries.
    #[command(subcommand, name = "eval")]
    Evaluation(EvaluationCmd),
    /// Export dataset rows.
    #[command(subcommand)]
    Dataset(DatasetCmd),
    /// Print JSON Schema for every record type, or for the one named.
    Schemas {
        #[arg(value_parser = parse_schema_name)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum RegistryCmd {
    /// Validate a registry file.
    Validate { file: PathBuf },
    /// Print registry/registry.example.json.
    Example,
}

#[derive(Subcommand)]
enum PolicyCmd {
    /// Print registry/policy.default.json.
    Default,
}

#[derive(Subcommand)]
enum AttemptCmd {
    /// Read one attempt JSON from stdin, validate, append and print its floor result.
    Record {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum LedgerCmd {
    /// Recompute every record hash and report `file:line` for each mismatch.
    Verify {
        #[arg(long)]
        ledger: PathBuf,
    },
    /// Print ledger records keyed by kind.
    Show {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        family: Option<String>,
        #[arg(long, value_enum)]
        kind: Option<Kind>,
    },
}

#[derive(Subcommand)]
enum EvaluationCmd {
    /// Split eval items into held-out and training sets by source family.
    Split {
        #[arg(long)]
        items: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
        #[arg(long)]
        ledger: Option<PathBuf>,
    },
    /// Summarise the ledger's attempts for (family, tier, candidate) into an eval run.
    Summarise {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        family: String,
        #[arg(long, value_parser = parse_tier)]
        tier: Tier,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        task: Option<PathBuf>,
        // decision: not in the CLI table; added so the "policy's required floors" used when no
        // task is given (and the task validation when one is) can use the operator's policy.
        #[arg(long)]
        policy: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DatasetCmd {
    /// Print the attempts whose rights allow the purpose, one canonical JSON line each.
    Export {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long, value_parser = parse_purpose)]
        purpose: Purpose,
        #[arg(long)]
        family: Option<String>,
    },
}

/// Parse a snake_case enum value through its serde representation.
fn parse_serde_enum<T: DeserializeOwned>(s: &str, allowed: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(s.to_string()))
        .map_err(|_| format!("`{s}` is not one of: {allowed}"))
}

fn parse_tier(s: &str) -> Result<Tier, String> {
    parse_serde_enum(
        s,
        "system_one, specialised, cheap_generalist, frontier, tailored",
    )
}

fn parse_reason(s: &str) -> Result<DisqualificationReason, String> {
    parse_serde_enum(
        s,
        "shape_mismatch, no_provisioned_candidates, no_qos_qualified_candidates, rights_forbid_remote, floors_not_met, operator",
    )
}

fn parse_purpose(s: &str) -> Result<Purpose, String> {
    parse_serde_enum(s, "process, train, send_remote, export")
}

fn parse_timestamp(s: &str) -> Result<String, String> {
    if clock::is_rfc3339_utc_seconds(s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "`{s}` is not an RFC 3339 UTC time of the form YYYY-MM-DDTHH:MM:SSZ"
        ))
    }
}

fn parse_schema_name(s: &str) -> Result<String, String> {
    let all = schemas();
    if all.contains_key(s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "`{s}` is not one of: {}",
            all.keys().copied().collect::<Vec<_>>().join(", ")
        ))
    }
}

/// Read and parse a JSON file.
fn read_json<T: DeserializeOwned>(path: &Path, what: &str) -> Result<T, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CliError::new("file_unreadable", format!("{what} {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| {
        CliError::new(
            &format!("invalid_{what}"),
            format!("{what} {}: {e}", path.display()),
        )
    })
}

fn to_value<T: Serialize>(v: &T) -> Result<Value, CliError> {
    serde_json::to_value(v).map_err(|e| CliError::new("serialise_failed", e.to_string()))
}

fn issues_or<T>(issues: Vec<Issue>, ok: T) -> Result<T, CliError> {
    if issues.is_empty() {
        Ok(ok)
    } else {
        Err(CliError::Issues(issues))
    }
}

/// A registry that parses and passes `validate_registry`.
// decision: every command that routes refuses an invalid registry (exit 1 with its issues), so
// no ledger record is ever written under a registry the core rejects.
fn load_registry(path: &Path) -> Result<Registry, CliError> {
    let registry: Registry = read_json(path, "registry")?;
    let issues = validate_registry(&registry);
    issues_or(issues, registry)
}

fn load_policy(path: Option<&Path>) -> Result<Policy, CliError> {
    match path {
        Some(p) => read_json(p, "policy"),
        None => Ok(Policy::default()),
    }
}

/// A task that parses and passes `validate_task` under `policy`.
// decision: commands that take a task refuse one the core's `validate_task` rejects (missing
// required floor, sampled review where strict is required), with exit 1 and the issues.
fn load_task(path: &Path, policy: &Policy) -> Result<Task, CliError> {
    let task: Task = read_json(path, "task")?;
    let issues = validate_task(&task, policy);
    issues_or(issues, task)
}

/// Append the task to `tasks.jsonl` unless a record with its `task_id` is already there.
// decision: when the stored task has the same id but different content (for example floors
// changed after results were recorded), the command fails with `task_conflicts_with_ledger`
// instead of silently routing under floors the ledger does not show.
fn ensure_task(ledger: &Ledger, task: &Task) -> Result<(), CliError> {
    let sealed = ledger::seal(task)?;
    let existing = ledger
        .values(Kind::Tasks)?
        .into_iter()
        .find(|v| v.get("task_id").and_then(Value::as_str) == Some(task.task_id.as_str()));
    match existing {
        None => {
            ledger.append(Kind::Tasks, task)?;
            Ok(())
        }
        Some(stored) if stored.get("record_sha256") == sealed.get("record_sha256") => Ok(()),
        Some(_) => Err(CliError::Issues(vec![Issue {
            code: "task_conflicts_with_ledger".to_string(),
            message: format!(
                "task {} is already in tasks.jsonl with different content; floors are fixed when the task is first recorded",
                task.task_id
            ),
        }])),
    }
}

/// The ledger's disqualifications that have not expired by the CLI's clock.
fn current_disqualifications(ledger: &Ledger) -> Result<Vec<Disqualification>, CliError> {
    let now = clock::now_rfc3339();
    Ok(ledger
        .read::<Disqualification>(Kind::Disqualifications)?
        .into_iter()
        .filter(|d| d.expires_at.as_deref().is_none_or(|e| e >= now.as_str()))
        .collect())
}

fn cmd_plan(
    task: &Path,
    registry: &Path,
    ledger: Option<&Path>,
    policy: Option<&Path>,
) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let registry = load_registry(registry)?;
    let task = load_task(task, &policy)?;
    let (dqs, attempts) = match ledger {
        Some(dir) => {
            let l = Ledger::new(dir);
            (
                current_disqualifications(&l)?,
                l.read::<Attempt>(Kind::Attempts)?,
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    let plan = plan_next(&task, &registry, &dqs, &attempts, &policy);
    Ok(Done::ok(to_value(&plan)?))
}

fn cmd_adjacent(
    from: &Path,
    task: &Path,
    registry: &Path,
    ledger: Option<&Path>,
    policy: Option<&Path>,
) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let registry = load_registry(registry)?;
    let task = load_task(task, &policy)?;
    let from: ModelRef = read_json(from, "from")?;
    // the ledger supplies in-force disqualifications, as for `plan`
    let dqs = match ledger {
        Some(dir) => current_disqualifications(&Ledger::new(dir))?,
        None => Vec::new(),
    };
    let moves = adjacent(&from, &task, &registry, &dqs, &policy);
    // decision: `from_not_fitting` is a warning by contract, so it keeps exit 0; an unknown model
    // or level makes the question unanswerable and exits 1 like any other validation failure.
    let ok = moves.issues.iter().all(|i| i.code == "from_not_fitting");
    Ok(Done {
        output: Output::Json(to_value(&moves)?),
        ok,
    })
}

fn cmd_qos(task: &Path, registry: &Path, policy: Option<&Path>) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let registry = load_registry(registry)?;
    let task = load_task(task, &policy)?;
    Ok(Done::ok(to_value(&qos_gate(&registry, &task, &policy))?))
}

/// Parse the attempt on stdin, filling `attempt_id` (a ULID) and `recorded_at` (now) when absent.
fn read_attempt_stdin() -> Result<Attempt, CliError> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| CliError::new("stdin_unreadable", e.to_string()))?;
    let mut raw: Value = serde_json::from_str(&input)
        .map_err(|e| CliError::new("invalid_attempt", format!("stdin: {e}")))?;
    let Some(map) = raw.as_object_mut() else {
        return Err(CliError::new(
            "invalid_attempt",
            "stdin: the attempt must be a JSON object".to_string(),
        ));
    };
    if !map.contains_key("attempt_id") {
        map.insert("attempt_id".to_string(), Value::String(clock::ulid()));
    }
    if !map.contains_key("recorded_at") {
        map.insert(
            "recorded_at".to_string(),
            Value::String(clock::now_rfc3339()),
        );
    }
    serde_json::from_value(raw).map_err(|e| CliError::new("invalid_attempt", format!("stdin: {e}")))
}

fn cmd_attempt_record(
    ledger: &Path,
    task: &Path,
    registry: &Path,
    policy: Option<&Path>,
) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let registry = load_registry(registry)?;
    let task = load_task(task, &policy)?;
    let ledger = Ledger::new(ledger);
    let attempt = read_attempt_stdin()?;

    let mut issues = validate_attempt(&task, &attempt, &registry);
    // decision: attempt ids are the evidence keys of disqualifications and decisions, so a
    // second record with an id already in attempts.jsonl is refused rather than appended.
    if ledger.contains(Kind::Attempts, "attempt_id", &attempt.attempt_id)? {
        issues.push(Issue {
            code: "duplicate_attempt_id".to_string(),
            message: format!(
                "attempt {} is already in attempts.jsonl",
                attempt.attempt_id
            ),
        });
    }
    if !issues.is_empty() {
        return Err(CliError::Issues(issues));
    }

    // The task goes in first so the ledger shows its floors before any result.
    ensure_task(&ledger, &task)?;
    ledger.append(Kind::Attempts, &attempt)?;
    let floor_result = check_floors(&task, &attempt, &policy);

    let tier_attempts: Vec<Attempt> = ledger
        .read::<Attempt>(Kind::Attempts)?
        .into_iter()
        .filter(|a| a.family == attempt.family && a.tier == attempt.tier)
        .collect();
    let assessed = assess_disqualification(
        &attempt.family,
        attempt.tier,
        &tier_attempts,
        &task,
        &registry,
        &registry.as_of,
        &policy,
    );
    let disqualification = match assessed {
        Some(d)
            if !ledger.contains(
                Kind::Disqualifications,
                "disqualification_id",
                &d.disqualification_id,
            )? =>
        {
            ledger.append(Kind::Disqualifications, &d)?
        }
        _ => Value::Null,
    };
    // Exit 1 on a floor failure, per the CLI contract; the attempt is recorded either way.
    Ok(Done {
        ok: floor_result.passed,
        output: Output::Json(json!({
            "floor_result": floor_result,
            "disqualification": disqualification,
        })),
    })
}

fn cmd_decide(
    task: &Path,
    registry: &Path,
    ledger: &Path,
    policy: Option<&Path>,
) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let registry = load_registry(registry)?;
    let task = load_task(task, &policy)?;
    let ledger = Ledger::new(ledger);
    let dqs = current_disqualifications(&ledger)?;
    let attempts: Vec<Attempt> = ledger
        .read::<Attempt>(Kind::Attempts)?
        .into_iter()
        .filter(|a| a.task_id == task.task_id)
        .collect();
    let decision = decide(&task, &registry, &dqs, &attempts, &policy);
    ensure_task(&ledger, &task)?;
    // decision: `winner: null` is a valid routing record, not a validation failure, so decide
    // exits 0 either way; the JSON says which.
    let sealed = ledger.append(Kind::Decisions, &decision)?;
    Ok(Done::ok(sealed))
}

/// The operator-authored disqualification fields taken from flags.
struct OperatorRecord<'a> {
    family: &'a str,
    tier: Tier,
    reason: DisqualificationReason,
    evidence: &'a [String],
    detail: &'a str,
    expires_at: Option<String>,
}

fn cmd_disqualify(
    ledger: &Path,
    registry: &Path,
    input: OperatorRecord<'_>,
) -> Result<Done, CliError> {
    let registry = load_registry(registry)?;
    let mut evidence: Vec<String> = input
        .evidence
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    evidence.sort();
    evidence.dedup();
    // Mirrors the core's id rule: "dq-" + first 16 hex of sha256(family|tier|sorted evidence ids).
    let material = format!(
        "{}|{}|{}",
        input.family,
        input.tier.as_str(),
        evidence.join("|")
    );
    let id = format!("dq-{}", &sha256_hex(material.as_bytes())[..16]);
    let record = Disqualification {
        schema_version: DISQUALIFICATION_SCHEMA_VERSION.to_string(),
        disqualification_id: id,
        family: input.family.to_string(),
        tier: input.tier,
        reason: input.reason,
        evidence,
        detail: input.detail.to_string(),
        registry_as_of: registry.as_of.clone(),
        decided_at: clock::now_rfc3339(),
        expires_at: input.expires_at,
        record_sha256: String::new(),
    };
    // decision: operator records are always appended, even when the id repeats (for example a
    // renewed expiry); the ledger keeps every record and the engine reads them all.
    let sealed = Ledger::new(ledger).append(Kind::Disqualifications, &record)?;
    Ok(Done::ok(sealed))
}

fn cmd_split(items: &Path, policy: Option<&Path>, ledger: Option<&Path>) -> Result<Done, CliError> {
    let policy = load_policy(policy)?;
    let items: Vec<EvalItem> = read_json(items, "items")?;
    let split = split_holdout(&items, &policy);
    let out = match ledger {
        Some(dir) => Ledger::new(dir).append(Kind::EvalSplits, &split)?,
        None => ledger::seal(&split)?,
    };
    Ok(Done::ok(out))
}

/// Flags of `eval summarise`.
struct SummariseInput<'a> {
    ledger: &'a Path,
    family: &'a str,
    tier: Tier,
    candidate: &'a Path,
    task: Option<&'a Path>,
    policy: Option<&'a Path>,
}

fn cmd_summarise(input: SummariseInput<'_>) -> Result<Done, CliError> {
    let policy = load_policy(input.policy)?;
    let candidate: Candidate = read_json(input.candidate, "candidate")?;
    let floors: Vec<Floor> = match input.task {
        Some(p) => load_task(p, &policy)?.floors,
        None => policy
            .required_floors
            .iter()
            .map(|m| Floor {
                metric: m.clone(),
                min: 0.0,
            })
            .collect(),
    };
    let ledger = Ledger::new(input.ledger);
    let attempts = ledger.read::<Attempt>(Kind::Attempts)?;
    let run = summarise_run(input.family, input.tier, &candidate, &attempts, &floors);
    Ok(Done::ok(ledger.append(Kind::EvalRuns, &run)?))
}

fn cmd_dataset_export(
    ledger: &Path,
    purpose: Purpose,
    family: Option<&str>,
) -> Result<Done, CliError> {
    let attempts = Ledger::new(ledger).read::<Attempt>(Kind::Attempts)?;
    let mut out = String::new();
    for a in dataset_filter(&attempts, purpose) {
        if family.is_some_and(|f| f != a.family) {
            continue;
        }
        out.push_str(&tier_drop_core::canonical_json(&to_value(a)?));
        out.push('\n');
    }
    Ok(Done::text(out))
}

fn cmd_schemas(name: Option<&str>) -> Result<Done, CliError> {
    let all = schemas();
    let value = match name {
        Some(n) => all
            .get(n)
            .map(|s| s.as_value().clone())
            .ok_or_else(|| CliError::new("unknown_schema", n.to_string()))?,
        None => Value::Object(
            all.iter()
                .map(|(k, s)| ((*k).to_string(), s.as_value().clone()))
                .collect(),
        ),
    };
    Ok(Done::ok(value))
}

fn cmd_verify(ledger: &Path) -> Result<Done, CliError> {
    let report = Ledger::new(ledger).verify()?;
    let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if let Some(ms) = report.get("mismatches").and_then(Value::as_array) {
        for m in ms {
            eprintln!(
                "{}: {}",
                m["location"].as_str().unwrap_or_default(),
                m["reason"].as_str().unwrap_or_default()
            );
        }
    }
    Ok(Done {
        output: Output::Json(report),
        ok,
    })
}

fn run(cli: Cli) -> Result<Done, CliError> {
    match cli.command {
        Command::Registry(RegistryCmd::Validate { file }) => {
            let registry: Registry = read_json(&file, "registry")?;
            let issues = validate_registry(&registry);
            Ok(Done {
                ok: issues.is_empty(),
                output: Output::Json(json!({ "valid": issues.is_empty(), "issues": issues })),
            })
        }
        Command::Registry(RegistryCmd::Example) => Ok(Done::text(
            tier_drop_core::registry::REGISTRY_EXAMPLE_JSON.to_string(),
        )),
        Command::Policy(PolicyCmd::Default) => Ok(Done::text(
            tier_drop_core::policy::POLICY_DEFAULT_JSON.to_string(),
        )),
        Command::Plan {
            task,
            registry,
            ledger,
            policy,
        } => cmd_plan(&task, &registry, ledger.as_deref(), policy.as_deref()),
        Command::Adjacent {
            from,
            task,
            registry,
            ledger,
            policy,
        } => cmd_adjacent(
            &from,
            &task,
            &registry,
            ledger.as_deref(),
            policy.as_deref(),
        ),
        Command::Qos {
            task,
            registry,
            policy,
        } => cmd_qos(&task, &registry, policy.as_deref()),
        Command::Attempt(AttemptCmd::Record {
            ledger,
            task,
            registry,
            policy,
        }) => cmd_attempt_record(&ledger, &task, &registry, policy.as_deref()),
        Command::Decide {
            task,
            registry,
            ledger,
            policy,
        } => cmd_decide(&task, &registry, &ledger, policy.as_deref()),
        Command::Disqualify {
            ledger,
            registry,
            family,
            tier,
            reason,
            evidence,
            detail,
            expires_at,
        } => cmd_disqualify(
            &ledger,
            &registry,
            OperatorRecord {
                family: &family,
                tier,
                reason,
                evidence: &evidence,
                detail: &detail,
                expires_at,
            },
        ),
        Command::Ledger(LedgerCmd::Verify { ledger }) => cmd_verify(&ledger),
        Command::Ledger(LedgerCmd::Show {
            ledger,
            family,
            kind,
        }) => Ok(Done::ok(
            Ledger::new(&ledger).show(kind, family.as_deref())?,
        )),
        Command::Evaluation(EvaluationCmd::Split {
            items,
            policy,
            ledger,
        }) => cmd_split(&items, policy.as_deref(), ledger.as_deref()),
        Command::Evaluation(EvaluationCmd::Summarise {
            ledger,
            family,
            tier,
            candidate,
            task,
            policy,
        }) => cmd_summarise(SummariseInput {
            ledger: &ledger,
            family: &family,
            tier,
            candidate: &candidate,
            task: task.as_deref(),
            policy: policy.as_deref(),
        }),
        Command::Dataset(DatasetCmd::Export {
            ledger,
            purpose,
            family,
        }) => cmd_dataset_export(&ledger, purpose, family.as_deref()),
        Command::Schemas { name } => cmd_schemas(name.as_deref()),
    }
}

fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("tier-drop: could not print output: {e}"),
    }
}

fn main() -> ExitCode {
    // clap prints usage errors itself and exits 2.
    let cli = Cli::parse();
    match run(cli) {
        Ok(done) => {
            match &done.output {
                Output::Json(v) => print_json(v),
                Output::Text(t) => print!("{t}"),
            }
            if done.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            print_json(&e.to_json());
            eprintln!("tier-drop: {}", e.describe());
            ExitCode::from(1)
        }
    }
}
