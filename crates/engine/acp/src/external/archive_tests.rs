//! A deleted document's archive, owed through the gate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use regolith::sync::SegQueue;
use storage::RegolithStore;

use super::*;
use crate::dac::DocumentACP;
use crate::identity::Identity;
use crate::outbox::{InFlight, OwedDenials, OwedOp, StageArchive, StageRelationship, StagedCount};
use crate::permission::DocumentPermission;
use crate::OwedWait;

const OWNER: &str = "did:key:owner";
const WRITER: &str = "did:key:writer";
const READER: &str = "did:key:reader";

/// A provider that names [`OWNER`] the owner of every object, allows every
/// read and records every write it takes.
#[derive(Default)]
struct Recording {
    writes: SegQueue<String>,
    unreachable: AtomicBool,
    /// Nothing lets the node sign for an actor.
    no_credential: AtomicBool,
    /// Every actor the node asked for authority to send a command as.
    asked: SegQueue<String>,
}

impl Recording {
    fn writes(&self) -> Vec<String> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }

    fn asked(&self) -> Vec<String> {
        std::iter::from_fn(|| self.asked.pop()).collect()
    }
}

#[async_trait]
impl DocumentACP for Recording {
    async fn register_doc_object(
        &self,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &[crate::DerivedRelationship],
    ) -> Result<()> {
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(true)
    }

    async fn get_doc_owner(&self, _: &str, _: &str, _: &str) -> Result<Option<Did>> {
        if self.unreachable.load(Ordering::Relaxed) {
            return Err(Error::ProviderUnavailable("no route".into()));
        }
        Ok(Some(did(OWNER)))
    }

    async fn check_doc_access(
        &self,
        _: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        Ok(true)
    }

    fn stages_archive_on_delete(&self) -> bool {
        true
    }

    fn command_authority(
        &self,
        actor: &Did,
        _: crate::ProviderCommand<'_>,
    ) -> Result<crate::outbox::OwedAuthority> {
        self.asked.push(actor.to_string());
        if self.no_credential.load(Ordering::Relaxed) {
            return Err(Error::OwedAuthorityUnavailable {
                actor: actor.to_string(),
                reason: "the request carries no delegation".into(),
            });
        }
        Ok(crate::outbox::OwedAuthority::NodeKey)
    }

    async fn archive_doc_object(&self, actor: &Did, _: &str, _: &str, doc: &str) -> Result<()> {
        self.writes.push(format!("archive {doc} as {actor}"));
        Ok(())
    }

    async fn add_actor_relationship(
        &self,
        _: &Did,
        target: &Did,
        _: &str,
        _: &str,
        doc: &str,
        relation: &str,
        _: &[String],
    ) -> Result<bool> {
        self.writes
            .push(format!("add {relation} {target} on {doc}"));
        Ok(true)
    }

    async fn delete_actor_relationship(
        &self,
        _: &Did,
        target: &Did,
        _: &str,
        _: &str,
        doc: &str,
        relation: &str,
        _: &[String],
    ) -> Result<bool> {
        self.writes
            .push(format!("delete {relation} {target} on {doc}"));
        Ok(true)
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
}

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

fn gate() -> (Arc<Recording>, ExternalDocumentAcp<RegolithStore>) {
    let provider = Arc::new(Recording::default());
    let gate = ExternalDocumentAcp::new(
        provider.clone(),
        Arc::new(RegolithStore::in_memory().unwrap()),
    );
    (provider, gate)
}

/// Stage in one transaction what `stage` asks of its outbox, and commit.
async fn in_transaction<F, Fut, T>(gate: &ExternalDocumentAcp<RegolithStore>, stage: F) -> Result<T>
where
    F: FnOnce(crate::outbox::TransactionOutbox) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let txn = gate.begin(false).await?;
    let view = txn.acpstore();
    let outbox = gate
        .transaction_outbox(&view, StagedCount::new())
        .expect("the gate keeps an outbox");
    let staged = stage(outbox).await;
    drop(view);
    match staged {
        Ok(value) => {
            txn.commit()
                .await
                .map_err(|error| Error::Storage(error.to_string()))?;
            Ok(value)
        }
        Err(error) => {
            txn.discard()
                .map_err(|error| Error::Storage(error.to_string()))?;
            Err(error)
        }
    }
}

