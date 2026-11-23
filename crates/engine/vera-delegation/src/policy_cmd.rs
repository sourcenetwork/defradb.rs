//! Policy commands as values of vera.rs's own command type.
//!
//! Every command this provider sends is a [`PolicyCmd`] serialized with
//! serde, and its request digest is computed from that same value, so a
//! change to the chain's command format breaks this crate's build when the
//! vera.rs pin moves instead of surfacing as a refused command.

use alloy_primitives::Bytes;
use alloy_sol_types::SolCall as _;
use vera_acp::{Relationship, Subject};
use vera_modules::acp::types::{Object, PolicyCmd};

use crate::abi::IAcp;
use crate::{
    did_spelling, parse_policy_id, CommandError, PolicyAction, PolicyCommand, PolicySubject,
    SubjectRef,
};

/// The command `command` is sent as.
pub fn typed(command: &PolicyCommand<'_>) -> Result<PolicyCmd, CommandError> {
    let relationship = |relation: &str, subject: &PolicySubject<'_>| {
        Ok::<_, CommandError>(Relationship::new(
            command.resource,
            command.object_id,
            relation,
            chain_subject(&defra_subject(subject)?)?,
        ))
    };
    Ok(match &command.action {
        PolicyAction::RegisterObject => {
            PolicyCmd::RegisterObject(object(command.resource, command.object_id))
        }
        PolicyAction::SetRelationship { relation, subject } => {
            PolicyCmd::SetRelationship(relationship(relation, subject)?)
        }
        PolicyAction::DeleteRelationship { relation, subject } => {
            PolicyCmd::DeleteRelationship(relationship(relation, subject)?)
        }
        PolicyAction::ArchiveObject => archive_object(command.resource, command.object_id),
    })
}

/// The command that archives one object.
pub fn archive_object(resource: &str, object_id: &str) -> PolicyCmd {
    PolicyCmd::ArchiveObject(object(resource, object_id))
}

/// The bytes `command` is submitted as.
pub fn encode(command: &PolicyCmd) -> Result<Vec<u8>, CommandError> {
    serde_json::to_vec(command).map_err(CommandError::Encode)
}

/// The call that submits `command` on `policy_id` under `bearer_token`.
pub fn calldata(
    bearer_token: &str,
    policy_id: &str,
    command: &PolicyCmd,
) -> Result<Bytes, CommandError> {
    let call = IAcp::bearerPolicyCmdCall {
        bearerToken: bearer_token.to_string(),
        policyId: parse_policy_id(policy_id)?,
        cmd: encode(command)?.into(),
    };
    Ok(call.abi_encode().into())
}

fn object(resource: &str, object_id: &str) -> Object {
    Object {
        resource: resource.into(),
        id: object_id.into(),
    }
}

/// A relationship subject as DefraDB's permission engine names it.
fn defra_subject(subject: &PolicySubject<'_>) -> Result<zanzibar::Subject, CommandError> {
    match subject {
        PolicySubject::Actor(SubjectRef::Actor(did)) => zanzibar::Did::new(did.as_str())
            .map(zanzibar::Subject::Entity)
            .map_err(|e| CommandError::Subject(e.to_string())),
        PolicySubject::Actor(SubjectRef::AllActors) => Ok(zanzibar::Subject::Wildcard),
        PolicySubject::Structured {
            kind,
            resource,
            object_id,
            relation,
        } => zanzibar::decode_subject(*kind, resource, object_id, relation)
            .map_err(|e| CommandError::Subject(e.to_string())),
    }
}

/// `subject` as vera.rs is sent it: an actor in the spelling vera.rs takes,
/// which is the one an access check for it is sent in. A subject vera.rs has
/// no command for is refused here, before anything is sent.
fn chain_subject(subject: &zanzibar::Subject) -> Result<Subject, CommandError> {
    match subject {
        zanzibar::Subject::Entity(did) => did_spelling::wire(did.as_str())
            .parse()
            .map(Subject::Entity)
            .map_err(|e| CommandError::Subject(e.to_string())),
        zanzibar::Subject::EntitySet {
            resource,
            object_id,
            relation,
        } => Ok(Subject::entity_set(resource, object_id, relation)),
        zanzibar::Subject::Wildcard => Ok(Subject::Wildcard),
        other => Err(CommandError::Subject(format!(
            "vera.rs takes no command for {other}"
        ))),
    }
}

#[cfg(test)]
mod tests;
