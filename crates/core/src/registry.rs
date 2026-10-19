//! The provisioned-model registry: deployments, models by tier, embedding helpers, and `validate_registry`.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::task::{Task, TaskShape};
use crate::tier::Tier;

/// The example registry file, embedded at compile time.
pub const REGISTRY_EXAMPLE_JSON: &str = include_str!("../../../registry/registry.example.json");

/// A validation finding: a short snake_case code and a readable message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    /// Short snake_case code, e.g. `unknown_deployment`.
    pub code: String,
    /// What is wrong and where.
    pub message: String,
}

/// Where a deployment runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Locality {
    /// On the operator's own equipment.
    Local,
    /// On a remote provider; task data leaves the operator's equipment.
    Remote,
}

/// QoS observed for a deployment over a window (measured values, supplied by the operator or harness).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QosObserved {
    /// Fraction of requests served (0..=1).
    pub availability: f64,
    /// 95th percentile latency in milliseconds.
    pub p95_latency_ms: u64,
    /// Fraction of requests that errored (0..=1).
    pub error_rate: f64,
    /// First date of the observation window.
    pub window_started: String,
    /// Last date of the observation window.
    pub window_ended: String,
    /// Number of requests observed.
    pub sample_count: u64,
}

/// A place models are served from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    /// Deployment id referenced by models (an id, never a hostname).
    pub id: String,
    /// Provider label.
    pub provider: String,
    /// Local or remote.
    pub locality: Locality,
    /// Observed QoS, or null when unmeasured.
    pub qos: Option<QosObserved>,
}

/// Price of a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    /// USD per million input tokens.
    pub input_usd_per_mtok: f64,
    /// USD per million output tokens.
    pub output_usd_per_mtok: f64,
}

impl Cost {
    /// Ordering key for "cheapest": input plus output price per million tokens.
    // decision: the contract orders by "cost" without naming a token mix; the sum of the
    // two prices is used, which ranks models the same way for any mix where both prices
    // move together and needs no per-task token estimate.
    pub fn key(&self) -> f64 {
        self.input_usd_per_mtok + self.output_usd_per_mtok
    }
}

/// Speed of a model (null when unknown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Speed {
    /// Output tokens per second.
    pub output_tokens_per_second: Option<f64>,
    /// Median latency in milliseconds.
    pub median_latency_ms: Option<u64>,
}

/// A provisioned model that can be a routing candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// Model id.
    pub id: String,
    /// Human-readable name.
    pub display_name: String,
    /// The tier the model belongs to.
    pub tier: Tier,
    /// The deployment serving it.
    pub deployment_id: String,
    /// Whether the weights are open.
    pub open_weight: bool,
    /// Reasoning levels in preference order; `["none"]` when the model has no levels.
    pub reasoning_levels: Vec<String>,
    /// Task shapes served; empty means any shape.
    pub shapes: Vec<TaskShape>,
    /// Task families bound; empty means any family (not allowed for tiers 2 and 5).
    pub families: Vec<String>,
    /// Price.
    pub cost: Cost,
    /// Speed.
    pub speed: Speed,
    /// Context window in tokens.
    pub context_window: u64,
    /// Free text.
    pub notes: String,
}

impl Model {
    /// Whether the model can serve the task's shape and family.
    pub fn fits(&self, task: &Task) -> bool {
        self.fits_family(&task.family)
            && (self.shapes.is_empty() || self.shapes.contains(&task.shape))
    }

    /// Whether the model's `families` is empty or contains `family`.
    pub fn fits_family(&self, family: &str) -> bool {
        self.families.is_empty() || self.families.iter().any(|f| f == family)
    }

    /// Whether `families` explicitly contains `family`.
    pub fn binds_family(&self, family: &str) -> bool {
        self.families.iter().any(|f| f == family)
    }

    /// The first (lowest) reasoning level, or `none` when the list is empty.
    pub fn lowest_level(&self) -> &str {
        self.reasoning_levels
            .first()
            .map(String::as_str)
            .unwrap_or("none")
    }
}

