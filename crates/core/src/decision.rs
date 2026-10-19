//! `decide`: the routing record for a task, with the per-tier status, ranking and winner.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attempt::{check_floors, Attempt, FloorResult, Outcome};
use crate::candidate::Candidate;
use crate::canonical::{seal, short_id};
use crate::disqualification::Disqualification;
use crate::plan::{in_force_ids, ladder, structural_skip, tiers_considered};
use crate::policy::Policy;
use crate::qos::qos_gate;
use crate::registry::Registry;
use crate::task::{Review, Task};
use crate::tier::Tier;

/// What happened to a tier for this task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TierStatus {
    /// An in-force disqualification exists; refs are disqualification ids.
    Disqualified,
    /// Passed over; detail gives the reason.
    Skipped,
    /// Attempts exist but none won; refs are attempt ids.
    Tried,
    /// The winner came from this tier; refs hold the winning attempt id.
    Selected,
}

/// One tier's line in the routing record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TierRecord {
    /// The tier.
    pub tier: Tier,
    /// What happened.
    pub status: TierStatus,
    /// Disqualification ids or attempt ids.
    pub refs: Vec<String>,
    /// Reason or explanation.
    pub detail: String,
}

/// The winner's score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Score {
    /// Mean of the required-floor metrics.
    pub quality: f64,
    /// Cost in USD.
    pub cost_usd: f64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
}

/// The winning candidate and its evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Winner {
    /// What won.
    pub candidate: Candidate,
    /// The winning attempt.
    pub attempt_id: String,
    /// Its floor check.
    pub floor_result: FloorResult,
    /// Its score.
    pub score: Score,
}

/// One qualified attempt in the ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RankEntry {
    /// What was tried.
    pub candidate: Candidate,
    /// The attempt.
    pub attempt_id: String,
    /// Mean of the required-floor metrics.
    pub quality: f64,
    /// Cost in USD.
    pub cost_usd: f64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
}

/// The routing record left behind every task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutingDecision {
    /// `rd-` followed by 16 hex characters.
    pub decision_id: String,
    /// The task.
    pub task_id: String,
    /// Its family.
    pub family: String,
    /// The task's ladder.
    pub ladder: Vec<Tier>,
    /// Every ladder tier, then tiers skipped off the ladder.
    pub tiers: Vec<TierRecord>,
    /// The winner, or null when nothing qualified.
    pub winner: Option<Winner>,
    /// Qualified attempts, lowest ladder tier first, each tier ordered by the band rule.
    pub ranking: Vec<RankEntry>,
    /// The task's review profile.
    pub review: Review,
    /// `record_sha256` of the policy used.
    pub policy_sha256: String,
    /// Latest attempt `recorded_at`, or the task's `created_at` when there are none.
    pub decided_at: String,
    /// SHA-256 over the canonical JSON without this field.
    #[serde(default)]
    pub record_sha256: String,
}

/// Mean of the attempt's metrics named in `policy.required_floors` (missing counts as 0).
fn quality(attempt: &Attempt, policy: &Policy) -> f64 {
    if policy.required_floors.is_empty() {
        return 0.0;
    }
    let sum: f64 = policy
        .required_floors
        .iter()
        .map(|m| attempt.metrics.get(m).copied().unwrap_or(0.0))
        .sum();
    sum / policy.required_floors.len() as f64
}

/// Order entries by quality band, then relative cost band, then latency.
// decision: the bands are applied repeatedly: take the best remaining quality, form its
// band (within `quality_band`), take the cheapest in that band, form its cost band (cost at
// most cheapest × (1 + cost_band)), emit that group by latency, and repeat on what remains.
// Ties fall back to quality, cost, then attempt id so the order is total and deterministic.
fn rank(mut rest: Vec<RankEntry>, policy: &Policy) -> Vec<RankEntry> {
    let tie = |a: &RankEntry, b: &RankEntry| {
        b.quality
            .total_cmp(&a.quality)
            .then(a.cost_usd.total_cmp(&b.cost_usd))
            .then(a.attempt_id.cmp(&b.attempt_id))
    };
    let mut out = Vec::with_capacity(rest.len());
    while !rest.is_empty() {
        let best_q = rest.iter().map(|e| e.quality).fold(f64::MIN, f64::max);
        let in_q = |e: &RankEntry| e.quality >= best_q - policy.quality_band;
        let cheapest = rest
            .iter()
            .filter(|e| in_q(e))
            .map(|e| e.cost_usd)
            .fold(f64::MAX, f64::min);
        let limit = cheapest * (1.0 + policy.cost_band);
        let (mut group, keep): (Vec<RankEntry>, Vec<RankEntry>) = rest
            .into_iter()
            .partition(|e| in_q(e) && e.cost_usd <= limit);
        group.sort_by(|a, b| a.latency_ms.cmp(&b.latency_ms).then_with(|| tie(a, b)));
        out.extend(group);
        rest = keep;
    }
    out
}

