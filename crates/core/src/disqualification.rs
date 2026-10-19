//! Disqualification of a tier for a task family, on recorded evidence or a structural reason.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attempt::{check_floors, Attempt};
use crate::canonical::{seal, short_id};
use crate::plan::{fitting_models, structural_skip, tier_offer, SkipReason};
use crate::policy::Policy;
use crate::qos::{qos_gate, ExclusionReason};
use crate::registry::Registry;
use crate::task::Task;
use crate::tier::Tier;

/// Why a tier was disqualified for a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DisqualificationReason {
    /// Tier 1 cannot serve the family's non-decision shape.
    ShapeMismatch,
    /// No model in the tier serves the family.
    NoProvisionedCandidates,
    /// No fitting model in the tier passes the QoS gate.
    NoQosQualifiedCandidates,
    /// Every fitting model is remote and the rights forbid sending remotely.
    RightsForbidRemote,
    /// Recorded attempts fail the floors at or above the policy threshold.
    FloorsNotMet,
    /// An operator disqualified the tier by hand.
    Operator,
}

/// Schema version written on every `Disqualification` record.
pub const DISQUALIFICATION_SCHEMA_VERSION: &str = "tier-drop.disqualification/1";

fn default_disqualification_schema_version() -> String {
    DISQUALIFICATION_SCHEMA_VERSION.to_string()
}

/// A recorded disqualification of (family, tier), bound to the registry date it was decided under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Disqualification {
    /// Record schema version; defaults to `tier-drop.disqualification/1` when omitted on input.
    #[serde(default = "default_disqualification_schema_version")]
    pub schema_version: String,
    /// `dq-` followed by 16 hex characters.
    pub disqualification_id: String,
    /// The task family (activity).
    pub family: String,
    /// The tier disqualified.
    pub tier: Tier,
    /// Why.
    pub reason: DisqualificationReason,
    /// Attempt ids or eval run ids.
    pub evidence: Vec<String>,
    /// Readable specifics.
    pub detail: String,
    /// The registry `as_of` this was decided under; a newer registry invalidates it.
    pub registry_as_of: String,
    /// RFC 3339 UTC time, taken from the evidence (the engine has no clock).
    pub decided_at: String,
    /// Expiry time, or null for none. The engine has no clock and never reads this field: the
    /// caller that owns the clock (the CLI) drops expired records before passing them in.
    pub expires_at: Option<String>,
    /// SHA-256 over the canonical JSON without this field.
    #[serde(default)]
    pub record_sha256: String,
}

impl Disqualification {
    /// In force when the family matches and `registry_as_of >= registry_as_of_now` (YYYY-MM-DD
    /// string order). `expires_at` is not checked here; the caller filters expired records first.
    pub fn is_in_force(&self, family: &str, registry_as_of_now: &str) -> bool {
        self.family == family && self.registry_as_of.as_str() >= registry_as_of_now
    }

    fn sealed(mut self) -> Self {
        self.record_sha256 = seal(&self);
        self
    }
}

