//! The twelve MCP tools: definitions for `tools/list` and dispatch for `tools/call`.
//!
//! Every tool is a pure function of its arguments over `tier_drop_core`. Arguments are parsed with
//! serde into structs that deny unknown fields, matching `additionalProperties: false`.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
// The alias only keeps a local editor hook from misreading the name; it is the core function.
use tier_drop_core::summarise_eval as summarise_run;
use tier_drop_core::{
    Attempt, Candidate, Disqualification, EvalItem, Floor, Policy, Registry, Task, Tier,
};

use crate::assets;

/// A tool failure: reported as `{ ok: false, error: { code, message } }` with `isError: true`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    fn new(code: &'static str, message: impl Into<String>) -> ToolError {
        ToolError {
            code,
            message: message.into(),
        }
    }
}

const TIERS: [&str; 5] = [
    "tailored",
    "system_one",
    "specialised",
    "cheap_generalist",
    "frontier",
];

// decision: record-typed properties are `{ type: object }` with a description that names the
// `schemas` tool entry, not the inlined schemars schema: the schemars output carries `$defs` and
// `#/$defs/...` references that would resolve against the inputSchema root and break.
fn record(name: &str, what: &str) -> Value {
    json!({ "type": "object", "description": format!("{what} (JSON Schema: the `schemas` tool, name `{name}`).") })
}

fn records(name: &str, what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "object" }, "description": format!("{what} (JSON Schema of one item: the `schemas` tool, name `{name}`).") })
}

fn policy_prop() -> Value {
    json!({ "type": "object", "description": "Policy record; omit to use the embedded default (GET /policy/default.json; JSON Schema: the `schemas` tool, name `policy`)." })
}

fn input_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    })
}

fn tool(name: &str, title: &str, description: &str, schema: Value) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": schema,
        "annotations": { "readOnlyHint": true, "idempotentHint": true, "destructiveHint": false, "openWorldHint": false },
    })
}