fn archive(actor: &Did) -> StageArchive<'_> {
    StageArchive {
        txn: None,
        actor,
        policy_id: "policy",
        resource: "users",
        object: "bae-1",
    }
}

fn grant(actor: &Did) -> StageRelationship<'_> {
    StageRelationship {
        txn: None,
        actor,
        subject: zanzibar::types::Subject::Entity(did(READER)),
        policy_id: "policy",
        resource: "users",
        object: "bae-1",
        relation: "reader",
        managing_relations: &[],
        op: OwedOp::Add,
    }
}

async fn reads(gate: &ExternalDocumentAcp<RegolithStore>, who: Identity) -> bool {
    gate.check_doc_access(&who, DocumentPermission::Read, "policy", "users", "bae-1")
        .await
        .unwrap()
}

#[test]
fn a_deleted_documents_archive_denies_everyone_until_it_settles() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        assert!(reads(&gate, Identity::Authenticated(did(READER))).await);
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(owner)).await
        })
        .await
        .unwrap();
        for who in [
            Identity::Authenticated(did(READER)),
            Identity::Authenticated(did(OWNER)),
            Identity::Anonymous,
        ] {
            assert!(
                !reads(&gate, who).await,
                "a deleted document grants nothing"
            );
        }

        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(provider.writes(), vec![format!("archive bae-1 as {OWNER}")]);
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_permitted_non_owners_delete_is_archived_as_the_owner_under_the_owners_authority() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let writer = &did(WRITER);
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(writer)).await
        })
        .await
        .expect("a permitted non-owner's delete commits");
        let owed = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!((owed[0].op, &owed[0].actor), (OwedOp::Archive, &did(OWNER)));
        assert_eq!(owed[0].state, crate::outbox::OwedState::Staged);
        assert_eq!(provider.asked(), vec![OWNER.to_owned()]);
        for who in [
            Identity::Authenticated(did(OWNER)),
            Identity::Authenticated(did(WRITER)),
            Identity::Authenticated(did(READER)),
            Identity::Anonymous,
        ] {
            assert!(
                !reads(&gate, who).await,
                "a deleted document grants nothing"
            );
        }

        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(provider.writes(), vec![format!("archive bae-1 as {OWNER}")]);
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(
            provider.asked().iter().all(|actor| actor == OWNER),
            "nothing is sent under the writer's authority"
        );
    })
    .unwrap()
}

#[test]
fn a_non_owners_archive_waits_for_the_owners_authority_and_settles_once_it_is_handed_in() {
    workers::block_on(async {
        let (provider, gate) = gate();
        provider.no_credential.store(true, Ordering::Relaxed);
        let writer = &did(WRITER);
        let staged = in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(writer)).await
        })
        .await
        .expect("a permitted non-owner's delete commits");
        let owed = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(owed.len(), 1);
        assert_eq!((owed[0].op, &owed[0].actor), (OwedOp::Archive, &did(OWNER)));
        assert_eq!(owed[0].state, crate::outbox::OwedState::AwaitingAuthority);
        for who in [
            Identity::Authenticated(did(OWNER)),
            Identity::Authenticated(did(WRITER)),
            Identity::Authenticated(did(READER)),
        ] {
            assert!(!reads(&gate, who).await, "a parked archive denies too");
        }
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert!(
            provider.writes().is_empty(),
            "nothing is sent without authority"
        );

        provider.no_credential.store(false, Ordering::Relaxed);
        let entry = crate::outbox::OwedRef::Change { seq: staged.seq };
        assert!(
            matches!(
                gate.reauthorize_owed(writer, &entry).await,
                Err(Error::OwedEntryNotFound { .. })
            ),
            "the writer owes nothing: the archive is the owner's"
        );
        gate.reauthorize_owed(&did(OWNER), &entry)
            .await
            .expect("the owner hands in its authority");
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(provider.writes(), vec![format!("archive bae-1 as {OWNER}")]);
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(provider.asked().iter().all(|actor| actor == OWNER));
    })
    .unwrap()
}