/// A `floors_not_met` disqualification of (family, tier), or None.
///
/// When `policy.disqualify.require_full_coverage` is true (the default), every candidate the tier
/// offers for `task` must first have at least one attempt in this family and tier (an equal
/// `Candidate` value). The candidates are the same list `plan_next` builds, ignoring which have
/// been tried: every single model × reasoning level, then the panel, cascade and draft when a
/// next-tier helper is QoS-qualified. Incomplete coverage returns None. Only then does the
/// threshold apply: at least `min_attempts` attempts with a pass rate at or below `max_pass_rate`.
/// With `require_full_coverage: false` only the threshold applies.
// decision: the contract signature has no registry, but the candidate list cannot be built
// without one, so `registry` is an added parameter. `registry_as_of` stays separate so the
// caller states which registry date the record binds to.
pub fn assess_disqualification(
    family: &str,
    tier: Tier,
    attempts: &[Attempt],
    task: &Task,
    registry: &Registry,
    registry_as_of: &str,
    policy: &Policy,
) -> Option<Disqualification> {
    let relevant: Vec<&Attempt> = attempts
        .iter()
        .filter(|a| a.family == family && a.tier == tier)
        .collect();
    if policy.disqualify.require_full_coverage {
        let gate = qos_gate(registry, task, policy);
        // decision: a structurally skipped tier offers nothing, so coverage is vacuously
        // complete and only the threshold decides.
        let covered = tier_offer(task, registry, &gate, tier, policy)
            .iter()
            .all(|c| relevant.iter().any(|a| &a.candidate == c));
        if !covered {
            return None;
        }
    }
    let n = relevant.len();
    if n == 0 || (n as u64) < policy.disqualify.min_attempts {
        return None;
    }
    let passed = relevant
        .iter()
        .filter(|a| check_floors(task, a, policy).passed)
        .count();
    let pass_rate = passed as f64 / n as f64;
    if pass_rate > policy.disqualify.max_pass_rate {
        return None;
    }
    let mut evidence: Vec<String> = relevant.iter().map(|a| a.attempt_id.clone()).collect();
    evidence.sort();
    evidence.dedup();
    // decision: id material is family, tier and the sorted attempt ids joined with `|`.
    let material = format!("{family}|{}|{}", tier.as_str(), evidence.join("|"));
    let decided_at = relevant
        .iter()
        .map(|a| a.recorded_at.as_str())
        .max()
        .unwrap_or_default()
        .to_string();
    Some(
        Disqualification {
            schema_version: DISQUALIFICATION_SCHEMA_VERSION.to_string(),
            disqualification_id: short_id("dq-", &material),
            family: family.to_string(),
            tier,
            reason: DisqualificationReason::FloorsNotMet,
            evidence,
            detail: format!(
                "{passed} of {n} attempts passed the floors (pass rate {pass_rate:.4} <= max {})",
                policy.disqualify.max_pass_rate
            ),
            registry_as_of: registry_as_of.to_string(),
            decided_at,
            expires_at: None,
            record_sha256: String::new(),
        }
        .sealed(),
    )
}

