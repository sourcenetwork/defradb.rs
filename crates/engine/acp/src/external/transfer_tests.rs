//! A document's ownership transfer, owed through the gate
//! (`proofs/tla/OwnerTransfer.tla`).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use regolith::sync::SegQueue;
use storage::RegolithStore;

use super::settle::Write;
use super::*;
use crate::dac::DocumentACP;
use crate::identity::Identity;
use crate::outbox::{OwedAuthority, OwedOp, StageRelationship, StageTransfer, StagedCount};
use crate::permission::DocumentPermission;

const OWNER: &str = "did:key:owner";
const NEW_OWNER: &str = "did:key:new-owner";
const READER: &str = "did:key:reader";
const MANAGER: &str = "did:key:manager";

/// A provider that holds one object: owned by [`OWNER`] until a transfer
/// lands, by [`NEW_OWNER`] after. It takes a change only from the owner of
/// the moment, lets only the owner read, and records every write it takes.
#[derive(Default)]
struct Owning {
    transferred: AtomicBool,
    writes: SegQueue<String>,
    unreachable: AtomicBool,
    /// A grant waits for a fresh credential.
    lapsed: AtomicBool,
    /// The provider refuses every transfer.
    refuse_transfers: AtomicBool,
    /// The next transfer takes effect and its answer is lost.
    lose_answer: AtomicBool,
    /// Transfers that took effect.
    transfers: AtomicU32,
    /// Refusals the gate told of: whether each was kept.
    told: SegQueue<(u64, bool)>,
}

impl Owning {
    fn owner(&self) -> Did {
        did(if self.transferred.load(Ordering::SeqCst) {
            NEW_OWNER
        } else {
            OWNER
        })
    }