#[test]
fn a_non_owners_deletes_leave_the_owners_admission_untouched() {
    workers::block_on(async {
        let (provider, gate) = gate();
        provider.no_credential.store(true, Ordering::Relaxed);
        let writer = &did(WRITER);
        let limit = crate::outbox::MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR;
        let objects: Vec<String> = (0..limit).map(|n| format!("bae-gone-{n}")).collect();
        for object in &objects {
            in_transaction(&gate, |outbox| async move {
                outbox
                    .stage_archive(StageArchive {
                        object,
                        ..archive(writer)
                    })
                    .await
            })
            .await
            .expect("a permitted non-owner's delete commits");
        }
        let parked = gate.owed_changes_after(None).await.unwrap();
        assert!(parked.iter().all(|change| change.op == OwedOp::Archive
            && change.actor == did(OWNER)
            && change.state == crate::outbox::OwedState::AwaitingAuthority));
        provider.no_credential.store(false, Ordering::Relaxed);
        let owner = &did(OWNER);
        in_transaction(
            &gate,
            |outbox| async move { outbox.stage(grant(owner)).await },
        )
        .await
        .expect("the writer's deletes count against nobody's admission");
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(owner)).await
        })
        .await
        .expect("the owner's own delete is not refused either");
    })
    .unwrap()
}

#[test]
fn a_node_full_of_parked_archives_still_admits_new_changes() {
    workers::block_on(async {
        let (provider, gate) = gate();
        provider.no_credential.store(true, Ordering::Relaxed);
        let writer = &did(WRITER);
        let limit = crate::outbox::MAX_UNSETTLED_OWED_CHANGES;
        for n in 0..limit {
            let object = &format!("bae-gone-{n}");
            in_transaction(&gate, |outbox| async move {
                outbox
                    .stage_archive(StageArchive {
                        object,
                        ..archive(writer)
                    })
                    .await
            })
            .await
            .expect("a permitted non-owner's delete commits");
        }
        let txn = gate.begin(true).await.unwrap();
        let view = txn.acpstore();
        let changes = gate.owed_changes(&view, StagedCount::new());
        assert_eq!(
            changes.unsettled().await.unwrap(),
            0,
            "parked archives are not in the node's admission count"
        );
        drop(view);
        txn.discard().unwrap();
        provider.no_credential.store(false, Ordering::Relaxed);
        let owner = &did(OWNER);
        in_transaction(
            &gate,
            |outbox| async move { outbox.stage(grant(owner)).await },
        )
        .await
        .expect("the node's bound does not count the writer's archives");
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(owner)).await
        })
        .await
        .expect("the owner's own delete is not refused either");
    })
    .unwrap()
}

#[test]
fn a_transactions_deletes_leave_its_own_staging_bound_untouched() {
    workers::block_on(async {
        let (_provider, gate) = gate();
        let writer = &did(WRITER);
        let owner = &did(OWNER);
        let limit = crate::outbox::MAX_OWED_CHANGES_PER_TRANSACTION;
        in_transaction(&gate, |outbox| async move {
            for n in 0..limit {
                let object = &format!("bae-gone-{n}");
                outbox
                    .stage_archive(StageArchive {
                        object,
                        ..archive(writer)
                    })
                    .await?;
            }
            assert_eq!(outbox.staged(), 0, "archives are not counted as staged");
            for n in 0..limit {
                let object = &format!("bae-kept-{n}");
                outbox
                    .stage(StageRelationship {
                        object,
                        ..grant(owner)
                    })
                    .await?;
            }
            assert!(matches!(
                outbox.stage(grant(owner)).await,
                Err(Error::OwedChangesPerTransaction { limit: bound }) if bound == limit
            ));
            Ok(())
        })
        .await
        .expect("a transaction's deletes leave it its whole bound of changes");
    })
    .unwrap()
}

