//! Recovery ledger and bounded state-walk tests.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use rapidhash::{HashMapExt, RapidHashMap};
use storage::RegolithStore;

use super::*;
use crate::dac::DocumentACP;
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
        .records
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

struct Refusing;

#[derive(Default)]
struct PartialBatch(std::sync::atomic::AtomicUsize);

#[async_trait]
impl DocumentACP for Refusing {
    async fn register_doc_object(&self, _: &Did, _: &str, _: &str, _: &str) -> Result<()> {
        Err(Error::ProviderUnavailable("not reached".into()))
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(false)
    }

    async fn check_doc_access(
        &self,
        _: &crate::Identity,
        _: crate::DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn add_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<bool> {
        Err(Error::NotManager {
            operation: "add actor relationship".into(),
        })
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
        Err(Error::NotManager {
            operation: "delete actor relationship".into(),
        })
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Err(Error::ProviderUnavailable("not reached".into()))
    }
}

#[async_trait]
impl DocumentACP for PartialBatch {
    async fn register_doc_object(&self, _: &Did, _: &str, _: &str, _: &str) -> Result<()> {
        match self.0.fetch_add(1, Ordering::SeqCst) {
            0 => Ok(()),
            _ => Err(Error::ProviderUnavailable(
                "second registration refused".into(),
            )),
        }
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(false)
    }

    async fn check_doc_access(
        &self,
        _: &crate::Identity,
        _: crate::DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn add_actor_relationship(
        &self,
        _: &Did,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<bool> {
        Err(Error::NotManager {
            operation: "add actor relationship".into(),
        })
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
        Err(Error::NotManager {
            operation: "delete actor relationship".into(),
        })
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Err(Error::ProviderUnavailable("not reached".into()))
    }
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
        let records = records.records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].1.resolved().owner, Some(owner()));
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
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

        assert!(
            gate.next(&verifier).await.unwrap().is_none(),
            "a completed cursor ends the pass"
        );
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_pass_pages_past_the_first_ledger_page_and_ends() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let txn = datastore::BasicTxn::new(store.as_ref(), 0, false)
            .await
            .unwrap();
        let ledger = RecoveryLedger::new(txn.acpstore());
        for index in 0..3000 {
            let mut record = RecoveryObject::new("policy", "users", &format!("bae-{index}"));
            record.owner = Some(owner());
            record.relationships.push(RecoveryRelationship {
                actor: owner(),
                relation: "reader".into(),
                subject: OwedSubject::Actor { did: reader() },
            });
            ledger.write(record).await.unwrap();
        }
        drop(ledger);
        txn.commit().await.unwrap();

        let verifier = Fake {
            term: AtomicU64::new(1),
            registered: false,
            grants: RapidHashMap::new(),
        };
        let gate = ExternalDocumentAcp::new(
            Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::default()))),
            store,
        );
        let mut effects = 0;
        while let Some(page) = gate.next(&verifier).await.unwrap() {
            effects += page.effects.len();
            assert!(page.unrestored.is_empty());
            gate.delivered(page.cursor).await.unwrap();
        }
        assert_eq!(effects, 6000);
        assert!(
            gate.next(&verifier).await.unwrap().is_none(),
            "the completed pass ends"
        );
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

        let reset = gate
            .next(&verifier)
            .await
            .unwrap()
            .expect("the new term starts the pass again");
        assert_eq!(reset.effects.len(), 1);
        assert!(reset.cursor.complete);
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

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_refused_grant_is_not_owed_again_by_recovery() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let gate = ExternalDocumentAcp::new(Arc::new(Refusing), Arc::clone(&store));
        let error = gate
            .add_actor_relationship(
                &owner(),
                &reader(),
                "policy",
                "users",
                "bae-1",
                "reader",
                &[],
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::NotManager { .. }));

        let txn = datastore::BasicTxn::new(store.as_ref(), 0, true)
            .await
            .unwrap();
        let records = RecoveryLedger::new(txn.acpstore())
            .page(None, RECOVERY_PAGE_BYTES)
            .await
            .unwrap()
            .records;
        assert_eq!(records.len(), 1);
        assert!(records[0].1.pending.is_none());
        assert!(records[0].1.relationships.is_empty());
        drop(records);
        drop(txn);

        let verifier = Fake {
            term: AtomicU64::new(1),
            registered: false,
            grants: RapidHashMap::new(),
        };
        let page = gate.next(&verifier).await.unwrap().expect("the page");
        assert!(page.effects.is_empty(), "a refused grant is not owed again");
        assert!(!page.unrestored.is_empty());
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
fn a_batch_failure_keeps_its_applied_registration_prefix() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let gate = ExternalDocumentAcp::new(Arc::new(PartialBatch::default()), store.clone());
        let objects = [
            crate::AcpObject {
                policy_id: "policy",
                resource: "users",
                object_id: "bae-1",
            },
            crate::AcpObject {
                policy_id: "policy",
                resource: "users",
                object_id: "bae-2",
            },
        ];
        let failure = gate.register_objects(&owner(), &objects).await.unwrap_err();
        assert_eq!(failure.index, 1);

        let txn = datastore::BasicTxn::new(store.as_ref(), 0, true)
            .await
            .unwrap();
        let records = RecoveryLedger::new(txn.acpstore())
            .page(None, RECOVERY_PAGE_BYTES)
            .await
            .unwrap()
            .records;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].1.owner, Some(owner()));
        assert!(records[0].1.pending.is_none());
        assert_eq!(records[1].1.owner, None);
        assert!(records[1].1.pending.is_none());
    })
    .unwrap()
}
