//! Disqualification of a tier for a task family, on recorded evidence or a structural reason.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attempt::{check_floors, Attempt};
use crate::canonical::{seal, short_id};
use crate::plan::{fitting_models, structural_skip, SkipReason};
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

/// A recorded disqualification of (family, tier), bound to the registry date it was decided under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Disqualification {
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
    /// Expiry time, or null for none.
    pub expires_at: Option<String>,
    /// SHA-256 over the canonical JSON without this field.
    #[serde(default)]
    pub record_sha256: String,
}

impl Disqualification {
    /// In force when the family matches, `registry_as_of >= registry_as_of_now` (YYYY-MM-DD string
    /// order) and `expires_at` is null. Callers that know the clock filter expired records first.
    pub fn is_in_force(&self, family: &str, registry_as_of_now: &str) -> bool {
        self.family == family
            && self.registry_as_of.as_str() >= registry_as_of_now
            && self.expires_at.is_none()
    }

    fn sealed(mut self) -> Self {
        self.record_sha256 = seal(&self);
        self
    }
}

/// A `floors_not_met` disqualification when this family's attempts of `tier` reach `min_attempts`
/// with a pass rate at or below `max_pass_rate`; otherwise None.
pub fn assess_disqualification(
    family: &str,
    tier: Tier,
    attempts: &[Attempt],
    task: &Task,
    registry_as_of: &str,
    policy: &Policy,
) -> Option<Disqualification> {
    let relevant: Vec<&Attempt> = attempts
        .iter()
        .filter(|a| a.family == family && a.tier == tier)
        .collect();
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
    use crate::canonical::record_sha256;
    use crate::testutil::{attempt, bad, good, single, task_generate};

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

    #[test]
    fn disqualify_four_failures_with_min_five_yields_none() {
        let t = task_generate();
        let a = failing(4);
        let p = Policy::default();
        assert!(assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &a,
            &t,
            "2026-10-19",
            &p
        )
        .is_none());
    }

    #[test]
    fn disqualify_five_failures_yield_floors_not_met_with_five_ids() {
        let t = task_generate();
        let a = failing(5);
        let d = assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &a,
            &t,
            "2026-10-19",
            &Policy::default(),
        )
        .expect("disqualified");
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
        let d2 = assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &rev,
            &t,
            "2026-10-19",
            &Policy::default(),
        )
        .unwrap();
        assert_eq!(d2.disqualification_id, d.disqualification_id);
    }

    #[test]
    fn disqualify_pass_rate_above_threshold_and_other_tiers_or_families_do_not_count() {
        let t = task_generate();
        let mut a = failing(5);
        // 3 of 5 pass: 0.6 > 0.5
        for x in a.iter_mut().take(3) {
            x.metrics = good().iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        }
        let p = Policy::default();
        assert!(assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &a,
            &t,
            "2026-10-19",
            &p
        )
        .is_none());
        let mut b = failing(5);
        b[0].family = "other".into();
        b[1].tier = Tier::Frontier;
        assert!(assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &b,
            &t,
            "2026-10-19",
            &p
        )
        .is_none());
        // abstentions count as attempts that did not pass
        let mut c = failing(5);
        for x in c.iter_mut() {
            x.metrics = good().iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
            x.outcome = Outcome::Abstained;
        }
        assert!(assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &c,
            &t,
            "2026-10-19",
            &p
        )
        .is_some());
    }

    #[test]
    fn disqualify_in_force_rule_binds_registry_as_of_and_expiry() {
        let t = task_generate();
        let d = assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &failing(5),
            &t,
            "2026-10-19",
            &Policy::default(),
        )
        .unwrap();
        assert!(d.is_in_force(&t.family, "2026-10-19"));
        assert!(!d.is_in_force(&t.family, "2026-11-01"));
        assert!(!d.is_in_force("other", "2026-10-19"));
        let mut e = d.clone();
        e.expires_at = Some("2026-12-01T00:00:00Z".into());
        assert!(!e.is_in_force(&t.family, "2026-10-19"));
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
}
