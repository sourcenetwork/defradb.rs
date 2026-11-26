//! A closed journal rebuilt from the node's own state, without a new worker.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use acp::{
    DocumentACP, DocumentPermission, ExternalDocumentAcp, Identity, Observed, OweAgain,
    OwedSubject, RecoveryCursor, RecoverySource, RecoveryStep, RecoveryVerifier, VoidedEffect,
};
use async_trait::async_trait;
use identity::Did;
use storage::RegolithStore;
use vera_delegation::testing::{Recording, Returns};

use super::signer_tests::provider;
use super::transport_tests::Workers;
use crate::vera_rs::client::tests::Recorded as RpcRecorded;

const OWNER: &str = "did:key:owner";
const READER: &str = "did:key:reader";

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

/// The deployment behind the gate: it takes every write and counts them.
#[derive(Default)]
struct Deployment {
    registrations: AtomicU64,
    grants: AtomicU64,
}

#[async_trait]
impl DocumentACP for Deployment {
    async fn register_doc_object(&self, _: &Did, _: &str, _: &str, _: &str) -> acp::Result<()> {
        self.registrations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn is_doc_registered(&self, _: &str, _: &str, _: &str) -> acp::Result<bool> {
        Ok(false)
    }

    async fn get_doc_owner(&self, _: &str, _: &str, _: &str) -> acp::Result<Option<Did>> {
        Ok(Some(did(OWNER)))
    }

    async fn check_doc_access(
        &self,
        _: &Identity,
        _: DocumentPermission,
        _: &str,
        _: &str,
        _: &str,
    ) -> acp::Result<bool> {
        Ok(true)
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
    ) -> acp::Result<bool> {
        self.grants.fetch_add(1, Ordering::SeqCst);
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
    ) -> acp::Result<bool> {
        Ok(true)
    }

    async fn unregister_doc_object(&self, _: &str, _: &str, _: &str) -> acp::Result<()> {
        Ok(())
    }
}

/// What the new term holds: nothing the node recorded.
struct Lost;

#[async_trait]
impl RecoveryVerifier for Lost {
    async fn term(&self) -> Result<u64, String> {
        Ok(0)
    }

    async fn object_owner(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }

    async fn relationship(
        &self,
        _: u64,
        _: &Did,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &OwedSubject,
    ) -> Result<Observed, String> {
        Ok(Observed::Absent)
    }
}

#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_uncovered_takeover_recovery")]
async fn an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = RpcRecorded::answering(Ok(serde_json::Value::Null));
    let mut workers = Workers::new();
    let deployment = Arc::new(Deployment::default());
    let gate = Arc::new(ExternalDocumentAcp::new(
        deployment.clone(),
        Arc::new(RegolithStore::in_memory().expect("a store")),
    ));
    gate.register_doc_object(&did(OWNER), "policy", "users", "bae-1")
        .await
        .expect("registered");
    gate.add_actor_relationship(
        &did(OWNER),
        &did(READER),
        "policy",
        "users",
        "bae-1",
        "reader",
        &[],
    )
    .await
    .expect("granted");

    let provider = provider(signer, Arc::clone(&rpc), &mut workers).await;
    let outbox = Arc::downgrade(&gate);
    let outbox: std::sync::Weak<dyn OweAgain> = outbox;
    provider.follower.sink.set(outbox).expect("one outbox");
    let source = Arc::downgrade(&gate);
    let source: std::sync::Weak<dyn RecoverySource> = source;
    provider
        .follower
        .recovery
        .set(source)
        .expect("one recovery source");
    {
        let mut worker = provider.worker.clone().lock_owned().await;
        worker
            .confirmed()
            .close_recoverable("the journal cannot vouch for the cut".into())
            .expect("closed");
        provider.follower.follow.update(worker.confirmed());
    }
    assert!(provider.follower.admit().is_err(), "closed until the pass");

    provider.follower.check_with(&Lost).await;

    assert!(provider.follower.admit().is_ok(), "the node decides again");
    let mut worker = provider.worker.clone().lock_owned().await;
    assert_eq!(worker.confirmed().closed(), None);
    assert_eq!(worker.confirmed().recovery_needed(), None);
    assert_eq!(worker.confirmed().handled_term(), 0);
    assert_eq!(
        deployment.registrations.load(Ordering::SeqCst),
        2,
        "the registration was owed again and sent"
    );
    assert_eq!(
        deployment.grants.load(Ordering::SeqCst),
        2,
        "the grant was owed again and sent"
    );
    assert!(
        matches!(
            gate.next(&Lost).await.expect("the ledger"),
            RecoveryStep::Done { term: 0 }
        ),
        "the pass is over"
    );
    assert!(rpc.calls.pop().is_none(), "recovery sent no decision");
}

