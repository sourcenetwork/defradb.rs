//! The bytes of a policy command and the digest a request-bound delegation
//! names it by.
//!
//! vera.rs binds a delegation to one request through the digest of the typed
//! command: SHA-256 of `vera/acp-operation/v1\0` and the compact JSON of
//! `{"PolicyCommand":[policy ID, command]}`, the policy ID as 64 lowercase
//! hexadecimal digits and the command as vera.rs decodes the bytes it is
//! sent. The digest is computed here from the very bytes the provider
//! submits, so a token that passes the node's check is one vera.rs accepts
//! for that submission, and an actor inside the command is in the spelling
//! the node sends.

use vera_modules::acp::delegated_operation::DelegatedOperation;
use vera_modules::acp::types::PolicyCmd;

use super::did_spelling;
use super::provider_commands::{
    encode_delete_relationship_cmd, encode_register_object_cmd, encode_set_relationship_cmd,
};
use crate::policy_command::{PolicyAction, PolicyCommand, PolicySubject};
use crate::provider::ProviderError;

/// A relationship subject as vera.rs is sent it: an actor in the spelling
/// vera.rs takes, which is the one an access check for it is sent in.
pub(super) fn vera_subject(
    kind: u8,
    resource: &str,
    object_id: &str,
    relation: &str,
) -> Result<zanzibar::Subject, ProviderError> {
    let object_id = if kind == 0 {
        did_spelling::wire(object_id)
    } else {
        object_id.into()
    };
    zanzibar::decode_subject(kind, resource, &object_id, relation)
        .map_err(|e| ProviderError::Config(format!("relationship subject: {e}")))
}

/// The command that adds (`set`) or deletes a relationship whose subject is
/// a userset or another object.
pub(super) fn encode_subject_relationship_cmd(
    set: bool,
    resource: &str,
    object_id: &str,
    relation: &str,
    subject: &zanzibar::Subject,
) -> Result<Vec<u8>, ProviderError> {
    let body = serde_json::json!({
        "resource": resource, "object_id": object_id, "relation": relation, "subject": subject
    });
    let command = if set {
        serde_json::json!({ "SetRelationship": body })
    } else {
        serde_json::json!({ "DeleteRelationship": body })
    };
    serde_json::to_vec(&command).map_err(|e| ProviderError::Config(e.to_string()))
}

/// The bytes `command` is submitted as.
fn encode(command: &PolicyCommand<'_>) -> Result<Vec<u8>, ProviderError> {
    let (resource, object_id) = (command.resource, command.object_id);
    let relationship = |set: bool, relation: &str, subject: &PolicySubject<'_>| match subject {
        PolicySubject::Actor(subject) if set => Ok(encode_set_relationship_cmd(
            resource, object_id, relation, subject,
        )),
        PolicySubject::Actor(subject) => Ok(encode_delete_relationship_cmd(
            resource, object_id, relation, subject,
        )),
        PolicySubject::Structured {
            kind,
            resource: subject_resource,
            object_id: subject_object,
            relation: subject_relation,
        } => encode_subject_relationship_cmd(
            set,
            resource,
            object_id,
            relation,
            &vera_subject(*kind, subject_resource, subject_object, subject_relation)?,
        ),
    };
    match &command.action {
        PolicyAction::RegisterObject => Ok(encode_register_object_cmd(resource, object_id)),
        PolicyAction::SetRelationship { relation, subject } => {
            relationship(true, relation, subject)
        }
        PolicyAction::DeleteRelationship { relation, subject } => {
            relationship(false, relation, subject)
        }
    }
}

/// The digest a delegation bound to one request names `command` by: what its
/// signer puts in the token's `request` claim.
pub fn request_digest(command: &PolicyCommand<'_>) -> Result<[u8; 32], ProviderError> {
    let policy = vera_client::parse_policy_id(command.policy_id)
        .map_err(|e| ProviderError::Query(e.to_string()))?;
    let typed: PolicyCmd = serde_json::from_slice(&encode(command)?)
        .map_err(|e| ProviderError::Config(format!("policy command: {e}")))?;
    DelegatedOperation::PolicyCommand(&hex::encode(policy.as_slice()), &typed)
        .digest()
        .map_err(|e| ProviderError::Config(format!("policy command digest: {e}")))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::provider::SubjectRef;

    const POLICY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn command(action: PolicyAction<'_>) -> PolicyCommand<'_> {
        PolicyCommand {
            policy_id: POLICY,
            resource: "users",
            object_id: "bae-1",
            action,
        }
    }

    fn digest_of(json: &str) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"vera/acp-operation/v1\0");
        hash.update(json.as_bytes());
        hash.finalize().into()
    }

    #[test]
    fn a_registration_is_named_by_the_digest_of_its_typed_command() {
        assert_eq!(
            request_digest(&command(PolicyAction::RegisterObject)).unwrap(),
            digest_of(&format!(
                r#"{{"PolicyCommand":["{POLICY}",{{"RegisterObject":{{"resource":"users","id":"bae-1"}}}}]}}"#
            ))
        );
    }

    #[test]
    fn the_policy_is_named_without_its_prefix() {
        let prefixed = format!("0x{POLICY}");
        let other = PolicyCommand {
            policy_id: &prefixed,
            ..command(PolicyAction::RegisterObject)
        };
        assert_eq!(
            request_digest(&other).unwrap(),
            request_digest(&command(PolicyAction::RegisterObject)).unwrap()
        );
    }

    #[test]
    fn every_part_of_a_command_changes_its_digest() {
        let reader =
            SubjectRef::Actor("did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR".into());
        let everyone = SubjectRef::AllActors;
        let grant = |relation, subject| {
            command(PolicyAction::SetRelationship {
                relation,
                subject: PolicySubject::Actor(subject),
            })
        };
        let revoke = command(PolicyAction::DeleteRelationship {
            relation: "reader",
            subject: PolicySubject::Actor(&reader),
        });
        let userset = command(PolicyAction::SetRelationship {
            relation: "reader",
            subject: PolicySubject::Structured {
                kind: 3,
                resource: "groups",
                object_id: "team",
                relation: "member",
            },
        });
        let other_object = PolicyCommand {
            object_id: "bae-2",
            ..grant("reader", &reader)
        };
        let other_resource = PolicyCommand {
            resource: "rows",
            ..grant("reader", &reader)
        };
        let other_policy = PolicyCommand {
            policy_id: "ff112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            ..grant("reader", &reader)
        };
        let commands = [
            command(PolicyAction::RegisterObject),
            grant("reader", &reader),
            grant("writer", &reader),
            grant("reader", &everyone),
            revoke,
            userset,
            other_object,
            other_resource,
            other_policy,
        ];
        let digests: Vec<_> = commands
            .iter()
            .map(|command| request_digest(command).unwrap())
            .collect();
        for (i, left) in digests.iter().enumerate() {
            for right in &digests[i + 1..] {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn a_policy_that_is_not_an_identifier_has_no_digest() {
        let unnamed = PolicyCommand {
            policy_id: "not a policy",
            ..command(PolicyAction::RegisterObject)
        };
        assert!(request_digest(&unnamed).is_err());
    }
}
