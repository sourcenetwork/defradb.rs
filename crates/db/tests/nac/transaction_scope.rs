use acp::{nac::NodePermission, PersistentZanzibarStore};
use db::{BasicTxn, NacConfig, NacManager, NacManagerApi};
use identity::Did;
use std::sync::Arc;
use storage::RegolithStore;

#[test]
fn node_permission_revocation_invalidates_authorized_write() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let manager = NacManager::new(
            Arc::new(PersistentZanzibarStore::from_store(store.clone())),
            NacConfig::new().with_enabled(),
        );
        let owner = Did::new("did:key:owner").unwrap();
        let writer = Did::new("did:key:writer").unwrap();
        manager.initialize(Some(&owner)).await.unwrap();
        manager
            .add_permission_grant(&owner, &writer, NodePermission::DocumentUpdate)
            .await
            .unwrap();
        let txn = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        assert!(manager
            .check_permission_in_txn(txn.acpstore(), &writer, NodePermission::DocumentUpdate)
            .await
            .unwrap());
        manager
            .remove_permission_grant(&owner, &writer, NodePermission::DocumentUpdate)
            .await
            .unwrap();
        txn.datastore()
            .set(b"authorized-document", b"changed")
            .await
            .unwrap();
        assert!(txn.commit().await.is_err());
        let retry = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        assert!(!manager
            .check_permission_in_txn(retry.acpstore(), &writer, NodePermission::DocumentUpdate)
            .await
            .unwrap());
        retry.discard().unwrap();
    })
    .unwrap()
}

/// The node policy is parsed once and shared: an authorized write reads NAC's
/// state in its own transaction but never the policy body again. A body that
/// no longer parses proves no transaction re-reads it.
#[test]
fn authorized_writes_share_the_loaded_node_policy() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let manager = NacManager::new(
            Arc::new(PersistentZanzibarStore::from_store(store.clone())),
            NacConfig::new().with_enabled(),
        );
        let owner = Did::new("did:key:owner").unwrap();
        let writer = Did::new("did:key:writer").unwrap();
        manager.initialize(Some(&owner)).await.unwrap();
        manager
            .add_permission_grant(&owner, &writer, NodePermission::DocumentUpdate)
            .await
            .unwrap();
        let corrupt = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        corrupt
            .acpstore()
            .set(
                format!("/zanzibar/policy/{}", acp::nac::NODE_POLICY_ID).as_bytes(),
                b"not a policy",
            )
            .await
            .unwrap();
        corrupt.commit().await.unwrap();

        for id in 2..4 {
            let txn = BasicTxn::new(store.as_ref(), id, false).await.unwrap();
            assert!(manager
                .check_permission_in_txn(txn.acpstore(), &writer, NodePermission::DocumentUpdate)
                .await
                .unwrap());
            txn.discard().unwrap();
        }
    })
    .unwrap()
}
