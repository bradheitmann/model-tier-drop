//! The tier ladder and `plan_next`: the lowest open tier and its untried candidates.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attempt::Attempt;
use crate::candidate::{Candidate, ModelRef};
use crate::disqualification::Disqualification;
use crate::policy::Policy;
use crate::qos::{qos_gate, QosGateResult};
use crate::registry::{Model, Registry};
use crate::task::Task;
use crate::tier::Tier;

/// Why a tier was passed over without being tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Tier 1 serves only decision shapes and the task is not one.
    ShapeMismatch,
    /// No model in the tier serves the task's shape and family.
    NoProvisionedCandidates,
    /// Every fitting model in the tier was removed by the QoS gate.
    NoQosQualifiedCandidates,
    /// An in-force disqualification exists for the family and tier.
    Disqualified,
}

impl SkipReason {
    /// The snake_case name used in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::ShapeMismatch => "shape_mismatch",
            SkipReason::NoProvisionedCandidates => "no_provisioned_candidates",
            SkipReason::NoQosQualifiedCandidates => "no_qos_qualified_candidates",
            SkipReason::Disqualified => "disqualified",
        }
    }
}

/// A tier passed over, with the reason and the specifics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkippedTier {
    /// The tier.
    pub tier: Tier,
    /// Why.
    pub reason: SkipReason,
    /// Specifics (QoS exclusions, disqualification ids, the shape or family).
    pub detail: String,
}

/// What to do next for a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Plan {
    /// Try these candidates in this tier, in order.
    Try {
        /// The lowest open tier on the ladder.
        tier: Tier,
        /// Untried candidates in order; empty when the tier awaits a disqualification assessment.
        candidates: Vec<Candidate>,
        /// Tiers passed over on the way, in ladder order.
        skipped: Vec<SkippedTier>,
        /// Explanation when only compositions remain or the tier is exhausted.
        // decision: the contract's Plan has no `detail`; it is added so an exhausted tier
        // says it is waiting for a disqualification instead of silently moving up.
        #[serde(default)]
        detail: String,
    },
    /// No tier on the ladder is open.
    Exhausted {
        /// Every tier passed over, in ladder order.
        skipped: Vec<SkippedTier>,
    },
}

fn binds_tailored(task: &Task, registry: &Registry) -> bool {
    registry
        .models
        .iter()
        .any(|m| m.tier == Tier::Tailored && m.binds_family(&task.family))
}

/// The task's ladder: tiers 1 to 4, with tailored first (or last) only when a tailored model binds the family.
pub fn ladder(task: &Task, registry: &Registry, policy: &Policy) -> Vec<Tier> {
    let mut l = Tier::ladder_default();
    if binds_tailored(task, registry) {
        if policy.tailored_first {
            l.insert(0, Tier::Tailored);
        } else {
            l.push(Tier::Tailored);
        }
    }
    l
}

/// The ladder followed by tailored when it is not on the ladder, so its skip is reported last.
pub(crate) fn tiers_considered(ladder: &[Tier]) -> Vec<Tier> {
    let mut all = ladder.to_vec();
    if !all.contains(&Tier::Tailored) {
        all.push(Tier::Tailored);
    }
    all
}

/// Models of `tier` that are candidates for the task: shape listed (or any) and family compatible.
/// Tiers 2 and 5 additionally require the family to be bound explicitly.
pub(crate) fn fitting_models<'r>(
    task: &Task,
    registry: &'r Registry,
    tier: Tier,
) -> Vec<&'r Model> {
    let needs_binding = matches!(tier, Tier::Specialised | Tier::Tailored);
    registry
        .models
        .iter()
        .filter(|m| m.tier == tier && m.fits(task))
        .filter(|m| !needs_binding || m.binds_family(&task.family))
        .collect()
}

