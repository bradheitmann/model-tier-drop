//! The QoS gate: a cutoff applied to deployments before any scoring.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::policy::Policy;
use crate::registry::{Locality, Registry};
use crate::task::Task;

/// Why a model was removed by the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    /// The model names a deployment the registry does not have.
    UnknownDeployment,
    /// The deployment is remote and the task's rights forbid `send_remote`.
    RightsForbidRemote,
    /// The deployment has no observed QoS and the policy does not allow unmeasured QoS.
    UnmeasuredQos,
    /// Observed availability is below the task's minimum.
    Availability,
    /// Observed p95 latency is above the task's maximum.
    P95Latency,
    /// Observed error rate is above the task's maximum.
    ErrorRate,
}

/// One model removed by the gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QosExclusion {
    /// The model removed.
    pub model_id: String,
    /// Its deployment.
    pub deployment_id: String,
    /// The first failing check.
    pub reason: ExclusionReason,
    /// Observed versus required values.
    pub detail: String,
}

/// Models that pass the gate and models removed with a reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QosGateResult {
    /// Model ids that pass, in registry order.
    pub qualified: Vec<String>,
    /// Models removed, in registry order.
    pub excluded: Vec<QosExclusion>,
}

impl QosGateResult {
    /// Whether `model_id` passed the gate.
    pub fn is_qualified(&self, model_id: &str) -> bool {
        self.qualified.iter().any(|m| m == model_id)
    }

    /// The exclusion for `model_id`, if any.
    pub fn exclusion(&self, model_id: &str) -> Option<&QosExclusion> {
        self.excluded.iter().find(|e| e.model_id == model_id)
    }
}

/// Remove every model whose deployment misses the task's QoS requirement or whose locality its rights forbid.
// decision: checks run in a fixed order (deployment known, rights, measured, availability,
// p95 latency, error rate) and the first failing check is reported, so each exclusion
// carries exactly one reason.
pub fn qos_gate(registry: &Registry, task: &Task, policy: &Policy) -> QosGateResult {
    let mut qualified = Vec::new();
    let mut excluded = Vec::new();
    let req = &task.qos;
    for m in &registry.models {
        let exclude = |reason: ExclusionReason, detail: String| QosExclusion {
            model_id: m.id.clone(),
            deployment_id: m.deployment_id.clone(),
            reason,
            detail,
        };
        let Some(d) = registry.deployment(&m.deployment_id) else {
            excluded.push(exclude(
                ExclusionReason::UnknownDeployment,
                format!("deployment `{}` is not in the registry", m.deployment_id),
            ));
            continue;
        };
        if d.locality == Locality::Remote && !task.rights.send_remote {
            excluded.push(exclude(
                ExclusionReason::RightsForbidRemote,
                format!("deployment `{}` is remote and send_remote is false", d.id),
            ));
            continue;
        }
        let failure = match &d.qos {
            None if policy.allow_unmeasured_qos => None,
            None => Some((
                ExclusionReason::UnmeasuredQos,
                format!("deployment `{}` has no observed QoS", d.id),
            )),
            Some(q) if q.availability < req.availability_min => Some((
                ExclusionReason::Availability,
                format!(
                    "availability {} < required {}",
                    q.availability, req.availability_min
                ),
            )),
            Some(q) if q.p95_latency_ms > req.p95_latency_max_ms => Some((
                ExclusionReason::P95Latency,
                format!(
                    "p95 latency {} ms > allowed {} ms",
                    q.p95_latency_ms, req.p95_latency_max_ms
                ),
            )),
            Some(q) if q.error_rate > req.error_rate_max => Some((
                ExclusionReason::ErrorRate,
                format!(
                    "error rate {} > allowed {}",
                    q.error_rate, req.error_rate_max
                ),
            )),
            Some(_) => None,
        };
        match failure {
            Some((reason, detail)) => excluded.push(exclude(reason, detail)),
            None => qualified.push(m.id.clone()),
        }
    }
    QosGateResult {
        qualified,
        excluded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::task_generate;

    #[test]
    fn qos_low_availability_is_excluded_with_availability() {
        let mut r = Registry::example();
        r.deployments
            .iter_mut()
            .find(|d| d.id == "deepseek")
            .unwrap()
            .qos
            .as_mut()
            .unwrap()
            .availability = 0.9;
        let g = qos_gate(&r, &task_generate(), &Policy::default());
        let e = g.exclusion("deepseek-4.1-flash").unwrap();
        assert_eq!(e.reason, ExclusionReason::Availability);
        assert_eq!(e.deployment_id, "deepseek");
        assert!(!g.is_qualified("deepseek-4.1-flash"));
        assert!(g.is_qualified("qwen-3.8-27b"));
        assert_eq!(g.excluded.len(), 1);
    }

    #[test]
    fn qos_unmeasured_is_excluded_unless_policy_allows() {
        let mut r = Registry::example();
        r.deployments
            .iter_mut()
            .find(|d| d.id == "mistral")
            .unwrap()
            .qos = None;
        let t = task_generate();
        let mut p = Policy::default();
        let g = qos_gate(&r, &t, &p);
        assert_eq!(
            g.exclusion("mistral-4").unwrap().reason,
            ExclusionReason::UnmeasuredQos
        );
        p.allow_unmeasured_qos = true;
        let g = qos_gate(&r, &t, &p);
        assert!(g.is_qualified("mistral-4"));
        assert!(g.excluded.is_empty());
    }

    #[test]
    fn qos_remote_excluded_when_rights_forbid_send_remote() {
        let r = Registry::example();
        let mut t = task_generate();
        t.rights.send_remote = false;
        let g = qos_gate(&r, &t, &Policy::default());
        let e = g.exclusion("opus-5.5").unwrap();
        assert_eq!(e.reason, ExclusionReason::RightsForbidRemote);
        assert!(g.is_qualified("qwen-3.8-27b"));
        assert!(g.is_qualified("jev"));
        for x in &g.excluded {
            assert_eq!(x.reason, ExclusionReason::RightsForbidRemote);
            assert_eq!(
                r.deployment(&x.deployment_id).unwrap().locality,
                Locality::Remote
            );
        }
        assert_eq!(
            serde_json::to_value(e).unwrap()["reason"],
            "rights_forbid_remote"
        );
    }

    #[test]
    fn qos_latency_and_error_rate_cutoffs() {
        let r = Registry::example();
        let mut t = task_generate();
        t.qos.p95_latency_max_ms = 3000;
        let g = qos_gate(&r, &t, &Policy::default());
        assert_eq!(
            g.exclusion("opus-5.5").unwrap().reason,
            ExclusionReason::P95Latency
        );
        let mut t = task_generate();
        t.qos.error_rate_max = 0.005;
        let g = qos_gate(&r, &t, &Policy::default());
        assert_eq!(
            g.exclusion("glm-5.3").unwrap().reason,
            ExclusionReason::ErrorRate
        );
        assert!(g.is_qualified("opus-5.5"));
    }
}