#[test]
fn a_delete_is_refused_and_nothing_is_staged_while_the_owner_cannot_be_learned() {
    workers::block_on(async {
        let (provider, gate) = gate();
        provider.unreachable.store(true, Ordering::Relaxed);
        for deleter in [did(OWNER), did(WRITER)] {
            let deleter = &deleter;
            let unknown = in_transaction(&gate, |outbox| async move {
                outbox.stage_archive(archive(deleter)).await
            })
            .await;
            assert!(
                matches!(unknown, Err(Error::OwedCheckUnavailable { .. })),
                "{unknown:?}"
            );
            assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        }
    })
    .unwrap()
}

#[test]
fn earlier_unsent_changes_go_with_the_delete_and_one_in_flight_is_kept() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        let first = in_transaction(
            &gate,
            |outbox| async move { outbox.stage(grant(owner)).await },
        )
        .await
        .unwrap();
        // The first grant's request was handed to the worker; a second waits.
        let flight = InFlight {
            digest: "0xdigest".into(),
            tx: Some("0xtx".into()),
            floor: Some(0),
        };
        assert!(gate
            .write(
                first.seq,
                settle::Write::MarkInFlight {
                    others: Vec::new(),
                    flight
                }
            )
            .await
            .unwrap());
        let mut second = grant(owner);
        second.subject = zanzibar::types::Subject::Wildcard;
        let second = in_transaction(&gate, |outbox| async move { outbox.stage(second).await })
            .await
            .unwrap();

        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(owner)).await
        })
        .await
        .unwrap();
        let owed: Vec<_> = gate
            .owed_changes_after(None)
            .await
            .unwrap()
            .into_iter()
            .map(|change| (change.seq, change.op))
            .collect();
        assert_eq!(
            owed[0],
            (first.seq, OwedOp::Add),
            "the change in flight stays"
        );
        assert_eq!(owed[1].1, OwedOp::Archive);
        assert_eq!(owed.len(), 2, "the unsent grant went with the delete");
        assert!(!owed.iter().any(|(seq, _)| *seq == second.seq));
        assert!(provider.writes().is_empty());
        // The delete's commit told the drainer.
        workers::timeout(
            std::time::Duration::from_secs(5),
            gate.staged_changes_committed(),
        )
        .await
        .expect("the drainer is woken");
    })
    .unwrap()
}

#[test]
fn an_owners_archive_the_node_cannot_sign_yet_waits_for_authority_and_denies() {
    workers::block_on(async {
        let (provider, gate) = gate();
        provider.no_credential.store(true, Ordering::Relaxed);
        let owner = &did(OWNER);
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(owner)).await
        })
        .await
        .expect("the owner's delete commits");
        let owed = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].op, OwedOp::Archive);
        assert_eq!(owed[0].state, crate::outbox::OwedState::AwaitingAuthority);
        assert!(!reads(&gate, Identity::Authenticated(did(READER))).await);
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert!(
            provider.writes().is_empty(),
            "nothing is sent without authority"
        );
    })
    .unwrap()
}

