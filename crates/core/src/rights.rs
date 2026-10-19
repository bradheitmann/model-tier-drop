//! Rights by purpose, carried on every task and attempt, and the dataset filter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attempt::Attempt;

/// What the input and output of a task may be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rights {
    /// May be processed by a model at all.
    pub process: bool,
    /// May be used as training data.
    pub train: bool,
    /// May be sent to a remote (non-local) deployment.
    pub send_remote: bool,
    /// May be exported outside the ledger.
    pub export: bool,
}

/// A purpose that a dataset export is filtered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Processing by a model.
    Process,
    /// Training.
    Train,
    /// Sending to a remote deployment.
    SendRemote,
    /// Export outside the ledger.
    Export,
}

impl Rights {
    /// Whether these rights allow the purpose.
    pub fn allows(&self, purpose: Purpose) -> bool {
        match purpose {
            Purpose::Process => self.process,
            Purpose::Train => self.train,
            Purpose::SendRemote => self.send_remote,
            Purpose::Export => self.export,
        }
    }
}

/// The attempts whose recorded rights allow `purpose`, in input order.
pub fn dataset_filter(attempts: &[Attempt], purpose: Purpose) -> Vec<&Attempt> {
    attempts
        .iter()
        .filter(|a| a.rights.allows(purpose))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt::Outcome;
    use crate::testutil::{attempt, single, task_classify};

    #[test]
    fn dataset_filter_train_omits_attempt_without_train_right() {
        let t = task_classify();
        let mut a1 = attempt(&t, "a1", single("jev-1", "none"), Outcome::Answered, &[]);
        let a2 = attempt(&t, "a2", single("laya-1", "none"), Outcome::Failed, &[]);
        a1.rights.train = false;
        let all = vec![a1, a2];
        let train: Vec<&str> = dataset_filter(&all, Purpose::Train)
            .iter()
            .map(|a| a.attempt_id.as_str())
            .collect();
        assert_eq!(train, vec!["a2"]);
        let process: Vec<&str> = dataset_filter(&all, Purpose::Process)
            .iter()
            .map(|a| a.attempt_id.as_str())
            .collect();
        assert_eq!(process, vec!["a1", "a2"]);
    }

    #[test]
    fn dataset_filter_each_purpose_reads_its_own_right() {
        let t = task_classify();
        let mut a = attempt(&t, "a1", single("jev-1", "none"), Outcome::Abstained, &[]);
        a.rights = Rights {
            process: true,
            train: false,
            send_remote: false,
            export: true,
        };
        let all = vec![a];
        assert_eq!(dataset_filter(&all, Purpose::Process).len(), 1);
        assert_eq!(dataset_filter(&all, Purpose::Train).len(), 0);
        assert_eq!(dataset_filter(&all, Purpose::SendRemote).len(), 0);
        assert_eq!(dataset_filter(&all, Purpose::Export).len(), 1);
        assert_eq!(
            serde_json::to_string(&Purpose::SendRemote).unwrap(),
            "\"send_remote\""
        );
    }
}