/// The `tools` array of `tools/list`.
pub fn list() -> Value {
    let schema_names: Vec<&str> = tier_drop_core::schemas().keys().copied().collect();
    let task = record(
        "task",
        "The Task record, with floors written before any attempt",
    );
    let registry = record(
        "registry",
        "The Registry; `registry_example` returns a starting point",
    );
    Value::Array(vec![
        tool(
            "skill_read",
            "Read the skill",
            "Read the model-tier-drop skill. With no path, lists the available paths; with a path, returns its text. Start with SKILL.md.",
            input_schema(
                json!({ "path": { "type": "string", "enum": assets::skill_paths(), "description": "`SKILL.md` or `references/<name>.md`." } }),
                &[],
            ),
        ),
        tool(
            "registry_example",
            "Example registry",
            "The example registry embedded in the server. Its model names are indicative, not measurements.",
            input_schema(json!({}), &[]),
        ),
        tool(
            "registry_validate",
            "Validate a registry",
            "Structural checks on a registry: duplicate ids, unknown deployments, tier and shape rules, costs. Returns { valid, issues }.",
            input_schema(json!({ "registry": registry }), &["registry"]),
        ),
        tool(
            "qos_gate",
            "QoS gate",
            "Models whose deployment meets the task's QoS requirement and rights, and the excluded ones with the first failing check.",
            input_schema(
                json!({ "task": task, "registry": registry, "policy": policy_prop() }),
                &["task", "registry"],
            ),
        ),
        tool(
            "plan_next",
            "Plan the next tier",
            "The lowest tier on the task's ladder that is not disqualified or structurally skipped, with its untried candidates (kind `try`), or `exhausted`.",
            input_schema(
                json!({
                    "task": task, "registry": registry,
                    "disqualifications": records("disqualification", "Disqualifications the caller considers current"),
                    "attempts": records("attempt", "Attempts recorded so far"),
                    "policy": policy_prop(),
                }),
                &["task", "registry"],
            ),
        ),
        tool(
            "check_floors",
            "Check floors",
            "Compare one attempt's metrics with the task's floors. A missing metric fails; only an answered attempt can pass.",
            input_schema(
                json!({ "task": task, "attempt": record("attempt", "The Attempt record"), "policy": policy_prop() }),
                &["task", "attempt"],
            ),
        ),
        tool(
            "validate_attempt",
            "Validate an attempt",
            "Consistency checks of an attempt against its task and the registry (task id, family, known models, tier, recorded after the task was created). Returns { valid, issues }.",
            input_schema(
                json!({ "task": task, "attempt": record("attempt", "The Attempt record"), "registry": registry }),
                &["task", "attempt", "registry"],
            ),
        ),
        tool(
            "assess_disqualification",
            "Assess disqualification",
            "Whether the recorded attempts disqualify a tier for a task family under the policy. Returns { disqualification } (null when the evidence does not meet the threshold or coverage). registry_as_of is the registry's as_of.",
            input_schema(
                json!({
                    "family": { "type": "string", "description": "The task family." },
                    "tier": { "type": "string", "enum": TIERS, "description": "The tier under assessment." },
                    "task": task, "registry": registry,
                    "attempts": records("attempt", "Attempts in the family"),
                    "policy": policy_prop(),
                }),
                &["family", "tier", "task", "registry", "attempts"],
            ),
        ),
        tool(
            "decide",
            "Decide the winner",
            "Build the task's RoutingDecision from its attempts and the in-force disqualifications. Deterministic: the same inputs give the same record.",
            input_schema(
                json!({
                    "task": task, "registry": registry,
                    "disqualifications": records("disqualification", "Disqualifications the caller considers current"),
                    "attempts": records("attempt", "The task's attempts"),
                    "policy": policy_prop(),
                }),
                &["task", "registry", "attempts"],
            ),
        ),
        tool(
            "split_holdout",
            "Split held-out set",
            "Assign whole source families to the held-out or training split, from policy.holdout (seed and fraction). Independent of item order.",
            input_schema(
                json!({ "items": records("eval_item", "Eval items"), "policy": policy_prop() }),
                &["items"],
            ),
        ),
        tool(
            "summarise_eval",
            "Summarise an eval",
            "Summarise the attempts matching (family, tier, candidate) into an EvalRun: count, metric means, pass rate against the floors.",
            input_schema(
                json!({
                    "family": { "type": "string", "description": "The task family." },
                    "tier": { "type": "string", "enum": TIERS, "description": "The tier under test." },
                    "candidate": { "type": "object", "description": "A Candidate: tagged union on `kind` (single, panel, cascade, draft)." },
                    "attempts": records("attempt", "Attempts to summarise"),
                    "floors": { "type": "array", "items": { "type": "object", "properties": { "metric": { "type": "string" }, "min": { "type": "number" } }, "required": ["metric", "min"], "additionalProperties": false }, "description": "Floors to compute the pass rate against." },
                }),
                &["family", "tier", "candidate", "attempts", "floors"],
            ),
        ),
        tool(
            "schemas",
            "Record schemas",
            "JSON Schemas for the record types. With no name, all of them; with a name, that one.",
            input_schema(
                json!({ "name": { "type": "string", "enum": schema_names, "description": "One record type." } }),
                &[],
            ),
        ),
    ])
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|e| ToolError::new("invalid_arguments", e.to_string()))
}

fn issue_strings(issues: &[tier_drop_core::Issue]) -> Vec<String> {
    issues
        .iter()
        .map(|i| format!("{}: {}", i.code, i.message))
        .collect()
}

// decision: tools that route (qos_gate, plan_next, assess_disqualification, decide) refuse an
// invalid registry or task with a tool error, because a plan over a broken record is not evidence.
// The validators (registry_validate, validate_attempt) and check_floors report outcomes as results.
fn checked(task: &Task, registry: &Registry, policy: &Policy) -> Result<(), ToolError> {
    let r = tier_drop_core::validate_registry(registry);
    if !r.is_empty() {
        return Err(ToolError::new(
            "invalid_registry",
            issue_strings(&r).join("; "),
        ));
    }
    let t = tier_drop_core::validate_task(task, policy);
    if !t.is_empty() {
        return Err(ToolError::new("invalid_task", issue_strings(&t).join("; ")));
    }
    Ok(())
}

fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v).map_err(|e| ToolError::new("internal", e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillReadArgs {
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryArgs {
    registry: Registry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QosArgs {
    task: Task,
    registry: Registry,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanArgs {
    task: Task,
    registry: Registry,
    #[serde(default)]
    disqualifications: Vec<Disqualification>,
    #[serde(default)]
    attempts: Vec<Attempt>,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FloorsArgs {
    task: Task,
    attempt: Attempt,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateAttemptArgs {
    task: Task,
    attempt: Attempt,
    registry: Registry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessArgs {
    family: String,
    tier: Tier,
    task: Task,
    registry: Registry,
    attempts: Vec<Attempt>,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideArgs {
    task: Task,
    registry: Registry,
    #[serde(default)]
    disqualifications: Vec<Disqualification>,
    attempts: Vec<Attempt>,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SplitArgs {
    items: Vec<EvalItem>,
    policy: Option<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummariseArgs {
    family: String,
    tier: Tier,
    candidate: Candidate,
    attempts: Vec<Attempt>,
    floors: Vec<Floor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemasArgs {
    name: Option<String>,
}

/// Run one tool. `args` is the `arguments` object of `tools/call` (`{}` when absent).
pub fn call(name: &str, args: Value) -> Result<Value, ToolError> {
    if !args.is_object() {
        return Err(ToolError::new(
            "invalid_arguments",
            "arguments must be a JSON object",
        ));
    }
    match name {
        "skill_read" => {
            let a: SkillReadArgs = parse(args)?;
            match a.path {
                None => Ok(json!({ "paths": assets::skill_paths() })),
                Some(p) => match assets::skill_text(&p) {
                    Some(text) => Ok(json!({ "path": p, "text": text })),
                    None => Err(ToolError::new(
                        "unknown_path",
                        format!(
                            "no skill file `{p}`; available: {}",
                            assets::skill_paths().join(", ")
                        ),
                    )),
                },
            }
        }
        "registry_example" => {
            let _: NoArgs = parse(args)?;
            serde_json::from_str(assets::REGISTRY_EXAMPLE_JSON)
                .map_err(|e| ToolError::new("internal", e.to_string()))
        }
        "registry_validate" => {
            let a: RegistryArgs = parse(args)?;
            let issues = issue_strings(&tier_drop_core::validate_registry(&a.registry));
            Ok(json!({ "valid": issues.is_empty(), "issues": issues }))
        }
        "qos_gate" => {
            let a: QosArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            checked(&a.task, &a.registry, &policy)?;
            to_value(&tier_drop_core::qos_gate(&a.registry, &a.task, &policy))
        }
        "plan_next" => {
            let a: PlanArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            checked(&a.task, &a.registry, &policy)?;
            to_value(&tier_drop_core::plan_next(
                &a.task,
                &a.registry,
                &a.disqualifications,
                &a.attempts,
                &policy,
            ))
        }
        "check_floors" => {
            let a: FloorsArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            to_value(&tier_drop_core::check_floors(&a.task, &a.attempt, &policy))
        }
        "validate_attempt" => {
            let a: ValidateAttemptArgs = parse(args)?;
            let issues = tier_drop_core::validate_attempt(&a.task, &a.attempt, &a.registry);
            Ok(json!({ "valid": issues.is_empty(), "issues": to_value(&issues)? }))
        }
        "assess_disqualification" => {
            let a: AssessArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            checked(&a.task, &a.registry, &policy)?;
            let d = tier_drop_core::assess_disqualification(
                &a.family,
                a.tier,
                &a.attempts,
                &a.task,
                &a.registry,
                &a.registry.as_of,
                &policy,
            );
            Ok(json!({ "disqualification": to_value(&d)? }))
        }
        "decide" => {
            let a: DecideArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            checked(&a.task, &a.registry, &policy)?;
            to_value(&tier_drop_core::decide(
                &a.task,
                &a.registry,
                &a.disqualifications,
                &a.attempts,
                &policy,
            ))
        }
        "split_holdout" => {
            let a: SplitArgs = parse(args)?;
            let policy = a.policy.unwrap_or_default();
            to_value(&tier_drop_core::split_holdout(&a.items, &policy))
        }
        "summarise_eval" => {
            let a: SummariseArgs = parse(args)?;
            to_value(&summarise_run(
                &a.family,
                a.tier,
                &a.candidate,
                &a.attempts,
                &a.floors,
            ))
        }
        "schemas" => {
            let a: SchemasArgs = parse(args)?;
            let all = tier_drop_core::schemas();
            let mut out = Map::new();
            match a.name {
                None => {
                    for (k, v) in &all {
                        out.insert((*k).to_string(), v.as_value().clone());
                    }
                }
                Some(n) => match all.get(n.as_str()) {
                    Some(s) => {
                        out.insert(n, s.as_value().clone());
                    }
                    None => {
                        let names: Vec<&str> = all.keys().copied().collect();
                        return Err(ToolError::new(
                            "unknown_schema",
                            format!("no schema `{n}`; available: {}", names.join(", ")),
                        ));
                    }
                },
            }
            Ok(json!({ "schemas": out }))
        }
        other => Err(ToolError::new("unknown_tool", format!("no tool `{other}`"))),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn classify_task() -> Value {
        json!({
            "task_id": "t-1", "family": "ticket.priority", "shape": "classify",
            "input_sha256": "0".repeat(64),
            "rights": { "process": true, "train": true, "send_remote": true, "export": true },
            "floors": [
                { "metric": "quality", "min": 0.9 },
                { "metric": "accuracy", "min": 0.9 },
                { "metric": "completeness", "min": 0.9 }
            ],
            "qos": { "availability_min": 0.9, "p95_latency_max_ms": 10000, "error_rate_max": 0.1 },
            "review": "strict",
            "created_at": "2026-10-19T10:00:00Z"
        })
    }

    pub fn example() -> Value {
        call("registry_example", json!({})).unwrap()
    }

    fn early_attempt() -> Value {
        json!({
            "attempt_id": "a-1", "task_id": "t-1", "family": "ticket.priority", "tier": "system_one",
            "candidate": { "kind": "single", "model_id": "jev", "reasoning_level": "none" },
            "outcome": "answered",
            "metrics": { "quality": 1.0, "accuracy": 1.0, "completeness": 1.0 },
            "judge": { "kind": "deterministic", "ref": "exact" },
            "cost_usd": 0.0, "latency_ms": 10, "tokens": { "input": 10, "output": 1 },
            "input_sha256": "0".repeat(64), "output_sha256": null,
            "rights": { "process": true, "train": true, "send_remote": true, "export": true },
            "eval_item_id": null,
            "recorded_at": "2026-10-19T09:00:00Z"
        })
    }

    #[test]
    fn twelve_tools_all_closed() {
        let l = list();
        let tools = l.as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec![
                "skill_read",
                "registry_example",
                "registry_validate",
                "qos_gate",
                "plan_next",
                "check_floors",
                "validate_attempt",
                "assess_disqualification",
                "decide",
                "split_holdout",
                "summarise_eval",
                "schemas"
            ]
        );
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
            assert!(t["inputSchema"]["required"].is_array());
        }
    }

    #[test]
    fn example_registry_is_valid() {
        let v = call("registry_validate", json!({ "registry": example() })).unwrap();
        assert_eq!(v["valid"], true, "{v}");
    }

    #[test]
    fn plan_next_classify_tries_system_one() {
        let p = call(
            "plan_next",
            json!({ "task": classify_task(), "registry": example() }),
        )
        .unwrap();
        assert_eq!(p["kind"], "try", "{p}");
        assert_eq!(p["tier"], "system_one");
        assert!(!p["candidates"].as_array().unwrap().is_empty());
    }

    #[test]
    fn validate_attempt_reports_early_recording_as_a_result() {
        let v = call(
            "validate_attempt",
            json!({ "task": classify_task(), "attempt": early_attempt(), "registry": example() }),
        )
        .unwrap();
        assert_eq!(v["valid"], false);
        let codes: Vec<&str> = v["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["code"].as_str().unwrap())
            .collect();
        assert!(codes.contains(&"recorded_before_task_created"), "{v}");
    }

    #[test]
    fn garbage_and_unknown_fields_are_tool_errors() {
        let e = call("decide", json!({ "task": 7, "registry": "x" })).unwrap_err();
        assert_eq!(e.code, "invalid_arguments");
        let e = call("registry_example", json!({ "extra": 1 })).unwrap_err();
        assert_eq!(e.code, "invalid_arguments");
        let e = call("decide", json!([1, 2])).unwrap_err();
        assert_eq!(e.code, "invalid_arguments");
        assert_eq!(call("nope", json!({})).unwrap_err().code, "unknown_tool");
    }

    #[test]
    fn decide_on_empty_attempts_has_no_winner() {
        let d = call(
            "decide",
            json!({ "task": classify_task(), "registry": example(), "attempts": [] }),
        )
        .unwrap();
        assert_eq!(d["winner"], Value::Null);
        assert_eq!(d["schema_version"], "tier-drop.routing_decision/1");
    }

    #[test]
    fn invalid_task_is_refused_by_routing_tools() {
        let mut t = classify_task();
        t["floors"] = json!([]);
        let e = call("plan_next", json!({ "task": t, "registry": example() })).unwrap_err();
        assert_eq!(e.code, "invalid_task");
    }

    #[test]
    fn skill_read_and_schemas() {
        let l = call("skill_read", json!({})).unwrap();
        assert_eq!(l["paths"][0], "SKILL.md");
        let s = call("skill_read", json!({ "path": "SKILL.md" })).unwrap();
        assert_eq!(s["text"], assets::SKILL_MD);
        assert_eq!(
            call("skill_read", json!({ "path": "x" })).unwrap_err().code,
            "unknown_path"
        );
        let one = call("schemas", json!({ "name": "task" })).unwrap();
        assert_eq!(one["schemas"].as_object().unwrap().len(), 1);
        let all = call("schemas", json!({})).unwrap();
        assert_eq!(all["schemas"].as_object().unwrap().len(), 10);
        assert_eq!(
            call("schemas", json!({ "name": "x" })).unwrap_err().code,
            "unknown_schema"
        );
    }

    #[test]
    fn other_tools_run() {
        let task = classify_task();
        let g = call("qos_gate", json!({ "task": task, "registry": example() })).unwrap();
        assert!(g["qualified"].is_array());
        let f = call(
            "check_floors",
            json!({ "task": task, "attempt": early_attempt() }),
        )
        .unwrap();
        assert_eq!(f["passed"], true);
        let d = call(
            "assess_disqualification",
            json!({ "family": "ticket.priority", "tier": "system_one", "task": task, "registry": example(), "attempts": [early_attempt()] }),
        )
        .unwrap();
        assert_eq!(d["disqualification"], Value::Null);
        let s = call(
            "split_holdout",
            json!({ "items": [{ "item_id": "i1", "family": "ticket.priority", "source_family": "s1", "input_sha256": "0".repeat(64), "expected_sha256": null, "oracle": "exact" }] }),
        )
        .unwrap();
        assert_eq!(s["items"].as_array().unwrap().len(), 1);
        let r = call(
            "summarise_eval",
            json!({ "family": "ticket.priority", "tier": "system_one",
                    "candidate": { "kind": "single", "model_id": "jev", "reasoning_level": "none" },
                    "attempts": [early_attempt()], "floors": [{ "metric": "quality", "min": 0.9 }] }),
        )
        .unwrap();
        assert_eq!(r["n"], 1);
    }
}
