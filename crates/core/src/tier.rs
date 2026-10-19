//! Compute tiers, ordered from cheapest (system-one) to tailored.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A compute tier. Variant order equals the ordinal: system_one 1 … tailored 5.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Tier 1: decision-shaped models (classify, choose, route, yes/no).
    SystemOne,
    /// Tier 2: small task-specialised models bound to task families.
    Specialised,
    /// Tier 3: inexpensive frontier-adjacent generalists.
    CheapGeneralist,
    /// Tier 4: frontier and frontier-lite models.
    Frontier,
    /// Tier 5: tailored (fine-tuned, pruned, RL-trained) models bound to a family.
    Tailored,
}

impl Tier {
    /// All five tiers in ordinal order.
    pub const ALL: [Tier; 5] = [
        Tier::SystemOne,
        Tier::Specialised,
        Tier::CheapGeneralist,
        Tier::Frontier,
        Tier::Tailored,
    ];

    /// Ordinal 1..=5 as named in the commission.
    pub fn ordinal(self) -> u8 {
        match self {
            Tier::SystemOne => 1,
            Tier::Specialised => 2,
            Tier::CheapGeneralist => 3,
            Tier::Frontier => 4,
            Tier::Tailored => 5,
        }
    }

    /// The snake_case name used in JSON and in hashed ids.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::SystemOne => "system_one",
            Tier::Specialised => "specialised",
            Tier::CheapGeneralist => "cheap_generalist",
            Tier::Frontier => "frontier",
            Tier::Tailored => "tailored",
        }
    }

    /// The default ladder without tailored models: tiers 1 to 4 in order.
    pub fn ladder_default() -> Vec<Tier> {
        vec![
            Tier::SystemOne,
            Tier::Specialised,
            Tier::CheapGeneralist,
            Tier::Frontier,
        ]
    }

    /// The tier that supplies judges, auditors and finishers for compositions in this tier.
    // decision: tiers 1 to 3 use the next ordinal; frontier has none (no compositions);
    // tailored uses frontier, because tailored models are trained to match work that
    // tiers 1 to 4 already satisfy, so frontier is the natural reference to judge them.
    pub fn next_up(self) -> Option<Tier> {
        match self {
            Tier::SystemOne => Some(Tier::Specialised),
            Tier::Specialised => Some(Tier::CheapGeneralist),
            Tier::CheapGeneralist => Some(Tier::Frontier),
            Tier::Frontier => None,
            Tier::Tailored => Some(Tier::Frontier),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tier_ordinals_and_names() {
        let ords: Vec<u8> = Tier::ALL.iter().map(|t| t.ordinal()).collect();
        assert_eq!(ords, vec![1, 2, 3, 4, 5]);
        assert!(Tier::SystemOne < Tier::Tailored);
        assert_eq!(
            serde_json::to_string(&Tier::CheapGeneralist).unwrap(),
            "\"cheap_generalist\""
        );
        for t in Tier::ALL {
            assert_eq!(
                serde_json::to_value(t).unwrap(),
                serde_json::Value::String(t.as_str().to_string())
            );
        }
    }
}
