//! Routing policy: floors, ranking bands, disqualification thresholds, compositions and holdout.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The default policy file, embedded at compile time.
pub const POLICY_DEFAULT_JSON: &str = include_str!("../../../registry/policy.default.json");

/// Routing policy (`tier-drop.policy/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Always `tier-drop.policy/1`.
    pub schema_version: String,
    /// Try a tailored model first when one is bound to the task family.
    pub tailored_first: bool,
    /// Metrics every task must set a floor for; their mean is the ranking quality.
    pub required_floors: Vec<String>,
    /// Absolute quality difference treated as indifferent when ranking.
    pub quality_band: f64,
    /// Relative cost difference treated as indifferent when ranking.
    pub cost_band: f64,
    /// When recorded evidence disqualifies a tier.
    pub disqualify: DisqualifyPolicy,
    /// Panel composition settings.
    pub panel: PanelPolicy,
    /// Cascade composition settings.
    pub cascade: CascadePolicy,
    /// Whether deployments without observed QoS pass the QoS gate.
    pub allow_unmeasured_qos: bool,
    /// Offer open-weight singles before closed-weight ones within a tier (default true).
    #[serde(default = "default_true")]
    pub prefer_open_weight: bool,
    /// Offer singles on local deployments before remote ones within a tier (default true).
    #[serde(default = "default_true")]
    pub prefer_local: bool,
    /// Held-out split settings.
    pub holdout: HoldoutPolicy,
}

/// Evidence threshold for a `floors_not_met` disqualification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisqualifyPolicy {
    /// Minimum number of attempts before a tier can be disqualified.
    pub min_attempts: u64,
    /// Disqualify when the pass rate is at or below this value.
    pub max_pass_rate: f64,
    /// Require an attempt for every candidate the tier offers before `floors_not_met` (default true).
    #[serde(default = "default_true")]
    pub require_full_coverage: bool,
}

fn default_true() -> bool {
    true
}

/// Panel composition settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelPolicy {
    /// Maximum number of panel members.
    pub max_members: u64,
}

/// Cascade composition settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CascadePolicy {
    /// Fraction of passed-through answers the auditor checks.
    pub audit_sample_rate: f64,
}

/// Held-out split settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoldoutPolicy {
    /// Target fraction of source families held out.
    pub fraction: f64,
    /// Seed hashed with each source family.
    pub seed: u64,
}

impl Default for Policy {
    /// The embedded `registry/policy.default.json`.
    fn default() -> Self {
        // The file is part of the crate and covered by a unit test, so parsing cannot fail at runtime.
        serde_json::from_str(POLICY_DEFAULT_JSON).expect("embedded policy.default.json is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_default_equals_the_file_and_contract_values() {
        let p = Policy::default();
        let file: serde_json::Value = serde_json::from_str(POLICY_DEFAULT_JSON).unwrap();
        assert_eq!(serde_json::to_value(&p).unwrap(), file);
        assert_eq!(p.schema_version, "tier-drop.policy/1");
        assert!(p.tailored_first);
        assert_eq!(
            p.required_floors,
            vec!["quality", "accuracy", "completeness"]
        );
        assert_eq!(p.quality_band, 0.02);
        assert_eq!(p.cost_band, 0.10);
        assert_eq!(p.disqualify.min_attempts, 5);
        assert_eq!(p.disqualify.max_pass_rate, 0.5);
        assert!(p.disqualify.require_full_coverage);
        assert_eq!(p.panel.max_members, 3);
        assert_eq!(p.cascade.audit_sample_rate, 0.1);
        assert!(!p.allow_unmeasured_qos);
        assert!(p.prefer_open_weight);
        assert!(p.prefer_local);
        assert_eq!(p.holdout.fraction, 0.2);
        assert_eq!(p.holdout.seed, 20261019);
    }

    #[test]
    fn policy_rejects_unknown_fields() {
        let mut v: serde_json::Value = serde_json::from_str(POLICY_DEFAULT_JSON).unwrap();
        v["surprise"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Policy>(v).is_err());
    }
}
