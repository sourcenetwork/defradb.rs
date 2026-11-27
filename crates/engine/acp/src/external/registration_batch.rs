//! Settling the registrations several creates owe, together.
//!
//! The registrations a commit settles are read as [`Self::settle`] reads
//! one: an object the provider already holds is cleared without a request,
//! and each of the others gets the provider it is sent through. Those sent
//! as one creator with one credential go in one list the provider applies in
//! order, all or none ([`DocumentACP::register_objects`]); a delegation bound
//! to one request names one command and goes alone. A list the provider
//! refuses is sent again one registration at a time, so each one is answered
//! for, and noted, as if it had been sent alone. Nothing is sent again on a
//! lost answer: the records stay, and the next settlement finds the objects
//! the list registered held by the provider. Another settler can register an
//! object between this one asking and sending: the provider refuses it, and
//! the provider is asked again before the refusal is reported, so a held
//! object is cleared as held (`proofs/tla/RegistrationBatch.tla`).

use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashSet};
use storage::corekv::Store;

use super::ExternalDocumentAcp;
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::object_registration::{AcpObject, RegistrationFailure};
use crate::outbox::OwedAuthority;
use crate::pending::PendingRegistration;

/// What lets one list be sent for several registrations: one creator, and
/// one credential that names no single command.
#[derive(Clone, PartialEq, Eq)]
enum Sender {
    /// The bearer the request being served carries for the creator.
    Serving(String),
    /// What the records hold: the node's key, or one plain delegation.
    Recorded(String, Option<String>),
}

