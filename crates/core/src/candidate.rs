//! Routing candidates: a single model at a reasoning level, or a composition of them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::Registry;
use crate::tier::Tier;

/// A model at a reasoning level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    /// Registry model id.
    pub model_id: String,
    /// One of the model's reasoning levels.
    pub reasoning_level: String,
}

/// What is tried for a task: a single model or a composition pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Candidate {
    /// One model at one reasoning level.
    Single {
        /// Registry model id.
        model_id: String,
        /// Reasoning level.
        reasoning_level: String,
    },
    /// Several same-tier members whose answers a next-tier judge decides between.
    Panel {
        /// The tier-under-test members.
        members: Vec<ModelRef>,
        /// The next-tier judge.
        judge: ModelRef,
    },
    /// A same-tier first answer passed through with a next-tier auditor sampling it.
    Cascade {
        /// The tier-under-test model.
        first: ModelRef,
        /// The next-tier auditor.
        auditor: ModelRef,
        /// Fraction of answers the auditor checks.
        audit_sample_rate: f64,
    },
    /// A same-tier draft finished by a next-tier model.
    Draft {
        /// The tier-under-test drafter.
        drafter: ModelRef,
        /// The next-tier finisher.
        finisher: ModelRef,
    },
}

impl Candidate {
    /// A single candidate.
    pub fn single(model_id: &str, reasoning_level: &str) -> Candidate {
        Candidate::Single {
            model_id: model_id.to_string(),
            reasoning_level: reasoning_level.to_string(),
        }
    }

    /// The model id whose tier is under test (the single model, the first member, `first`, or `drafter`).
    pub fn primary_model_id(&self) -> Option<&str> {
        match self {
            Candidate::Single { model_id, .. } => Some(model_id),
            Candidate::Panel { members, .. } => members.first().map(|m| m.model_id.as_str()),
            Candidate::Cascade { first, .. } => Some(&first.model_id),
            Candidate::Draft { drafter, .. } => Some(&drafter.model_id),
        }
    }

    /// The tier under test, looked up in the registry; None when the model is unknown.
    pub fn tier(&self, registry: &Registry) -> Option<Tier> {
        self.primary_model_id()
            .and_then(|id| registry.model(id))
            .map(|m| m.tier)
    }

    /// Every model id the candidate names: members, judge, auditor and finisher included.
    pub fn model_ids(&self) -> Vec<&str> {
        match self {
            Candidate::Single { model_id, .. } => vec![model_id.as_str()],
            Candidate::Panel { members, judge } => members
                .iter()
                .map(|m| m.model_id.as_str())
                .chain(std::iter::once(judge.model_id.as_str()))
                .collect(),
            Candidate::Cascade { first, auditor, .. } => {
                vec![first.model_id.as_str(), auditor.model_id.as_str()]
            }
            Candidate::Draft { drafter, finisher } => {
                vec![drafter.model_id.as_str(), finisher.model_id.as_str()]
            }
        }
    }

    /// True for `single`.
    pub fn is_single(&self) -> bool {
        matches!(self, Candidate::Single { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_candidate_json_is_tagged_by_kind() {
        let c = Candidate::single("jev", "none");
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            serde_json::json!({"kind": "single", "model_id": "jev", "reasoning_level": "none"})
        );
        let p: Candidate = serde_json::from_value(serde_json::json!({
            "kind": "panel",
            "members": [{"model_id": "qwen-3.8-27b", "reasoning_level": "none"}],
            "judge": {"model_id": "mistral-4", "reasoning_level": "none"}
        }))
        .unwrap();
        assert_eq!(p.tier(&Registry::example()), Some(Tier::CheapGeneralist));
        let bad = serde_json::json!({"kind": "single", "model_id": "jev", "reasoning_level": "none", "x": 1});
        assert!(serde_json::from_value::<Candidate>(bad).is_err());
    }
}
