//! Command encoding helpers for vera.rs provider operations.

use crate::provider::SubjectRef;

pub(crate) fn subject_to_cmd_json(subject: &SubjectRef) -> serde_json::Value {
    match subject {
        SubjectRef::Actor(did) => {
            serde_json::json!({ "Entity": super::did_spelling::defra(did) })
        }
        SubjectRef::AllActors => serde_json::json!("Wildcard"),
    }
}

pub(crate) fn encode_register_object_cmd(resource: &str, object_id: &str) -> Vec<u8> {
    // Matches vera.rs PolicyCmd::RegisterObject(Object { resource, id })
    serde_json::to_vec(&serde_json::json!({
        "RegisterObject": { "resource": resource, "id": object_id }
    }))
    .unwrap_or_default()
}

pub(crate) fn encode_archive_object_cmd(resource: &str, object_id: &str) -> Vec<u8> {
    // Matches vera.rs PolicyCmd::ArchiveObject(Object { resource, id })
    serde_json::to_vec(&serde_json::json!({
        "ArchiveObject": { "resource": resource, "id": object_id }
    }))
    .unwrap_or_default()
}

pub(crate) fn encode_set_relationship_cmd(
    resource: &str,
    object_id: &str,
    relation: &str,
    subject: &SubjectRef,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "SetRelationship": {
            "resource": resource,
            "object_id": object_id,
            "relation": relation,
            "subject": subject_to_cmd_json(subject),
        }
    }))
    .unwrap_or_default()
}

pub(crate) fn encode_delete_relationship_cmd(
    resource: &str,
    object_id: &str,
    relation: &str,
    subject: &SubjectRef,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "DeleteRelationship": {
            "resource": resource,
            "object_id": object_id,
            "relation": relation,
            "subject": subject_to_cmd_json(subject),
        }
    }))
    .unwrap_or_default()
}

/// [`resolve_registered_or_passthrough_bearer_token_as`] in
/// [`defra_core::RequestContext::ambient`].
#[cfg(test)]
pub(crate) async fn resolve_registered_or_passthrough_bearer_token(
    did: &str,
    submitter: &str,
    deployment_id: u64,
) -> Result<Option<String>, crate::provider::ProviderError> {
    resolve_registered_or_passthrough_bearer_token_as(
        &defra_core::RequestContext::ambient(),
        did,
        submitter,
        deployment_id,
    )
    .await
}

/// The bearer token `did` acts with in the request `context` serves: one
/// built from its registered secp256k1 key when the node holds it, the token
/// the request carries for it otherwise.
pub(crate) async fn resolve_registered_or_passthrough_bearer_token_as(
    context: &defra_core::RequestContext,
    did: &str,
    submitter: &str,
    deployment_id: u64,
) -> Result<Option<String>, crate::provider::ProviderError> {
    use super::signer::Secp256k1Signer;

    if let Some(signing_config) = defra_core::signing::get_identity(did) {
        if signing_config.has_local_private_key()
            && signing_config.key_type == defra_core::signing::SigningKeyType::Secp256k1
        {
            let signer =
                Secp256k1Signer::from_slice(&signing_config.private_key_bytes).map_err(|e| {
                    crate::provider::ProviderError::Config(format!("invalid signing key: {}", e))
                })?;

            let issuer = super::did_spelling::defra(did);
            return super::bearer::create_bearer_token(
                &signer,
                &issuer,
                submitter,
                deployment_id,
                300,
            )
            .await
            .map(Some)
            .map_err(|e| {
                crate::provider::ProviderError::Config(format!(
                    "bearer token creation failed: {}",
                    e
                ))
            });
        }
    }

    let token = context.bearer_token_for(did);
    if let Some(Err((issuer, expected))) =
        token.as_deref().map(super::did_spelling::issuer_spelling)
    {
        return Err(crate::provider::ProviderError::Config(format!(
            "the request token for {did} names its issuer as {issuer}; \
             this node records that key as {expected}, the uncompressed-key did:key"
        )));
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    #[test]
    fn encodes_register_object_as_vera_rs_policy_cmd() {
        let value: Value =
            serde_json::from_slice(&encode_register_object_cmd("users", "doc-1")).unwrap();

        assert_eq!(
            value,
            json!({
                "RegisterObject": { "resource": "users", "id": "doc-1" }
            })
        );
    }

    #[test]
    fn an_actor_subject_is_sent_under_one_spelling() {
        let key = vera_client::Secp256k1SigningKey::from_slice(&[42; 32]).unwrap();
        let compressed = crate::vera_rs::bearer::did_from_signing_key(&key, true);
        let uncompressed = crate::vera_rs::bearer::did_from_signing_key(&key, false);
        for did in [&compressed, &uncompressed] {
            for cmd in [
                encode_set_relationship_cmd(
                    "users",
                    "doc",
                    "reader",
                    &SubjectRef::Actor(did.clone()),
                ),
                encode_delete_relationship_cmd(
                    "users",
                    "doc",
                    "reader",
                    &SubjectRef::Actor(did.clone()),
                ),
            ] {
                let value: Value = serde_json::from_slice(&cmd).unwrap();
                let subject = value
                    .as_object()
                    .and_then(|command| command.values().next())
                    .map(|body| body["subject"]["Entity"].clone());
                assert_eq!(subject, Some(json!(uncompressed)));
            }
        }
    }

    #[test]
    fn encodes_relationship_subjects_as_vera_rs_policy_cmds() {
        let actor = crate::provider::SubjectRef::Actor("did:key:zActor".to_string());
        let value: Value = serde_json::from_slice(&encode_set_relationship_cmd(
            "users", "doc-1", "reader", &actor,
        ))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "SetRelationship": {
                    "resource": "users",
                    "object_id": "doc-1",
                    "relation": "reader",
                    "subject": { "Entity": "did:key:zActor" }
                }
            })
        );

        let value: Value = serde_json::from_slice(&encode_delete_relationship_cmd(
            "users",
            "doc-1",
            "reader",
            &crate::provider::SubjectRef::AllActors,
        ))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "DeleteRelationship": {
                    "resource": "users",
                    "object_id": "doc-1",
                    "relation": "reader",
                    "subject": "Wildcard"
                }
            })
        );
    }
}

#[cfg(test)]
mod request_bearer_tests {
    use super::*;

    #[nuclei::test]
    async fn a_remote_identity_acts_with_the_token_its_request_carries() {
        let did = "did:key:z6MkVeraRsRequestBearer";
        defra_core::signing::set_request_bearer_token(did, "stored");
        let request = defra_core::RequestContext::default()
            .with_bearer(Some(did.to_string()), Some("request".to_string()));

        let own = resolve_registered_or_passthrough_bearer_token_as(
            &request,
            did,
            "did:key:submitter",
            9001,
        )
        .await
        .unwrap();
        let unscoped = resolve_registered_or_passthrough_bearer_token_as(
            &defra_core::RequestContext::default(),
            did,
            "did:key:submitter",
            9001,
        )
        .await
        .unwrap();
        defra_core::signing::clear_request_bearer_token(did);

        assert_eq!(own.as_deref(), Some("request"));
        assert_eq!(unscoped.as_deref(), Some("stored"));
    }
}
