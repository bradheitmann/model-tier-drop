//! Attempt records (every try, including failures and abstentions) and `check_floors`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::candidate::Candidate;
use crate::policy::Policy;
use crate::registry::{Issue, Registry};
use crate::rights::Rights;
use crate::task::{Floor, Task};
use crate::tier::Tier;

/// How an attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The candidate produced an answer.
    Answered,
    /// The candidate declined to answer.
    Abstained,
    /// The call or the answer failed.
    Failed,
    /// The call did not finish in time.
    TimedOut,
}

/// Who scored the attempt's metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JudgeKind {
    /// A deterministic checker.
    Deterministic,
    /// A model judge.
    Model,
    /// A person.
    Human,
}

/// The scorer of an attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Judge {
    /// Kind of scorer.
    pub kind: JudgeKind,
    /// Reference to the scorer (checker name, model id, reviewer role).
    #[serde(rename = "ref")]
    pub reference: String,
}

/// Token counts of an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    /// Input tokens.
    pub input: u64,
    /// Output tokens.
    pub output: u64,
}

/// Schema version written on every `Attempt` record.
pub const ATTEMPT_SCHEMA_VERSION: &str = "tier-drop.attempt/1";

fn default_attempt_schema_version() -> String {
    ATTEMPT_SCHEMA_VERSION.to_string()
}

/// One recorded try of a candidate on a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// Record schema version; defaults to `tier-drop.attempt/1` when omitted on input.
    #[serde(default = "default_attempt_schema_version")]
    pub schema_version: String,
    /// Caller-supplied attempt id.
    pub attempt_id: String,
    /// The task tried.
    pub task_id: String,
    /// The task's family.
    pub family: String,
    /// The tier under test.
    pub tier: Tier,
    /// What was tried.
    pub candidate: Candidate,
    /// How it ended.
    pub outcome: Outcome,
    /// Metric vector.
    pub metrics: BTreeMap<String, f64>,
    /// Who scored it.
    pub judge: Judge,
    /// Cost in USD.
    pub cost_usd: f64,
    /// Wall-clock latency in milliseconds, measured by the harness.
    pub latency_ms: u64,
    /// Token counts.
    pub tokens: Tokens,
    /// SHA-256 of the input.
    pub input_sha256: String,
    /// SHA-256 of the output, null when there is none.
    pub output_sha256: Option<String>,
    /// The task's rights at attempt time.
    pub rights: Rights,
    /// Eval item the attempt ran on, if any.
    pub eval_item_id: Option<String>,
    /// RFC 3339 UTC time supplied by the caller.
    pub recorded_at: String,
    /// SHA-256 over the canonical JSON without this field; empty until sealed.
    // decision: defaulted so a harness can submit an attempt before it is hashed; the CLI
    // fills it before writing to the ledger.
    #[serde(default)]
    pub record_sha256: String,
}

/// One floor an attempt did not clear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FloorFailure {
    /// Metric name.
    pub metric: String,
    /// Required minimum.
    pub min: f64,
    /// Observed value, null when the metric is missing.
    pub observed: Option<f64>,
}

/// The result of checking an attempt against floors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FloorResult {
    /// The attempt checked.
    pub attempt_id: String,
    /// True only when the attempt answered and cleared every floor.
    pub passed: bool,
    /// Every floor not cleared.
    pub failures: Vec<FloorFailure>,
}

/// Check an attempt against an explicit list of floors.
pub(crate) fn check_against(attempt: &Attempt, floors: &[Floor]) -> FloorResult {
    let mut failures = Vec::new();
    for f in floors {
        match attempt.metrics.get(&f.metric) {
            Some(v) if *v >= f.min => {}
            observed => failures.push(FloorFailure {
                metric: f.metric.clone(),
                min: f.min,
                observed: observed.copied(),
            }),
        }
    }
    FloorResult {
        attempt_id: attempt.attempt_id.clone(),
        passed: failures.is_empty() && attempt.outcome == Outcome::Answered,
        failures,
    }
}

