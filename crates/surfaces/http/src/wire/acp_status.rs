//! The light client's status, as `GET /acp/status` answers it.

use serde::Serialize;

/// [`acp::DelegationAudience`]: what a caller's delegation must name for the
/// remote ACP to act for it through this node.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct DelegationAudience {
    pub worker: String,
    pub deployment: u64,
    /// `0x` and 64 hexadecimal digits, which is what the source type writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genesis_id: Option<String>,
}

/// [`acp::LightClientStatus`]: the state of this node's light client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct LightClientStatus {
    pub height: u64,
    pub module_state_root: String,
    pub cache_entries: usize,
    pub last_invalidation_height: u64,
    pub connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<DelegationAudience>,
}

impl From<acp::LightClientStatus> for LightClientStatus {
    fn from(status: acp::LightClientStatus) -> Self {
        Self {
            height: status.height,
            module_state_root: status.module_state_root,
            cache_entries: status.cache_entries,
            last_invalidation_height: status.last_invalidation_height,
            connected: status.connected,
            chain_time: status.chain_time,
            delegation: status.delegation.map(|audience| DelegationAudience {
                worker: audience.worker,
                deployment: audience.deployment,
                // The source writes `0x` and 64 hex digits; the mirror keeps the
                // bytes and renders the same string.
                genesis_id: audience.genesis_id.map(|bytes| {
                    format!(
                        "0x{}",
                        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    )
                }),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The status serializes identically through the mirror, including the
    /// absent optional fields and the hex genesis id.
    #[test]
    fn light_client_status_matches_its_source() {
        let source = acp::LightClientStatus {
            height: 42,
            module_state_root: "root".to_string(),
            cache_entries: 3,
            last_invalidation_height: 41,
            connected: true,
            chain_time: Some(1_700_000_000),
            delegation: Some(acp::DelegationAudience {
                worker: "worker".to_string(),
                deployment: 7,
                genesis_id: Some([0xab; 32]),
            }),
        };
        assert_eq!(
            serde_json::to_value(LightClientStatus::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }

    /// The optional fields are left out when absent, as the source leaves them.
    #[test]
    fn absent_options_are_omitted() {
        let source = acp::LightClientStatus::default();
        let value = serde_json::to_value(LightClientStatus::from(source.clone())).unwrap();
        assert_eq!(value, serde_json::to_value(&source).unwrap());
        assert!(value.get("chain_time").is_none());
    }
}
