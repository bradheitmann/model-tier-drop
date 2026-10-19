//! The task record, its shape, floors, QoS requirement and review profile, and `validate_task`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::policy::Policy;
use crate::registry::Issue;
use crate::rights::Rights;

/// The shape of work a task asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskShape {
    /// Assign one label from a fixed set.
    Classify,
    /// Choose among fixed options.
    Choose,
    /// Pick a route or handler.
    Route,
    /// Answer yes or no.
    YesNo,
    /// Extract fields or values from an input.
    Extract,
    /// Transform an input into another form.
    Transform,
    /// Generate new text or content.
    Generate,
    /// Review or critique an input.
    Review,
    /// Summarise an input.
    Summarise,
    /// Write or change code.
    Code,
    /// Anything else.
    Other,
}

impl TaskShape {
    /// True for the decision shapes a tier-1 model may serve: classify, choose, route, yes_no.
    pub fn is_decision(self) -> bool {
        matches!(
            self,
            TaskShape::Classify | TaskShape::Choose | TaskShape::Route | TaskShape::YesNo
        )
    }

    /// The snake_case name used in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskShape::Classify => "classify",
            TaskShape::Choose => "choose",
            TaskShape::Route => "route",
            TaskShape::YesNo => "yes_no",
            TaskShape::Extract => "extract",
            TaskShape::Transform => "transform",
            TaskShape::Generate => "generate",
            TaskShape::Review => "review",
            TaskShape::Summarise => "summarise",
            TaskShape::Code => "code",
            TaskShape::Other => "other",
        }
    }
}

/// A quality floor set before results are seen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Floor {
    /// Metric name in the attempt's metric vector.
    pub metric: String,
    /// Minimum acceptable value.
    pub min: f64,
}

/// The QoS a deployment must meet to be considered at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QosRequirement {
    /// Minimum observed availability (0..=1).
    pub availability_min: f64,
    /// Maximum observed p95 latency in milliseconds.
    pub p95_latency_max_ms: u64,
    /// Maximum observed error rate (0..=1).
    pub error_rate_max: f64,
}

/// Review profile: strict by default; sampled only for a named low-risk class with a recorded decision.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Review {
    /// Every assertion is judged under the strict rule.
    #[default]
    Strict,
    /// Sampled review for a named class, authorised by a recorded decision.
    Sampled {
        /// The named low-risk class.
        class: String,
        /// Reference to the decision that authorised sampling for the class.
        decision_ref: String,
    },
}

/// Schema version written on every `Task` record.
pub const TASK_SCHEMA_VERSION: &str = "tier-drop.task/1";

fn default_task_schema_version() -> String {
    TASK_SCHEMA_VERSION.to_string()
}

/// One model-using task, written (with its floors) before any attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// Record schema version; defaults to `tier-drop.task/1` when omitted on input.
    #[serde(default = "default_task_schema_version")]
    pub schema_version: String,
    /// Caller-supplied task id.
    pub task_id: String,
    /// The activity (task family) disqualifications are recorded against.
    pub family: String,
    /// The shape of work.
    pub shape: TaskShape,
    /// SHA-256 of the task input.
    pub input_sha256: String,
    /// Rights over the task's input and outputs.
    pub rights: Rights,
    /// Quality floors; must include every `policy.required_floors` metric.
    pub floors: Vec<Floor>,
    /// QoS cutoff applied before scoring.
    pub qos: QosRequirement,
    /// Review profile; defaults to strict.
    #[serde(default)]
    pub review: Review,
    /// Free-form string flags; `financial`, `regulated` and `numeric` set to "true" restrict review.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// RFC 3339 UTC creation time supplied by the caller.
    pub created_at: String,
    /// SHA-256 over the canonical JSON without this field; empty until the caller seals the
    /// record (the CLI fills it before appending to the ledger).
    #[serde(default)]
    pub record_sha256: String,
}

impl Task {
    fn flag(&self, key: &str) -> bool {
        self.metadata.get(key).map(String::as_str) == Some("true")
    }
}

fn issue(code: &str, message: String) -> Issue {
    Issue {
        code: code.to_string(),
        message,
    }
}

