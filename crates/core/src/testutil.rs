//! Shared fixtures for unit tests.

use std::collections::BTreeMap;

use crate::attempt::{Attempt, Judge, JudgeKind, Outcome, Tokens};
use crate::candidate::Candidate;
use crate::registry::Registry;
use crate::rights::Rights;
use crate::task::{Floor, QosRequirement, Review, Task, TaskShape};
use crate::tier::Tier;

pub fn all_rights() -> Rights {
    Rights {
        process: true,
        train: true,
        send_remote: true,
        export: true,
    }
}

fn base_task(task_id: &str, family: &str, shape: TaskShape) -> Task {
    Task {
        task_id: task_id.into(),
        family: family.into(),
        shape,
        input_sha256: "0".repeat(64),
        rights: all_rights(),
        floors: ["quality", "accuracy", "completeness"]
            .iter()
            .map(|m| Floor {
                metric: (*m).into(),
                min: 0.9,
            })
            .collect(),
        qos: QosRequirement {
            availability_min: 0.99,
            p95_latency_max_ms: 8000,
            error_rate_max: 0.02,
        },
        review: Review::Strict,
        metadata: BTreeMap::new(),
        created_at: "2026-10-19T10:00:00Z".into(),
    }
}

/// A `classify` task in family `ticket.priority`.
pub fn task_classify() -> Task {
    base_task("t-classify", "ticket.priority", TaskShape::Classify)
}

/// A `generate` task in family `release_notes.draft`.
pub fn task_generate() -> Task {
    base_task("t-generate", "release_notes.draft", TaskShape::Generate)
}

/// An `extract` task in family `invoice.total_extract` (bound to the example tailored model).
pub fn task_invoice() -> Task {
    base_task("t-invoice", "invoice.total_extract", TaskShape::Extract)
}

pub fn single(model_id: &str, level: &str) -> Candidate {
    Candidate::single(model_id, level)
}

pub fn good() -> &'static [(&'static str, f64)] {
    &[
        ("quality", 0.95),
        ("accuracy", 0.95),
        ("completeness", 0.95),
    ]
}

pub fn bad() -> &'static [(&'static str, f64)] {
    &[("quality", 0.3), ("accuracy", 0.3), ("completeness", 0.3)]
}

/// An attempt on `task`; the tier is taken from the example registry (cheap_generalist when unknown).
pub fn attempt(
    task: &Task,
    id: &str,
    candidate: Candidate,
    outcome: Outcome,
    metrics: &[(&str, f64)],
) -> Attempt {
    let tier = candidate
        .tier(&Registry::example())
        .unwrap_or(Tier::CheapGeneralist);
    Attempt {
        attempt_id: id.into(),
        task_id: task.task_id.clone(),
        family: task.family.clone(),
        tier,
        candidate,
        outcome,
        metrics: metrics
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect(),
        judge: Judge {
            kind: JudgeKind::Deterministic,
            reference: "exact-match".into(),
        },
        cost_usd: 0.001,
        latency_ms: 500,
        tokens: Tokens {
            input: 100,
            output: 20,
        },
        input_sha256: task.input_sha256.clone(),
        output_sha256: None,
        rights: task.rights,
        eval_item_id: None,
        recorded_at: "2026-10-19T11:00:00Z".into(),
        record_sha256: String::new(),
    }
}
