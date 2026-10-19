//! Eval records: items, held-out splits by source family, and eval-run summaries.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::attempt::{check_against, Attempt};
use crate::candidate::Candidate;
use crate::canonical::{canonical_json, seal, short_id};
use crate::policy::Policy;
use crate::task::Floor;
use crate::tier::Tier;

/// How an eval item's expected answer is judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Oracle {
    /// Exact comparison with `expected_sha256`.
    Exact,
    /// A judge decides.
    Judge,
    /// A person decides.
    Human,
}

/// One eval item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalItem {
    /// Item id.
    pub item_id: String,
    /// The task family it exercises.
    pub family: String,
    /// The source the item came from; whole source families stay on one side of the split.
    pub source_family: String,
    /// SHA-256 of the input.
    pub input_sha256: String,
    /// SHA-256 of the expected output, if known.
    pub expected_sha256: Option<String>,
    /// How the answer is judged.
    pub oracle: Oracle,
}

/// Which side of the split an item is on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    /// Held out from training and tuning.
    Holdout,
    /// Available for training and tuning.
    Train,
}

/// An item's split assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SplitItem {
    /// Item id.
    pub item_id: String,
    /// Assignment.
    pub split: Split,
}

/// Schema version written on every `EvalSplit` record.
pub const EVAL_SPLIT_SCHEMA_VERSION: &str = "tier-drop.eval_split/1";

fn default_eval_split_schema_version() -> String {
    EVAL_SPLIT_SCHEMA_VERSION.to_string()
}

/// A deterministic held-out split by source family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalSplit {
    /// Record schema version; defaults to `tier-drop.eval_split/1` when omitted on input.
    #[serde(default = "default_eval_split_schema_version")]
    pub schema_version: String,
    /// The items' task family (distinct families joined with `,` when mixed).
    pub family: String,
    /// The seed used.
    pub seed: u64,
    /// The fraction used.
    pub fraction: f64,
    /// Held-out source families, sorted.
    pub holdout_source_families: Vec<String>,
    /// Training source families, sorted.
    pub train_source_families: Vec<String>,
    /// Every item's assignment, sorted by item id.
    pub items: Vec<SplitItem>,
}

/// Schema version written on every `EvalRun` record.
pub const EVAL_RUN_SCHEMA_VERSION: &str = "tier-drop.eval_run/1";

fn default_eval_run_schema_version() -> String {
    EVAL_RUN_SCHEMA_VERSION.to_string()
}

/// A summary of attempts for one (family, tier, candidate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalRun {
    /// Record schema version; defaults to `tier-drop.eval_run/1` when omitted on input.
    #[serde(default = "default_eval_run_schema_version")]
    pub schema_version: String,
    /// `er-` followed by 16 hex characters.
    pub eval_run_id: String,
    /// The task family.
    pub family: String,
    /// The tier under test.
    pub tier: Tier,
    /// The candidate.
    pub candidate: Candidate,
    /// The attempts summarised, sorted.
    pub attempt_ids: Vec<String>,
    /// Number of attempts.
    pub n: u64,
    /// Per-metric mean over the attempts that report the metric.
    pub aggregate: BTreeMap<String, f64>,
    /// Attempts that answered and cleared every floor, divided by `n`.
    pub pass_rate: f64,
    /// The floors used.
    pub floors: Vec<Floor>,
    /// Latest attempt `recorded_at` (empty when there are none).
    pub recorded_at: String,
    /// SHA-256 over the canonical JSON without this field.
    #[serde(default)]
    pub record_sha256: String,
}

/// Whether a source family is held out: sha256(seed as 8 big-endian bytes, then the UTF-8 name),
/// first 8 bytes as a big-endian u64, divided by u64::MAX, compared with `fraction`.
pub fn is_holdout(source_family: &str, seed: u64, fraction: f64) -> bool {
    let mut h = Sha256::new();
    h.update(seed.to_be_bytes());
    h.update(source_family.as_bytes());
    let digest = h.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    let value = u64::from_be_bytes(first);
    (value as f64 / u64::MAX as f64) < fraction
}

/// Assign whole source families to the held-out or training split from `policy.holdout`.
pub fn split_holdout(items: &[EvalItem], policy: &Policy) -> EvalSplit {
    let seed = policy.holdout.seed;
    let fraction = policy.holdout.fraction;
    let sources: BTreeSet<&str> = items.iter().map(|i| i.source_family.as_str()).collect();
    let (holdout, train): (Vec<&str>, Vec<&str>) = sources
        .into_iter()
        .partition(|s| is_holdout(s, seed, fraction));
    let held: BTreeSet<&str> = holdout.iter().copied().collect();
    let mut assigned: Vec<SplitItem> = items
        .iter()
        .map(|i| SplitItem {
            item_id: i.item_id.clone(),
            split: if held.contains(i.source_family.as_str()) {
                Split::Holdout
            } else {
                Split::Train
            },
        })
        .collect();
    assigned.sort_by(|a, b| a.item_id.cmp(&b.item_id));
    // decision: EvalSplit has one `family`; mixed input lists every distinct family, sorted.
    let families: BTreeSet<&str> = items.iter().map(|i| i.family.as_str()).collect();
    EvalSplit {
        schema_version: EVAL_SPLIT_SCHEMA_VERSION.to_string(),
        family: families.into_iter().collect::<Vec<_>>().join(","),
        seed,
        fraction,
        holdout_source_families: holdout.into_iter().map(str::to_string).collect(),
        train_source_families: train.into_iter().map(str::to_string).collect(),
        items: assigned,
    }
}

