//! Owed changes sent to a provider whose worker journals its requests (see
//! `acp::OwedJournal`).
//!
//! Each change keeps its own credential: a request carries one command per
//! change, each under the token [`owed::token`] gives it, and several
//! commands go in one `batchCalls`, which vera.rs applies together or rolls
//! back whole.

use std::sync::Arc;

use acp::{
    InFlight, OwedAuthority, OwedChange, OwedJournal, OwedOp, OwedSubject, PreparedRequest,
    RequestOutcome, Submission,
};
use async_trait::async_trait;

use super::{owed, VeraDocumentACP};
use crate::provider::{owed_request_digest, OwedCall, OwedRequestOutcome, ProviderError};
use crate::{PolicyAction, PolicyCommand, PolicySubject, SubjectRef};

/// The most bytes one request built for owed changes may take. A change
/// whose command alone is larger still goes, alone.
pub const MAX_OWED_REQUEST_BYTES: usize = 256 * 1024;

/// A change's subject, owned, as its command names it.
enum Subject {
    Actor(SubjectRef),
    Structured {
        kind: u8,
        resource: String,
        object: String,
        relation: String,
    },
}

/// One change's command, owned, and the token it goes under.
struct Call<'a> {
    change: &'a OwedChange,
    token: String,
    subject: Subject,
}

impl Call<'_> {
    fn borrowed(&self) -> OwedCall<'_> {
        let subject = match &self.subject {
            Subject::Actor(actor) => PolicySubject::Actor(actor),
            Subject::Structured {
                kind,
                resource,
                object,
                relation,
            } => PolicySubject::Structured {
                kind: *kind,
                resource,
                object_id: object,
                relation,
            },
        };
        let tuple = &self.change.tuple;
        let relation = tuple.relation.as_str();
        // `subject` admits a transfer only to one actor.
        let new_owner = match &self.subject {
            Subject::Actor(SubjectRef::Actor(did)) => did.as_str(),
            _ => "",
        };
        OwedCall {
            bearer_token: &self.token,
            command: PolicyCommand {
                policy_id: &tuple.policy_id,
                resource: &tuple.resource,
                object_id: &tuple.object,
                action: match self.change.op {
                    OwedOp::Add => PolicyAction::SetRelationship { relation, subject },
                    OwedOp::Delete => PolicyAction::DeleteRelationship { relation, subject },
                    OwedOp::Archive => PolicyAction::ArchiveObject,
                    OwedOp::Transfer => PolicyAction::TransferObject { new_owner },
                },
            },
        }
    }
}

/// The subject `change` names, or the answer that ends its submission: a
/// kind of subject the provider cannot take.
fn subject(change: &OwedChange) -> Result<Subject, Submission> {
    if change.op == OwedOp::Transfer && change.tuple.new_owner().is_none() {
        return Err(Submission::Unsupported {
            reason: acp::Error::UnsupportedTransfer.to_string(),
        });
    }
    match &change.tuple.subject {
        OwedSubject::Actor { did } => Ok(Subject::Actor(SubjectRef::Actor(did.to_string()))),
        OwedSubject::Wildcard => Ok(Subject::Actor(SubjectRef::AllActors)),
        OwedSubject::EntitySet { .. } | OwedSubject::TypedWildcard { .. } => {
            let subject = zanzibar::Subject::from(change.tuple.subject.clone());
            match zanzibar::encode_subject(&subject) {
                Ok((kind, resource, object, relation))
                    if matches!(subject, zanzibar::Subject::EntitySet { .. }) =>
                {
                    Ok(Subject::Structured {
                        kind,
                        resource,
                        object,
                        relation,
                    })
                }
                _ => Err(Submission::Unsupported {
                    reason: acp::Error::UnsupportedSubject(subject.to_string()).to_string(),
                }),
            }
        }
    }
}

/// The journal of the provider behind a [`VeraDocumentACP`] bound to no
/// request.
pub(super) struct VeraOwedJournal {
    pub(super) acp: Arc<VeraDocumentACP>,
}

impl VeraOwedJournal {
    fn request(&self, calls: &[Call<'_>]) -> Result<Vec<u8>, ProviderError> {
        let borrowed: Vec<OwedCall<'_>> = calls.iter().map(Call::borrowed).collect();
        self.acp.provider.owed_request(&borrowed)
    }
}

fn unsent(error: ProviderError) -> Submission {
    match error {
        ProviderError::Unavailable(reason) => Submission::Unavailable { reason },
        other => Submission::NotSent {
            reason: other.to_string(),
        },
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl OwedJournal for VeraOwedJournal {
    async fn prepare(&self, changes: &[OwedChange]) -> Result<PreparedRequest, Submission> {
        // Taken before the request exists: whatever the worker journals for
        // it is signed with this sequence or a later one.
        let floor = self
            .acp
            .remote(|provider| async move { provider.owed_request_floor().await })
            .await
            .map_err(unsent)?;
        let mut calls: Vec<Call<'_>> = Vec::with_capacity(changes.len());
        let mut bytes = Vec::new();
        for change in changes {
            let first = calls.is_empty();
            let call = match (owed::token(&self.acp, change).await, subject(change)) {
                (Ok(token), Ok(subject)) => Call {
                    change,
                    token,
                    subject,
                },
                (Err(answer), _) | (_, Err(answer)) if first => return Err(answer),
                _ => break,
            };
            calls.push(call);
            match self.request(&calls) {
                Ok(request) if first || request.len() <= MAX_OWED_REQUEST_BYTES => bytes = request,
                Ok(_) => {
                    calls.pop();
                    break;
                }
                Err(error) if first => return Err(unsent(error)),
                Err(_) => {
                    calls.pop();
                    break;
                }
            }
        }
        Ok(PreparedRequest {
            digest: owed_request_digest(&bytes),
            bytes,
            carries: calls.len(),
            floor: Some(floor),
        })
    }

    async fn journal(&self, request: &PreparedRequest) -> Result<String, Submission> {
        let bytes = request.bytes.clone();
        self.acp
            .remote(move |provider| async move { provider.journal_owed_request(&bytes).await })
            .await
            .map_err(unsent)
    }

    async fn outcome(&self, flight: &InFlight) -> RequestOutcome {
        let (digest, tx, floor) = (flight.digest.clone(), flight.tx.clone(), flight.floor);
        let outcome = self
            .acp
            .remote(move |provider| async move {
                provider
                    .owed_request_outcome(&digest, tx.as_deref(), floor)
                    .await
            })
            .await;
        match outcome {
            Ok(OwedRequestOutcome::Applied) => RequestOutcome::Applied,
            Ok(OwedRequestOutcome::Rejected(reason)) => RequestOutcome::Rejected { reason },
            Ok(OwedRequestOutcome::Unknown) => RequestOutcome::Unknown,
            Err(error) => RequestOutcome::Unavailable {
                reason: error.to_string(),
            },
        }
    }

    fn refusal(&self, change: &OwedChange, reason: String) -> Submission {
        let lapsed = match &change.authority {
            OwedAuthority::Delegation { token } => owed::lapsed(&self.acp, change, token),
            OwedAuthority::NodeKey if !self.acp.provider.holds_key_for(change.actor.as_str()) => {
                Some(format!(
                    "the node no longer holds a key for {}",
                    change.actor
                ))
            }
            OwedAuthority::NodeKey => None,
        };
        match lapsed {
            Some(reason) => Submission::AwaitingAuthority { reason },
            None => Submission::Refused { reason },
        }
    }
}