/// A disqualification for a tier that is structurally unusable for the task's family
/// (shape mismatch, nothing provisioned, nothing QoS-qualified, or rights forbid every remote model).
// decision: not in the contract's function list; added to cover R-14's "or when a structural
// reason applies". Evidence is empty, `decided_at` is the task's `created_at`.
pub fn structural_disqualification(
    task: &Task,
    tier: Tier,
    registry: &Registry,
    policy: &Policy,
) -> Option<Disqualification> {
    let gate = qos_gate(registry, task, policy);
    let (skip, detail) = structural_skip(task, registry, &gate, tier).err()?;
    let reason = match skip {
        SkipReason::ShapeMismatch => DisqualificationReason::ShapeMismatch,
        SkipReason::NoProvisionedCandidates => DisqualificationReason::NoProvisionedCandidates,
        SkipReason::NoQosQualifiedCandidates => {
            let all_rights = fitting_models(task, registry, tier).into_iter().all(|m| {
                gate.exclusion(&m.id).map(|e| e.reason) == Some(ExclusionReason::RightsForbidRemote)
            });
            if all_rights {
                DisqualificationReason::RightsForbidRemote
            } else {
                DisqualificationReason::NoQosQualifiedCandidates
            }
        }
        SkipReason::Disqualified => return None,
    };
    let reason_name = serde_json::to_value(reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let material = format!("{}|{}|{reason_name}", task.family, tier.as_str());
    Some(
        Disqualification {
            schema_version: DISQUALIFICATION_SCHEMA_VERSION.to_string(),
            disqualification_id: short_id("dq-", &material),
            family: task.family.clone(),
            tier,
            reason,
            evidence: Vec::new(),
            detail,
            registry_as_of: registry.as_of.clone(),
            decided_at: task.created_at.clone(),
            expires_at: None,
            record_sha256: String::new(),
        }
        .sealed(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt::Outcome;
    use crate::candidate::Candidate;
    use crate::canonical::record_sha256;
    use crate::testutil::{attempt, bad, good, single, task_generate};

    /// The default policy with coverage switched off: the threshold alone decides.
    fn threshold_only() -> Policy {
        let mut p = Policy::default();
        p.disqualify.require_full_coverage = false;
        p
    }

    fn assess(attempts: &[Attempt], policy: &Policy) -> Option<Disqualification> {
        let t = task_generate();
        assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            attempts,
            &t,
            &Registry::example(),
            "2026-10-19",
            policy,
        )
    }

    fn failing(n: usize) -> Vec<Attempt> {
        let t = task_generate();
        (0..n)
            .map(|i| {
                let mut a = attempt(
                    &t,
                    &format!("att-{i}"),
                    single("qwen-3.8-27b", "none"),
                    Outcome::Answered,
                    bad(),
                );
                a.recorded_at = format!("2026-10-19T11:0{i}:00Z");
                a
            })
            .collect()
    }

    /// The tier-3 offer for the generate task: six singles, then panel, cascade and draft.
    fn tier3_offer() -> Vec<Candidate> {
        let t = task_generate();
        let r = Registry::example();
        let p = Policy::default();
        let offer = tier_offer(&t, &r, &qos_gate(&r, &t, &p), Tier::CheapGeneralist, &p);
        assert_eq!(offer.len(), 9);
        assert_eq!(offer.iter().filter(|c| c.is_single()).count(), 6);
        offer
    }

    /// One failing attempt per candidate.
    fn failing_for(candidates: &[Candidate]) -> Vec<Attempt> {
        let t = task_generate();
        candidates
            .iter()
            .enumerate()
            .map(|(i, c)| attempt(&t, &format!("cov-{i}"), c.clone(), Outcome::Answered, bad()))
            .collect()
    }

    #[test]
    fn disqualify_four_failures_with_min_five_yields_none() {
        assert!(assess(&failing(4), &threshold_only()).is_none());
    }

    #[test]
    fn disqualify_five_failures_yield_floors_not_met_with_five_ids() {
        let a = failing(5);
        let d = assess(&a, &threshold_only()).expect("disqualified");
        assert_eq!(d.reason, DisqualificationReason::FloorsNotMet);
        assert_eq!(
            d.evidence,
            vec!["att-0", "att-1", "att-2", "att-3", "att-4"]
        );
        assert_eq!(d.registry_as_of, "2026-10-19");
        assert_eq!(d.expires_at, None);
        assert_eq!(d.decided_at, "2026-10-19T11:04:00Z");
        assert!(d.disqualification_id.starts_with("dq-"));
        assert_eq!(d.disqualification_id.len(), 19);
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(d.record_sha256, record_sha256(&v));
        // same evidence in another order gives the same id
        let mut rev = a.clone();
        rev.reverse();
        let d2 = assess(&rev, &threshold_only()).unwrap();
        assert_eq!(d2.disqualification_id, d.disqualification_id);
    }

    #[test]
    fn disqualify_pass_rate_above_threshold_and_other_tiers_or_families_do_not_count() {
        let p = threshold_only();
        let mut a = failing(5);
        // 3 of 5 pass: 0.6 > 0.5
        for x in a.iter_mut().take(3) {
            x.metrics = good().iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        }
        assert!(assess(&a, &p).is_none());
        let mut b = failing(5);
        b[0].family = "other".into();
        b[1].tier = Tier::Frontier;
        assert!(assess(&b, &p).is_none());
        // abstentions count as attempts that did not pass
        let mut c = failing(5);
        for x in c.iter_mut() {
            x.metrics = good().iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
            x.outcome = Outcome::Abstained;
        }
        assert!(assess(&c, &p).is_some());
    }

    #[test]
    fn disqualify_coverage_threshold_met_but_one_single_untried_yields_none() {
        let mut offer = tier3_offer();
        let dropped = offer.remove(1);
        assert!(dropped.is_single());
        let a = failing_for(&offer);
        // 8 attempts, all failing: the threshold alone would disqualify
        assert!(a.len() as u64 >= Policy::default().disqualify.min_attempts);
        assert!(assess(&a, &Policy::default()).is_none());
    }

    #[test]
    fn disqualify_coverage_all_candidates_tried_and_threshold_met_yields_some() {
        let offer = tier3_offer();
        let a = failing_for(&offer);
        let d = assess(&a, &Policy::default()).expect("disqualified");
        assert_eq!(d.reason, DisqualificationReason::FloorsNotMet);
        assert_eq!(d.evidence.len(), 9);
        // compositions are part of the coverage: dropping the panel blocks it again
        let without_panel: Vec<Candidate> = offer
            .into_iter()
            .filter(|c| !matches!(c, Candidate::Panel { .. }))
            .collect();
        assert!(assess(&failing_for(&without_panel), &Policy::default()).is_none());
    }

    #[test]
    fn disqualify_coverage_switched_off_restores_threshold_only() {
        let mut offer = tier3_offer();
        offer.remove(1);
        let a = failing_for(&offer);
        assert!(assess(&a, &Policy::default()).is_none());
        let d = assess(&a, &threshold_only()).expect("disqualified");
        assert_eq!(d.evidence.len(), 8);
    }

    #[test]
    fn disqualify_in_force_rule_binds_registry_as_of_not_expiry() {
        let t = task_generate();
        let d = assess(&failing(5), &threshold_only()).unwrap();
        assert!(d.is_in_force(&t.family, "2026-10-19"));
        assert!(!d.is_in_force(&t.family, "2026-11-01"));
        assert!(!d.is_in_force("other", "2026-10-19"));
        // the engine has no clock: a non-null expires_at does not change the answer
        let mut e = d.clone();
        e.expires_at = Some("2026-01-01T00:00:00Z".into());
        assert!(e.is_in_force(&t.family, "2026-10-19"));
    }

    #[test]
    fn disqualify_structural_reasons() {
        let r = Registry::example();
        let p = Policy::default();
        let t = task_generate();
        let d = structural_disqualification(&t, Tier::SystemOne, &r, &p).unwrap();
        assert_eq!(d.reason, DisqualificationReason::ShapeMismatch);
        let d = structural_disqualification(&t, Tier::Specialised, &r, &p).unwrap();
        assert_eq!(d.reason, DisqualificationReason::NoProvisionedCandidates);
        assert!(structural_disqualification(&t, Tier::CheapGeneralist, &r, &p).is_none());
        let mut local_only = t.clone();
        local_only.rights.send_remote = false;
        // every frontier model in the example is remote
        let d = structural_disqualification(&local_only, Tier::Frontier, &r, &p).unwrap();
        assert_eq!(d.reason, DisqualificationReason::RightsForbidRemote);
        assert_eq!(d.registry_as_of, r.as_of);
    }

    #[test]
    fn disqualify_record_carries_schema_version_inside_the_hash() {
        let t = task_generate();
        let d = assess(&failing(5), &threshold_only()).unwrap();
        assert_eq!(d.schema_version, "tier-drop.disqualification/1");
        let mut v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["schema_version"], "tier-drop.disqualification/1");
        assert_eq!(d.record_sha256, record_sha256(&v));
        v["schema_version"] = serde_json::json!("tier-drop.disqualification/2");
        assert_ne!(d.record_sha256, record_sha256(&v));
        let s = structural_disqualification(
            &t,
            Tier::SystemOne,
            &Registry::example(),
            &Policy::default(),
        )
        .unwrap();
        assert_eq!(s.schema_version, DISQUALIFICATION_SCHEMA_VERSION);
    }
}
