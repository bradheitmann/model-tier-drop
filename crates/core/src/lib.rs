//! tier-drop-core: the routing and evidence engine for tier-drop model routing.
//!
//! The library plans which model candidates to try for a task, gates them on
//! QoS, checks quality floors, assesses disqualification of a tier for a task
//! family, decides the winner, splits held-out sets by source family and
//! filters dataset rows by rights. It never calls a model and never touches the
//! filesystem, the clock or the network: ids, timestamps and `as_of` dates are
//! plain strings supplied by the caller.
//!
//! Judges, auditors and finishers for compositions come from [`Tier::next_up`]: system_one to
//! specialised to cheap_generalist to frontier, and tailored to frontier; frontier has none, so
//! it offers no compositions.
//!
//! Structural skips (`shape_mismatch`, `no_provisioned_candidates`,
//! `no_qos_qualified_candidates`) and `disqualified` are reported in the plan's `skipped` list
//! and as `skipped` or `disqualified` entries in `decide`'s `tiers[]`. `plan_next` and `decide`
//! never create `Disqualification` records for them; only `assess_disqualification` (on recorded
//! evidence) produces one in the normal flow. `structural_disqualification` is a separate helper
//! for a caller that chooses to record a structural reason itself.
//!
//! Disqualifications passed in are treated as in force by family and registry date only. The
//! engine has no clock, so the caller (the CLI) drops expired records before passing them in.

pub mod attempt;
pub mod candidate;
pub mod canonical;
pub mod decision;
pub mod disqualification;
pub mod eval;
pub mod plan;
pub mod policy;
pub mod qos;
pub mod registry;
pub mod rights;
pub mod task;
pub mod tier;

#[cfg(test)]
mod testutil;

use std::collections::BTreeMap;

pub use attempt::ATTEMPT_SCHEMA_VERSION;
pub use attempt::{
    check_floors, Attempt, FloorFailure, FloorResult, Judge, JudgeKind, Outcome, Tokens,
};
pub use candidate::{Candidate, ModelRef};
pub use canonical::{canonical_json, record_sha256, sha256_hex};
pub use decision::ROUTING_DECISION_SCHEMA_VERSION;
pub use decision::{decide, RankEntry, RoutingDecision, Score, TierRecord, TierStatus, Winner};
pub use disqualification::DISQUALIFICATION_SCHEMA_VERSION;
pub use disqualification::{
    assess_disqualification, structural_disqualification, Disqualification, DisqualificationReason,
};
pub use eval::{
    split_holdout, summarise_eval, EvalItem, EvalRun, EvalSplit, Oracle, Split, SplitItem,
};
pub use eval::{EVAL_RUN_SCHEMA_VERSION, EVAL_SPLIT_SCHEMA_VERSION};
pub use plan::{ladder, plan_next, Plan, SkipReason, SkippedTier};
pub use policy::{CascadePolicy, DisqualifyPolicy, HoldoutPolicy, PanelPolicy, Policy};
pub use qos::{qos_gate, ExclusionReason, QosExclusion, QosGateResult};
pub use registry::{
    validate_registry, Cost, Deployment, EmbeddingModel, Issue, Locality, Model, QosObserved,
    Registry, Speed,
};
pub use rights::{dataset_filter, Purpose, Rights};
pub use task::TASK_SCHEMA_VERSION;
pub use task::{validate_task, Floor, QosRequirement, Review, Task, TaskShape};
pub use tier::Tier;

/// JSON Schemas for every record type, keyed by the names the CLI `schemas` command accepts.
pub fn schemas() -> BTreeMap<&'static str, schemars::Schema> {
    let mut out = BTreeMap::new();
    out.insert("registry", schemars::schema_for!(Registry));
    out.insert("policy", schemars::schema_for!(Policy));
    out.insert("task", schemars::schema_for!(Task));
    out.insert("attempt", schemars::schema_for!(Attempt));
    out.insert("disqualification", schemars::schema_for!(Disqualification));
    out.insert("routing_decision", schemars::schema_for!(RoutingDecision));
    out.insert("eval_item", schemars::schema_for!(EvalItem));
    out.insert("eval_split", schemars::schema_for!(EvalSplit));
    out.insert("eval_run", schemars::schema_for!(EvalRun));
    out.insert("plan", schemars::schema_for!(Plan));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_cover_all_ten_names_with_type_titles() {
        let s = schemas();
        let names: Vec<&str> = s.keys().copied().collect();
        assert_eq!(
            names,
            vec![
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
        let title = s["routing_decision"]
            .as_value()
            .get("title")
            .and_then(|t| t.as_str());
        assert_eq!(title, Some("RoutingDecision"));
    }
}
