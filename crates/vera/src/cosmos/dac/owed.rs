//! Submitting a relationship change the node owes Vera (see `acp::outbox`).
//!
//! The change is submitted as its acting DID with what was recorded when it
//! was staged, never with the credentials of whatever request runs the
//! submission: a token signed now with the node's key for that DID, or the
//! caller's delegation as it was presented, which may be bound to this one
//! change. That holds for every subject: a
//! userset or cross-object subject is sent under the same token, not under
//! the provider's own actor. Vera sets a relationship it
//! already holds and deletes one it does not hold without error, so a change
//! submitted a second time is applied again and found applied.

use acp::{OwedAuthority, OwedChange, OwedOp, OwedSubject, Submission};
use defra_core::RequestContext;

use super::{owned, VeraDocumentACP};
use crate::provider::{ProviderError, SubjectRef};

/// Why `token` no longer lets the node submit `change`, if it does not. A
/// token bound to one request is checked against the change it is submitted
/// with.
fn lapsed(acp: &VeraDocumentACP, change: &OwedChange, token: &str) -> Option<String> {
    let actor = change.actor.as_str();
    let command = acp::ProviderCommand::Relationship {
        tuple: &change.tuple,
        op: change.op,
    };
    let bound = super::request_bound::with_policy_command(command, |command| {
        command.and_then(|command| acp.request_bound(actor, token, command))
    });
    if let Some(bound) = bound {
        return bound.err();
    }
    match acp.provider.caller_of_delegation(token)? {
        Ok(caller)
            if acp.provider.canonical_actor(caller.did.as_str())
                == acp.provider.canonical_actor(actor) =>
        {
            None
        }
        Ok(caller) => Some(format!(
            "the recorded delegation is signed by {}",
            caller.did
        )),
        Err(error) => Some(error.to_string()),
    }
}

/// The token the change is submitted with, or the answer that ends the
/// submission before it is sent.
async fn token(acp: &VeraDocumentACP, change: &OwedChange) -> Result<String, Submission> {
    let actor = change.actor.as_str();
    match &change.authority {
        OwedAuthority::Delegation { token } => match lapsed(acp, change, token) {
            Some(reason) => Err(Submission::AwaitingAuthority { reason }),
            None => Ok(token.clone()),
        },
        OwedAuthority::NodeKey => {
            if !acp.provider.holds_key_for(actor) {
                return Err(Submission::AwaitingAuthority {
                    reason: format!("the node no longer holds a key for {actor}"),
                });
            }
            let did = actor.to_owned();
            acp.remote(move |provider| async move {
                provider
                    .create_bearer_token_as(&RequestContext::default(), &did)
                    .await
            })
            .await
            .map_err(|error| match error {
                ProviderError::Unavailable(reason) => Submission::Unavailable { reason },
                other => Submission::AwaitingAuthority {
                    reason: other.to_string(),
                },
            })
        }
    }
}

pub(super) async fn submit(acp: &VeraDocumentACP, change: &OwedChange) -> Submission {
    let token = match token(acp, change).await {
        Ok(token) => token,
        Err(answer) => return answer,
    };
    let tuple = &change.tuple;
    let add = change.op == OwedOp::Add;
    let [policy, resource, object, relation] = owned([
        &tuple.policy_id,
        &tuple.resource,
        &tuple.object,
        &tuple.relation,
    ]);
    let actor_subject = match &tuple.subject {
        OwedSubject::Actor { did } => Some(SubjectRef::Actor(did.to_string())),
        OwedSubject::Wildcard => Some(SubjectRef::AllActors),
        OwedSubject::EntitySet { .. } | OwedSubject::TypedWildcard { .. } => None,
    };
    let written = match actor_subject {
        Some(subject) => {
            acp.remote(move |provider| async move {
                if add {
                    provider
                        .set_relationship(&token, &policy, &resource, &object, &relation, &subject)
                        .await
                } else {
                    provider
                        .delete_relationship(
                            &token, &policy, &resource, &object, &relation, &subject,
                        )
                        .await
                }
            })
            .await
        }
        None => {
            let subject = zanzibar::Subject::from(tuple.subject.clone());
            let (kind, subject_resource, subject_object, subject_relation) =
                match zanzibar::encode_subject(&subject) {
                    Ok(encoded) if matches!(subject, zanzibar::Subject::EntitySet { .. }) => {
                        encoded
                    }
                    _ => {
                        return Submission::Unsupported {
                            reason: acp::Error::UnsupportedSubject(subject.to_string()).to_string(),
                        }
                    }
                };
            acp.remote(move |provider| async move {
                if add {
                    provider
                        .set_relationship_subject_as(
                            &token,
                            &policy,
                            &resource,
                            &object,
                            &relation,
                            kind,
                            &subject_resource,
                            &subject_object,
                            &subject_relation,
                        )
                        .await
                } else {
                    provider
                        .delete_relationship_subject_as(
                            &token,
                            &policy,
                            &resource,
                            &object,
                            &relation,
                            kind,
                            &subject_resource,
                            &subject_object,
                            &subject_relation,
                        )
                        .await
                }
            })
            .await
        }
    };
    match written {
        Ok(_) => {
            acp.invalidate_cached_access(&tuple.policy_id);
            Submission::Applied
        }
        Err(ProviderError::Unavailable(reason)) => Submission::Unavailable { reason },
        Err(error) if acp.provider.write_refused(&error) => {
            // A delegation that expired on the way is not the provider's
            // word on the change.
            let expired = match &change.authority {
                OwedAuthority::Delegation { token } => lapsed(acp, change, token),
                OwedAuthority::NodeKey => None,
            };
            match expired {
                Some(reason) => Submission::AwaitingAuthority { reason },
                None => Submission::Refused {
                    reason: error.to_string(),
                },
            }
        }
        Err(ProviderError::Unsupported(reason)) => Submission::Unsupported { reason },
        // The provider executed or admitted the request and failed it.
        Err(error @ ProviderError::Transaction(_)) => Submission::Failed {
            reason: error.to_string(),
        },
        // Anything else failed on this node before the request was sent.
        Err(error) => Submission::NotSent {
            reason: error.to_string(),
        },
    }
}
