use acp_light_client::{Actor, AssignmentReadError, AssignmentSelector};

use super::{did_spelling, verified, VeraRsProvider};
use crate::provider::{AssignmentPosition, AssignmentQuery, CertifiedAssignments, ProviderError};

pub(super) async fn read(
    provider: &VeraRsProvider,
    query: AssignmentQuery<'_>,
) -> Result<CertifiedAssignments, ProviderError> {
    let policy = hex::encode(VeraRsProvider::policy_id_to_bytes32(query.policy)?);
    let subject = Actor(
        did_spelling::wire(query.subject)
            .parse()
            .map_err(|error| ProviderError::Query(format!("subject DID: {error}")))?,
    );
    let issuer = Actor(
        did_spelling::wire(query.issuer)
            .parse()
            .map_err(|error| ProviderError::Query(format!("issuer DID: {error}")))?,
    );
    let selector = AssignmentSelector {
        resource: query.resource,
        relation: query.relation,
        subject: &subject,
        issuer: &issuer,
    };
    let (revision, evidence) = verified(provider.light_client.header_chain(), || async {
        provider
            .light_client
            .read_certified_assignments(&policy, &selector)
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> {
                match error {
                    AssignmentReadError::Overtaken => {
                        Box::new(ProviderError::ProofsOvertaken("assignment"))
                    }
                    other => Box::new(other),
                }
            })
    })
    .await
    .map_err(|error| {
        super::reach::read_error(&provider.light_client, "assignment evidence", error)
    })?;
    let position = AssignmentPosition {
        term: revision.term,
        height: revision.height,
        block_hash: format!("{:#x}", revision.block_hash),
    };
    check_position(&position, query.not_before)?;
    let records = evidence
        .proof
        .verify_assignments(
            revision.module_state_root,
            &policy,
            &selector,
            vera_permission::RECORD_PROOF_BYTES,
        )
        .map_err(|error| ProviderError::Query(error.to_string()))?;
    Ok(CertifiedAssignments {
        object_ids: records
            .into_iter()
            .map(|record| record.relationship.object_id)
            .collect(),
        position,
        timestamp: revision.timestamp,
        module_state_root: format!("{:#x}", revision.module_state_root),
        evidence: serde_json::to_value(evidence)
            .map_err(|error| ProviderError::Query(error.to_string()))?,
    })
}

fn check_position(
    current: &AssignmentPosition,
    minimum: Option<&AssignmentPosition>,
) -> Result<(), ProviderError> {
    let Some(minimum) = minimum else {
        return Ok(());
    };
    let minimum_hash: alloy_primitives::B256 = minimum
        .block_hash
        .parse()
        .map_err(|_| ProviderError::Query("invalid minimum block hash".into()))?;
    let current_hash: alloy_primitives::B256 = current
        .block_hash
        .parse()
        .map_err(|_| ProviderError::Query("invalid certified block hash".into()))?;
    if (current.term, current.height) < (minimum.term, minimum.height)
        || ((current.term, current.height) == (minimum.term, minimum.height)
            && current_hash != minimum_hash)
    {
        return Err(ProviderError::Unavailable(
            "assignment evidence precedes the required certified position".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(term: u64, height: u64, byte: u8) -> AssignmentPosition {
        AssignmentPosition {
            term,
            height,
            block_hash: format!("{:#x}", alloy_primitives::B256::repeat_byte(byte)),
        }
    }

    #[test]
    fn position_floor_rejects_replays_and_accepts_a_verified_new_term() {
        let floor = position(2, 10, 1);
        assert!(check_position(&floor, Some(&floor)).is_ok());
        assert!(check_position(&position(2, 11, 2), Some(&floor)).is_ok());
        assert!(check_position(&position(3, 3, 3), Some(&floor)).is_ok());
        assert!(check_position(&position(1, 100, 1), Some(&floor)).is_err());
        assert!(check_position(&position(2, 9, 1), Some(&floor)).is_err());
        assert!(check_position(&position(2, 10, 2), Some(&floor)).is_err());
    }
}
