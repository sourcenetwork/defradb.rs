//! A database transaction owes the edge a snapshot refresh only once it
//! writes a tuple of the node's access control policy: other policies' writes
//! owe nothing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use acp::nac::NODE_POLICY_ID;
use acp::zanzibar::store::TransactionZanzibarStore;
use acp::{Relationship, Subject, ZanzibarStore};
use datastore::{BasicTxn, NodeAccessWrites, OnCommit};
use identity::Did;
use storage::RegolithStore;

/// Counts the writes it is told about.
#[derive(Default)]
struct Counted(AtomicUsize);

impl NodeAccessWrites for Counted {
    fn written(&self) -> OnCommit {
        self.0.fetch_add(1, Ordering::SeqCst);
        OnCommit::AsyncOnce(
            "test::edge",
            storage::corekv::make_async_callback(|| async {}),
        )
    }
}

fn grant(object: &str) -> Relationship {
    Relationship::new(
        "node",
        object,
        "admin",
        Subject::Entity(
            Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap(),
        ),
    )
}

#[test]
fn only_node_policy_writes_owe_the_edge_refresh() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = BasicTxn::new(&store, 1, false).await.unwrap();
        let counted = Arc::new(Counted::default());
        txn.set_node_access_writes(counted.clone());
        let zanzibar = TransactionZanzibarStore::new(txn.acpstore());

        zanzibar
            .store_relationship("some-document-policy", &grant("doc"))
            .await
            .unwrap();
        zanzibar
            .delete_relationship("some-document-policy", &grant("doc"))
            .await
            .unwrap();
        assert_eq!(
            counted.0.load(Ordering::SeqCst),
            0,
            "a document policy write"
        );

        zanzibar
            .delete_relationship(NODE_POLICY_ID, &grant("singleton"))
            .await
            .unwrap();
        assert_eq!(
            counted.0.load(Ordering::SeqCst),
            0,
            "deleting a tuple that is not there writes nothing"
        );

        zanzibar
            .store_relationship(NODE_POLICY_ID, &grant("singleton"))
            .await
            .unwrap();
        zanzibar
            .delete_relationship(NODE_POLICY_ID, &grant("singleton"))
            .await
            .unwrap();
        assert_eq!(
            counted.0.load(Ordering::SeqCst),
            2,
            "a grant and its revocation"
        );

        drop(zanzibar);
        txn.commit().await.unwrap();
    })
    .unwrap();
}
