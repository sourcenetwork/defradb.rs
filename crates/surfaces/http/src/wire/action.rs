//! The long-running action a node reports, as `GET /actions` answers it.

use serde::{Deserialize, Serialize};

/// [`defra_core::ActionExecution`]: one action's observable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionExecution {
    /// `defra_core::Action`, a `u16` on the wire.
    #[serde(rename = "CollectionID")]
    pub collection_id: String,
    #[serde(rename = "Action")]
    pub action: u16,
    #[serde(rename = "Subject")]
    pub subject: String,
    /// `defra_core::ActionStatus`, a `u16` on the wire.
    #[serde(rename = "Status")]
    pub status: u16,
    #[serde(rename = "Reason")]
    pub reason: String,
}

impl From<defra_core::ActionExecution> for ActionExecution {
    fn from(execution: defra_core::ActionExecution) -> Self {
        Self {
            collection_id: execution.collection_id,
            action: execution.action.value(),
            subject: execution.subject,
            status: execution.status.value(),
            reason: execution.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both numbers are transparent on the wire, so the mirror carries the same
    /// JSON the source does.
    #[test]
    fn action_execution_matches_its_source() {
        let source = defra_core::ActionExecution {
            collection_id: "bafkreia".to_string(),
            action: defra_core::Action::BACKFILL_INDEX,
            subject: "index:by_name".to_string(),
            status: defra_core::ActionStatus::IN_PROGRESS,
            reason: String::new(),
        };
        assert_eq!(
            serde_json::to_value(ActionExecution::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
