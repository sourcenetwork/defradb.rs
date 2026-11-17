//! The unit a receiving node owes for a replicated document it registers,
//! and the marker of one it adopted (see [`crate::replica`]).
//!
//! The unit is recorded in the merge transaction of the document's genesis.
//! It is settled after that commit, registration first, then its grants in
//! order, through the same pass as every owed change, and again whenever a
//! change to the document is put back while it is owed. A registration the
//! provider already holds for another owner makes the unit yield: its
//! grants, which the node cannot give, are discarded with a note, and the
//! marker becomes adopted. An adopted marker goes once the provider lets the
//! creator update the document.

use identity::Did;
use rapidhash::{HashSetExt, RapidHashSet};
use storage::corekv::Store;
use zanzibar::types::Subject;

use super::settle::Write;
use super::ExternalDocumentAcp;
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::outbox::{OwedOp, OwedState, OwedSubject, OwedTuple, StageRelationship, StagedCount};
use crate::pending::{PendingRegistration, PendingRegistrations};
use crate::permission::DocumentPermission;
use crate::provider_command::ProviderCommand;
use crate::replica::{
    replica_key, MarkedGrant, ReplicaMarker, ReplicaMarkers, ReplicaOwed, ReplicaUnit,
};

/// Why a grant of a unit that yielded was discarded.
const YIELDED: &str =
    "another actor holds the replicated document; its owner gives the creator's grants";

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// The marker kept for a document, as committed.
    pub(super) async fn replica_marker(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<ReplicaMarker>> {
        let txn = self.begin(true).await?;
        let marker = ReplicaMarkers::new(&txn.acpstore())
            .get(policy_id, resource_name, doc_id)
            .await;
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        marker
    }

    /// Whether a grant of the unit `marker` keeps is still owed: staged, or
    /// waiting for authority. A refused grant is owed no more; the provider
    /// decides the creator from then on.
    async fn unit_grants_owed(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        marker: &ReplicaMarker,
    ) -> Result<bool> {
        for grant in &marker.grants {
            let tuple = OwedTuple {
                policy_id: policy_id.to_owned(),
                resource: resource_name.to_owned(),
                object: doc_id.to_owned(),
                relation: grant.relation.clone(),
                subject: grant.subject.clone(),
            };
            if let Some(oldest) = self.oldest_on(&tuple).await? {
                if matches!(
                    oldest.state,
                    OwedState::Staged | OwedState::AwaitingAuthority
                ) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub(super) async fn owed_on_replica(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<ReplicaOwed> {
        let mut owed = ReplicaOwed {
            registration: self
                .owed(policy_id, resource_name, doc_id)
                .await?
                .map(|registration| registration.creator),
            ..ReplicaOwed::default()
        };
        if let Some(marker) = self
            .replica_marker(policy_id, resource_name, doc_id)
            .await?
        {
            owed.adopted = marker.adopted;
            if !marker.adopted {
                owed.creator_grants = self
                    .unit_grants_owed(policy_id, resource_name, doc_id, &marker)
                    .await?;
            }
            owed.creator = Some(marker.creator);
        }
        Ok(owed)
    }

    /// `subject` as an owed change keeps it: an actor in the provider's
    /// spelling.
    fn owed_subject(&self, subject: &Subject) -> Result<OwedSubject> {
        Ok(match OwedSubject::try_from(subject.clone())? {
            OwedSubject::Actor { did } => OwedSubject::Actor {
                did: self.provider.canonical_actor(&did),
            },
            other => other,
        })
    }

    pub(super) async fn record_replica_unit(
        &self,
        view: &datastore::NamespaceView,
        unit: &ReplicaUnit<'_>,
    ) -> Result<()> {
        let (policy_id, resource, doc_id) = (unit.policy_id, unit.resource, unit.doc_id);
        let pending = PendingRegistrations::new(view);
        let markers = ReplicaMarkers::new(view);
        if pending.get(policy_id, resource, doc_id).await?.is_some()
            || markers.get(policy_id, resource, doc_id).await?.is_some()
        {
            return Ok(());
        }
        let registrant = self.provider.canonical_actor(unit.registrant);
        let creator = self.provider.canonical_actor(unit.creator);
        let authority = self.provider.command_authority(
            &registrant,
            ProviderCommand::RegisterObject {
                policy_id,
                resource,
                object: doc_id,
            },
        )?;
        pending
            .record(
                &PendingRegistration::new(registrant.clone(), policy_id, resource, doc_id)
                    .with_authority(Ok(authority), false)
                    .for_replica_of(creator.clone()),
            )
            .await?;
        let Some(outbox) = self.transaction_outbox(view, StagedCount::new()) else {
            return Err(Error::Storage("the gate keeps no outbox".into()));
        };
        let mut grants = Vec::with_capacity(unit.grants.len());
        for grant in unit.grants {
            outbox
                .stage(StageRelationship {
                    txn: None,
                    actor: &registrant,
                    subject: grant.subject.clone(),
                    policy_id,
                    resource,
                    object: doc_id,
                    relation: &grant.relation,
                    managing_relations: &[],
                    op: OwedOp::Add,
                })
                .await?;
            grants.push(MarkedGrant {
                relation: grant.relation.clone(),
                subject: self.owed_subject(&grant.subject)?,
            });
        }
        markers
            .put(
                policy_id,
                resource,
                doc_id,
                &ReplicaMarker {
                    creator,
                    registrant: Some(registrant),
                    adopted: false,
                    grants,
                },
            )
            .await
    }

    pub(super) async fn record_adopted_replica(
        &self,
        view: &datastore::NamespaceView,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        creator: &Did,
    ) -> Result<()> {
        let markers = ReplicaMarkers::new(view);
        if markers
            .get(policy_id, resource_name, doc_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        markers
            .put(
                policy_id,
                resource_name,
                doc_id,
                &ReplicaMarker {
                    creator: self.provider.canonical_actor(creator),
                    registrant: None,
                    adopted: true,
                    grants: Vec::new(),
                },
            )
            .await
    }

    /// After the registration `owed` found the document held: when the
    /// owner is not the node's own actor, discard the unit's grants with a
    /// note and keep the marker as adopted. A provider that holds the
    /// document for the node's actor already took its registration before a
    /// crash, and the grants go on as owed.
    pub(super) async fn yield_replica_if_held_elsewhere(
        &self,
        owed: &PendingRegistration,
    ) -> Result<()> {
        if owed.replica_of.is_none() {
            return Ok(());
        }
        let owner = self
            .provider
            .get_doc_owner(&owed.policy_id, &owed.resource_name, &owed.doc_id)
            .await?
            .map(|owner| self.provider.canonical_actor(&owner));
        if owner.as_ref() == Some(&self.provider.canonical_actor(&owed.creator)) {
            return Ok(());
        }
        let (policy_id, resource, doc_id) = (&owed.policy_id, &owed.resource_name, &owed.doc_id);
        let Some(marker) = self.replica_marker(policy_id, resource, doc_id).await? else {
            return Ok(());
        };
        if marker.adopted {
            return Ok(());
        }
        let mut after = None;
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                break;
            };
            after = Some(last.seq);
            for change in &page {
                let tuple = &change.tuple;
                let of_unit = Some(&change.actor) == marker.registrant.as_ref()
                    && (&tuple.policy_id, &tuple.resource, &tuple.object)
                        == (policy_id, resource, doc_id)
                    && marker.grants.iter().any(|grant| {
                        grant.relation == tuple.relation && grant.subject == tuple.subject
                    });
                if of_unit {
                    self.write(change.seq, Write::Discard(YIELDED.to_owned()))
                        .await?;
                }
            }
        }
        tracing::info!(
            target: "acp::audit",
            doc_id = %doc_id,
            owner = owner.as_ref().map(Did::as_str).unwrap_or(""),
            "Replicated document held by another actor; waiting for its owner's grants to the creator"
        );
        self.replace_marker(
            policy_id,
            resource,
            doc_id,
            &marker,
            Some(ReplicaMarker {
                creator: marker.creator.clone(),
                registrant: None,
                adopted: true,
                grants: Vec::new(),
            }),
        )
        .await
    }

    /// Replace the committed marker `current` with `next`, or delete it,
    /// unless a transaction changed it meanwhile.
    async fn replace_marker(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        current: &ReplicaMarker,
        next: Option<ReplicaMarker>,
    ) -> Result<()> {
        let key = replica_key(policy_id, resource_name, doc_id);
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        let stored = ReplicaMarkers::new(&view)
            .get(policy_id, resource_name, doc_id)
            .await?;
        if stored.as_ref() != Some(current) {
            drop(view);
            return txn
                .discard()
                .map_err(|error| Error::Storage(error.to_string()));
        }
        let written = match &next {
            Some(next) => view.set(&key, &next.encode()?).await,
            None => view.delete(&key).await,
        };
        written.map_err(|error| Error::Storage(error.to_string()))?;
        drop(view);
        match txn.commit().await {
            Ok(()) => Ok(()),
            Err(datastore::Error::Storage(error)) if error.is_txn_conflict() => Ok(()),
            Err(error) => Err(Error::Storage(error.to_string())),
        }
    }

    pub(super) async fn settle_replica_unit(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let Some(marker) = self
            .replica_marker(policy_id, resource_name, doc_id)
            .await?
        else {
            return Ok(());
        };
        if marker.adopted {
            let creator = Identity::Authenticated(marker.creator.clone());
            if self
                .provider
                .check_doc_access(
                    &creator,
                    DocumentPermission::Update,
                    policy_id,
                    resource_name,
                    doc_id,
                )
                .await?
            {
                self.replace_marker(policy_id, resource_name, doc_id, &marker, None)
                    .await?;
            }
            return Ok(());
        }
        if let Some(owed) = self.owed(policy_id, resource_name, doc_id).await? {
            self.settle(&owed).await?;
        }
        {
            let _pass = self.settling.lock().await;
            let object = (
                policy_id.to_owned(),
                resource_name.to_owned(),
                doc_id.to_owned(),
            );
            let mut objects = RapidHashSet::with_capacity(1);
            objects.insert(object);
            let report = self.pass(Some(&objects)).await?;
            if report.provider_unavailable {
                return Err(Error::ProviderUnavailable(format!(
                    "the grants owed for replicated document {doc_id} stay staged"
                )));
            }
        }
        let Some(current) = self
            .replica_marker(policy_id, resource_name, doc_id)
            .await?
        else {
            return Ok(());
        };
        if current.adopted
            || self.owed(policy_id, resource_name, doc_id).await?.is_some()
            || self
                .unit_grants_owed(policy_id, resource_name, doc_id, &current)
                .await?
        {
            return Ok(());
        }
        self.replace_marker(policy_id, resource_name, doc_id, &current, None)
            .await
    }
}