/// An embedding model: a helper tool usable by any tier, never a routing candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingModel {
    /// Embedding model id (must not equal any model id).
    pub id: String,
    /// The deployment serving it.
    pub deployment_id: String,
    /// Vector dimensions.
    pub dimensions: u64,
    /// Free text.
    pub notes: String,
}

/// The registry (`tier-drop.registry/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    /// Always `tier-drop.registry/1`.
    pub schema_version: String,
    /// YYYY-MM-DD date the model names and values were current; binds disqualifications.
    pub as_of: String,
    /// Free text about the registry as a whole.
    // decision: the contract has no registry-level notes; a defaulted `notes` field is added
    // so the example can say once that its numbers are example values, not measurements.
    #[serde(default)]
    pub notes: String,
    /// Deployments.
    pub deployments: Vec<Deployment>,
    /// Routing-candidate models.
    pub models: Vec<Model>,
    /// Embedding helpers.
    pub embeddings: Vec<EmbeddingModel>,
}

impl Registry {
    /// The embedded `registry/registry.example.json`.
    pub fn example() -> Registry {
        // The file is part of the crate and covered by a unit test, so parsing cannot fail at runtime.
        serde_json::from_str(REGISTRY_EXAMPLE_JSON)
            .expect("embedded registry.example.json is valid")
    }

    /// Look up a model by id.
    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Look up a deployment by id.
    pub fn deployment(&self, id: &str) -> Option<&Deployment> {
        self.deployments.iter().find(|d| d.id == id)
    }
}