/// Fitting, QoS-qualified models of `tier`, or the structural reason the tier is skipped.
pub(crate) fn structural_skip<'r>(
    task: &Task,
    registry: &'r Registry,
    gate: &QosGateResult,
    tier: Tier,
) -> Result<Vec<&'r Model>, (SkipReason, String)> {
    if tier == Tier::SystemOne && !task.shape.is_decision() {
        return Err((
            SkipReason::ShapeMismatch,
            format!(
                "task shape `{}` is not a decision shape (classify, choose, route, yes_no)",
                task.shape.as_str()
            ),
        ));
    }
    let fitting = fitting_models(task, registry, tier);
    if fitting.is_empty() {
        return Err((
            SkipReason::NoProvisionedCandidates,
            format!(
                "no {} model serves shape `{}` for family `{}`",
                tier.as_str(),
                task.shape.as_str(),
                task.family
            ),
        ));
    }
    let qualified: Vec<&Model> = fitting
        .iter()
        .copied()
        .filter(|m| gate.is_qualified(&m.id))
        .collect();
    if qualified.is_empty() {
        let detail = fitting
            .iter()
            .filter_map(|m| gate.exclusion(&m.id))
            .map(|e| {
                let reason = serde_json::to_value(e.reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                format!(
                    "{} on {}: {} ({})",
                    e.model_id, e.deployment_id, reason, e.detail
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err((SkipReason::NoQosQualifiedCandidates, detail));
    }
    Ok(qualified)
}

/// Ids of in-force disqualifications for the task's family and `tier`.
pub(crate) fn in_force_ids(
    disqualifications: &[Disqualification],
    task: &Task,
    tier: Tier,
    registry: &Registry,
) -> Vec<String> {
    disqualifications
        .iter()
        .filter(|d| d.tier == tier && d.is_in_force(&task.family, &registry.as_of))
        .map(|d| d.disqualification_id.clone())
        .collect()
}

/// Models sorted cheapest first; ties keep registry order.
fn by_cost<'r>(models: &[&'r Model], registry: &Registry) -> Vec<&'r Model> {
    let index = |m: &Model| registry.models.iter().position(|x| x.id == m.id);
    let mut sorted = models.to_vec();
    sorted.sort_by(|a, b| {
        a.cost
            .key()
            .total_cmp(&b.cost.key())
            .then_with(|| index(a).cmp(&index(b)))
    });
    sorted
}

fn lowest(m: &Model) -> ModelRef {
    ModelRef {
        model_id: m.id.clone(),
        reasoning_level: m.lowest_level().to_string(),
    }
}

/// Every single (model × reasoning level) of the tier, cheapest model first, levels in registry order.
// decision: ordering key is (cost, registry position, level position), so the levels of one
// model stay together even when two models cost the same.
fn singles(models: &[&Model], registry: &Registry) -> Vec<Candidate> {
    by_cost(models, registry)
        .into_iter()
        .flat_map(|m| {
            m.reasoning_levels
                .iter()
                .map(move |l| Candidate::single(&m.id, l))
        })
        .collect()
}

/// The cheapest QoS-qualified, family-compatible model of the next tier up, at its lowest level.
// decision: helpers (judge, auditor, finisher) must be QoS-qualified and family-compatible;
// their shapes are not checked because they review or finish rather than answer alone.
fn helper(tier: Tier, task: &Task, registry: &Registry, gate: &QosGateResult) -> Option<ModelRef> {
    let up = tier.next_up()?;
    let pool: Vec<&Model> = registry
        .models
        .iter()
        .filter(|m| m.tier == up && m.fits_family(&task.family) && gate.is_qualified(&m.id))
        .collect();
    by_cost(&pool, registry).first().map(|m| lowest(m))
}

