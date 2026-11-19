//! What a caller's delegation names for a remote ACP to act for that caller
//! through a node.

/// The worker and deployment a caller's delegation names: the node's
/// submitting identity on the remote ACP, and that ACP's deployment. With
/// them, the deployment's genesis identity, which a delegation bound to one
/// request also names; absent when the provider does not know it, and then
/// such delegations cannot be made from what the node reports.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DelegationAudience {
    pub worker: String,
    pub deployment: u64,
    /// In JSON, `0x` and 64 hexadecimal digits.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "genesis_hex")]
    pub genesis_id: Option<[u8; 32]>,
}

mod genesis_hex {
    use serde::{de::Error as _, Deserialize as _, Deserializer, Serializer};

    pub(super) fn serialize<S>(value: &Option<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(bytes) => serializer.serialize_str(&format!("0x{}", hex::encode(bytes))),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<[u8; 32]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let Some(text) = Option::<String>::deserialize(deserializer)? else {
            return Ok(None);
        };
        let mut bytes = [0; 32];
        hex::decode_to_slice(text.strip_prefix("0x").unwrap_or(&text), &mut bytes)
            .map_err(|error| D::Error::custom(format!("genesis identity: {error}")))?;
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::DelegationAudience;

    #[test]
    fn the_genesis_identity_travels_as_hexadecimal_and_only_when_known() {
        let known = DelegationAudience {
            worker: "did:key:worker".into(),
            deployment: 9,
            genesis_id: Some([0xab; 32]),
        };
        let json = serde_json::to_value(&known).unwrap();
        assert_eq!(json["genesis_id"], format!("0x{}", "ab".repeat(32)));
        assert_eq!(
            serde_json::from_value::<DelegationAudience>(json).unwrap(),
            known
        );

        let unknown = DelegationAudience {
            genesis_id: None,
            ..known
        };
        let json = serde_json::to_value(&unknown).unwrap();
        assert!(json.get("genesis_id").is_none(), "{json}");
        assert_eq!(
            serde_json::from_value::<DelegationAudience>(json).unwrap(),
            unknown
        );

        for wrong in ["0x00", "not hex"] {
            assert!(
                serde_json::from_value::<DelegationAudience>(serde_json::json!({
                    "worker": "did:key:worker", "deployment": 9, "genesis_id": wrong
                }))
                .is_err()
            );
        }
    }
}