#[test]
fn an_archive_named_by_a_transaction_is_scoped_to_it() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        let txn_id = 7_u64;
        let mut archive = archive(owner);
        archive.txn = Some(txn_id);
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive).await
        })
        .await
        .expect("the owner's delete commits");
        let committed = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(committed.len(), 1, "{committed:?}");
        assert_eq!(committed[0].txn, Some(txn_id), "{committed:?}");
        assert_eq!(
            committed[0].epoch,
            Some(crate::outbox::record::run_epoch()),
            "{committed:?}"
        );
        let wait = gate.await_owed(txn_id, None).await.unwrap();
        assert_eq!(wait, OwedWait::Settled);
        assert!(!provider.writes().is_empty());
    })
    .unwrap()
}

#[test]
fn a_parked_archive_names_its_owner_for_history_visibility() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        let txn = gate.begin(false).await.unwrap();
        let view = txn.acpstore();
        let outbox = gate
            .transaction_outbox(&view, StagedCount::new())
            .expect("the gate keeps an outbox");
        outbox.stage_archive(archive(owner)).await.unwrap();
        let denials = OwedDenials::new(&view);
        assert_eq!(
            denials
                .archive_actor("policy", "users", "bae-1")
                .await
                .unwrap(),
            Some(did(OWNER))
        );
        drop(view);
        drop(outbox);
        txn.commit().await.unwrap();
        let (provider, gate) = (provider, gate);
        assert_eq!(provider.writes(), Vec::<String>::new());
        assert!(!gate.owed_changes_after(None).await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn an_archive_for_someone_elses_delete_names_nobody_for_history_visibility() {
    workers::block_on(async {
        let (_provider, gate) = gate();
        let writer = &did(WRITER);
        let txn = gate.begin(false).await.unwrap();
        let view = txn.acpstore();
        let outbox = gate
            .transaction_outbox(&view, StagedCount::new())
            .expect("the gate keeps an outbox");
        outbox.stage_archive(archive(writer)).await.unwrap();
        assert_eq!(
            OwedDenials::new(&view)
                .archive_actor("policy", "users", "bae-1")
                .await
                .unwrap(),
            None,
            "neither the writer nor the owner keeps the history"
        );
        drop(view);
        drop(outbox);
        txn.discard().unwrap();
    })
    .unwrap()
}

#[test]
fn a_registration_still_owed_grants_its_creator_nothing_once_the_document_is_deleted() {
    workers::block_on(async {
        let (_provider, gate) = gate();
        let owner = did(OWNER);
        let txn = gate.begin(false).await.unwrap();
        let view = txn.acpstore();
        crate::PendingRegistrations::new(&view)
            .record(&crate::PendingRegistration::new(
                owner.clone(),
                "policy",
                "users",
                "bae-1",
            ))
            .await
            .unwrap();
        drop(view);
        txn.commit().await.unwrap();
        assert!(reads(&gate, Identity::Authenticated(owner.clone())).await);
        let deleter = &owner;
        in_transaction(&gate, |outbox| async move {
            outbox.stage_archive(archive(deleter)).await
        })
        .await
        .unwrap();
        assert!(
            !reads(&gate, Identity::Authenticated(owner.clone())).await,
            "the owed archive denies the creator its owed registration named"
        );
    })
    .unwrap()
}

#[test]
fn a_pass_for_one_actor_takes_a_bounded_number_of_its_objects() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        // Archives count against no bound on staging, so an owner whose
        // documents were deleted may owe more objects than a scoped pass
        // takes.
        let limit = MAX_SCOPED_PASS_OBJECTS;
        for n in 0..=limit {
            let object = &format!("bae-gone-{n}");
            in_transaction(&gate, |outbox| async move {
                outbox
                    .stage_archive(StageArchive {
                        object,
                        ..archive(owner)
                    })
                    .await
            })
            .await
            .expect("a delete commits");
        }
        let pass = gate
            .settle_owed_changes_once(OwedScope::Actor(owner))
            .await
            .unwrap();
        assert_eq!(pass.settled, limit as u64);
        let left = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].tuple.object, format!("bae-gone-{limit}"));
        let rest = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(rest.settled, 1);
        assert_eq!(provider.writes().len(), limit + 1);
    })
    .unwrap()
}