/// Panel, cascade and draft for the tier, in that order; empty when there is no next-tier helper.
fn compositions(
    tier: Tier,
    models: &[&Model],
    task: &Task,
    registry: &Registry,
    gate: &QosGateResult,
    policy: &Policy,
) -> Vec<Candidate> {
    let Some(up) = helper(tier, task, registry, gate) else {
        return Vec::new();
    };
    let sorted = by_cost(models, registry);
    let mut out = Vec::new();
    // decision: a panel needs at least two distinct members; one member is just a single.
    let size = usize::try_from(policy.panel.max_members)
        .unwrap_or(usize::MAX)
        .min(sorted.len());
    if size >= 2 {
        out.push(Candidate::Panel {
            members: sorted[..size].iter().map(|m| lowest(m)).collect(),
            judge: up.clone(),
        });
    }
    if let Some(cheapest) = sorted.first() {
        out.push(Candidate::Cascade {
            first: lowest(cheapest),
            auditor: up.clone(),
            audit_sample_rate: policy.cascade.audit_sample_rate,
        });
        out.push(Candidate::Draft {
            drafter: lowest(cheapest),
            finisher: up,
        });
    }
    out
}

/// The lowest non-disqualified, non-skipped tier on the ladder and its untried candidates, or `exhausted`.
pub fn plan_next(
    task: &Task,
    registry: &Registry,
    disqualifications: &[Disqualification],
    attempts: &[Attempt],
    policy: &Policy,
) -> Plan {
    let gate = qos_gate(registry, task, policy);
    let tried: Vec<&Candidate> = attempts
        .iter()
        .filter(|a| a.task_id == task.task_id)
        .map(|a| &a.candidate)
        .collect();
    let untried = |cs: Vec<Candidate>| -> Vec<Candidate> {
        cs.into_iter().filter(|c| !tried.contains(&c)).collect()
    };
    let mut skipped = Vec::new();
    for tier in tiers_considered(&ladder(task, registry, policy)) {
        let models = match structural_skip(task, registry, &gate, tier) {
            Ok(models) => models,
            Err((reason, detail)) => {
                skipped.push(SkippedTier {
                    tier,
                    reason,
                    detail,
                });
                continue;
            }
        };
        let dq = in_force_ids(disqualifications, task, tier, registry);
        if !dq.is_empty() {
            skipped.push(SkippedTier {
                tier,
                reason: SkipReason::Disqualified,
                detail: dq.join(", "),
            });
            continue;
        }
        let open_singles = untried(singles(&models, registry));
        if !open_singles.is_empty() {
            return Plan::Try {
                tier,
                candidates: open_singles,
                skipped,
                detail: String::new(),
            };
        }
        let open_comps = untried(compositions(tier, &models, task, registry, &gate, policy));
        let detail = if open_comps.is_empty() {
            format!(
                "tier {} is exhausted: every single and composition has an attempt; it awaits a disqualification assessment before the task may move up",
                tier.as_str()
            )
        } else {
            format!(
                "every single in tier {} has an attempt; offering compositions",
                tier.as_str()
            )
        };
        return Plan::Try {
            tier,
            candidates: open_comps,
            skipped,
            detail,
        };
    }
    Plan::Exhausted { skipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt::Outcome;
    use crate::disqualification::DisqualificationReason;
    use crate::disqualification::DISQUALIFICATION_SCHEMA_VERSION;
    use crate::testutil::{attempt, bad, single, task_classify, task_generate, task_invoice};

    fn dq(family: &str, tier: Tier, as_of: &str, id: &str) -> Disqualification {
        Disqualification {
            schema_version: DISQUALIFICATION_SCHEMA_VERSION.to_string(),
            disqualification_id: id.into(),
            family: family.into(),
            tier,
            reason: DisqualificationReason::Operator,
            evidence: vec![],
            detail: "test".into(),
            registry_as_of: as_of.into(),
            decided_at: "2026-10-19T09:00:00Z".into(),
            expires_at: None,
            record_sha256: String::new(),
        }
    }

    fn try_parts(p: &Plan) -> (Tier, &Vec<Candidate>, &Vec<SkippedTier>, &str) {
        match p {
            Plan::Try {
                tier,
                candidates,
                skipped,
                detail,
            } => (*tier, candidates, skipped, detail),
            Plan::Exhausted { .. } => panic!("expected try, got {p:?}"),
        }
    }

    #[test]
    fn plan_classify_starts_at_system_one_with_singles() {
        let p = plan_next(
            &task_classify(),
            &Registry::example(),
            &[],
            &[],
            &Policy::default(),
        );
        let (tier, cands, skipped, _) = try_parts(&p);
        assert_eq!(tier, Tier::SystemOne);
        assert_eq!(cands, &vec![single("jev", "none"), single("laya", "none")]);
        assert!(skipped.is_empty());
        assert_eq!(serde_json::to_value(&p).unwrap()["kind"], "try");
    }

    #[test]
    fn plan_generate_skips_tier1_shape_mismatch_and_tier2_unbound() {
        let p = plan_next(
            &task_generate(),
            &Registry::example(),
            &[],
            &[],
            &Policy::default(),
        );
        let (tier, cands, skipped, _) = try_parts(&p);
        assert_eq!(tier, Tier::CheapGeneralist);
        assert_eq!(skipped[0].tier, Tier::SystemOne);
        assert_eq!(skipped[0].reason, SkipReason::ShapeMismatch);
        assert_eq!(skipped[1].tier, Tier::Specialised);
        assert_eq!(skipped[1].reason, SkipReason::NoProvisionedCandidates);
        // cheapest model first, each model's levels in registry order
        assert_eq!(
            cands,
            &vec![
                single("qwen-3.8-27b", "none"),
                single("qwen-3.8-27b", "thinking"),
                single("glm-5.3-flash", "none"),
                single("glm-5.3-flash", "thinking"),
                single("deepseek-4.1-flash", "none"),
                single("deepseek-4.1-flash", "thinking"),
            ]
        );
    }

    #[test]
    fn plan_tailored_family_starts_at_tailored_tier() {
        let r = Registry::example();
        let t = task_invoice();
        let mut p = Policy::default();
        assert_eq!(
            ladder(&t, &r, &p),
            vec![
                Tier::Tailored,
                Tier::SystemOne,
                Tier::Specialised,
                Tier::CheapGeneralist,
                Tier::Frontier
            ]
        );
        let plan = plan_next(&t, &r, &[], &[], &p);
        let (tier, cands, _, _) = try_parts(&plan);
        assert_eq!(tier, Tier::Tailored);
        assert_eq!(cands, &vec![single("invoice-total-tailored", "none")]);
        p.tailored_first = false;
        assert_eq!(ladder(&t, &r, &p).last(), Some(&Tier::Tailored));
        assert_eq!(ladder(&task_generate(), &r, &p), Tier::ladder_default());
    }

    #[test]
    fn plan_offers_tier3_panel_with_tier4_judge_after_all_singles_fail() {
        let r = Registry::example();
        let t = task_generate();
        let policy = Policy::default();
        let first = plan_next(&t, &r, &[], &[], &policy);
        let (_, singles, _, _) = try_parts(&first);
        let attempts: Vec<Attempt> = singles
            .iter()
            .enumerate()
            .map(|(i, c)| attempt(&t, &format!("a{i}"), c.clone(), Outcome::Answered, bad()))
            .collect();
        let next = plan_next(&t, &r, &[], &attempts, &policy);
        let (tier, cands, _, detail) = try_parts(&next);
        assert_eq!(tier, Tier::CheapGeneralist);
        assert!(detail.contains("compositions"));
        match &cands[0] {
            Candidate::Panel { members, judge } => {
                assert_eq!(members.len(), 3);
                assert_eq!(members[0].model_id, "qwen-3.8-27b");
                for m in members {
                    assert_eq!(r.model(&m.model_id).unwrap().tier, Tier::CheapGeneralist);
                }
                assert_eq!(r.model(&judge.model_id).unwrap().tier, Tier::Frontier);
                assert_eq!(judge.model_id, "mistral-4");
            }
            other => panic!("expected panel first, got {other:?}"),
        }
        assert!(matches!(cands[1], Candidate::Cascade { .. }));
        assert!(matches!(cands[2], Candidate::Draft { .. }));
    }

    #[test]
    fn plan_exhausted_tier_waits_for_disqualification_instead_of_moving_up() {
        let r = Registry::example();
        let t = task_generate();
        let policy = Policy::default();
        let mut attempts = Vec::new();
        // round 1 tries the six singles, round 2 the three compositions
        for _ in 0..2 {
            let p = plan_next(&t, &r, &[], &attempts, &policy);
            let cands = try_parts(&p).1.clone();
            assert!(!cands.is_empty());
            for c in cands {
                let id = format!("a{}", attempts.len());
                attempts.push(attempt(&t, &id, c, Outcome::Failed, &[]));
            }
        }
        let p = plan_next(&t, &r, &[], &attempts, &policy);
        let (tier, cands, _, detail) = try_parts(&p);
        assert_eq!(tier, Tier::CheapGeneralist);
        assert!(cands.is_empty());
        assert!(detail.contains("exhausted"));
    }

    #[test]
    fn plan_every_tier_disqualified_is_exhausted() {
        let r = Registry::example();
        let t = task_classify();
        let dqs: Vec<Disqualification> = Tier::ladder_default()
            .into_iter()
            .map(|tier| {
                dq(
                    &t.family,
                    tier,
                    "2026-10-19",
                    &format!("dq-{}", tier.as_str()),
                )
            })
            .collect();
        let p = plan_next(&t, &r, &dqs, &[], &Policy::default());
        match p {
            Plan::Exhausted { skipped } => {
                let reasons: Vec<(Tier, SkipReason)> =
                    skipped.iter().map(|s| (s.tier, s.reason)).collect();
                assert_eq!(
                    reasons,
                    vec![
                        (Tier::SystemOne, SkipReason::Disqualified),
                        (Tier::Specialised, SkipReason::NoProvisionedCandidates),
                        (Tier::CheapGeneralist, SkipReason::Disqualified),
                        (Tier::Frontier, SkipReason::Disqualified),
                        (Tier::Tailored, SkipReason::NoProvisionedCandidates),
                    ]
                );
                assert_eq!(skipped[0].detail, "dq-system_one");
            }
            other => panic!("expected exhausted, got {other:?}"),
        }
    }

    #[test]
    fn plan_skips_tier_with_no_qos_qualified_candidates() {
        let mut r = Registry::example();
        // both tier-1 models run on local-mini; make its observed p95 too slow for the task
        r.deployments
            .iter_mut()
            .find(|d| d.id == "local-mini")
            .unwrap()
            .qos
            .as_mut()
            .unwrap()
            .p95_latency_ms = 9000;
        let t = task_classify();
        let p = plan_next(&t, &r, &[], &[], &Policy::default());
        let (tier, _, skipped, _) = try_parts(&p);
        assert_eq!(tier, Tier::CheapGeneralist);
        assert_eq!(skipped[0].tier, Tier::SystemOne);
        assert_eq!(skipped[0].reason, SkipReason::NoQosQualifiedCandidates);
        assert!(skipped[0].detail.contains("p95_latency"));
        assert!(skipped[0].detail.contains("jev on local-mini"));
        assert!(skipped[0].detail.contains("laya on local-mini"));
    }

    #[test]
    fn disqualify_stale_registry_as_of_is_ignored_by_plan_next() {
        let r = Registry::example();
        let t = task_classify();
        let stale = dq(&t.family, Tier::SystemOne, "2026-09-01", "dq-stale");
        let p = plan_next(
            &t,
            &r,
            std::slice::from_ref(&stale),
            &[],
            &Policy::default(),
        );
        assert_eq!(try_parts(&p).0, Tier::SystemOne);
        let mut current = stale.clone();
        current.registry_as_of = "2026-10-19".into();
        let p = plan_next(&t, &r, &[current.clone()], &[], &Policy::default());
        let (tier, _, skipped, _) = try_parts(&p);
        assert_eq!(tier, Tier::CheapGeneralist);
        assert_eq!(skipped[0].reason, SkipReason::Disqualified);
        // another family's disqualification does not apply
        let mut other = current;
        other.family = "other.family".into();
        let p = plan_next(&t, &r, &[other], &[], &Policy::default());
        assert_eq!(try_parts(&p).0, Tier::SystemOne);
    }
}