/// Summarise the attempts matching (family, tier, candidate) into an `EvalRun`.
pub fn summarise_eval(
    family: &str,
    tier: Tier,
    candidate: &Candidate,
    attempts: &[Attempt],
    floors: &[Floor],
) -> EvalRun {
    let mine: Vec<&Attempt> = attempts
        .iter()
        .filter(|a| a.family == family && a.tier == tier && &a.candidate == candidate)
        .collect();
    let n = mine.len();
    let mut sums: BTreeMap<String, (f64, u64)> = BTreeMap::new();
    for a in &mine {
        for (k, v) in &a.metrics {
            let e = sums.entry(k.clone()).or_insert((0.0, 0));
            e.0 += v;
            e.1 += 1;
        }
    }
    let aggregate = sums
        .into_iter()
        .map(|(k, (s, c))| (k, s / c as f64))
        .collect();
    let passed = mine
        .iter()
        .filter(|a| check_against(a, floors).passed)
        .count();
    let pass_rate = if n == 0 {
        0.0
    } else {
        passed as f64 / n as f64
    };
    let mut ids: Vec<String> = mine.iter().map(|a| a.attempt_id.clone()).collect();
    ids.sort();
    let candidate_json = canonical_json(&serde_json::to_value(candidate).unwrap_or_default());
    let material = format!(
        "{family}|{}|{candidate_json}|{}",
        tier.as_str(),
        ids.join("|")
    );
    let recorded_at = mine
        .iter()
        .map(|a| a.recorded_at.as_str())
        .max()
        .unwrap_or_default()
        .to_string();
    let mut run = EvalRun {
        schema_version: EVAL_RUN_SCHEMA_VERSION.to_string(),
        eval_run_id: short_id("er-", &material),
        family: family.to_string(),
        tier,
        candidate: candidate.clone(),
        attempt_ids: ids,
        n: n as u64,
        aggregate,
        pass_rate,
        floors: floors.to_vec(),
        recorded_at,
        record_sha256: String::new(),
    };
    run.record_sha256 = seal(&run);
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt::Outcome;
    use crate::canonical::record_sha256;
    use crate::testutil::{attempt, bad, good, single, task_generate};

    fn items(sources: usize, per_source: usize) -> Vec<EvalItem> {
        let mut out = Vec::new();
        for s in 0..sources {
            for k in 0..per_source {
                out.push(EvalItem {
                    item_id: format!("item-{s:03}-{k}"),
                    family: "invoice.total_extract".into(),
                    source_family: format!("vendor-{s:03}"),
                    input_sha256: format!("{s:064}"),
                    expected_sha256: None,
                    oracle: Oracle::Exact,
                });
            }
        }
        out
    }

    #[test]
    fn holdout_same_seed_gives_identical_split_independent_of_order() {
        let p = Policy::default();
        let a = items(40, 3);
        let mut b = a.clone();
        b.reverse();
        let s1 = split_holdout(&a, &p);
        let s2 = split_holdout(&a, &p);
        let s3 = split_holdout(&b, &p);
        assert_eq!(s1, s2);
        assert_eq!(s1, s3);
        assert!(!s1.holdout_source_families.is_empty());
        assert!(!s1.train_source_families.is_empty());
        let mut sorted = s1.holdout_source_families.clone();
        sorted.sort();
        assert_eq!(sorted, s1.holdout_source_families);
        let ids: Vec<&str> = s1.items.iter().map(|i| i.item_id.as_str()).collect();
        let mut again = ids.clone();
        again.sort();
        assert_eq!(ids, again);
        assert_eq!(s1.family, "invoice.total_extract");
    }

    #[test]
    fn holdout_whole_source_families_share_a_split_across_seeds() {
        let base = items(30, 4);
        for seed in [0u64, 1, 7, 20261019, u64::MAX] {
            let mut p = Policy::default();
            p.holdout.seed = seed;
            let s = split_holdout(&base, &p);
            let held: BTreeSet<&str> = s
                .holdout_source_families
                .iter()
                .map(String::as_str)
                .collect();
            let train: BTreeSet<&str> =
                s.train_source_families.iter().map(String::as_str).collect();
            assert!(held.is_disjoint(&train));
            assert_eq!(held.len() + train.len(), 30);
            let by_id: BTreeMap<&str, Split> = s
                .items
                .iter()
                .map(|i| (i.item_id.as_str(), i.split))
                .collect();
            let mut per_source: BTreeMap<&str, BTreeSet<Split>> = BTreeMap::new();
            for it in &base {
                let split = by_id[it.item_id.as_str()];
                per_source
                    .entry(it.source_family.as_str())
                    .or_default()
                    .insert(split);
                let expected = if held.contains(it.source_family.as_str()) {
                    Split::Holdout
                } else {
                    Split::Train
                };
                assert_eq!(split, expected);
            }
            assert!(per_source.values().all(|s| s.len() == 1));
        }
    }

    #[test]
    fn holdout_hash_rule_matches_the_contract_and_fraction_bounds() {
        // Recompute the rule by hand for one family.
        let digest = Sha256::digest([20261019u64.to_be_bytes().as_slice(), b"vendor-001"].concat());
        let mut first = [0u8; 8];
        first.copy_from_slice(&digest[..8]);
        let v = u64::from_be_bytes(first) as f64 / u64::MAX as f64;
        assert_eq!(is_holdout("vendor-001", 20261019, 0.2), v < 0.2);
        let all = items(20, 1);
        let mut p = Policy::default();
        p.holdout.fraction = 0.0;
        assert!(split_holdout(&all, &p).holdout_source_families.is_empty());
        p.holdout.fraction = 1.01;
        assert_eq!(split_holdout(&all, &p).holdout_source_families.len(), 20);
    }

    #[test]
    fn eval_three_attempts_two_passing_give_two_thirds() {
        let t = task_generate();
        let c = single("qwen-3.8-27b", "none");
        let mut a = vec![
            attempt(&t, "e1", c.clone(), Outcome::Answered, good()),
            attempt(&t, "e2", c.clone(), Outcome::Answered, good()),
            attempt(&t, "e3", c.clone(), Outcome::Answered, bad()),
        ];
        a[2].recorded_at = "2026-10-19T13:00:00Z".into();
        // an attempt for another candidate is not summarised
        a.push(attempt(
            &t,
            "x",
            single("glm-5.3-flash", "none"),
            Outcome::Answered,
            good(),
        ));
        let run = summarise_eval(&t.family, Tier::CheapGeneralist, &c, &a, &t.floors);
        assert_eq!(run.n, 3);
        assert_eq!((run.pass_rate * 10_000.0).round() / 10_000.0, 0.6667);
        assert_eq!(run.attempt_ids, vec!["e1", "e2", "e3"]);
        let mean_q = (0.95 + 0.95 + 0.3) / 3.0;
        assert!((run.aggregate["quality"] - mean_q).abs() < 1e-12);
        assert_eq!(run.recorded_at, "2026-10-19T13:00:00Z");
        assert!(run.eval_run_id.starts_with("er-") && run.eval_run_id.len() == 19);
        assert_eq!(
            run.record_sha256,
            record_sha256(&serde_json::to_value(&run).unwrap())
        );
    }

    #[test]
    fn eval_aggregate_treats_missing_metric_as_absent_not_zero() {
        let t = task_generate();
        let c = single("qwen-3.8-27b", "none");
        let a = vec![
            attempt(&t, "e1", c.clone(), Outcome::Answered, &[("quality", 0.8)]),
            attempt(
                &t,
                "e2",
                c.clone(),
                Outcome::Failed,
                &[("quality", 0.6), ("f1", 0.5)],
            ),
        ];
        let run = summarise_eval(&t.family, Tier::CheapGeneralist, &c, &a, &t.floors);
        assert!((run.aggregate["quality"] - 0.7).abs() < 1e-12);
        assert_eq!(run.aggregate["f1"], 0.5);
        assert_eq!(run.pass_rate, 0.0);
        let empty = summarise_eval("none", Tier::Frontier, &c, &a, &t.floors);
        assert_eq!(empty.n, 0);
        assert_eq!(empty.pass_rate, 0.0);
    }

    #[test]
    fn eval_run_and_split_carry_schema_version() {
        let t = task_generate();
        let c = single("qwen-3.8-27b", "none");
        let a = vec![attempt(&t, "e1", c.clone(), Outcome::Answered, good())];
        let run = summarise_eval(&t.family, Tier::CheapGeneralist, &c, &a, &t.floors);
        assert_eq!(run.schema_version, "tier-drop.eval_run/1");
        let v = serde_json::to_value(&run).unwrap();
        assert_eq!(v["schema_version"], "tier-drop.eval_run/1");
        assert_eq!(run.record_sha256, record_sha256(&v));
        let split = split_holdout(&items(3, 1), &Policy::default());
        assert_eq!(split.schema_version, "tier-drop.eval_split/1");
        assert_eq!(
            serde_json::to_value(&split).unwrap()["schema_version"],
            "tier-drop.eval_split/1"
        );
    }
}
