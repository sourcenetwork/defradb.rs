//! What a confirmed submission did, read back from the request as it was
//! signed.
//!
//! Every policy command the provider sends is a `bearerPolicyCmd` call: the
//! actor's delegation, the policy and the command as JSON. A registration or
//! a relationship change read from one can be owed again through the outbox,
//! under the actor that issued the delegation, and so can an archive (its
//! document was deleted), which the outbox owes as any change. A created
//! policy widens nothing; one that is lost leaves the documents under it
//! unreadable.

use alloy_sol_types::SolCall as _;
use serde::{Deserialize, Serialize};
use vera_acp::Subject;
use vera_domain::NativeTx;
use vera_modules::acp::types::PolicyCmd;

use super::super::abi::IAcp;
use super::super::did_spelling;

/// A registration or relationship change a term voided, to be owed again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Lost {
    Registration {
        policy_id: String,
        resource: String,
        object: String,
        /// The delegation it was sent under.
        token: String,
    },
    Relationship {
        add: bool,
        policy_id: String,
        resource: String,
        object: String,
        relation: String,
        subject: LostSubject,
        token: String,
    },
    Archive {
        policy_id: String,
        resource: String,
        object: String,
        token: String,
    },
}

/// A relationship's subject, as the outbox records one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LostSubject {
    Actor {
        did: String,
    },
    Wildcard,
    EntitySet {
        resource: String,
        object: String,
        relation: String,
    },
    TypedWildcard {
        resource: String,
    },
}

/// What one signed request did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// A change the outbox can owe again.
    Owed(Lost),
    /// A policy creation, a checked decision, or anything else that changes
    /// no document's access.
    Other,
}

impl Lost {
    /// The object the change is on: changes after a lost one on the same
    /// object are owed again too, so none overtakes it.
    pub(crate) fn object(&self) -> (&str, &str, &str) {
        match self {
            Self::Registration {
                policy_id,
                resource,
                object,
                ..
            }
            | Self::Relationship {
                policy_id,
                resource,
                object,
                ..
            }
            | Self::Archive {
                policy_id,
                resource,
                object,
                ..
            } => (policy_id, resource, object),
        }
    }

    pub(crate) fn token(&self) -> &str {
        match self {
            Self::Registration { token, .. }
            | Self::Relationship { token, .. }
            | Self::Archive { token, .. } => token,
        }
    }
}

impl Effect {
    /// The object an effect is on, when it is on one.
    pub(crate) fn object(&self) -> Option<(String, String, String)> {
        match self {
            Self::Owed(lost) => {
                let (policy, resource, object) = lost.object();
                Some((policy.into(), resource.into(), object.into()))
            }
            Self::Other => None,
        }
    }
}

/// What the signed request `wire` did. A request that is not a policy
/// command the provider sends does nothing a term could take away from a
/// document.
pub(crate) fn effect_of(wire: &[u8]) -> Effect {
    let Ok(tx) = NativeTx::decode_wire(wire) else {
        return Effect::Other;
    };
    let Ok(call) = IAcp::bearerPolicyCmdCall::abi_decode(&tx.calldata) else {
        return Effect::Other;
    };
    let Ok(command) = serde_json::from_slice::<PolicyCmd>(&call.cmd) else {
        return Effect::Other;
    };
    let policy_id = hex::encode(call.policyId);
    let token = call.bearerToken;
    let (add, relationship) = match command {
        PolicyCmd::RegisterObject(object) => {
            return Effect::Owed(Lost::Registration {
                policy_id,
                resource: object.resource,
                object: object.id,
                token,
            })
        }
        PolicyCmd::ArchiveObject(object) => {
            return Effect::Owed(Lost::Archive {
                policy_id,
                resource: object.resource,
                object: object.id,
                token,
            })
        }
        PolicyCmd::SetRelationship(relationship) => (true, relationship),
        PolicyCmd::DeleteRelationship(relationship) => (false, relationship),
        _ => return Effect::Other,
    };
    match subject(&relationship.subject) {
        Some(subject) => Effect::Owed(Lost::Relationship {
            add,
            policy_id,
            resource: relationship.resource,
            object: relationship.object_id,
            relation: relationship.relation,
            subject,
            token,
        }),
        None => Effect::Other,
    }
}

fn subject(subject: &Subject) -> Option<LostSubject> {
    Some(match subject {
        Subject::Entity(did) => LostSubject::Actor {
            did: did_spelling::defra(did.as_str()).into_owned(),
        },
        Subject::Wildcard => LostSubject::Wildcard,
        Subject::EntitySet {
            resource,
            object_id,
            relation,
        } => LostSubject::EntitySet {
            resource: resource.clone(),
            object: object_id.clone(),
            relation: relation.clone(),
        },
        Subject::TypedWildcard { resource } => LostSubject::TypedWildcard {
            resource: resource.clone(),
        },
        _ => return None,
    })
}

#[cfg(test)]
#[path = "effect_tests.rs"]
mod tests;
