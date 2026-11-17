//! The digest a request-bound delegation names a policy command by.
//!
//! vera.rs binds a delegation to one request through the digest of the typed
//! command: SHA-256 of `vera/acp-operation/v1\0` and the compact JSON of
//! `{"PolicyCommand":[policy ID, command]}`, the policy ID as 64 lowercase
//! hexadecimal digits and the command as vera.rs decodes the bytes it is
//! sent. The digest is computed here from the very value the provider
//! serializes and submits, so a token that passes the node's check is one
//! vera.rs accepts for that submission, and an actor inside the command is in
//! the spelling the node sends.

use vera_modules::acp::delegated_operation::DelegatedOperation;

use super::policy_cmd;
use crate::policy_command::PolicyCommand;
use crate::provider::ProviderError;

/// The digest a delegation bound to one request names `command` by: what its
/// signer puts in the token's `request` claim.
pub fn request_digest(command: &PolicyCommand<'_>) -> Result<[u8; 32], ProviderError> {
    let policy = vera_client::parse_policy_id(command.policy_id)
        .map_err(|e| ProviderError::Query(e.to_string()))?;
    let typed = policy_cmd::typed(command)?;
    DelegatedOperation::PolicyCommand(&hex::encode(policy.as_slice()), &typed)
        .digest()
        .map_err(|e| ProviderError::Config(format!("policy command digest: {e}")))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::policy_command::{PolicyAction, PolicySubject};
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