    fn writes(&self) -> Vec<String> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }

    fn reachable(&self) -> Result<()> {
        if self.unreachable.load(Ordering::SeqCst) {
            return Err(Error::ProviderUnavailable("no route".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl DocumentACP for Owning {
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
        self.reachable()?;
        Ok(Some(self.owner()))
    }

    async fn check_doc_access(
        &self,
        who: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        self.reachable()?;
        Ok(matches!(who, Identity::Authenticated(actor) if *actor == self.owner()))
    }

    async fn check_named_permission(
        &self,
        subject: &Did,
        permission: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        self.reachable()?;
        Ok(*subject == did(MANAGER) && permission == "manager")
    }

    fn command_authority(&self, _: &Did, _: crate::ProviderCommand<'_>) -> Result<OwedAuthority> {
        Ok(OwedAuthority::NodeKey)
    }

    async fn transfer_doc_object(
        &self,
        actor: &Did,
        _: &str,
        _: &str,
        doc: &str,
        new_owner: &Did,
    ) -> Result<()> {
        self.reachable()?;
        if self.refuse_transfers.load(Ordering::SeqCst) || *actor != self.owner() {
            return Err(Error::NotOwner {
                operation: "transfer".into(),
            });
        }
        self.transferred.store(true, Ordering::SeqCst);
        self.transfers.fetch_add(1, Ordering::SeqCst);
        self.writes.push(format!("transfer {doc} to {new_owner}"));
        if self.lose_answer.swap(false, Ordering::SeqCst) {
            return Err(Error::ProviderUnavailable("the answer was lost".into()));
        }
        Ok(())
    }

    async fn add_actor_relationship(
        &self,
        actor: &Did,
        target: &Did,
        _: &str,
        _: &str,
        doc: &str,
        relation: &str,
        _: &[String],
    ) -> Result<bool> {
        self.reachable()?;
        if self.lapsed.load(Ordering::SeqCst) {
            return Err(Error::OwedAuthorityUnavailable {
                actor: actor.to_string(),
                reason: "the delegation expired".into(),
            });
        }
        if *actor != self.owner() {
            return Err(Error::NotOwner {
                operation: "add".into(),
            });
        }
        self.writes
            .push(format!("add {relation} {target} on {doc} as {actor}"));
        Ok(true)
    }

    async fn delete_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<bool> {
        Ok(true)
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
}

/// Takes what the gate tells of refused changes.
struct Told(Arc<Owning>);

impl crate::OwedRefusalListener for Told {
    fn refused(&self, change: &crate::outbox::OwedChange, _: &str, kept: bool) {
        self.0.told.push((change.seq, kept));
    }
}

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

fn gate() -> (Arc<Owning>, ExternalDocumentAcp<RegolithStore>) {
    let provider = Arc::new(Owning::default());
    let gate = ExternalDocumentAcp::new(
        provider.clone(),
        Arc::new(RegolithStore::in_memory().unwrap()),
    );
    gate.watch_owed_refusals(Arc::new(Told(provider.clone())));
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

fn grant<'a>(actor: &'a Did, to: &str) -> StageRelationship<'a> {
    StageRelationship {
        txn: Some(7),
        actor,
        subject: zanzibar::types::Subject::Entity(did(to)),
        policy_id: "policy",
        resource: "users",
        object: "bae-1",
        relation: "reader",
        managing_relations: &[],
        op: OwedOp::Add,
    }
}

fn managed_grant<'a>(
    actor: &'a Did,
    to: &str,
    managing_relations: &'a [String],
) -> StageRelationship<'a> {
    StageRelationship {
        managing_relations,
        ..grant(actor, to)
    }
}

fn transfer<'a>(actor: &'a Did, new_owner: &'a Did) -> StageTransfer<'a> {
    StageTransfer {
        txn: Some(7),
        actor,
        new_owner,
        policy_id: "policy",
        resource: "users",
        object: "bae-1",
    }
}

async fn reads(gate: &ExternalDocumentAcp<RegolithStore>, who: &str) -> bool {
    gate.check_doc_access(
        &Identity::Authenticated(did(who)),
        DocumentPermission::Read,
        "policy",
        "users",
        "bae-1",
    )
    .await
    .unwrap()
}

/// The creator grants and transfers in one transaction, and a later grant
/// follows: the transfer waits while the grant waits for authority, and the
/// later grant waits for the transfer. Each lands once, in that order, under
/// the owner of its moment.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_waits_for_its_objects_earlier_changes_and_later_ones_wait_for_it() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        let (grant_seq, transfer_seq) = in_transaction(&gate, |outbox| async move {
            let granted = outbox.stage(grant(owner, READER)).await?;
            let moved = outbox.stage_transfer(transfer(owner, new_owner)).await?;
            Ok((granted.seq, moved.expect("a transfer").seq))
        })
        .await
        .unwrap();
        // The new owner's grant, staged while the provider cannot say who
        // owns the object, goes unchecked behind the transfer.
        provider.unreachable.store(true, Ordering::SeqCst);
        in_transaction(&gate, |outbox| async move {
            outbox.stage(grant(new_owner, "did:key:later")).await
        })
        .await
        .unwrap();
        provider.unreachable.store(false, Ordering::SeqCst);

        provider.lapsed.store(true, Ordering::SeqCst);
        let first = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(first.awaiting_authority, 1);
        assert!(provider.writes().is_empty(), "nothing passes the grant");

        provider.lapsed.store(false, Ordering::SeqCst);
        assert!(gate
            .write(grant_seq, Write::Reauthorize(OwedAuthority::NodeKey))
            .await
            .unwrap());
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(
            provider.writes(),
            vec![
                format!("add reader {READER} on bae-1 as {OWNER}"),
                format!("transfer bae-1 to {NEW_OWNER}"),
                format!("add reader did:key:later on bae-1 as {NEW_OWNER}"),
            ]
        );
        assert_eq!(provider.transfers.load(Ordering::SeqCst), 1);
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(transfer_seq > grant_seq);
    })
    .unwrap()
}

/// From the transfer's commit the old owner is denied on the object and
/// stages nothing more on it, and the new owner is let in only once the
/// provider names it.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn an_owed_transfer_denies_its_old_owner_and_grants_its_new_owner_nothing() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        assert!(reads(&gate, OWNER).await);
        let refused = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, new_owner)).await?;
            outbox.stage(grant(owner, READER)).await
        })
        .await;
        assert!(
            matches!(refused, Err(Error::PermissionDenied(_))),
            "{refused:?}"
        );
        in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, new_owner)).await
        })
        .await
        .unwrap();
        provider.unreachable.store(true, Ordering::SeqCst);
        assert!(
            !reads(&gate, OWNER).await,
            "the old owner is denied before the provider is asked"
        );
        provider.unreachable.store(false, Ordering::SeqCst);
        assert!(!reads(&gate, OWNER).await);
        assert!(!reads(&gate, NEW_OWNER).await);
        let again = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, &did(READER))).await
        })
        .await;
        assert!(
            matches!(again, Err(Error::PermissionDenied(_))),
            "{again:?}"
        );

        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert!(reads(&gate, NEW_OWNER).await);
        assert!(!reads(&gate, OWNER).await);
    })
    .unwrap()
}

