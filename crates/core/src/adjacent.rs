//! `adjacent`: the lawful next candidates after a starting candidate failed.
//!
//! A router that started from a recommended candidate rather than from the bottom of the ladder
//! asks a narrower question than [`crate::plan_next`]: "after this candidate failed, which next
//! candidates are one step away?" The answer is [`AdjacentMoves`], a pure value. Nothing here
//! chooses among the moves; the router does, and records the move it takes as an ordinary attempt.
//!
//! The rules are the amendment `plan/amendments/2026-10-31-adjacent-escalation.md`:
//!
//! - The *effective ladder* is [`crate::ladder`] without tiers that have no fitting model for the
//!   task or that carry an in-force [`Disqualification`] for its family.
//! - A *fitting model* is one [`crate::plan`] itself would offer for the tier: it serves the task's
//!   shape, its family is compatible, and it passes [`crate::qos_gate`]. The same helpers are
//!   called, so `adjacent` and `plan_next` cannot disagree about which models fit a task.
//! - Moves come in kind order (`more_reasoning`, `same_tier_other_lineage`, `one_tier_up`,
//!   `one_tier_down_specialist`); targets inside a kind follow the single-candidate order of
//!   [`crate::plan`] (open-weight, then local, then cost ascending, then registry level order).
//! - A tier passed over because it has no fitting candidate or is disqualified is one effective
//!   step and is listed in the move's `passed_over`, so a skipped tier is never silent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::candidate::ModelRef;
use crate::disqualification::Disqualification;
use crate::plan::{in_force_ids, ladder, singles, structural_skip, tiers_considered};
use crate::policy::Policy;
use crate::qos::qos_gate;
use crate::registry::{Issue, Model, Registry};
use crate::task::Task;
use crate::tier::Tier;

/// Schema version written on every `AdjacentMoves` value.
pub const ADJACENT_MOVES_SCHEMA_VERSION: &str = "tier-drop.adjacent_moves/1";

/// Which adjacent option a move is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MoveKind {
    /// The same model at the next reasoning level after the starting level.
    MoreReasoning,
    /// Another model of the same tier whose lineage differs from the starting model's.
    SameTierOtherLineage,
    /// Every fitting model of the next tier up the effective ladder.
    OneTierUp,
    /// Every fitting model of the next tier down that is bound to the task's family, or is a
    /// `system_one` model.
    OneTierDownSpecialist,
}

/// Why a tier was passed over on the way to a move's target tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PassedOverReason {
    /// The tier has no model that serves the task's shape, family and QoS requirement.
    NoFittingCandidates,
    /// An in-force disqualification exists for the task's family and the tier.
    Disqualified,
}

/// A tier passed over between the starting tier and the target tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PassedOver {
    /// The tier that added no step.
    pub tier: Tier,
    /// Why it added no step.
    pub reason: PassedOverReason,
}

/// The starting point of the search: the model and reasoning level that failed.
// decision: the contract types `from.tier` as `Tier`, which has no value when `model_id` is not in
// the registry. That single case (`issue: unknown_model`) reports `tier: null`; every other answer
// states the tier. A caller that reads `tier` without checking `issues` would otherwise read a
// made-up tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FromModel {
    /// The model the router started from.
    pub model_id: String,
    /// The reasoning level it ran at.
    pub reasoning_level: String,
    /// The model's tier, or null when the model is unknown.
    pub tier: Option<Tier>,
}

/// One lawful next candidate, one effective step from the starting candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Move {
    /// Which adjacent option this is.
    pub kind: MoveKind,
    /// The candidate to try next.
    pub to: ModelRef,
    /// The starting tier.
    pub tier_from: Tier,
    /// The target tier; equal to `tier_from` for the two same-tier kinds.
    pub tier_to: Tier,
    /// Tiers passed over on the way; empty for the two same-tier kinds.
    pub passed_over: Vec<PassedOver>,
    /// Whether both models name a lineage. A missing lineage on either side counts as different.
    pub lineage_known: bool,
}

/// The answer: every lawful next move, or the issues that made the question unanswerable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdjacentMoves {
    /// Always `tier-drop.adjacent_moves/1`.
    pub schema_version: String,
    /// The task the question was asked about.
    pub task_id: String,
    /// The task's family.
    pub family: String,
    /// The starting candidate.
    pub from: FromModel,
    /// The effective ladder the moves were measured against.
    pub ladder: Vec<Tier>,
    /// The moves, in kind order then single-candidate order. Empty when `issues` is non-empty, and
    /// empty is a valid answer: a human decides next.
    pub moves: Vec<Move>,
    /// Non-empty only when the input is invalid.
    pub issues: Vec<Issue>,
}

