//! What a provider answers when an owed change is submitted to it.

use defra_core::RequestContext;
use zanzibar::types::Subject;

use super::record::{OwedAuthority, OwedChange, OwedOp};
use crate::dac::DocumentACP;
use crate::error::{Error, Result};

/// The provider's answer to one submitted change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Submission {
    /// The provider holds the change: it applied it now, or found it applied.
    Applied,
    /// The provider's final answer is no: the acting DID lacks the authority,
    /// or the request was rejected for what it asks.
    Refused { reason: String },
    /// What was recorded with the change no longer lets the node submit it
    /// as its acting DID: a delegation that expired, a key no longer held.
    AwaitingAuthority { reason: String },
    /// The provider could not be reached or did not answer. Nothing is known
    /// about the change, which may have been applied.
    Unavailable { reason: String },
    /// The provider answered with an error that does not say whether it is
    /// its final word on the change.
    Failed { reason: String },
    /// The provider cannot take this kind of change at all, whoever asks.
    Unsupported { reason: String },
    /// The node could not make or send the request: an error of its own,
    /// which says nothing about the provider or the change.
    NotSent { reason: String },
}

impl Submission {
    /// How the answer of a provider's relationship call is read when the
    /// provider classifies nothing itself: an error that names a refusal is
    /// one, [`Error::ProviderUnavailable`] says the provider was not
    /// reached, and any other error is the provider's unclassified answer.
    pub fn of(result: Result<bool>) -> Self {
        match result {
            Ok(_) => Self::Applied,
            Err(
                error @ (Error::NotOwner { .. }
                | Error::NotManager { .. }
                | Error::PermissionDenied(_)
                | Error::DocumentNotRegistered(_)
                | Error::InvalidRelation(_)),
            ) => Self::Refused {
                reason: error.to_string(),
            },
            Err(error @ Error::UnsupportedSubject(_)) => Self::Unsupported {
                reason: error.to_string(),
            },
            Err(
                error @ (Error::OwedAuthorityUnavailable { .. }
                | Error::DelegationOutsideWindow { .. }),
            ) => Self::AwaitingAuthority {
                reason: error.to_string(),
            },
            Err(Error::ProviderUnavailable(reason)) => Self::Unavailable { reason },
            Err(error) => Self::Failed {
                reason: error.to_string(),
            },
        }
    }
}

/// Submit `change` through `provider`'s relationship calls, as its acting
/// DID. A provider that takes its callers' credentials is bound to the
/// delegation recorded with the change. The provider is given no managing
/// relations: it decides the acting DID's authority from its own state.
pub async fn submit_relationship<P>(provider: &P, change: &OwedChange) -> Submission
where
    P: DocumentACP + ?Sized,
{
    let bound = match &change.authority {
        OwedAuthority::Delegation { token } => provider.bind_request(
            &RequestContext::default()
                .with_bearer(Some(change.actor.to_string()), Some(token.clone())),
        ),
        OwedAuthority::NodeKey => None,
    };
    Submission::of(match &bound {
        Some(bound) => relationship_call(&**bound, change).await,
        None => relationship_call(provider, change).await,
    })
}

async fn relationship_call<P>(provider: &P, change: &OwedChange) -> Result<bool>
where
    P: DocumentACP + ?Sized,
{
    let tuple = &change.tuple;
    let subject = Subject::from(tuple.subject.clone());
    match change.op {
        OwedOp::Add => {
            provider
                .add_relationship(
                    &change.actor,
                    subject,
                    &tuple.policy_id,
                    &tuple.resource,
                    &tuple.object,
                    &tuple.relation,
                    &[],
                )
                .await
        }
        OwedOp::Delete => {
            provider
                .delete_relationship(
                    &change.actor,
                    subject,
                    &tuple.policy_id,
                    &tuple.resource,
                    &tuple.object,
                    &tuple.relation,
                    &[],
                )
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_told_from_a_failure_to_reach_the_provider() {
        assert_eq!(Submission::of(Ok(true)), Submission::Applied);
        assert_eq!(Submission::of(Ok(false)), Submission::Applied);
        assert!(matches!(
            Submission::of(Err(Error::NotManager {
                operation: "add relationship".into()
            })),
            Submission::Refused { .. }
        ));
        assert!(matches!(
            Submission::of(Err(Error::OwedAuthorityUnavailable {
                actor: "did:key:a".into(),
                reason: "expired".into()
            })),
            Submission::AwaitingAuthority { .. }
        ));
        assert!(matches!(
            Submission::of(Err(Error::ProviderUnavailable("unreachable".into()))),
            Submission::Unavailable { .. }
        ));
        assert!(matches!(
            Submission::of(Err(Error::Storage("no such policy".into()))),
            Submission::Failed { .. }
        ));
        assert!(matches!(
            Submission::of(Err(Error::UnsupportedSubject("groups:g#member".into()))),
            Submission::Unsupported { .. }
        ));
    }
}
