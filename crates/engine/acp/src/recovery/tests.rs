//! Recovery ledger and bounded state-walk tests.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use rapidhash::{HashMapExt, RapidHashMap};
use storage::RegolithStore;

use super::*;
use crate::external::ExternalDocumentAcp;
use crate::local::{LocalDocumentACP, MemoryAcpStore};
use crate::outbox::{OwedOp, OwedSubject};

const OWNER: &str = "did:key:owner";
const READER: &str = "did:key:reader";

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

fn owner() -> Did {
    did(OWNER)
}

fn reader() -> Did {
    did(READER)
}

async fn ledger(
    policy: &str,
    resource: &str,
    object: &str,
) -> (Arc<RegolithStore>, RecoveryObject) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let txn = datastore::BasicTxn::new(store.as_ref(), 0, false)
        .await
        .unwrap();
    let ledger = RecoveryLedger::new(txn.acpstore());
    ledger
        .prepare(policy, resource, object, RecoveryOperation::Unregister)
        .await
        .unwrap();
    ledger
        .settle(
            policy,
            resource,
            object,
            RecoveryOperation::Unregister,
            true,
        )
        .await
        .unwrap();
    let records = ledger
        .page(None, RECOVERY_PAGE_BYTES)
        .await
        .unwrap()
        .remove(0)
        .1;
    drop(ledger);
    txn.commit().await.unwrap();
    (Arc::clone(&store), records)
}

async fn write(store: &RegolithStore, record: &RecoveryObject) {
    let txn = datastore::BasicTxn::new(store, 0, false).await.unwrap();
    RecoveryLedger::new(txn.acpstore())
        .write(record.clone())
        .await
        .unwrap();
    txn.commit().await.unwrap();
}

struct Fake {
    term: AtomicU64,
    registered: bool,
    grants: RapidHashMap<String, bool>,
}

impl Fake {
    fn key(relation: &str) -> String {
        format!("bae-1/{relation}/{READER}")
    }
}

#[async_trait]
impl RecoveryVerifier for Fake {
    async fn term(&self) -> std::result::Result<u64, String> {
        Ok(self.term.load(Ordering::SeqCst))
    }

    async fn object_owner(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<(bool, String), String> {
        Ok((self.registered, OWNER.to_owned()))
    }

    async fn actor_relationship(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
        relation: &str,
        _: &OwedSubject,
    ) -> std::result::Result<bool, String> {
        Ok(self
            .grants
            .get(&Self::key(relation))
            .copied()
            .unwrap_or(false))
    }
}

#[test]
fn a_pending_operation_is_applied_before_the_pass_asks_the_deployment() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let txn = datastore::BasicTxn::new(store.as_ref(), 0, false)
            .await
            .unwrap();
        RecoveryLedger::new(txn.acpstore())
            .prepare(
                "policy",
                "users",
                "bae-1",
                RecoveryOperation::Register { actor: owner() },
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let records = RecoveryLedger::new(
            datastore::BasicTxn::new(store.as_ref(), 0, true)
                .await
                .unwrap()
                .acpstore(),
        )
        .page(None, RECOVERY_PAGE_BYTES)
        .await
        .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].1.resolved().owner, Some(owner()));
    })
    .unwrap()
}

#[test]
fn a_page_is_owed_again_and_the_pass_resumes_without_repeating_it() {
    workers::block_on(async {
        let (store, mut records) = ledger("policy", "users", "bae-1").await;
        records.owner = Some(owner());
        records.relationships.push(RecoveryRelationship {
            actor: owner(),
            relation: "reader".into(),
            subject: OwedSubject::Actor { did: reader() },
        });
        write(&store, &records).await;
        let verifier = Fake {
            term: AtomicU64::new(1),
            registered: false,
            grants: RapidHashMap::new(),
        };

        let gate = ExternalDocumentAcp::new(
            Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::default()))),
            store,
        );
        let first = gate.next(&verifier).await.unwrap().expect("the first page");
        assert_eq!(first.effects.len(), 2);
        assert!(matches!(
            first.effects[0],
            crate::VoidedEffect::Registration { .. }
        ));
        assert!(matches!(
            first.effects[1],
            crate::VoidedEffect::Change(ref change) if change.op == OwedOp::Add
        ));
        gate.delivered(first.cursor).await.unwrap();

        let next = gate.next(&verifier).await.unwrap().expect("the pass ends");
        assert!(next.effects.is_empty());
        gate.delivered(next.cursor).await.unwrap();
        let ended = gate.next(&verifier).await.unwrap().expect("the pass ends");
        assert!(ended.effects.is_empty());
        gate.delivered(ended.cursor).await.unwrap();
    })
    .unwrap()
}

#[test]
fn a_new_term_resets_an_old_recovery_cursor() {
    workers::block_on(async {
        let (store, mut records) = ledger("policy", "users", "bae-1").await;
        records.owner = Some(owner());
        write(&store, &records).await;
        let verifier = Fake {
            term: AtomicU64::new(1),
            registered: false,
            grants: RapidHashMap::new(),
        };
        let gate = ExternalDocumentAcp::new(
            Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::default()))),
            store,
        );
        let page = gate.next(&verifier).await.unwrap().expect("the first page");
        gate.delivered(page.cursor).await.unwrap();
        verifier.term.store(2, Ordering::SeqCst);

        assert!(
            gate.next(&verifier).await.unwrap().is_some(),
            "the new term starts the pass again"
        );
    })
    .unwrap()
}

#[test]
fn a_confirmed_delete_is_reflected_when_the_pass_rebuilds() {
    workers::block_on(async {
        let (store, mut records) = ledger("policy", "users", "bae-1").await;
        records.owner = Some(owner());
        records.tombstones.push(RecoveryRelationship {
            actor: owner(),
            relation: "reader".into(),
            subject: OwedSubject::Actor { did: reader() },
        });
        write(&store, &records).await;
        let verifier = Fake {
            term: AtomicU64::new(1),
            registered: true,
            grants: {
                let mut grants = RapidHashMap::new();
                grants.insert(Fake::key("reader"), true);
                grants
            },
        };

        let gate = ExternalDocumentAcp::new(
            Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::default()))),
            store,
        );
        let page = gate.next(&verifier).await.unwrap().expect("the page");
        assert_eq!(page.effects.len(), 1);
        assert!(matches!(
            page.effects[0],
            crate::VoidedEffect::Change(ref change) if change.op == OwedOp::Delete
        ));
    })
    .unwrap()
}