/// Compare an attempt's metric vector with the task's floors; a missing metric is a failure.
// decision: a `policy.required_floors` metric that the task forgot to floor is checked
// with `min: 0.0`, so a missing required metric still fails even on an invalid task.
pub fn check_floors(task: &Task, attempt: &Attempt, policy: &Policy) -> FloorResult {
    let mut floors = task.floors.clone();
    for required in &policy.required_floors {
        if !floors.iter().any(|f| &f.metric == required) {
            floors.push(Floor {
                metric: required.clone(),
                min: 0.0,
            });
        }
    }
    check_against(attempt, &floors)
}

/// Consistency checks of an attempt against its task and the registry; an empty list means valid.
///
/// Codes: `task_id_mismatch`, `family_mismatch`, `unknown_model` (any model the candidate names,
/// judge, auditor and finisher included), `tier_mismatch` (the candidate's registry tier differs
/// from `attempt.tier`) and `recorded_before_task_created` (`recorded_at < task.created_at`,
/// compared as RFC 3339 UTC strings). The last one is how a ledger shows the floors, written with
/// the task, predate the results.
pub fn validate_attempt(task: &Task, attempt: &Attempt, registry: &Registry) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut push = |code: &str, message: String| {
        issues.push(Issue {
            code: code.to_string(),
            message,
        })
    };
    if attempt.task_id != task.task_id {
        push(
            "task_id_mismatch",
            format!(
                "attempt {} names task `{}`, expected `{}`",
                attempt.attempt_id, attempt.task_id, task.task_id
            ),
        );
    }
    if attempt.family != task.family {
        push(
            "family_mismatch",
            format!(
                "attempt {} names family `{}`, expected `{}`",
                attempt.attempt_id, attempt.family, task.family
            ),
        );
    }
    for id in attempt.candidate.model_ids() {
        if registry.model(id).is_none() {
            push(
                "unknown_model",
                format!("attempt {} names unknown model `{id}`", attempt.attempt_id),
            );
        }
    }
    if let Some(tier) = attempt.candidate.tier(registry) {
        if tier != attempt.tier {
            push(
                "tier_mismatch",
                format!(
                    "attempt {} records tier `{}` but its candidate is in tier `{}`",
                    attempt.attempt_id,
                    attempt.tier.as_str(),
                    tier.as_str()
                ),
            );
        }
    }
    if attempt.recorded_at.as_str() < task.created_at.as_str() {
        push(
            "recorded_before_task_created",
            format!(
                "attempt {} recorded at {} before task {} was created at {}",
                attempt.attempt_id, attempt.recorded_at, task.task_id, task.created_at
            ),
        );
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::ModelRef;
    use crate::testutil::{attempt, good, single, task_classify};

    fn issue_codes(t: &Task, a: &Attempt) -> Vec<String> {
        validate_attempt(t, a, &Registry::example())
            .into_iter()
            .map(|i| i.code)
            .collect()
    }

    #[test]
    fn floors_validate_attempt_clean_attempt_has_no_issues() {
        let t = task_classify();
        let a = attempt(&t, "a1", single("jev", "none"), Outcome::Answered, good());
        assert!(a.recorded_at > t.created_at);
        assert_eq!(issue_codes(&t, &a), Vec::<String>::new());
    }

    #[test]
    fn floors_validate_attempt_recorded_before_task_created() {
        let t = task_classify();
        let mut a = attempt(&t, "a1", single("jev", "none"), Outcome::Answered, good());
        a.recorded_at = "2026-10-19T09:59:59Z".into();
        assert_eq!(issue_codes(&t, &a), vec!["recorded_before_task_created"]);
    }

    #[test]
    fn floors_validate_attempt_unknown_model_and_mismatches() {
        let t = task_classify();
        let a = attempt(
            &t,
            "a1",
            single("no-such-model", "none"),
            Outcome::Answered,
            good(),
        );
        assert_eq!(issue_codes(&t, &a), vec!["unknown_model"]);
        // an unknown judge inside a panel is caught too
        let panel = Candidate::Panel {
            members: vec![ModelRef {
                model_id: "jev".into(),
                reasoning_level: "none".into(),
            }],
            judge: ModelRef {
                model_id: "ghost-judge".into(),
                reasoning_level: "none".into(),
            },
        };
        let p = attempt(&t, "a2", panel, Outcome::Answered, good());
        assert_eq!(issue_codes(&t, &p), vec!["unknown_model"]);
        let mut m = attempt(&t, "a3", single("jev", "none"), Outcome::Answered, good());
        m.task_id = "other-task".into();
        m.family = "other.family".into();
        m.tier = Tier::Frontier;
        assert_eq!(
            issue_codes(&t, &m),
            vec!["task_id_mismatch", "family_mismatch", "tier_mismatch"]
        );
    }

    #[test]
    fn floors_missing_completeness_fails_with_null_observed() {
        let t = task_classify();
        let a = attempt(
            &t,
            "a1",
            single("jev", "none"),
            Outcome::Answered,
            &[("quality", 0.95), ("accuracy", 0.95)],
        );
        let r = check_floors(&t, &a, &Policy::default());
        assert!(!r.passed);
        assert_eq!(
            r.failures,
            vec![FloorFailure {
                metric: "completeness".into(),
                min: 0.9,
                observed: None
            }]
        );
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["failures"][0]["observed"], serde_json::Value::Null);
    }

    #[test]
    fn floors_below_min_fails_with_observed_value_and_pass_needs_answered() {
        let t = task_classify();
        let a = attempt(
            &t,
            "a1",
            single("jev", "none"),
            Outcome::Answered,
            &[("quality", 0.95), ("accuracy", 0.5), ("completeness", 0.95)],
        );
        let r = check_floors(&t, &a, &Policy::default());
        assert!(!r.passed);
        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].metric, "accuracy");
        assert_eq!(r.failures[0].observed, Some(0.5));

        let ok = attempt(&t, "a2", single("jev", "none"), Outcome::Answered, good());
        assert!(check_floors(&t, &ok, &Policy::default()).passed);
        let abstained = attempt(&t, "a3", single("jev", "none"), Outcome::Abstained, good());
        let r = check_floors(&t, &abstained, &Policy::default());
        assert!(!r.passed);
        assert!(r.failures.is_empty());
    }

    #[test]
    fn floors_required_metric_without_task_floor_still_must_be_present() {
        let mut t = task_classify();
        t.floors.retain(|f| f.metric != "completeness");
        let a = attempt(
            &t,
            "a1",
            single("jev", "none"),
            Outcome::Answered,
            &[("quality", 0.95), ("accuracy", 0.95)],
        );
        let r = check_floors(&t, &a, &Policy::default());
        assert!(!r.passed);
        assert_eq!(r.failures[0].metric, "completeness");
        assert_eq!(r.failures[0].observed, None);
    }

    #[test]
    fn floors_attempt_judge_ref_field_name() {
        let t = task_classify();
        let a = attempt(&t, "a1", single("jev", "none"), Outcome::Answered, good());
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["judge"]["ref"], "exact-match");
        assert_eq!(v["candidate"]["kind"], "single");
        let back: Attempt = serde_json::from_value(v).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn attempt_without_schema_version_deserializes_with_default() {
        let t = task_classify();
        let a = attempt(&t, "a1", single("jev", "none"), Outcome::Answered, good());
        let mut v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["schema_version"], "tier-drop.attempt/1");
        v.as_object_mut().unwrap().remove("schema_version");
        let back: Attempt = serde_json::from_value(v).unwrap();
        assert_eq!(back.schema_version, ATTEMPT_SCHEMA_VERSION);
        assert_eq!(back, a);
        let reserialised = serde_json::to_value(&back).unwrap();
        assert_eq!(reserialised["schema_version"], "tier-drop.attempt/1");
        let mut tv = serde_json::to_value(&t).unwrap();
        tv.as_object_mut().unwrap().remove("schema_version");
        let task: Task = serde_json::from_value(tv).unwrap();
        assert_eq!(task.schema_version, "tier-drop.task/1");
    }
}
