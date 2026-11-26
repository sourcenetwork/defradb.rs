//! Several registrations of one owner sent as one list.
//!
//! The list goes under one bearer token for the owner, as vera.rs's
//! `bearerPolicyCmds`, at most [`MAX_REGISTRATIONS_PER_REQUEST`] at a time:
//! vera.rs authorizes it once and applies its commands in order, all or none.
//! A list vera.rs refuses applied nothing, and its registrations are sent
//! again one at a time, first to last, so each is answered as it would be
//! alone and the first refused one is named. A provider that takes no list
//! is sent them one at a time as well. A list whose answer is lost is not
//! sent again from here: the caller keeps what is owed, and asks the
//! provider what it holds before it sends anything again
//! (`proofs/tla/RegistrationBatch.tla`).

use acp::{AcpObject, RegistrationFailure};
use identity::Did;

use super::{provider_err, VeraDocumentACP};
use crate::provider::{ProviderError, MAX_REGISTRATIONS_PER_REQUEST};
use crate::{PolicyAction, PolicyCommand};

impl VeraDocumentACP {
    /// Register `objects` to `identity` in order, as lists where the token
    /// the node acts for `identity` with covers every one of them.
    pub(super) async fn register_listed(
        &self,
        identity: &Did,
        objects: &[AcpObject<'_>],
    ) -> Result<(), RegistrationFailure> {
        if objects.len() < 2 {
            return self.register_one_by_one(identity, objects, 0).await;
        }
        let bearer_token = self
            .create_bearer_token(identity.as_str())
            .await
            .map_err(|error| RegistrationFailure::new(0, error))?;
        // A delegation bound to one request names one command: a list it
        // cannot cover goes one registration at a time, each admitted alone.
        let covered = objects.iter().all(|object| {
            self.admit_for(identity.as_str(), &bearer_token, &command(object))
                .is_ok()
        });
        if !covered {
            return self.register_one_by_one(identity, objects, 0).await;
        }
        for (chunk, listed) in objects.chunks(MAX_REGISTRATIONS_PER_REQUEST).enumerate() {
            let offset = chunk * MAX_REGISTRATIONS_PER_REQUEST;
            let owned: Vec<([String; 3], Vec<acp::DerivedRelationship>)> = listed
                .iter()
                .map(|object| {
                    (
                        [object.policy_id, object.resource, object.object_id].map(str::to_owned),
                        object.relationships.to_vec(),
                    )
                })
                .collect();
            let token = bearer_token.clone();
            let sent = self
                .remote(move |provider| async move {
                    let refs: Vec<AcpObject<'_>> = owned
                        .iter()
                        .map(
                            |([policy_id, resource, object_id], relationships)| AcpObject {
                                policy_id,
                                resource,
                                object_id,
                                relationships,
                            },
                        )
                        .collect();
                    provider.register_objects(&token, &refs).await
                })
                .await;
            match sent {
                Ok(()) => {
                    for object in listed {
                        self.invalidate_cached_access(object.policy_id);
                    }
                }
                Err(error @ ProviderError::Unavailable(_)) => {
                    return Err(RegistrationFailure::new(offset, provider_err(error)));
                }
                Err(_) => self.register_one_by_one(identity, listed, offset).await?,
            }
        }
        Ok(())
    }

    /// Register `objects` one request each, stopping at the first that
    /// fails; `offset` is where they start in the caller's list.
    async fn register_one_by_one(
        &self,
        identity: &Did,
        objects: &[AcpObject<'_>],
        offset: usize,
    ) -> Result<(), RegistrationFailure> {
        for (index, object) in objects.iter().enumerate() {
            acp::DocumentACP::register_doc_object(
                self,
                identity,
                object.policy_id,
                object.resource,
                object.object_id,
                object.relationships,
            )
            .await
            .map_err(|error| RegistrationFailure::new(offset + index, error))?;
        }
        Ok(())
    }
}

fn command<'a>(object: &AcpObject<'a>) -> PolicyCommand<'a> {
    PolicyCommand {
        policy_id: object.policy_id,
        resource: object.resource,
        object_id: object.object_id,
        action: PolicyAction::register(object.relationships),
    }
}