/// Only the owner may transfer, a transfer is never staged unchecked, and
/// giving an object to its owner or to everyone is no transfer.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn only_the_owner_stages_a_transfer_and_only_to_one_other_actor() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, reader) = (&did(OWNER), &did(READER));
        let stranger = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(reader, owner)).await
        })
        .await;
        assert!(
            matches!(stranger, Err(Error::NotOwner { .. })),
            "{stranger:?}"
        );

        provider.unreachable.store(true, Ordering::SeqCst);
        let unchecked = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, reader)).await
        })
        .await;
        assert!(
            matches!(unchecked, Err(Error::OwedCheckUnavailable { .. })),
            "{unchecked:?}"
        );
        provider.unreachable.store(false, Ordering::SeqCst);

        let to_itself = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, owner)).await
        })
        .await;
        assert!(matches!(to_itself, Ok(None)), "{to_itself:?}");
        let everyone = Did::wildcard();
        let to_everyone = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, &everyone)).await
        })
        .await;
        assert!(
            matches!(to_everyone, Err(Error::UnsupportedSubject(_))),
            "{to_everyone:?}"
        );
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(reads(&gate, OWNER).await);
    })
    .unwrap()
}

/// A transfer the provider refuses leaves the object its creator's on the
/// provider, is kept as refused and told of, keeps denying its actor, and
/// stops denying once its actor abandons it.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_refused_transfer_is_kept_and_told_and_denies_until_its_actor_abandons_it() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        let seq = in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, new_owner)).await
        })
        .await
        .unwrap()
        .expect("a transfer")
        .seq;
        provider.refuse_transfers.store(true, Ordering::SeqCst);
        let pass = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(pass.refused, 1);
        assert_eq!(provider.owner(), did(OWNER));
        assert_eq!(provider.told.pop(), Some((seq, true)));
        let kept = gate.owed_changes_after(None).await.unwrap();
        assert!(
            matches!(kept[..], [ref change] if change.op == OwedOp::Transfer
                && matches!(change.state, crate::outbox::OwedState::Refused { .. })),
            "{kept:?}"
        );
        assert!(!reads(&gate, OWNER).await);

        gate.abandon_owed_change(owner, seq).await.unwrap();
        assert!(reads(&gate, OWNER).await);
        assert!(!reads(&gate, NEW_OWNER).await);
    })
    .unwrap()
}

/// A transfer that took effect while its answer was lost is sent again; the
/// provider refuses the second one, its actor owning nothing any more, and
/// the node settles it because the provider names the new owner: it took
/// effect once and is never reported refused.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_whose_answer_was_lost_is_settled_and_not_reported_refused() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, new_owner)).await
        })
        .await
        .unwrap();
        provider.lose_answer.store(true, Ordering::SeqCst);
        let lost = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert!(lost.provider_unavailable);
        assert_eq!(gate.owed_changes_after(None).await.unwrap().len(), 1);

        let again = gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!((again.settled, again.refused), (1, 0));
        assert_eq!(provider.transfers.load(Ordering::SeqCst), 1);
        assert!(provider.told.pop().is_none());
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(reads(&gate, NEW_OWNER).await);
    })
    .unwrap()
}

/// Two transactions open at once on one object, `first` staged before
/// `second`; both are committed, `second` first. What each commit came to.
async fn concurrently<F1, Fut1, F2, Fut2>(
    gate: &ExternalDocumentAcp<RegolithStore>,
    first: F1,
    second: F2,
) -> (bool, bool)
where
    F1: FnOnce(crate::outbox::TransactionOutbox) -> Fut1,
    Fut1: std::future::Future<Output = ()>,
    F2: FnOnce(crate::outbox::TransactionOutbox) -> Fut2,
    Fut2: std::future::Future<Output = ()>,
{
    let t1 = gate.begin(false).await.unwrap();
    let v1 = t1.acpstore();
    first(gate.transaction_outbox(&v1, StagedCount::new()).unwrap()).await;
    drop(v1);
    let t2 = gate.begin(false).await.unwrap();
    let v2 = t2.acpstore();
    second(gate.transaction_outbox(&v2, StagedCount::new()).unwrap()).await;
    drop(v2);
    let second_ok = t2.commit().await.is_ok();
    let first_ok = t1.commit().await.is_ok();
    (first_ok, second_ok)
}