/// A source and outbox that count what a pass asks of them.
#[derive(Default)]
struct Watched {
    asked: AtomicU64,
}

#[async_trait]
impl RecoverySource for Watched {
    async fn next(&self, _: &dyn RecoveryVerifier) -> acp::Result<RecoveryStep> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(RecoveryStep::Done { term: 0 })
    }

    async fn delivered(&self, _: RecoveryCursor) -> acp::Result<()> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl OweAgain for Watched {
    async fn owe_again(&self, _: &[VoidedEffect]) -> acp::Result<()> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn settle_owed(&self) {
        self.asked.fetch_add(1, Ordering::SeqCst);
    }
}

#[nuclei::test]
async fn a_journal_closed_for_good_is_not_recovered() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = RpcRecorded::answering(Ok(serde_json::Value::Null));
    let mut workers = Workers::new();
    let provider = provider(signer, rpc, &mut workers).await;
    let watched = Arc::new(Watched::default());
    let outbox = Arc::downgrade(&watched);
    let outbox: std::sync::Weak<dyn OweAgain> = outbox;
    provider.follower.sink.set(outbox).expect("one outbox");
    let source = Arc::downgrade(&watched);
    let source: std::sync::Weak<dyn RecoverySource> = source;
    provider
        .follower
        .recovery
        .set(source)
        .expect("one recovery source");
    {
        let mut worker = provider.worker.clone().lock_owned().await;
        worker
            .confirmed()
            .close("several nodes share the provider".into())
            .expect("closed");
        provider.follower.follow.update(worker.confirmed());
    }

    provider.follower.check_with(&Lost).await;

    assert!(provider.follower.admit().is_err());
    assert_eq!(
        watched.asked.load(Ordering::SeqCst),
        0,
        "no pass ran for a journal that cannot be rebuilt"
    );
    let mut worker = provider.worker.clone().lock_owned().await;
    assert_eq!(
        worker.confirmed().closed(),
        Some("several nodes share the provider")
    );
}

/// A source whose pass a second node's outbox joins before it says it is
/// over.
struct Joined {
    follower: Arc<super::terms::Follower>,
}

#[async_trait]
impl RecoverySource for Joined {
    async fn next(&self, _: &dyn RecoveryVerifier) -> acp::Result<RecoveryStep> {
        self.follower.shared.store(true, Ordering::Release);
        Ok(RecoveryStep::Done { term: 0 })
    }

    async fn delivered(&self, _: RecoveryCursor) -> acp::Result<()> {
        Ok(())
    }
}

#[nuclei::test]
async fn a_pass_a_second_outbox_joined_leaves_the_term_unrecorded() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = RpcRecorded::answering(Ok(serde_json::Value::Null));
    let mut workers = Workers::new();
    let provider = provider(signer, rpc, &mut workers).await;
    let watched = Arc::new(Watched::default());
    let outbox = Arc::downgrade(&watched);
    let outbox: std::sync::Weak<dyn OweAgain> = outbox;
    provider.follower.sink.set(outbox).expect("one outbox");
    let joined = Arc::new(Joined {
        follower: Arc::clone(&provider.follower),
    });
    let source = Arc::downgrade(&joined);
    let source: std::sync::Weak<dyn RecoverySource> = source;
    provider
        .follower
        .recovery
        .set(source)
        .expect("one recovery source");
    {
        let mut worker = provider.worker.clone().lock_owned().await;
        worker
            .confirmed()
            .close_recoverable("the journal cannot vouch for the cut".into())
            .expect("closed");
        provider.follower.follow.update(worker.confirmed());
    }

    provider.follower.check_with(&Lost).await;

    assert!(
        provider.follower.admit().is_err(),
        "the second node's voided changes are owed by nobody"
    );
    let mut worker = provider.worker.clone().lock_owned().await;
    assert!(
        worker.confirmed().recovery_needed().is_some(),
        "the term is not recorded as recovered"
    );
    assert_eq!(
        watched.asked.load(Ordering::SeqCst),
        0,
        "nothing was owed or settled"
    );
}