/// Registrations sent through one provider, as one list when they can be.
struct Group {
    sender: Option<Sender>,
    registrar: Arc<dyn DocumentACP>,
    members: Vec<(usize, PendingRegistration)>,
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// [`Self::settle_if_owed`] for each of `objects`, in their order: what
    /// each came to.
    pub(super) async fn settle_all_if_owed(&self, objects: &[AcpObject<'_>]) -> Vec<Result<()>> {
        if let [object] = objects {
            return vec![
                self.settle_if_owed(object.policy_id, object.resource, object.object_id)
                    .await,
            ];
        }
        let mut outcomes: Vec<Option<Result<()>>> = objects.iter().map(|_| None).collect();
        let mut groups: Vec<Group> = Vec::new();
        let mut seen = RapidHashSet::new();
        let mut away: Option<String> = None;
        for (index, object) in objects.iter().enumerate() {
            let owed = match self
                .owed(object.policy_id, object.resource, object.object_id)
                .await
            {
                Ok(Some(owed)) if seen.insert(owed.key()) => {
                    if owed.create_refused() {
                        outcomes[index] = Some(self.parked_create(&owed).await);
                        continue;
                    }
                    owed
                }
                Ok(_) => {
                    outcomes[index] = Some(Ok(()));
                    continue;
                }
                Err(error) => {
                    outcomes[index] = Some(Err(error));
                    continue;
                }
            };
            // A provider found away is not asked again for the rest now:
            // their records stay, as a redrive leaves them.
            if let Some(reason) = &away {
                outcomes[index] = Some(
                    self.failed(&owed, Error::ProviderUnavailable(reason.clone()))
                        .await,
                );
                continue;
            }
            match self.to_send(&owed).await {
                Ok(Some((registrar, sender))) => {
                    let group = sender.as_ref().and_then(|sender| {
                        groups
                            .iter_mut()
                            .find(|group| group.sender.as_ref() == Some(sender))
                    });
                    match group {
                        Some(group) => group.members.push((index, owed)),
                        None => groups.push(Group {
                            sender,
                            registrar,
                            members: vec![(index, owed)],
                        }),
                    }
                }
                Ok(None) => outcomes[index] = Some(Ok(())),
                Err(error) => {
                    if let Error::ProviderUnavailable(reason) = &error {
                        away = Some(reason.clone());
                    }
                    outcomes[index] = Some(self.failed(&owed, error).await);
                }
            }
        }
        for group in groups {
            if let Some(reason) = &away {
                for (index, owed) in &group.members {
                    outcomes[*index] = Some(
                        self.failed(owed, Error::ProviderUnavailable(reason.clone()))
                            .await,
                    );
                }
                continue;
            }
            self.send_group(group, &mut outcomes).await;
        }
        outcomes
            .into_iter()
            .map(|outcome| outcome.unwrap_or(Ok(())))
            .collect()
    }

    /// The provider `owed` is sent through and what it is sent with, or
    /// `None` when the provider already holds its object and its record is
    /// cleared.
    async fn to_send(
        &self,
        owed: &PendingRegistration,
    ) -> Result<Option<(Arc<dyn DocumentACP>, Option<Sender>)>> {
        if self
            .provider
            .is_doc_registered(&owed.policy_id, &owed.resource_name, &owed.doc_id)
            .await?
        {
            self.registered(owed, true).await?;
            return Ok(None);
        }
        let (registrar, serving) = self.registrar_from(owed, None)?;
        let creator = self.provider.canonical_actor(&owed.creator).to_string();
        let sender = match (&owed.authority, serving) {
            (_, true) if self.serving_binds_one_request() => None,
            (_, true) => Some(Sender::Serving(creator)),
            _ if owed.request_bound => None,
            (Some(OwedAuthority::NodeKey), false) => Some(Sender::Recorded(creator, None)),
            (Some(OwedAuthority::Delegation { token }), false) => {
                Some(Sender::Recorded(creator, Some(token.clone())))
            }
            (None, false) => None,
        };
        Ok(Some((registrar, sender)))
    }

    /// Send `group`'s registrations, as lists while they go, and record what
    /// each came to.
    async fn send_group(&self, group: Group, outcomes: &mut [Option<Result<()>>]) {
        let mut rest = group.members.as_slice();
        while let Some((_, first)) = rest.first() {
            let sent = if rest.len() == 1 {
                first
                    .submit(group.registrar.as_ref())
                    .await
                    .map_err(|error| RegistrationFailure::new(0, error))
            } else {
                let objects: Vec<AcpObject<'_>> = rest
                    .iter()
                    .map(|(_, owed)| AcpObject {
                        policy_id: &owed.policy_id,
                        resource: &owed.resource_name,
                        object_id: &owed.doc_id,
                        relationships: &owed.relationships,
                    })
                    .collect();
                group
                    .registrar
                    .register_objects(&first.creator, &objects)
                    .await
            };
            let (stop, failure) = match sent {
                Ok(()) => (rest.len(), None),
                Err(RegistrationFailure { index, error }) => (index, Some(error)),
            };
            for (index, owed) in &rest[..stop] {
                outcomes[*index] = Some(self.registered_or_noted(owed, false).await);
            }
            let Some(error) = failure else {
                return;
            };
            let (index, owed) = &rest[stop];
            let unavailable = match &error {
                Error::ProviderUnavailable(reason) => Some(reason.clone()),
                _ => None,
            };
            outcomes[*index] = Some(if self.held_after_refusal(owed, &error).await {
                self.registered_or_noted(owed, true).await
            } else {
                self.failed(owed, error).await
            });
            rest = &rest[stop + 1..];
            // An unreachable provider is not asked again for the rest now:
            // their records stay, as a redrive leaves them.
            if let Some(reason) = unavailable {
                for (index, owed) in rest {
                    outcomes[*index] = Some(
                        self.failed(owed, Error::ProviderUnavailable(reason.clone()))
                            .await,
                    );
                }
                return;
            }
        }
    }

    /// [`Self::registered`], its failure noted with the record.
    async fn registered_or_noted(&self, owed: &PendingRegistration, held: bool) -> Result<()> {
        match self.registered(owed, held).await {
            Ok(()) => Ok(()),
            Err(error) => self.failed(owed, error).await,
        }
    }

    /// Note `error` with the record of `owed` and return it.
    async fn failed(&self, owed: &PendingRegistration, error: Error) -> Result<()> {
        self.note_failure(owed, &error).await?;
        Err(error)
    }
}