/// The owner's grant commits before a concurrent transfer that took the
/// earlier place in the queue: the transfer's commit is aborted, and staged
/// again it goes behind the grant, which reaches the provider as the owner's.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_staged_beside_a_grant_that_commits_first_is_aborted() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        let (transfer_ok, grant_ok) = concurrently(
            &gate,
            |outbox| async move {
                outbox
                    .stage_transfer(transfer(owner, new_owner))
                    .await
                    .unwrap();
            },
            |outbox| async move {
                outbox.stage(grant(owner, READER)).await.unwrap();
            },
        )
        .await;
        assert!(grant_ok, "the grant commits");
        assert!(
            !transfer_ok,
            "the transfer that committed second is aborted"
        );
        in_transaction(&gate, |outbox| async move {
            outbox.stage_transfer(transfer(owner, new_owner)).await
        })
        .await
        .unwrap();
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(
            provider.writes(),
            vec![
                format!("add reader {READER} on bae-1 as {OWNER}"),
                format!("transfer bae-1 to {NEW_OWNER}"),
            ]
        );
        assert!(provider.told.pop().is_none());
    })
    .unwrap()
}

/// A settle of an earlier change on the object changes no order around a
/// transfer staged beside it, so the transfer still commits.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_settle_beside_a_staged_transfer_does_not_abort_it() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        in_transaction(&gate, |outbox| async move {
            outbox.stage(grant(owner, READER)).await
        })
        .await
        .unwrap();

        let txn = gate.begin(false).await.unwrap();
        let view = txn.acpstore();
        let outbox = gate.transaction_outbox(&view, StagedCount::new()).unwrap();
        outbox
            .stage_transfer(transfer(owner, new_owner))
            .await
            .unwrap();
        drop(outbox);
        drop(view);

        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert!(txn.commit().await.is_ok(), "the settle aborts nothing");
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(
            provider.writes(),
            vec![
                format!("add reader {READER} on bae-1 as {OWNER}"),
                format!("transfer bae-1 to {NEW_OWNER}"),
            ]
        );
    })
    .unwrap();
}

/// A transfer commits before the owner's concurrent grant that took the
/// earlier place in the queue: the grant's commit is aborted, and staged
/// again the owner is refused, its transfer owed.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_grant_staged_beside_a_transfer_that_commits_first_is_aborted() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, new_owner) = (&did(OWNER), &did(NEW_OWNER));
        let (grant_ok, transfer_ok) = concurrently(
            &gate,
            |outbox| async move {
                outbox.stage(grant(owner, READER)).await.unwrap();
            },
            |outbox| async move {
                outbox
                    .stage_transfer(transfer(owner, new_owner))
                    .await
                    .unwrap();
            },
        )
        .await;
        assert!(transfer_ok, "the transfer commits");
        assert!(!grant_ok, "the grant that committed second is aborted");
        let again = in_transaction(&gate, |outbox| async move {
            outbox.stage(grant(owner, READER)).await
        })
        .await;
        assert!(
            matches!(again, Err(Error::PermissionDenied(_))),
            "{again:?}"
        );
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(
            provider.writes(),
            vec![format!("transfer bae-1 to {NEW_OWNER}")]
        );
    })
    .unwrap()
}

/// A manager's grant commits before a concurrent transfer that took the
/// earlier place in the queue: the grant's commit is aborted on the
/// transfer key alone, since the transfer denies only its old owner.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_managers_grant_staged_beside_a_transfer_that_commits_first_is_aborted() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let (owner, manager, new_owner) = (&did(OWNER), &did(MANAGER), &did(NEW_OWNER));
        let managing = vec!["manager".to_string()];
        let (grant_ok, transfer_ok) = concurrently(
            &gate,
            |outbox| async move {
                outbox
                    .stage(managed_grant(manager, READER, &managing))
                    .await
                    .unwrap();
            },
            |outbox| async move {
                outbox
                    .stage_transfer(transfer(owner, new_owner))
                    .await
                    .unwrap();
            },
        )
        .await;
        assert!(transfer_ok, "the transfer commits");
        assert!(
            !grant_ok,
            "the manager's grant that commits second is aborted"
        );
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(
            provider.writes(),
            vec![format!("transfer bae-1 to {NEW_OWNER}")]
        );
    })
    .unwrap();
}

/// Changes that are not transfers still stage on one object at once and
/// both commit: only a transfer orders its object by commit.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn concurrent_grants_on_one_object_do_not_conflict() {
    workers::block_on(async {
        let (provider, gate) = gate();
        let owner = &did(OWNER);
        let (first, second) = concurrently(
            &gate,
            |outbox| async move {
                outbox.stage(grant(owner, READER)).await.unwrap();
            },
            |outbox| async move {
                outbox.stage(grant(owner, "did:key:later")).await.unwrap();
            },
        )
        .await;
        assert!(first && second);
        gate.settle_owed_changes_once(OwedScope::All).await.unwrap();
        assert_eq!(provider.writes().len(), 2);
    })
    .unwrap()
}