/// One tier of the ordered tier sequence, with what the plan helpers say about it.
struct TierView<'r> {
    tier: Tier,
    /// Fitting, QoS-qualified models, or the structural reason the tier is not a step.
    models: Result<Vec<&'r Model>, ()>,
    /// In-force disqualification ids for the family.
    disqualified: Vec<String>,
}

impl TierView<'_> {
    /// A tier is a step of the effective ladder when it has a fitting model and no disqualification.
    fn effective(&self) -> bool {
        self.models.is_ok() && self.disqualified.is_empty()
    }

    /// Why the tier added no step; a recorded disqualification outranks an empty tier.
    fn passed_over_reason(&self) -> PassedOverReason {
        if self.disqualified.is_empty() {
            PassedOverReason::NoFittingCandidates
        } else {
            PassedOverReason::Disqualified
        }
    }
}

fn issue(code: &str, message: String) -> Issue {
    Issue {
        code: code.to_string(),
        message,
    }
}

/// Whether two lineages differ. A missing lineage on either side counts as different, so an
/// unknown lineage is never reported as the same lineage.
fn lineages_differ(a: Option<&String>, b: Option<&String>) -> bool {
    !(a.is_some() && a == b)
}

/// Whether both models name a lineage.
fn lineage_known(a: Option<&String>, b: Option<&String>) -> bool {
    a.is_some() && b.is_some()
}

/// The model at `model_id` named by a single candidate, for the lineage comparison.
fn target<'r>(registry: &'r Registry, to: &ModelRef) -> &'r Model {
    // every target comes from the registry's fitting models, so it is present
    registry
        .model(&to.model_id)
        .expect("a target is a registry model")
}

/// Everything a move of one kind carries apart from its target.
struct Step<'a> {
    kind: MoveKind,
    from_model: &'a Model,
    from_tier: Tier,
    to_tier: Tier,
    passed_over: Vec<PassedOver>,
    registry: &'a Registry,
    policy: &'a Policy,
}

/// Moves of one kind from the given fitting models, in the single-candidate order.
fn moves_of(step: &Step<'_>, models: &[&Model]) -> Vec<Move> {
    let passed_over = &step.passed_over;
    singles(models, step.registry, step.policy)
        .into_iter()
        .map(|c| match c {
            crate::candidate::Candidate::Single {
                model_id,
                reasoning_level,
            } => ModelRef {
                model_id,
                reasoning_level,
            },
            // `singles` returns single candidates only
            other => panic!("singles returned {other:?}"),
        })
        .map(|to| {
            let m = target(step.registry, &to);
            Move {
                kind: step.kind,
                lineage_known: lineage_known(step.from_model.lineage.as_ref(), m.lineage.as_ref()),
                to,
                tier_from: step.from_tier,
                tier_to: step.to_tier,
                passed_over: passed_over.clone(),
            }
        })
        .collect()
}