/// Check a task against the policy: required floors present, floor ranges, and the strict-review rule.
pub fn validate_task(task: &Task, policy: &Policy) -> Vec<Issue> {
    let mut issues = Vec::new();
    for required in &policy.required_floors {
        if !task.floors.iter().any(|f| &f.metric == required) {
            issues.push(issue(
                "missing_required_floor",
                format!(
                    "task {} has no floor for required metric `{required}`",
                    task.task_id
                ),
            ));
        }
    }
    for f in &task.floors {
        let raw = f.metric.ends_with("_raw");
        if !f.min.is_finite() || (!raw && !(0.0..=1.0).contains(&f.min)) {
            issues.push(issue(
                "floor_out_of_range",
                format!(
                    "floor `{}` min {} is outside 0..=1 (only `_raw` metrics may exceed it)",
                    f.metric, f.min
                ),
            ));
        }
    }
    if let Review::Sampled {
        class,
        decision_ref,
    } = &task.review
    {
        if class.trim().is_empty() || decision_ref.trim().is_empty() {
            issues.push(issue(
                "sampled_review_incomplete",
                "sampled review requires a non-empty `class` and `decision_ref`".to_string(),
            ));
        }
        let numeric_extract = task.shape == TaskShape::Extract && task.flag("numeric");
        if task.flag("financial") || task.flag("regulated") || numeric_extract {
            issues.push(issue(
                "sampled_review_forbidden",
                format!(
                    "task {} feeds a financial figure, a regulated disclosure or a numeric extraction; review must be strict",
                    task.task_id
                ),
            ));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{task_classify, task_generate};

    fn codes(issues: &[Issue]) -> Vec<&str> {
        issues.iter().map(|i| i.code.as_str()).collect()
    }

    fn sampled() -> Review {
        Review::Sampled {
            class: "low_risk_label".into(),
            decision_ref: "decision-review-001".into(),
        }
    }

    #[test]
    fn review_defaults_to_strict_when_absent() {
        let mut v = serde_json::to_value(task_classify()).unwrap();
        v.as_object_mut().unwrap().remove("review");
        let t: Task = serde_json::from_value(v).unwrap();
        assert_eq!(t.review, Review::Strict);
        assert_eq!(serde_json::to_value(&t.review).unwrap(), "strict");
    }

    #[test]
    fn review_sampled_forbidden_for_financial_task() {
        let mut t = task_classify();
        t.review = sampled();
        t.metadata.insert("financial".into(), "true".into());
        assert_eq!(
            codes(&validate_task(&t, &Policy::default())),
            vec!["sampled_review_forbidden"]
        );
    }

    #[test]
    fn review_sampled_forbidden_for_regulated_and_numeric_extract() {
        let p = Policy::default();
        let mut t = task_classify();
        t.review = sampled();
        t.metadata.insert("regulated".into(), "true".into());
        assert_eq!(
            codes(&validate_task(&t, &p)),
            vec!["sampled_review_forbidden"]
        );

        let mut e = task_classify();
        e.shape = TaskShape::Extract;
        e.review = sampled();
        e.metadata.insert("numeric".into(), "true".into());
        assert_eq!(
            codes(&validate_task(&e, &p)),
            vec!["sampled_review_forbidden"]
        );
    }

    #[test]
    fn review_sampled_needs_class_and_decision_ref_and_is_allowed_otherwise() {
        let p = Policy::default();
        let mut t = task_generate();
        t.review = sampled();
        assert!(validate_task(&t, &p).is_empty());
        t.review = Review::Sampled {
            class: "".into(),
            decision_ref: "d".into(),
        };
        assert_eq!(
            codes(&validate_task(&t, &p)),
            vec!["sampled_review_incomplete"]
        );
        let parsed: Review =
            serde_json::from_str(r#"{"sampled":{"class":"c","decision_ref":"d"}}"#).unwrap();
        assert_eq!(
            parsed,
            Review::Sampled {
                class: "c".into(),
                decision_ref: "d".into()
            }
        );
    }

    #[test]
    fn floors_validate_task_requires_every_policy_floor_and_range() {
        let p = Policy::default();
        let mut t = task_classify();
        t.floors.retain(|f| f.metric != "completeness");
        t.floors.push(Floor {
            metric: "accuracy_raw".into(),
            min: 42.0,
        });
        t.floors.push(Floor {
            metric: "f1".into(),
            min: 1.5,
        });
        assert_eq!(
            codes(&validate_task(&t, &p)),
            vec!["missing_required_floor", "floor_out_of_range"]
        );
    }

    #[test]
    fn floors_task_record_carries_schema_version_and_record_sha256() {
        let mut v = serde_json::to_value(task_classify()).unwrap();
        assert_eq!(v["schema_version"], "tier-drop.task/1");
        assert_eq!(v["record_sha256"], "");
        let obj = v.as_object_mut().unwrap();
        obj.remove("schema_version");
        obj.remove("record_sha256");
        let t: Task = serde_json::from_value(v).unwrap();
        assert_eq!(t.schema_version, TASK_SCHEMA_VERSION);
        assert!(t.record_sha256.is_empty());
    }
}
