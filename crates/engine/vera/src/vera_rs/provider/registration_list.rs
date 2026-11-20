//! Several registrations in one request: vera.rs's `bearerPolicyCmds`,
//! which authorizes the list once under one token and applies its commands
//! in order, all or none (`proofs/tla/RegistrationBatch.tla`).

use alloy_primitives::Bytes;

use super::{policy_cmd, ProviderError};
use crate::cosmos::MAX_OWED_REQUEST_BYTES;
use crate::provider::MAX_REGISTRATIONS_PER_REQUEST;
use crate::{PolicyAction, PolicyCommand};

/// The request that registers each of `objects` under `bearer_token`, in
/// their order. Refused before anything is sent when it holds no object,
/// more than [`MAX_REGISTRATIONS_PER_REQUEST`], or more bytes than a
/// request built for owed changes may take.
pub(super) fn calldata(
    bearer_token: &str,
    objects: &[acp::AcpObject<'_>],
) -> Result<Bytes, ProviderError> {
    if objects.is_empty() || objects.len() > MAX_REGISTRATIONS_PER_REQUEST {
        return Err(ProviderError::Config(format!(
            "a registration list holds 1 to {MAX_REGISTRATIONS_PER_REQUEST} objects, not {}",
            objects.len()
        )));
    }
    let mut commands = Vec::with_capacity(objects.len());
    for object in objects {
        let typed = policy_cmd::typed(&PolicyCommand {
            policy_id: object.policy_id,
            resource: object.resource,
            object_id: object.object_id,
            action: PolicyAction::RegisterObject,
        })?;
        let policy = vera_client::parse_policy_id(object.policy_id)
            .map_err(|error| ProviderError::Query(error.to_string()))?;
        commands.push((hex::encode(policy), typed));
    }
    let calldata = vera_client::PolicyOperation::list(commands)
        .and_then(|operation| operation.calldata(bearer_token))
        .map_err(|error| ProviderError::Config(format!("registration list: {error}")))?;
    if calldata.len() > MAX_OWED_REQUEST_BYTES {
        return Err(ProviderError::Config(format!(
            "a registration list of {} bytes exceeds {MAX_OWED_REQUEST_BYTES}",
            calldata.len()
        )));
    }
    Ok(calldata.into())
}

#[cfg(test)]
mod tests {
    use alloy_sol_types::SolCall as _;
    use vera_modules::acp::types::PolicyCmd;

    use super::*;
    use crate::vera_rs::abi::IAcp;

    const POLICY: &str = "0x0707070707070707070707070707070707070707070707070707070707070707";

    fn objects(ids: &[String]) -> Vec<acp::AcpObject<'_>> {
        ids.iter()
            .map(|id| acp::AcpObject {
                policy_id: POLICY,
                resource: "users",
                object_id: id,
            })
            .collect()
    }

    #[test]
    fn a_list_carries_each_registration_in_order_under_one_token() {
        let ids: Vec<String> = (0..3).map(|n| format!("bae-{n}")).collect();
        let calldata = calldata("header.claims.signature", &objects(&ids)).unwrap();
        let call = IAcp::bearerPolicyCmdsCall::abi_decode(&calldata).unwrap();
        assert_eq!(call.bearerToken, "header.claims.signature");
        let commands: Vec<(String, PolicyCmd)> = serde_json::from_slice(&call.commands).unwrap();
        let named: Vec<(String, String)> = commands
            .into_iter()
            .map(|(policy, command)| match command {
                PolicyCmd::RegisterObject(object) => (policy, object.id),
                other => panic!("not a registration: {other:?}"),
            })
            .collect();
        let policy = POLICY.trim_start_matches("0x").to_owned();
        assert_eq!(
            named,
            ids.iter()
                .map(|id| (policy.clone(), id.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_list_beyond_what_vera_rs_applies_at_once_is_not_built() {
        let none: Vec<String> = Vec::new();
        assert!(matches!(
            calldata("t", &objects(&none)),
            Err(ProviderError::Config(_))
        ));
        let full: Vec<String> = (0..MAX_REGISTRATIONS_PER_REQUEST)
            .map(|n| format!("bae-{n}"))
            .collect();
        assert!(calldata("t", &objects(&full)).is_ok());
        let over: Vec<String> = (0..=MAX_REGISTRATIONS_PER_REQUEST)
            .map(|n| format!("bae-{n}"))
            .collect();
        assert!(matches!(
            calldata("t", &objects(&over)),
            Err(ProviderError::Config(_))
        ));
        let large: Vec<String> = (0..2)
            .map(|n| format!("{n}{}", "x".repeat(MAX_OWED_REQUEST_BYTES / 2)))
            .collect();
        assert!(matches!(
            calldata("t", &objects(&large)),
            Err(ProviderError::Config(_))
        ));
    }

    #[test]
    fn a_policy_identifier_that_names_no_policy_is_a_query_error_as_alone() {
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let mut listed = objects(&ids);
        listed[1].policy_id = "not-a-policy";
        assert!(matches!(
            calldata("t", &listed),
            Err(ProviderError::Query(_))
        ));
    }
}