/// The lawful adjacent moves after `from` failed for `task`.
///
/// Pure: it reads its arguments and returns a value. It runs no model and reads no file, clock or
/// network. `disqualifications` are honoured by family and registry date only, as in
/// [`crate::plan_next`]; the caller that owns the clock drops expired records first.
pub fn adjacent(
    from: &ModelRef,
    task: &Task,
    registry: &Registry,
    disqualifications: &[Disqualification],
    policy: &Policy,
) -> AdjacentMoves {
    let gate = qos_gate(registry, task, policy);
    let model = registry.model(&from.model_id);
    let from_tier = model.map(|m| m.tier);

    // The tier sequence is the task's ladder with the unbound tailored tier last, which is the
    // order `plan_next` reports it in. The effective ladder is that sequence without the tiers
    // that add no step.
    let sequence = tiers_considered(&ladder(task, registry, policy));
    let views: Vec<TierView> = sequence
        .iter()
        .map(|tier| TierView {
            tier: *tier,
            models: structural_skip(task, registry, &gate, *tier).map_err(|_| ()),
            disqualified: in_force_ids(disqualifications, task, *tier, registry),
        })
        .collect();
    let effective_ladder: Vec<Tier> = views
        .iter()
        .filter(|v| v.effective())
        .map(|v| v.tier)
        .collect();

    let mut issues = Vec::new();
    let answer = |issues: Vec<Issue>, moves: Vec<Move>| AdjacentMoves {
        schema_version: ADJACENT_MOVES_SCHEMA_VERSION.to_string(),
        task_id: task.task_id.clone(),
        family: task.family.clone(),
        from: FromModel {
            model_id: from.model_id.clone(),
            reasoning_level: from.reasoning_level.clone(),
            tier: from_tier,
        },
        ladder: effective_ladder.clone(),
        moves,
        issues,
    };

    let Some(model) = model else {
        issues.push(issue(
            "unknown_model",
            format!("model `{}` is not in the registry", from.model_id),
        ));
        return answer(issues, Vec::new());
    };
    if !model
        .reasoning_levels
        .iter()
        .any(|l| l == &from.reasoning_level)
    {
        issues.push(issue(
            "unknown_reasoning_level",
            format!(
                "model `{}` has no reasoning level `{}`; it lists: {}",
                model.id,
                from.reasoning_level,
                model.reasoning_levels.join(", ")
            ),
        ));
        return answer(issues, Vec::new());
    }

    let position = |tier: Tier| views.iter().position(|v| v.tier == tier);
    let pos = position(model.tier);
    // Fitting models of the starting tier, or nothing when the tier is not a step for this task.
    let from_fitting = pos
        .and_then(|p| views[p].models.as_ref().ok())
        .is_some_and(|models| models.iter().any(|m| m.id == model.id));
    if !from_fitting {
        issues.push(issue(
            "from_not_fitting",
            format!(
                "model `{}` does not serve shape `{}` for family `{}` under the QoS gate and rights now in force; the moves are still enumerated",
                model.id,
                task.shape.as_str(),
                task.family
            ),
        ));
    }

    let mut moves = Vec::new();

    // 1. more_reasoning: only the next level, and only when the model itself is a fitting model
    // for the task, because every target must be fitting (R-23).
    if from_fitting {
        let levels = &model.reasoning_levels;
        if let Some(i) = levels.iter().position(|l| l == &from.reasoning_level) {
            if let Some(next) = levels.get(i + 1) {
                moves.push(Move {
                    kind: MoveKind::MoreReasoning,
                    to: ModelRef {
                        model_id: model.id.clone(),
                        reasoning_level: next.clone(),
                    },
                    tier_from: model.tier,
                    tier_to: model.tier,
                    passed_over: Vec::new(),
                    lineage_known: lineage_known(model.lineage.as_ref(), model.lineage.as_ref()),
                });
            }
        }
    }

    // 2. same_tier_other_lineage: every other fitting model of the tier whose lineage differs,
    // at each of its levels.
    if let Some(p) = pos {
        if let Ok(models) = &views[p].models {
            let others: Vec<&Model> = models
                .iter()
                .copied()
                .filter(|m| m.id != model.id)
                .filter(|m| lineages_differ(model.lineage.as_ref(), m.lineage.as_ref()))
                .collect();
            moves.extend(moves_of(
                &Step {
                    kind: MoveKind::SameTierOtherLineage,
                    from_model: model,
                    from_tier: model.tier,
                    to_tier: model.tier,
                    passed_over: Vec::new(),
                    registry,
                    policy,
                },
                &others,
            ));
        }
    }

    // 3 and 4. The next effective tier up and down, with the tiers skipped on the way recorded.
    if let Some(p) = pos {
        let up = (p + 1..views.len()).find(|q| views[*q].effective());
        let passed_over = |range: &[TierView]| -> Vec<PassedOver> {
            range
                .iter()
                .filter(|v| !v.effective())
                .map(|v| PassedOver {
                    tier: v.tier,
                    reason: v.passed_over_reason(),
                })
                .collect()
        };
        if let Some(q) = up {
            if let Ok(models) = &views[q].models {
                moves.extend(moves_of(
                    &Step {
                        kind: MoveKind::OneTierUp,
                        from_model: model,
                        from_tier: model.tier,
                        to_tier: views[q].tier,
                        passed_over: passed_over(&views[p + 1..q]),
                        registry,
                        policy,
                    },
                    models,
                ));
            }
        }
        let down = (0..p).rev().find(|q| views[*q].effective());
        if let Some(q) = down {
            if let Ok(models) = &views[q].models {
                let specialists: Vec<&Model> = models
                    .iter()
                    .copied()
                    .filter(|m| m.tier == Tier::SystemOne || m.binds_family(&task.family))
                    .collect();
                moves.extend(moves_of(
                    &Step {
                        kind: MoveKind::OneTierDownSpecialist,
                        from_model: model,
                        from_tier: model.tier,
                        to_tier: views[q].tier,
                        passed_over: passed_over(&views[q + 1..p]),
                        registry,
                        policy,
                    },
                    &specialists,
                ));
            }
        }
    }

    answer(issues, moves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::ModelRef;
    use crate::disqualification::{DisqualificationReason, DISQUALIFICATION_SCHEMA_VERSION};
    use crate::registry::Locality;
    use crate::testutil::{task_classify, task_generate, task_invoice};

    fn refs_of(moves: &[Move], kind: MoveKind) -> Vec<String> {
        moves
            .iter()
            .filter(|m| m.kind == kind)
            .map(|m| format!("{}@{}", m.to.model_id, m.to.reasoning_level))
            .collect()
    }

    fn dq(family: &str, tier: Tier, id: &str) -> Disqualification {
        Disqualification {
            schema_version: DISQUALIFICATION_SCHEMA_VERSION.to_string(),
            disqualification_id: id.into(),
            family: family.into(),
            tier,
            reason: DisqualificationReason::Operator,
            evidence: vec![],
            detail: "test".into(),
            registry_as_of: "2026-10-19".into(),
            decided_at: "2026-10-19T09:00:00Z".into(),
            expires_at: None,
            record_sha256: String::new(),
        }
    }

    fn from(model_id: &str, reasoning_level: &str) -> ModelRef {
        ModelRef {
            model_id: model_id.into(),
            reasoning_level: reasoning_level.into(),
        }
    }

    fn ask(
        from: &ModelRef,
        task: &Task,
        registry: &Registry,
        disqualifications: &[Disqualification],
    ) -> AdjacentMoves {
        adjacent(from, task, registry, disqualifications, &Policy::default())
    }

    fn example_ask(from: &ModelRef, task: &Task) -> AdjacentMoves {
        ask(from, task, &Registry::example(), &[])
    }

    fn codes(a: &AdjacentMoves) -> Vec<String> {
        a.issues.iter().map(|i| i.code.clone()).collect()
    }

    /// T-23: from a tier-3 model at its first of two levels the first move is `more_reasoning` to
    /// its second level; the same model at its last level offers no `more_reasoning` move.
    #[test]
    fn adjacent_from_a_two_level_model_first_move_is_the_next_level_only() {
        let a = example_ask(&from("qwen-3.8-27b", "none"), &task_generate());
        assert!(a.issues.is_empty(), "{:?}", a.issues);
        assert_eq!(a.moves[0].kind, MoveKind::MoreReasoning);
        assert_eq!(a.moves[0].to.model_id, "qwen-3.8-27b");
        assert_eq!(a.moves[0].to.reasoning_level, "thinking");
        assert_eq!(a.moves[0].tier_from, Tier::CheapGeneralist);
        assert_eq!(a.moves[0].tier_to, Tier::CheapGeneralist);
        assert!(a.moves[0].passed_over.is_empty());
        assert_eq!(refs_of(&a.moves, MoveKind::MoreReasoning).len(), 1);

        let last = example_ask(&from("qwen-3.8-27b", "thinking"), &task_generate());
        assert!(refs_of(&last.moves, MoveKind::MoreReasoning).is_empty());
    }

    /// Only the next level is named, even when the model has three levels.
    #[test]
    fn adjacent_more_reasoning_names_only_the_next_of_three_levels() {
        let a = example_ask(&from("opus-5.5", "low"), &task_generate());
        assert_eq!(
            refs_of(&a.moves, MoveKind::MoreReasoning),
            vec!["opus-5.5@medium"]
        );
        let levels: Vec<&str> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::MoreReasoning)
            .map(|m| m.to.reasoning_level.as_str())
            .collect();
        assert!(!levels.contains(&"high"), "{levels:?}");
        let top = example_ask(&from("opus-5.5", "high"), &task_generate());
        assert!(refs_of(&top.moves, MoveKind::MoreReasoning).is_empty());
    }

    /// T-23: from a tier-3 model with three reasoning levels at its first level, the first move is
    /// `more_reasoning` to its second level, and no `more_reasoning` move names its third level; at
    /// its last level there is none.
    #[test]
    fn adjacent_more_reasoning_from_a_three_level_tier3_model_names_only_the_next() {
        let mut r = Registry::example();
        let qwen = r
            .models
            .iter_mut()
            .find(|m| m.id == "qwen-3.8-27b")
            .unwrap();
        assert_eq!(qwen.tier, Tier::CheapGeneralist);
        qwen.reasoning_levels = vec!["none".into(), "thinking".into(), "deep".into()];

        let a = ask(&from("qwen-3.8-27b", "none"), &task_generate(), &r, &[]);
        assert!(a.issues.is_empty(), "{:?}", a.issues);
        assert_eq!(a.moves[0].kind, MoveKind::MoreReasoning);
        assert_eq!(
            refs_of(&a.moves, MoveKind::MoreReasoning),
            vec!["qwen-3.8-27b@thinking"]
        );
        assert!(!refs_of(&a.moves, MoveKind::MoreReasoning)
            .iter()
            .any(|m| m.ends_with("@deep")));

        let last = ask(&from("qwen-3.8-27b", "deep"), &task_generate(), &r, &[]);
        assert!(refs_of(&last.moves, MoveKind::MoreReasoning).is_empty());
    }

    /// A model with one reasoning level has no `more_reasoning` move.
    #[test]
    fn adjacent_more_reasoning_absent_for_a_single_level_model() {
        let a = example_ask(&from("jev", "none"), &task_generate());
        assert!(refs_of(&a.moves, MoveKind::MoreReasoning).is_empty());
    }

    /// T-23: a tier-3 model of another lineage appears as `same_tier_other_lineage`, and one of the
    /// same lineage does not.
    #[test]
    fn adjacent_same_tier_other_lineage_lists_other_lineages_only() {
        let a = example_ask(&from("qwen-3.8-27b", "none"), &task_generate());
        assert_eq!(
            refs_of(&a.moves, MoveKind::SameTierOtherLineage),
            vec![
                "glm-5.3-flash@none",
                "glm-5.3-flash@thinking",
                "deepseek-4.1-flash@none",
                "deepseek-4.1-flash@thinking"
            ]
        );
        // the same model at its other level is not an "other lineage" move
        assert!(!refs_of(&a.moves, MoveKind::SameTierOtherLineage)
            .iter()
            .any(|r| r.starts_with("qwen-3.8-27b@")));

        let mut r = Registry::example();
        r.models
            .iter_mut()
            .find(|m| m.id == "glm-5.3-flash")
            .unwrap()
            .lineage = Some("qwen".into());
        let same = ask(&from("qwen-3.8-27b", "none"), &task_generate(), &r, &[]);
        assert_eq!(
            refs_of(&same.moves, MoveKind::SameTierOtherLineage),
            vec!["deepseek-4.1-flash@none", "deepseek-4.1-flash@thinking"]
        );
    }

    /// A missing lineage on either side counts as different and the move says the lineage is unknown.
    #[test]
    fn adjacent_same_tier_move_reports_unknown_lineage() {
        let mut r = Registry::example();
        r.models
            .iter_mut()
            .find(|m| m.id == "qwen-3.8-27b")
            .unwrap()
            .lineage = None;
        let a = ask(&from("qwen-3.8-27b", "none"), &task_generate(), &r, &[]);
        let same: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::SameTierOtherLineage)
            .collect();
        assert_eq!(same.len(), 4);
        for m in &same {
            assert!(!m.lineage_known, "{m:?}");
        }
        // both lineages known and different: the move says the lineage is known
        let known = example_ask(&from("qwen-3.8-27b", "none"), &task_generate());
        assert!(known
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::SameTierOtherLineage)
            .all(|m| m.lineage_known));
    }

    /// T-23: `one_tier_up` from tier 3 lists only tier-4 models, never a tier-5 model.
    #[test]
    fn adjacent_one_tier_up_from_tier3_lists_only_tier4() {
        let r = Registry::example();
        let a = example_ask(&from("qwen-3.8-27b", "none"), &task_generate());
        let up: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::OneTierUp)
            .collect();
        assert_eq!(up.len(), 14);
        for m in &up {
            assert_eq!(m.tier_to, Tier::Frontier);
            assert_eq!(r.model(&m.to.model_id).unwrap().tier, Tier::Frontier);
            assert!(m.passed_over.is_empty());
        }
        assert!(!up.iter().any(|m| m.tier_to == Tier::Tailored));
        // single-candidate order: open-weight, then local, then cost ascending
        assert_eq!(up[0].to.model_id, "mistral-4");
        assert_eq!(
            up.last()
                .map(|m| format!("{}@{}", m.to.model_id, m.to.reasoning_level)),
            Some("opus-5.5@high".to_string())
        );
    }

    /// T-23 and the owner decision point: with tier 2 empty for the family, `one_tier_up` from tier 1
    /// lists tier-3 models and records the empty tier in `passed_over` as one step.
    #[test]
    fn adjacent_one_tier_up_skips_an_empty_tier_as_one_step() {
        let a = example_ask(&from("jev", "none"), &task_generate());
        assert_eq!(
            refs_of(&a.moves, MoveKind::OneTierUp),
            vec![
                "qwen-3.8-27b@none",
                "qwen-3.8-27b@thinking",
                "glm-5.3-flash@none",
                "glm-5.3-flash@thinking",
                "deepseek-4.1-flash@none",
                "deepseek-4.1-flash@thinking"
            ]
        );
        let up: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::OneTierUp)
            .collect();
        for m in &up {
            assert_eq!(m.tier_from, Tier::SystemOne);
            assert_eq!(m.tier_to, Tier::CheapGeneralist);
            assert_eq!(
                m.passed_over,
                vec![PassedOver {
                    tier: Tier::Specialised,
                    reason: PassedOverReason::NoFittingCandidates,
                }]
            );
        }
    }

    /// The alternative reading of "never several tiers at once": if a neighbour tier must be
    /// non-empty, the move above would not exist. The stated reading is implemented; this test
    /// pins the difference so a reader can see what the alternative would change.
    #[test]
    fn adjacent_owner_alternative_reading_would_return_no_cross_tier_move() {
        let r = Registry::example();
        let task = task_generate();
        let gate = qos_gate(&r, &task, &Policy::default());
        // the alternative reading's precondition holds: the neighbouring tier is empty for this family
        assert!(structural_skip(&task, &r, &gate, Tier::Specialised).is_err());
        // so the alternative ("return nothing when the neighbouring tier is empty") predicts no move
        let alternative: Vec<String> = Vec::new();
        assert!(alternative.is_empty());
        // and the stated reading returns the move, with the empty tier recorded
        let a = example_ask(&from("jev", "none"), &task);
        assert!(!refs_of(&a.moves, MoveKind::OneTierUp).is_empty());
    }

    /// T-23: `one_tier_down_specialist` lists a tier-2 model bound to the task's family and never
    /// an unbound tier-2 or tier-3 model.
    #[test]
    fn adjacent_one_tier_down_lists_bound_specialists_only() {
        let r = Registry::example();
        let a = example_ask(&from("qwen-3.8-27b", "none"), &task_invoice());
        let down: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::OneTierDownSpecialist)
            .collect();
        assert_eq!(
            refs_of(&a.moves, MoveKind::OneTierDownSpecialist),
            vec!["docling@none", "qwen-ocr@none"]
        );
        for m in &down {
            let model = r.model(&m.to.model_id).unwrap();
            assert_eq!(m.tier_from, Tier::CheapGeneralist);
            assert_eq!(m.tier_to, Tier::Specialised);
            assert!(model.binds_family("invoice.total_extract"));
            assert!(m.passed_over.is_empty());
        }
        // an unbound tier-2 model and every tier-3 model are absent
        for absent in ["scaledown", "glm-5.3-flash", "deepseek-4.1-flash"] {
            assert!(!down.iter().any(|m| m.to.model_id == absent), "{absent}");
        }
    }

    /// The same kind admits a `system_one` target even when its `families` is empty.
    #[test]
    fn adjacent_one_tier_down_includes_system_one() {
        let a = example_ask(&from("qwen-3.8-27b", "thinking"), &task_classify());
        assert_eq!(
            refs_of(&a.moves, MoveKind::OneTierDownSpecialist),
            vec!["jev@none", "laya@none"]
        );
        let down: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::OneTierDownSpecialist)
            .collect();
        assert_eq!(down[0].tier_to, Tier::SystemOne);
        assert_eq!(
            down[0].passed_over,
            vec![PassedOver {
                tier: Tier::Specialised,
                reason: PassedOverReason::NoFittingCandidates,
            }]
        );
    }

    /// A disqualified tier is passed over, not targeted, and says so.
    #[test]
    fn adjacent_disqualified_tier_is_passed_over_and_not_targeted() {
        let r = Registry::example();
        let task = task_generate();
        let dqs = vec![dq(&task.family, Tier::CheapGeneralist, "dq-t3")];
        let a = ask(&from("jev", "none"), &task, &r, &dqs);
        let up: Vec<&Move> = a
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::OneTierUp)
            .collect();
        assert_eq!(up.len(), 14);
        for m in &up {
            assert_eq!(m.tier_to, Tier::Frontier);
            assert_eq!(
                m.passed_over,
                vec![
                    PassedOver {
                        tier: Tier::Specialised,
                        reason: PassedOverReason::NoFittingCandidates,
                    },
                    PassedOver {
                        tier: Tier::CheapGeneralist,
                        reason: PassedOverReason::Disqualified,
                    },
                ]
            );
        }
        assert!(a.moves.iter().all(|m| m.tier_to != Tier::CheapGeneralist));
    }

    /// With every tier disqualified the effective ladder is empty and no cross-tier move exists.
    #[test]
    fn adjacent_empty_ladder_has_no_cross_tier_move() {
        let r = Registry::example();
        let task = task_generate();
        let dqs = vec![
            dq(&task.family, Tier::CheapGeneralist, "dq-t3"),
            dq(&task.family, Tier::Frontier, "dq-t4"),
        ];
        let a = ask(&from("qwen-3.8-27b", "none"), &task, &r, &dqs);
        assert!(a.ladder.is_empty(), "{:?}", a.ladder);
        assert!(refs_of(&a.moves, MoveKind::OneTierUp).is_empty());
        assert!(refs_of(&a.moves, MoveKind::OneTierDownSpecialist).is_empty());
    }

    /// The `ladder` field is the effective ladder for the task.
    #[test]
    fn adjacent_ladder_is_the_effective_ladder() {
        let generate = example_ask(&from("qwen-3.8-27b", "none"), &task_generate());
        assert_eq!(generate.ladder, vec![Tier::CheapGeneralist, Tier::Frontier]);
        let invoice = example_ask(&from("qwen-3.8-27b", "none"), &task_invoice());
        assert_eq!(
            invoice.ladder,
            vec![
                Tier::Tailored,
                Tier::Specialised,
                Tier::CheapGeneralist,
                Tier::Frontier
            ]
        );
        let classify = example_ask(&from("qwen-3.8-27b", "none"), &task_classify());
        assert_eq!(
            classify.ladder,
            vec![Tier::SystemOne, Tier::CheapGeneralist, Tier::Frontier]
        );
    }

    /// T-23: a remote target is absent when `send_remote` is false.
    #[test]
    fn adjacent_remote_target_absent_when_send_remote_is_false() {
        let r = Registry::example();
        let mut task = task_generate();
        task.rights.send_remote = false;
        let a = ask(&from("qwen-3.8-27b", "none"), &task, &r, &[]);
        // the local model still escalates its reasoning level
        assert_eq!(
            refs_of(&a.moves, MoveKind::MoreReasoning),
            vec!["qwen-3.8-27b@thinking"]
        );
        for m in &a.moves {
            let d = r
                .deployment(&r.model(&m.to.model_id).unwrap().deployment_id)
                .unwrap();
            assert_eq!(d.locality, Locality::Local, "{m:?}");
        }
        for absent in ["deepseek-4.1-flash", "glm-5.3-flash", "mistral-4"] {
            assert!(!a.moves.iter().any(|m| m.to.model_id == absent), "{absent}");
        }
    }

    /// T-23: a model whose deployment fails the QoS gate is never a target.
    #[test]
    fn adjacent_qos_failing_model_is_never_a_target() {
        let mut r = Registry::example();
        r.deployments
            .iter_mut()
            .find(|d| d.id == "deepseek")
            .unwrap()
            .qos
            .as_mut()
            .unwrap()
            .p95_latency_ms = 9000;
        let task = task_generate();
        let gate = qos_gate(&r, &task, &Policy::default());
        assert!(!gate.is_qualified("deepseek-4.1-flash"));
        let a = ask(&from("qwen-3.8-27b", "none"), &task, &r, &[]);
        assert_eq!(
            refs_of(&a.moves, MoveKind::SameTierOtherLineage),
            vec!["glm-5.3-flash@none", "glm-5.3-flash@thinking"]
        );
        assert!(!a
            .moves
            .iter()
            .any(|m| m.to.model_id == "deepseek-4.1-flash"));
    }

    /// T-23: an unknown model id yields issue `unknown_model` and no moves.
    #[test]
    fn adjacent_unknown_model_yields_issue_and_no_moves() {
        let a = example_ask(&from("nope", "none"), &task_generate());
        assert_eq!(codes(&a), vec!["unknown_model"]);
        assert!(a.moves.is_empty());
        assert_eq!(a.from.model_id, "nope");
        assert_eq!(a.from.tier, None);
        // the effective ladder is still reported
        assert_eq!(a.ladder, vec![Tier::CheapGeneralist, Tier::Frontier]);
        assert_eq!(a.schema_version, "tier-drop.adjacent_moves/1");
        assert_eq!(a.task_id, "t-generate");
        assert_eq!(a.family, "release_notes.draft");
    }

    /// An unknown reasoning level yields issue `unknown_reasoning_level` and no moves.
    #[test]
    fn adjacent_unknown_reasoning_level_yields_issue_and_no_moves() {
        let a = example_ask(&from("qwen-3.8-27b", "turbo"), &task_generate());
        assert_eq!(codes(&a), vec!["unknown_reasoning_level"]);
        assert!(a.moves.is_empty());
        assert_eq!(a.from.tier, Some(Tier::CheapGeneralist));
    }

    /// A starting model the gate would now exclude is a warning, not a refusal.
    #[test]
    fn adjacent_from_not_fitting_is_a_warning_not_a_refusal() {
        let r = Registry::example();
        let mut task = task_generate();
        task.rights.send_remote = false;
        let a = ask(&from("deepseek-4.1-flash", "none"), &task, &r, &[]);
        assert_eq!(codes(&a), vec!["from_not_fitting"]);
        assert!(!a.moves.is_empty());
        // the same tier's local model is still a lawful target
        assert_eq!(
            refs_of(&a.moves, MoveKind::SameTierOtherLineage),
            vec!["qwen-3.8-27b@none", "qwen-3.8-27b@thinking"]
        );
        // the target must be fitting, so no same-model level move
        assert!(refs_of(&a.moves, MoveKind::MoreReasoning).is_empty());
    }

    /// Moves come in kind order, and one move never spans two effective steps, for every model and
    /// level as the starting candidate.
    #[test]
    fn adjacent_no_move_spans_two_effective_steps() {
        let r = Registry::example();
        for task in [task_generate(), task_invoice(), task_classify()] {
            let policy = Policy::default();
            let sequence = tiers_considered(&ladder(&task, &r, &policy));
            for model in &r.models {
                for level in &model.reasoning_levels {
                    let a = example_ask(&from(&model.id, level), &task);
                    assert!(
                        a.issues.iter().all(|i| i.code == "from_not_fitting"),
                        "{:?}",
                        a.issues
                    );
                    let kinds: Vec<MoveKind> = a.moves.iter().map(|m| m.kind).collect();
                    let mut sorted = kinds.clone();
                    sorted.sort();
                    assert_eq!(kinds, sorted, "{model:?} {level} is not in kind order");
                    for m in &a.moves {
                        if !matches!(
                            m.kind,
                            MoveKind::OneTierUp | MoveKind::OneTierDownSpecialist
                        ) {
                            continue;
                        }
                        let p = sequence.iter().position(|t| *t == m.tier_from).unwrap();
                        let q = sequence.iter().position(|t| *t == m.tier_to).unwrap();
                        let between = if p < q { p + 1..q } else { q + 1..p };
                        for i in between {
                            assert!(
                                !a.ladder.contains(&sequence[i]),
                                "{} from {} to {} spans two effective steps through {}",
                                model.id,
                                m.tier_from.as_str(),
                                m.tier_to.as_str(),
                                sequence[i].as_str()
                            );
                        }
                        assert!(a.ladder.contains(&m.tier_to));
                    }
                }
            }
        }
    }

    /// Every target is a fitting model: it serves the shape, is family-compatible and passes the gate.
    #[test]
    fn adjacent_every_target_is_fitting() {
        let r = Registry::example();
        for task in [task_generate(), task_invoice(), task_classify()] {
            let gate = qos_gate(&r, &task, &Policy::default());
            for model in &r.models {
                for level in &model.reasoning_levels {
                    let a = example_ask(&from(&model.id, level), &task);
                    for m in &a.moves {
                        let t = r.model(&m.to.model_id).unwrap();
                        assert!(t.fits(&task), "{} does not fit", t.id);
                        assert!(gate.is_qualified(&t.id), "{} is not qualified", t.id);
                    }
                }
            }
        }
    }
}
