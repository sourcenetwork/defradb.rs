//! What a term voided, owed again through the gate.

use std::sync::Arc;

use async_trait::async_trait;
use regolith::sync::SegQueue;
use storage::RegolithStore;

use super::*;
use crate::dac::DocumentACP;
use crate::identity::Identity;
use crate::outbox::{OwedOp, OwedSubject, OwedTuple};
use crate::permission::DocumentPermission;

const OWNER: &str = "did:key:owner";
const READER: &str = "did:key:reader";

/// A provider that allows every read and records every write it takes. It
/// holds a registration once it was sent one.
#[derive(Default)]
struct Recording {
    writes: SegQueue<String>,
    registered: std::sync::atomic::AtomicBool,
}

impl Recording {
    fn writes(&self) -> Vec<String> {
        std::iter::from_fn(|| self.writes.pop()).collect()
    }
}

#[async_trait]
impl DocumentACP for Recording {
    async fn register_doc_object(
        &self,
        actor: &Did,
        _: &str,
        _: &str,
        doc: &str,
        _: &[crate::DerivedRelationship],
    ) -> Result<()> {
        self.writes.push(format!("register {doc} as {actor}"));
        self.registered
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(self.registered.load(std::sync::atomic::Ordering::Relaxed))
    }

    async fn get_doc_owner(&self, _: &str, _: &str, _: &str) -> Result<Option<Did>> {
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

fn change(doc: &str, op: OwedOp) -> VoidedEffect {
    VoidedEffect::Change(NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: "policy".into(),
            resource: "users".into(),
            object: doc.into(),
            relation: "reader".into(),
            subject: OwedSubject::Actor { did: did(READER) },
        },
        op,
        actor: did(OWNER),
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    })
}

fn registration(doc: &str) -> VoidedEffect {
    VoidedEffect::Registration {
        relationships: Vec::new(),
        creator: did(OWNER),
        policy_id: "policy".into(),
        resource: "users".into(),
        object: doc.into(),
        authority: OwedAuthority::NodeKey,
        request_bound: false,
    }
}

fn gate() -> (Arc<Recording>, ExternalDocumentAcp<RegolithStore>) {
    let provider = Arc::new(Recording::default());
    let gate = ExternalDocumentAcp::new(
        provider.clone(),
        Arc::new(RegolithStore::in_memory().unwrap()),
    );
    (provider, gate)
}

async fn reads(gate: &ExternalDocumentAcp<RegolithStore>, who: &str, doc: &str) -> bool {
    gate.check_doc_access(
        &Identity::Authenticated(did(who)),
        DocumentPermission::Read,
        "policy",
        "users",
        doc,
    )
    .await
    .unwrap()
}

#[test]
fn a_lost_registration_and_grant_are_owed_again_and_sent_registration_first() {
    workers::block_on(async {
        let (provider, gate) = gate();
        gate.owe_again(&[registration("bae-1"), change("bae-1", OwedOp::Add)])
            .await
            .unwrap();
        // Owed again, the document is readable by its creator only, and the
        // owed grant grants nothing yet.
        assert!(reads(&gate, OWNER, "bae-1").await);
        assert!(!reads(&gate, READER, "bae-1").await);

        gate.settle_owed().await;
        assert_eq!(
            provider.writes(),
            vec![
                format!("register bae-1 as {OWNER}"),
                format!("add reader {READER} on bae-1"),
            ]
        );
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
        assert!(reads(&gate, READER, "bae-1").await);
    })
    .unwrap()
}

#[test]
fn a_lost_revocation_denies_until_it_is_settled_again() {
    workers::block_on(async {
        let (provider, gate) = gate();
        assert!(reads(&gate, READER, "bae-2").await, "the provider allows");
        gate.owe_again(&[change("bae-2", OwedOp::Delete)])
            .await
            .unwrap();
        assert!(
            !reads(&gate, READER, "bae-2").await,
            "the revocation the term voided denies on this node"
        );
        assert!(reads(&gate, OWNER, "bae-2").await, "the owner is spared");

        gate.settle_owed().await;
        assert_eq!(
            provider.writes(),
            vec![format!("delete reader {READER} on bae-2")]
        );
        assert!(gate.owed_changes_after(None).await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_change_whose_tuple_already_owes_a_later_one_is_not_owed_again() {
    workers::block_on(async {
        let (provider, gate) = gate();
        // A later revocation of the tuple is owed already; the grant a term
        // voided is older and must not be sent after it.
        gate.owe_again(&[change("bae-3", OwedOp::Delete)])
            .await
            .unwrap();
        gate.owe_again(&[change("bae-3", OwedOp::Add)])
            .await
            .unwrap();
        let owed = gate.owed_changes_after(None).await.unwrap();
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].op, OwedOp::Delete);
        gate.settle_owed().await;
        assert_eq!(
            provider.writes(),
            vec![format!("delete reader {READER} on bae-3")]
        );
    })
    .unwrap()
}

#[test]
fn successive_changes_of_one_tuple_handed_back_together_are_all_owed_in_order() {
    workers::block_on(async {
        let (provider, gate) = gate();
        gate.owe_again(&[
            change("bae-4", OwedOp::Add),
            change("bae-4", OwedOp::Delete),
        ])
        .await
        .unwrap();
        let owed: Vec<_> = gate
            .owed_changes_after(None)
            .await
            .unwrap()
            .into_iter()
            .map(|change| change.op)
            .collect();
        assert_eq!(owed, vec![OwedOp::Add, OwedOp::Delete]);
        assert!(
            !reads(&gate, READER, "bae-4").await,
            "the revocation handed back last denies"
        );
        gate.settle_owed().await;
        assert_eq!(
            provider.writes(),
            vec![
                format!("add reader {READER} on bae-4"),
                format!("delete reader {READER} on bae-4"),
            ]
        );
    })
    .unwrap()
}

#[test]
fn what_a_term_voided_is_owed_again_past_the_bounds_on_new_changes() {
    workers::block_on(async {
        let (_, gate) = gate();
        // More than one transaction may stage, all as one actor.
        let many: Vec<_> = (0..crate::outbox::MAX_OWED_CHANGES_PER_TRANSACTION + 10)
            .map(|index| change(&format!("bae-{index}"), OwedOp::Delete))
            .collect();
        gate.owe_again(&many).await.unwrap();
        assert_eq!(
            gate.owed_changes_after(None).await.unwrap().len(),
            many.len()
        );
    })
    .unwrap()
}

#[test]
fn what_is_owed_again_wakes_the_drainer() {
    workers::block_on(async {
        let (_, gate) = gate();
        gate.owe_again(&[change("bae-5", OwedOp::Delete)])
            .await
            .unwrap();
        // The drainer, between passes, waits for a commit that staged
        // changes; owing again left it the permit.
        workers::timeout(
            std::time::Duration::from_secs(5),
            gate.staged_changes_committed(),
        )
        .await
        .expect("the drainer is woken");
    })
    .unwrap()
}
