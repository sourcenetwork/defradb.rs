use std::sync::Arc;

use acp_light_client::{AcpLightClient, ModuleId, Object};
use alloy_primitives::B256;
use vera_modules::acp::{
    keys, object_state,
    relation_generations::RelationPair,
    types::{PolicyRecord, RelationshipRecord},
};

use crate::provider::{ActorRelationshipQuery, ProviderError};

pub(super) async fn query(
    client: &Arc<AcpLightClient>,
    query: ActorRelationshipQuery<'_>,
) -> Result<bool, ProviderError> {
    let subject = entity_subject(query.actor)?;
    query_subject(client, query, subject, false)
        .await
        .map(|(present, _)| present)
}

async fn query_subject(
    client: &Arc<AcpLightClient>,
    query: ActorRelationshipQuery<'_>,
    subject: vera_acp::Subject,
    require_owner: bool,
) -> Result<(bool, B256), ProviderError> {
    async {
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
        at_policy_root(
            policy.module_state_root,
            owner_revision.module_state_root,
            "ownership",
        )?;
        let is_owner = owner_authorizes(
            owner.as_ref().map(|owner| owner.0.as_str()),
            query.requester,
        )?;
        if !is_owner {
            if require_owner {
                return Err(ProviderError::RelationshipObservationDenied);
            }
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
                at_policy_root(
                    policy.module_state_root,
                    revision.module_state_root,
                    "authorization",
                )?;
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
        at_policy_root(policy.module_state_root, object_root, "object")?;
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
        let relationship =
            vera_acp::Relationship::new(query.resource, object_id, query.relation, subject);
        let generations = record
            .relations
            .pair(&relationship)
            .map_err(|e| fail(e.to_string()))?;
        let storage_key = keys::relationship_storage_key(&relationship, incarnation);
        let proof = client
            .read_relationship(&policy_id, generations, &storage_key)
            .await
            .map_err(|e| fail(e.to_string()))?;
        at_policy_root(
            policy.module_state_root,
            proof.module_state_root,
            "relationship",
        )?;
        let present = granted(
            proof.value.as_deref(),
            &policy_id,
            &relationship,
            incarnation,
            generations,
        )?;
        if present && require_owner && query.relation != "owner" {
            let found: RelationshipRecord =
                serde_json::from_slice(proof.value.as_deref().expect("present record"))
                    .map_err(|error| ProviderError::Query(error.to_string()))?;
            if !issued_by(&found.metadata.owner_did, query.requester) {
                return Ok((false, policy.module_state_root));
            }
        }
        Ok((present, policy.module_state_root))
    }
    .await
}

pub(super) async fn sentinel(
    client: &Arc<AcpLightClient>,
    selection: &acp::TransportSentinelQuery,
) -> Result<bool, ProviderError> {
    let acp::TransportSentinelQuery {
        policy,
        resource,
        child,
        parent,
        device_node,
        group,
        account,
        device,
    } = selection;
    for actor in [account, device] {
        let did =
            identity::Did::new(actor).map_err(|error| ProviderError::Query(error.to_string()))?;
        if did.is_wildcard() {
            return Err(ProviderError::Query("explicit actor required".into()));
        }
    }
    if [policy, resource, child, parent, device_node, group]
        .iter()
        .any(|value| value.is_empty())
    {
        return Err(ProviderError::Query(
            "sentinel selector is incomplete".into(),
        ));
    }
    let query = |resource, object, relation, actor| ActorRelationshipQuery {
        requester: account,
        policy,
        resource,
        object,
        relation,
        actor,
    };
    let (parented, root) = query_subject(
        client,
        query(resource, child, "parent", account),
        vera_acp::Subject::entity_set("nodes", parent, ""),
        true,
    )
    .await?;
    let (parent_owned, parent_root) = query_subject(
        client,
        query("nodes", parent, "owner", account),
        entity_subject(account)?,
        true,
    )
    .await?;
    let (admitted, device_root) = query_subject(
        client,
        query("nodes", device_node, "transport_publisher", device),
        entity_subject(device)?,
        true,
    )
    .await?;
    let (member, group_root) = query_subject(
        client,
        query("device_groups", group, "member", device),
        entity_subject(device)?,
        true,
    )
    .await?;
    at_policy_root(root, parent_root, "sentinel parent")?;
    at_policy_root(root, device_root, "device admission")?;
    at_policy_root(root, group_root, "device group")?;
    Ok(parented && parent_owned && admitted && member)
}

fn entity_subject(actor: &str) -> Result<vera_acp::Subject, ProviderError> {
    serde_json::from_value(serde_json::json!({"Entity": vera_modules::actor::canonical_str(actor)}))
        .map_err(|error| ProviderError::Query(error.to_string()))
}

fn issued_by(issuer: &str, account: &str) -> bool {
    vera_modules::actor::canonical_str(issuer) == vera_modules::actor::canonical_str(account)
}

/// Whether the proven owner is the requester. A proven absence of an owner
/// refuses the requester as an owner that is not the requester and a relation
/// nobody can hold would: an unregistered document is not told apart from a
/// registered one the requester may not manage.
fn owner_authorizes(owner: Option<&str>, requester: &str) -> Result<bool, ProviderError> {
    let Some(owner) = owner else {
        return Err(ProviderError::RelationshipObservationDenied);
    };
    Ok(vera_modules::actor::canonical_str(owner) == vera_modules::actor::canonical_str(requester))
}

/// Every proof of one observation must be taken at the policy's certified
/// root; proofs of different roots are an overtaken observation, which the
/// provider asks again
/// (`proofs/tla/ExactRelationshipObservation.tla`, `INV_SameRoot`).
fn at_policy_root(policy: B256, found: B256, what: &'static str) -> Result<(), ProviderError> {
    if found == policy {
        return Ok(());
    }
    Err(ProviderError::ProofsOvertaken(what))
}

/// What a verified relationship record says about the requested grant: an
/// absent record is a proven absence, an archived one a revoked grant, and a
/// record of any other grant is an error, never an absence.
fn granted(
    value: Option<&[u8]>,
    policy_id: &str,
    relationship: &vera_acp::Relationship,
    incarnation: u64,
    generations: RelationPair,
) -> Result<bool, ProviderError> {
    let Some(value) = value else {
        return Ok(false);
    };
    let found: RelationshipRecord =
        serde_json::from_slice(value).map_err(|e| ProviderError::Query(e.to_string()))?;
    if found.policy_id != policy_id
        || &found.relationship != relationship
        || found.incarnation != incarnation
        || found.generations != generations
    {
        return Err(ProviderError::Query(
            "relationship proof names a different grant".into(),
        ));
    }
    Ok(!found.archived)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_previous_owners_grant_is_not_issued_by_the_new_owner() {
        assert!(issued_by("did:key:first", "did:key:first"));
        assert!(!issued_by("did:key:first", "did:key:second"));
    }

    fn relationship() -> vera_acp::Relationship {
        serde_json::from_value(serde_json::json!({
            "resource": "users",
            "object_id": "doc",
            "relation": "reader",
            "subject": { "Entity": "did:key:z6Mkreader" },
        }))
        .unwrap()
    }

    #[test]
    fn a_proof_at_another_root_answers_nothing() {
        let policy = B256::repeat_byte(1);
        assert!(at_policy_root(policy, policy, "object").is_ok());
        for what in ["ownership", "authorization", "object", "relationship"] {
            let refused = at_policy_root(policy, B256::repeat_byte(2), what);
            assert!(
                matches!(&refused, Err(ProviderError::ProofsOvertaken(named)) if *named == what),
                "{refused:?}"
            );
        }
    }

    type Failure = Box<dyn std::error::Error + Send + Sync>;

    /// An observation whose later proof came back at a newer root, `overtaken`
    /// times, and then at the policy's: asked again through `verified`.
    async fn asked_through_verified(overtaken: usize) -> (Result<bool, Failure>, usize) {
        let asked = std::cell::Cell::new(0);
        let heads = super::super::transport_tests::silent_heads().await;
        let outcome = super::super::verified::verified(&heads, || {
            asked.set(asked.get() + 1);
            let ask = asked.get();
            async move {
                let policy = B256::repeat_byte(1);
                let found = if ask <= overtaken {
                    B256::repeat_byte(2)
                } else {
                    policy
                };
                at_policy_root(policy, found, "relationship")
                    .map(|()| true)
                    .map_err(|error| Box::new(error) as Failure)
            }
        })
        .await;
        (outcome, asked.get())
    }

    #[nuclei::test]
    async fn an_overtaken_observation_is_asked_again_at_one_root() {
        let (outcome, asked) = asked_through_verified(1).await;
        assert!(outcome.unwrap());
        assert_eq!(asked, 2);
    }

    #[nuclei::test]
    async fn an_observation_overtaken_at_every_ask_ends_in_the_refusal() {
        let (outcome, asked) = asked_through_verified(usize::MAX).await;
        let error = outcome.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<ProviderError>(),
            Some(ProviderError::ProofsOvertaken("relationship"))
        ));
        assert_eq!(asked, 4, "the four asks a verified read has");
    }

    #[test]
    fn an_absent_owner_refuses_as_a_registered_document_does() {
        assert!(matches!(
            owner_authorizes(None, "did:key:z6Mkreader"),
            Err(ProviderError::RelationshipObservationDenied)
        ));
        assert!(!owner_authorizes(Some("did:key:z6Mkowner"), "did:key:z6Mkreader").unwrap());
        assert!(owner_authorizes(Some("did:key:z6Mkowner"), "did:key:z6Mkowner").unwrap());
    }

    #[test]
    fn only_a_verified_record_of_the_grant_answers() {
        let pair = RelationPair {
            target: 1,
            subject: 0,
        };
        assert!(!granted(None, "policy", &relationship(), 0, pair).unwrap());
        assert!(granted(Some(b"not a record"), "policy", &relationship(), 0, pair).is_err());
    }
}
