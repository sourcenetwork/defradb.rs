use super::*;
use crate::corekv::{Store, Writer};
use std::sync::Arc;

#[test]
fn an_identical_regrant_cannot_commit_over_revocation() {
    workers::block_on(async {
        let peers = Peerstore::new(Arc::new(crate::RegolithStore::in_memory().unwrap()));
        peers
            .grant_selected_car_root("peer", b"root")
            .await
            .unwrap();
        let key = key("peer", b"root").unwrap();
        let mut granting = peers.new_txn(false).await.unwrap();
        assert_eq!(granting.get(&key).await.unwrap().as_deref(), Some(GRANTED));
        granting.set(&key, GRANTED).await.unwrap();
        peers
            .revoke_selected_car_root("peer", b"root")
            .await
            .unwrap();
        assert!(granting.commit().await.unwrap_err().is_retriable());
        assert!(!peers
            .selected_car_root_allowed("peer", b"root")
            .await
            .unwrap());
        peers
            .grant_selected_car_root("peer", b"root")
            .await
            .unwrap();
        assert!(peers
            .selected_car_root_allowed("peer", b"root")
            .await
            .unwrap());
    })
    .unwrap();
}

#[test]
fn malformed_authority_is_not_permission() {
    workers::block_on(async {
        let peers = Peerstore::new(Arc::new(crate::RegolithStore::in_memory().unwrap()));
        let mut txn = peers.new_txn(false).await.unwrap();
        txn.set(&key("peer", b"root").unwrap(), b"corrupt")
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert!(peers
            .selected_car_root_allowed("peer", b"root")
            .await
            .is_err());
        peers
            .revoke_selected_car_root("peer", b"root")
            .await
            .unwrap();
        assert!(!peers
            .selected_car_root_allowed("peer", b"root")
            .await
            .unwrap());
    })
    .unwrap();
}

#[test]
fn authority_is_exact_revocable_and_persistent() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        {
            let store = Arc::new(crate::RegolithStore::open(directory.path()).unwrap());
            let peers = Peerstore::new(store);
            peers
                .grant_selected_car_root("alice", b"root")
                .await
                .unwrap();
            peers
                .grant_selected_car_root("alice", b"other")
                .await
                .unwrap();
            assert!(peers
                .selected_car_root_allowed("alice", b"root")
                .await
                .unwrap());
            assert!(!peers
                .selected_car_root_allowed("bob", b"root")
                .await
                .unwrap());
            assert!(!peers
                .selected_car_root_allowed("alice", b"roo")
                .await
                .unwrap());
        }
        {
            let store = Arc::new(crate::RegolithStore::open(directory.path()).unwrap());
            let peers = Peerstore::new(store);
            assert!(peers
                .selected_car_root_allowed("alice", b"root")
                .await
                .unwrap());
            peers
                .revoke_selected_car_root("alice", b"root")
                .await
                .unwrap();
            assert!(!peers
                .selected_car_root_allowed("alice", b"root")
                .await
                .unwrap());
            assert!(peers
                .selected_car_root_allowed("alice", b"other")
                .await
                .unwrap());
        }
        let peers = Peerstore::new(Arc::new(
            crate::RegolithStore::open(directory.path()).unwrap(),
        ));
        assert!(!peers
            .selected_car_root_allowed("alice", b"root")
            .await
            .unwrap());
        peers
            .grant_selected_car_root("alice", b"root")
            .await
            .unwrap();
        assert!(peers
            .selected_car_root_allowed("alice", b"root")
            .await
            .unwrap());
        assert!(peers.grant_selected_car_root("", b"root").await.is_err());
        assert!(peers.revoke_selected_car_root("alice", b"").await.is_err());
    })
    .unwrap();
}
