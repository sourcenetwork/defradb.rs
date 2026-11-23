//! A closed journal rebuilt from the node's own state, without a new worker.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use regolith::sync::SegQueue;

use super::signer_tests::provider;
use super::transport_tests::Workers;
use crate::vera_rs::client::tests::Recorded as RpcRecorded;
use acp::{
    NewOwedChange, OweAgain, OwedOp, OwedSubject, OwedTuple, RecoveryCursor, RecoveryPage,
    RecoverySource, RecoveryVerifier, VoidedEffect,
};
use vera_delegation::testing::{Recording, Returns};

struct Outbox {
    effects: SegQueue<VoidedEffect>,
}

#[async_trait]
impl OweAgain for Outbox {
    async fn owe_again(&self, effects: &[VoidedEffect]) -> acp::Result<()> {
        for effect in effects {
            self.effects.push(effect.clone());
        }
        Ok(())
    }

    async fn settle_owed(&self) {}
}

struct Source {
    sent: AtomicBool,
}

#[async_trait]
impl RecoverySource for Source {
    async fn next(&self, _: &dyn RecoveryVerifier) -> acp::Result<Option<RecoveryPage>> {
        if self
            .sent
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(None);
        }
        Ok(Some(RecoveryPage {
            effects: vec![VoidedEffect::Change(NewOwedChange {
                txn: None,
                tuple: OwedTuple {
                    policy_id: "policy".into(),
                    resource: "users".into(),
                    object: "bae-1".into(),
                    relation: "reader".into(),
                    subject: OwedSubject::Actor {
                        did: identity::Did::new("did:key:reader").unwrap(),
                    },
                },
                op: OwedOp::Add,
                actor: identity::Did::new("did:key:owner").unwrap(),
                authority: acp::OwedAuthority::NodeKey,
                authority_checked: true,
            })],
            cursor: RecoveryCursor {
                term: 0,
                cursor: Some(b"after".to_vec()),
                complete: true,
            },
            unrestored: Vec::new(),
        }))
    }

    async fn delivered(&self, _: RecoveryCursor) -> acp::Result<()> {
        Ok(())
    }
}

struct Verifier;

#[async_trait]
impl RecoveryVerifier for Verifier {
    async fn term(&self) -> std::result::Result<u64, String> {
        Ok(0)
    }

    async fn object_owner(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<(bool, String), String> {
        Err("not called".into())
    }

    async fn actor_relationship(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &acp::OwedSubject,
    ) -> std::result::Result<bool, String> {
        Err("not called".into())
    }
}

#[nuclei::test]
async fn an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker() {
    let signer = Recording::returning(Returns::Genuine);
    let rpc = RpcRecorded::answering(Ok(serde_json::Value::Null));
    let mut workers = Workers::new();
    let outbox = Arc::new(Outbox {
        effects: SegQueue::new(),
    });
    let source = Arc::new(Source {
        sent: AtomicBool::new(false),
    });
    let verfier = Verifier;
    let provider = provider(signer, Arc::clone(&rpc), &mut workers).await;
    provider
        .follower
        .sink
        .set(Arc::downgrade(&outbox) as std::sync::Weak<dyn OweAgain>)
        .expect("one outbox");
    provider
        .follower
        .recovery
        .set(source)
        .map_err(|_| "one recovery source")
        .unwrap();
    {
        let mut worker = provider.worker.clone().lock_owned().await;
        worker
            .confirmed()
            .close_recoverable("the journal cannot vouch for the cut".into())
            .expect("closed");
        provider.follower.follow.update(worker.confirmed());
    }

    provider.follower.check_with(&verfier).await;

    let mut worker = provider.worker.clone().lock_owned().await;
    assert_eq!(worker.confirmed().closed(), None);
    assert_eq!(worker.confirmed().recovery_needed(), None);
    assert_eq!(worker.confirmed().handled_term(), 0);
    assert_eq!(outbox.effects.pop().is_some(), true, "the page was owed");
    assert_eq!(outbox.effects.pop().is_none(), true, "one page only");
    assert!(rpc.calls.pop().is_none(), "recovery sent no decision");
}