fn issue(code: &str, message: String) -> Issue {
    Issue {
        code: code.to_string(),
        message,
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Structural checks on a registry; an empty list means valid.
pub fn validate_registry(registry: &Registry) -> Vec<Issue> {
    let mut issues = Vec::new();
    if registry.schema_version != "tier-drop.registry/1" {
        issues.push(issue(
            "unsupported_schema_version",
            format!(
                "schema_version `{}` is not tier-drop.registry/1",
                registry.schema_version
            ),
        ));
    }
    if !is_date(&registry.as_of) {
        issues.push(issue(
            "invalid_as_of",
            format!("as_of `{}` is not YYYY-MM-DD", registry.as_of),
        ));
    }
    let mut deployment_ids = BTreeSet::new();
    for d in &registry.deployments {
        if !deployment_ids.insert(d.id.as_str()) {
            issues.push(issue(
                "duplicate_deployment_id",
                format!("deployment id `{}` appears more than once", d.id),
            ));
        }
    }
    let mut model_ids = BTreeSet::new();
    for m in &registry.models {
        if !model_ids.insert(m.id.as_str()) {
            issues.push(issue(
                "duplicate_model_id",
                format!("model id `{}` appears more than once", m.id),
            ));
        }
        if !deployment_ids.contains(m.deployment_id.as_str()) {
            issues.push(issue(
                "unknown_deployment",
                format!(
                    "model `{}` names unknown deployment `{}`",
                    m.id, m.deployment_id
                ),
            ));
        }
        // decision: an empty `shapes` means any shape, which includes non-decision shapes,
        // so a tier-1 model must list its decision shapes explicitly.
        if m.tier == Tier::SystemOne
            && (m.shapes.is_empty() || m.shapes.iter().any(|s| !s.is_decision()))
        {
            issues.push(issue(
                "tier1_non_decision_shape",
                format!(
                    "tier-1 model `{}` must list only decision shapes (classify, choose, route, yes_no)",
                    m.id
                ),
            ));
        }
        if matches!(m.tier, Tier::Specialised | Tier::Tailored) && m.families.is_empty() {
            issues.push(issue(
                "empty_families",
                format!(
                    "{} model `{}` must bind at least one task family",
                    m.tier.as_str(),
                    m.id
                ),
            ));
        }
        if m.reasoning_levels.is_empty() {
            issues.push(issue(
                "empty_reasoning_levels",
                format!(
                    "model `{}` lists no reasoning levels (use [\"none\"])",
                    m.id
                ),
            ));
        }
        let c = &m.cost;
        if !(c.input_usd_per_mtok >= 0.0 && c.output_usd_per_mtok >= 0.0) {
            issues.push(issue(
                "negative_cost",
                format!("model `{}` has a negative or non-numeric cost", m.id),
            ));
        }
    }
    let mut embedding_ids = BTreeSet::new();
    for e in &registry.embeddings {
        if model_ids.contains(e.id.as_str()) {
            issues.push(issue(
                "embedding_id_collision",
                format!(
                    "embedding model `{}` reuses a model id; embeddings are helpers and cannot be routing candidates",
                    e.id
                ),
            ));
        }
        if !embedding_ids.insert(e.id.as_str()) {
            issues.push(issue(
                "duplicate_embedding_id",
                format!("embedding id `{}` appears more than once", e.id),
            ));
        }
        if !deployment_ids.contains(e.deployment_id.as_str()) {
            issues.push(issue(
                "unknown_deployment",
                format!(
                    "embedding model `{}` names unknown deployment `{}`",
                    e.id, e.deployment_id
                ),
            ));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(r: &Registry) -> Vec<String> {
        validate_registry(r).into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn registry_example_parses_and_is_valid() {
        let r = Registry::example();
        assert_eq!(validate_registry(&r), vec![]);
        assert_eq!(r.as_of, "2026-10-19");
        assert!(r.notes.contains("example values, not measurements"));
        for t in Tier::ALL {
            assert!(r.models.iter().any(|m| m.tier == t), "no {t:?} model");
        }
        assert_eq!(r.embeddings.len(), 2);
        let text = REGISTRY_EXAMPLE_JSON.to_lowercase();
        for banned in ["http", "://", "www.", "api_key", "apikey", "secret", "@"] {
            assert!(!text.contains(banned), "example contains `{banned}`");
        }
    }

    #[test]
    fn registry_rejects_tier1_model_with_non_decision_shape() {
        let mut r = Registry::example();
        let m = r
            .models
            .iter_mut()
            .find(|m| m.tier == Tier::SystemOne)
            .unwrap();
        m.shapes.push(TaskShape::Generate);
        assert_eq!(codes(&r), vec!["tier1_non_decision_shape"]);
    }

    #[test]
    fn registry_rejects_tier2_and_tier5_models_without_families() {
        let mut r = Registry::example();
        for m in r.models.iter_mut() {
            if matches!(m.tier, Tier::Specialised | Tier::Tailored) {
                m.families.clear();
            }
        }
        let n = r
            .models
            .iter()
            .filter(|m| matches!(m.tier, Tier::Specialised | Tier::Tailored))
            .count();
        let c = codes(&r);
        assert_eq!(c.len(), n);
        assert!(c.iter().all(|c| c == "empty_families"));
        assert!(n >= 4);
    }

    #[test]
    fn registry_rejects_unknown_deployment() {
        let mut r = Registry::example();
        r.models[0].deployment_id = "nowhere".into();
        assert_eq!(codes(&r), vec!["unknown_deployment"]);
    }

    #[test]
    fn registry_rejects_embedding_offered_as_candidate() {
        let mut r = Registry::example();
        r.embeddings[0].id = r.models[3].id.clone();
        assert_eq!(codes(&r), vec!["embedding_id_collision"]);
        // An embedding cannot be written as a model: the embedding shape is not a Model.
        let v = serde_json::to_value(&Registry::example().embeddings[0]).unwrap();
        assert!(serde_json::from_value::<Model>(v).is_err());
    }

    #[test]
    fn registry_rejects_duplicates_empty_levels_and_negative_cost() {
        let mut r = Registry::example();
        let dup = r.models[5].clone();
        r.models.push(dup);
        r.models[4].reasoning_levels.clear();
        r.models[6].cost.output_usd_per_mtok = -1.0;
        assert_eq!(
            codes(&r),
            vec![
                "empty_reasoning_levels",
                "negative_cost",
                "duplicate_model_id"
            ]
        );
    }

    #[test]
    fn registry_rejects_unknown_fields() {
        let mut v: serde_json::Value = serde_json::from_str(REGISTRY_EXAMPLE_JSON).unwrap();
        v["models"][0]["hostname"] = serde_json::json!("x");
        assert!(serde_json::from_value::<Registry>(v).is_err());
    }
}