/// Build the task's `RoutingDecision` from its attempts and the in-force disqualifications.
pub fn decide(
    task: &Task,
    registry: &Registry,
    disqualifications: &[Disqualification],
    attempts: &[Attempt],
    policy: &Policy,
) -> RoutingDecision {
    let lad = ladder(task, registry, policy);
    let gate = qos_gate(registry, task, policy);
    let mine: Vec<&Attempt> = attempts
        .iter()
        .filter(|a| a.task_id == task.task_id)
        .collect();
    // decision: an attempt's tier is its candidate's tier in the registry, falling back to
    // the tier recorded on the attempt when the model is not in the registry.
    let tier_of = |a: &Attempt| a.candidate.tier(registry).unwrap_or(a.tier);
    let considered = tiers_considered(&lad);
    let position = |t: Tier| {
        considered
            .iter()
            .position(|x| *x == t)
            .unwrap_or(usize::MAX)
    };

    // Qualified attempts grouped by tier in ladder order, each group ranked by the band rule.
    let mut ranking: Vec<(Tier, RankEntry)> = Vec::new();
    let mut tiers_with_qualified: Vec<Tier> = mine
        .iter()
        .filter(|a| a.outcome == Outcome::Answered && check_floors(task, a, policy).passed)
        .map(|a| tier_of(a))
        .collect();
    tiers_with_qualified.sort_by_key(|t| (position(*t), *t));
    tiers_with_qualified.dedup();
    for tier in &tiers_with_qualified {
        let entries: Vec<RankEntry> = mine
            .iter()
            .filter(|a| tier_of(a) == *tier)
            .filter(|a| a.outcome == Outcome::Answered && check_floors(task, a, policy).passed)
            .map(|a| RankEntry {
                candidate: a.candidate.clone(),
                attempt_id: a.attempt_id.clone(),
                quality: quality(a, policy),
                cost_usd: a.cost_usd,
                latency_ms: a.latency_ms,
            })
            .collect();
        ranking.extend(rank(entries, policy).into_iter().map(|e| (*tier, e)));
    }

    // decision: a tier with an in-force disqualification cannot supply the winner, even if one
    // of this task's attempts in it cleared the floors; the winner is the first ranked entry
    // in the lowest tier that is not disqualified.
    let dq_of = |t: Tier| in_force_ids(disqualifications, task, t, registry);
    let winner_entry = ranking.iter().find(|(t, _)| dq_of(*t).is_empty()).cloned();
    let winner = winner_entry.as_ref().and_then(|(_, e)| {
        mine.iter()
            .find(|a| a.attempt_id == e.attempt_id)
            .map(|a| Winner {
                candidate: e.candidate.clone(),
                attempt_id: e.attempt_id.clone(),
                floor_result: check_floors(task, a, policy),
                score: Score {
                    quality: e.quality,
                    cost_usd: e.cost_usd,
                    latency_ms: e.latency_ms,
                },
            })
    });
    let winner_tier = winner_entry.as_ref().map(|(t, _)| *t);

    let mut tiers = Vec::new();
    for tier in considered.iter().copied() {
        let attempt_ids: Vec<String> = mine
            .iter()
            .filter(|a| tier_of(a) == tier)
            .map(|a| a.attempt_id.clone())
            .collect();
        let dq = dq_of(tier);
        let record = if Some(tier) == winner_tier {
            TierRecord {
                tier,
                status: TierStatus::Selected,
                refs: winner.iter().map(|w| w.attempt_id.clone()).collect(),
                detail: format!(
                    "winner chosen from {} attempt(s) in this tier",
                    attempt_ids.len()
                ),
            }
        } else if !dq.is_empty() {
            TierRecord {
                tier,
                status: TierStatus::Disqualified,
                refs: dq,
                detail: "in-force disqualification for this family".to_string(),
            }
        } else if let Err((reason, detail)) = structural_skip(task, registry, &gate, tier) {
            TierRecord {
                tier,
                status: TierStatus::Skipped,
                refs: Vec::new(),
                detail: format!("{}: {detail}", reason.as_str()),
            }
        } else if !attempt_ids.is_empty() {
            TierRecord {
                tier,
                status: TierStatus::Tried,
                refs: attempt_ids,
                detail: "no attempt in this tier won".to_string(),
            }
        } else {
            // decision: an open tier with no attempts (above the winner, or never reached)
            // is reported as skipped with `not_tried`.
            TierRecord {
                tier,
                status: TierStatus::Skipped,
                refs: Vec::new(),
                detail: "not_tried: no attempt was recorded for this tier".to_string(),
            }
        };
        // Tailored off the ladder is listed only when it is structurally skipped.
        if lad.contains(&tier) || record.status == TierStatus::Skipped {
            tiers.push(record);
        }
    }

    let mut ids: Vec<&str> = mine.iter().map(|a| a.attempt_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let decided_at = mine
        .iter()
        .map(|a| a.recorded_at.as_str())
        .max()
        .unwrap_or(task.created_at.as_str())
        .to_string();
    let mut d = RoutingDecision {
        decision_id: short_id("rd-", &format!("{}|{}", task.task_id, ids.join("|"))),
        task_id: task.task_id.clone(),
        family: task.family.clone(),
        ladder: lad,
        tiers,
        winner,
        ranking: ranking.into_iter().map(|(_, e)| e).collect(),
        review: task.review.clone(),
        policy_sha256: seal(policy),
        decided_at,
        record_sha256: String::new(),
    };
    d.record_sha256 = seal(&d);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::record_sha256;
    use crate::disqualification::DisqualificationReason;
    use crate::testutil::{attempt, bad, good, single, task_classify, task_generate};

    fn scored(t: &Task, id: &str, model: &str, q: f64, cost: f64, latency: u64) -> Attempt {
        let mut a = attempt(
            t,
            id,
            single(model, "none"),
            Outcome::Answered,
            &[("quality", q), ("accuracy", q), ("completeness", q)],
        );
        a.cost_usd = cost;
        a.latency_ms = latency;
        a
    }

    fn order(d: &RoutingDecision) -> Vec<&str> {
        d.ranking.iter().map(|e| e.attempt_id.as_str()).collect()
    }

    #[test]
    fn decide_within_quality_band_ranks_by_cost() {
        let t = task_generate();
        let a = scored(&t, "a-dear", "glm-5.3-flash", 0.95, 0.010, 100);
        let b = scored(&t, "b-cheap", "qwen-3.8-27b", 0.94, 0.002, 900);
        let d = decide(&t, &Registry::example(), &[], &[a, b], &Policy::default());
        assert_eq!(order(&d), vec!["b-cheap", "a-dear"]);
        assert_eq!(d.winner.as_ref().unwrap().attempt_id, "b-cheap");
    }

    #[test]
    fn decide_within_both_bands_ranks_by_latency() {
        let t = task_generate();
        let a = scored(&t, "a-slow", "qwen-3.8-27b", 0.95, 0.0100, 900);
        let b = scored(&t, "b-fast", "glm-5.3-flash", 0.94, 0.0105, 200);
        let d = decide(&t, &Registry::example(), &[], &[a, b], &Policy::default());
        assert_eq!(order(&d), vec!["b-fast", "a-slow"]);
    }

    #[test]
    fn decide_outside_quality_band_higher_quality_wins_despite_cost() {
        let t = task_generate();
        let a = scored(&t, "a-best", "glm-5.3-flash", 0.99, 0.050, 900);
        let b = scored(&t, "b-cheap", "qwen-3.8-27b", 0.92, 0.001, 100);
        let d = decide(&t, &Registry::example(), &[], &[a, b], &Policy::default());
        assert_eq!(order(&d), vec!["a-best", "b-cheap"]);
    }

    #[test]
    fn decide_below_floor_never_ranks_above_one_that_clears_it() {
        let t = task_generate();
        // cheaper, faster and higher on two metrics, but completeness misses the floor
        let mut below = scored(&t, "below", "qwen-3.8-27b", 0.99, 0.0001, 10);
        below.metrics.insert("completeness".into(), 0.5);
        let clears = scored(&t, "clears", "glm-5.3-flash", 0.91, 0.5, 5000);
        let d = decide(
            &t,
            &Registry::example(),
            &[],
            &[below, clears],
            &Policy::default(),
        );
        assert_eq!(order(&d), vec!["clears"]);
        assert_eq!(d.winner.as_ref().unwrap().attempt_id, "clears");
        assert!(d.winner.as_ref().unwrap().floor_result.passed);
    }

    #[test]
    fn decide_winner_comes_from_lowest_ladder_tier_with_a_qualified_attempt() {
        let t = task_generate();
        let cheap = scored(&t, "t3", "qwen-3.8-27b", 0.91, 0.010, 900);
        let frontier = scored(&t, "t4", "mistral-4", 0.99, 0.001, 100);
        let d = decide(
            &t,
            &Registry::example(),
            &[],
            &[frontier, cheap],
            &Policy::default(),
        );
        let w = d.winner.as_ref().unwrap();
        assert_eq!(w.attempt_id, "t3");
        assert_eq!(w.score.latency_ms, 900);
        assert!((w.score.quality - 0.91).abs() < 1e-12);
        assert_eq!(order(&d), vec!["t3", "t4"]);
        let statuses: Vec<(Tier, TierStatus)> =
            d.tiers.iter().map(|r| (r.tier, r.status)).collect();
        assert_eq!(
            statuses,
            vec![
                (Tier::SystemOne, TierStatus::Skipped),
                (Tier::Specialised, TierStatus::Skipped),
                (Tier::CheapGeneralist, TierStatus::Selected),
                (Tier::Frontier, TierStatus::Tried),
                (Tier::Tailored, TierStatus::Skipped),
            ]
        );
        assert_eq!(d.tiers[2].refs, vec!["t3"]);
        assert!(d.tiers[0].detail.starts_with("shape_mismatch"));
    }

    #[test]
    fn decide_records_disqualified_tiers_null_winner_and_ids() {
        let t = task_generate();
        let r = Registry::example();
        let p = Policy::default();
        let failed: Vec<Attempt> = (0..5)
            .map(|i| {
                let mut a = attempt(
                    &t,
                    &format!("f{i}"),
                    single("qwen-3.8-27b", "none"),
                    Outcome::Answered,
                    bad(),
                );
                a.recorded_at = format!("2026-10-19T12:0{i}:00Z");
                a
            })
            .collect();
        let dq = crate::disqualification::assess_disqualification(
            &t.family,
            Tier::CheapGeneralist,
            &failed,
            &t,
            &r.as_of,
            &p,
        )
        .unwrap();
        assert_eq!(dq.reason, DisqualificationReason::FloorsNotMet);
        let d = decide(&t, &r, std::slice::from_ref(&dq), &failed, &p);
        assert!(d.winner.is_none());
        assert!(d.ranking.is_empty());
        assert!(d.tiers.iter().all(|x| x.status != TierStatus::Selected));
        let tier3 = d
            .tiers
            .iter()
            .find(|x| x.tier == Tier::CheapGeneralist)
            .unwrap();
        assert_eq!(tier3.status, TierStatus::Disqualified);
        assert_eq!(tier3.refs, vec![dq.disqualification_id.clone()]);
        assert_eq!(d.decided_at, "2026-10-19T12:04:00Z");
        assert!(d.decision_id.starts_with("rd-") && d.decision_id.len() == 19);
        assert_eq!(
            d.policy_sha256,
            record_sha256(&serde_json::to_value(&p).unwrap())
        );
        assert_eq!(
            d.record_sha256,
            record_sha256(&serde_json::to_value(&d).unwrap())
        );
        assert_eq!(d.review, Review::Strict);
        // a passing attempt in a disqualified tier does not win
        let mut ok = attempt(
            &t,
            "ok",
            single("glm-5.3-flash", "none"),
            Outcome::Answered,
            good(),
        );
        ok.cost_usd = 0.001;
        let d = decide(&t, &r, std::slice::from_ref(&dq), &[ok], &p);
        assert!(d.winner.is_none());
    }

    #[test]
    fn decide_without_attempts_uses_created_at_and_is_deterministic() {
        let t = task_classify();
        let r = Registry::example();
        let p = Policy::default();
        let d = decide(&t, &r, &[], &[], &p);
        assert_eq!(d.decided_at, t.created_at);
        assert!(d.winner.is_none());
        assert_eq!(d.ladder, Tier::ladder_default());
        assert_eq!(d, decide(&t, &r, &[], &[], &p));
    }
}
