use std::sync::Arc;

use acp_light_client::{AcpLightClient, ModuleId, Object};
use alloy_primitives::B256;
use vera_modules::acp::{
    keys, object_state,
    types::{PolicyRecord, RelationshipRecord},
};

use crate::provider::{ActorRelationshipQuery, ProviderError};

pub(super) async fn query(
    client: &Arc<AcpLightClient>,
    query: ActorRelationshipQuery<'_>,
) -> Result<bool, ProviderError> {
    edge::compat(async {
        let fail = |error: String| ProviderError::Query(error);
        let policy_id = hex::encode(super::VeraRsProvider::policy_id_to_bytes32(query.policy)?);
        let policy = client
            .read_policy(&policy_id)
            .await
            .map_err(|e| fail(e.to_string()))?;
        let record: PolicyRecord = serde_json::from_slice(
            policy
                .value
                .as_deref()
                .ok_or_else(|| fail("policy is unavailable".into()))?,
        )
        .map_err(|e| fail(e.to_string()))?;
        if record.policy.id != policy_id {
            return Err(fail("policy identity does not match its proof".into()));
        }
        record
            .relations
            .validate(&record.policy)
            .map_err(|e| fail(e.to_string()))?;
        let object_id = if record
            .policy
            .actor
            .as_ref()
            .is_some_and(|actor| actor.name == query.resource)
        {
            vera_modules::actor::canonical_str(query.object).into_owned()
        } else {
            query.object.to_owned()
        };
        let owner_object = Object {
            resource: query.resource.into(),
            id: object_id.clone(),
        };
        let minimum = client
            .header_chain()
            .fresh_state()
            .map_err(|e| fail(e.to_string()))?
            .height;
        let (owner_revision, owner) = client
            .proof_client()
            .read_current_object_owner(&policy_id, &owner_object, minimum)
            .await
            .map_err(|e| fail(e.to_string()))?;
        if owner.is_none() || owner_revision.module_state_root != policy.module_state_root {
            return Err(fail(
                "object ownership proof is unavailable at the policy revision".into(),
            ));
        }
        let requester = vera_modules::actor::canonical_str(query.requester);
        let is_owner = owner
            .as_ref()
            .is_some_and(|owner| vera_modules::actor::canonical_str(owner.0.as_str()) == requester);
        if !is_owner {
            let mut authorized = false;
            for manager in record
                .policy
                .get_managers_for_relation(query.resource, query.relation)
            {
                let request =
                    super::access_request(query.resource, &object_id, manager, query.requester)?;
                let (revision, allowed) = client
                    .proof_client()
                    .verify_current_permission(&policy_id, &request, minimum)
                    .await
                    .map_err(|e| fail(e.to_string()))?;
                if revision.module_state_root != policy.module_state_root {
                    return Err(fail(
                        "authorization and policy proofs have different roots".into(),
                    ));
                }
                if allowed {
                    authorized = true;
                    break;
                }
            }
            if !authorized {
                return Err(ProviderError::RelationshipObservationDenied);
            }
        }
        object_state::validate_key(&policy_id, query.resource, &object_id)
            .map_err(|e| fail(e.to_string()))?;
        let object_key = object_state::key(&policy_id, query.resource, &object_id);
        let object = client
            .proof_client()
            .fetch_and_verify_record(ModuleId::Acp, &object_key, minimum)
            .await
            .map_err(|e| fail(e.to_string()))?;
        let object_root = object
            .revision
            .module_state_root
            .parse::<B256>()
            .map_err(|e| fail(e.to_string()))?;
        if object_root != policy.module_state_root {
            return Err(fail("object and policy proofs have different roots".into()));
        }
        let current_incarnation = object
            .record
            .value
            .as_deref()
            .map(|bytes| object_state::decode(bytes.as_ref()))
            .transpose()
            .map_err(|e| fail(e.to_string()))?
            .unwrap_or(0);
        let incarnation = if query.relation == "owner" {
            0
        } else {
            current_incarnation
        };
        let subject = vera_modules::actor::canonical_str(query.actor);
        let relationship: vera_acp::Relationship = serde_json::from_value(serde_json::json!({
            "resource": query.resource,
            "object_id": object_id,
            "relation": query.relation,
            "subject": { "Entity": subject },
        }))
        .map_err(|e| fail(e.to_string()))?;
        let generations = record
            .relations
            .pair(&relationship)
            .map_err(|e| fail(e.to_string()))?;
        let storage_key = keys::relationship_storage_key(&relationship, incarnation);
        let proof = client
            .read_relationship(&policy_id, generations, &storage_key)
            .await
            .map_err(|e| fail(e.to_string()))?;
        if proof.module_state_root != policy.module_state_root {
            return Err(fail(
                "relationship and policy proofs have different roots".into(),
            ));
        }
        let Some(value) = proof.value.as_deref() else {
            return Ok(false);
        };
        let found: RelationshipRecord =
            serde_json::from_slice(value).map_err(|e| fail(e.to_string()))?;
        if found.policy_id != policy_id
            || found.relationship != relationship
            || found.incarnation != incarnation
            || found.generations != generations
        {
            return Err(fail("relationship proof names a different grant".into()));
        }
        Ok(!found.archived)
    })
    .await
}
