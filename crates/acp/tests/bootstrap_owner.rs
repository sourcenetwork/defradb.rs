use std::sync::Arc;

use acp::nac::{NodeACP, NodePermission, BOOTSTRAP_MARKER};
use acp::zanzibar::store::TransactionZanzibarStore;
use acp::{DocumentACP, PersistentZanzibarStore, ZanzibarDocumentACP};
use datastore::BasicTxn;
use identity::Did;
use storage::RegolithStore;

fn did(name: &str) -> Did {
    Did::new(format!("did:key:{name}")).unwrap()
}

#[test]
fn bootstrap_marker_is_removed_on_delete_and_same_id_recreation_cannot_rebind() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        let nac = NodeACP::new(persistent.clone());
        nac.enable(&did("runtime")).await.unwrap();
        let dac = ZanzibarDocumentACP::new(persistent.clone());
        dac.register_doc_object(&did("user"), "p", "items", "doc")
            .await
            .unwrap();
        let txn = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let scoped = nac.scoped(Arc::new(TransactionZanzibarStore::new(txn.acpstore())));
        scoped.load().await.unwrap();
        scoped
            .bind_initial_bootstrap_owner(
                &did("runtime"),
                "p",
                "items",
                "doc",
                &did("user"),
                &[NodePermission::DocumentRead],
            )
            .await
            .unwrap();
        drop(scoped);
        txn.commit().await.unwrap();
        assert!(nac
            .check_permission(&did("user"), NodePermission::DocumentRead)
            .await
            .unwrap());
        assert!(!nac
            .check_permission(&did("user"), NodePermission::DacBypass)
            .await
            .unwrap());
        assert!(dac
            .add_actor_relationship(
                &did("user"),
                &did("runtime"),
                "p",
                "items",
                "doc",
                BOOTSTRAP_MARKER,
                &[]
            )
            .await
            .is_err());
        dac.unregister_doc_object("p", "items", "doc")
            .await
            .unwrap();
        dac.register_doc_object(&did("attacker"), "p", "items", "doc")
            .await
            .unwrap();
        assert!(!nac
            .check_permission(&did("user"), NodePermission::DocumentRead)
            .await
            .unwrap());
        assert!(!nac
            .check_permission(&did("attacker"), NodePermission::DocumentRead)
            .await
            .unwrap());
        let txn = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        let scoped = nac.scoped(Arc::new(TransactionZanzibarStore::new(txn.acpstore())));
        scoped.load().await.unwrap();
        assert!(scoped
            .bind_initial_bootstrap_owner(
                &did("runtime"),
                "p",
                "items",
                "doc",
                &did("attacker"),
                &[NodePermission::DocumentRead]
            )
            .await
            .is_err());
        drop(scoped);
        txn.discard().unwrap();
    })
    .unwrap()
}

#[test]
fn bootstrap_binding_and_delete_conflict() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        let nac = NodeACP::new(persistent.clone());
        nac.enable(&did("runtime")).await.unwrap();
        let dac = ZanzibarDocumentACP::new(persistent.clone());
        dac.register_doc_object(&did("user"), "p", "items", "doc")
            .await
            .unwrap();
        let bind = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let delete = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        let scoped = nac.scoped(Arc::new(TransactionZanzibarStore::new(bind.acpstore())));
        scoped.load().await.unwrap();
        scoped
            .bind_initial_bootstrap_owner(
                &did("runtime"),
                "p",
                "items",
                "doc",
                &did("user"),
                &[NodePermission::DocumentRead],
            )
            .await
            .unwrap();
        let delete_dac = dac.bind_transaction(delete.acpstore()).unwrap();
        delete_dac
            .unregister_doc_object("p", "items", "doc")
            .await
            .unwrap();
        drop(delete_dac);
        delete.commit().await.unwrap();
        drop(scoped);
        assert!(bind.commit().await.is_err());
        assert!(!nac
            .check_permission(&did("user"), NodePermission::DocumentRead)
            .await
            .unwrap());
    })
    .unwrap()
}

#[test]
fn preexisting_direct_permissions_are_ambiguous_and_fail_closed() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        let nac = NodeACP::new(persistent.clone());
        nac.enable(&did("runtime")).await.unwrap();
        let dac = ZanzibarDocumentACP::new(persistent);
        dac.register_doc_object(&did("user"), "p", "items", "doc")
            .await
            .unwrap();
        nac.add_permission_grant(
            &did("runtime"),
            &did("partner"),
            NodePermission::DocumentRead,
        )
        .await
        .unwrap();
        let txn = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let scoped = nac.scoped(Arc::new(TransactionZanzibarStore::new(txn.acpstore())));
        scoped.load().await.unwrap();
        assert!(scoped
            .bind_initial_bootstrap_owner(
                &did("runtime"),
                "p",
                "items",
                "doc",
                &did("user"),
                &[NodePermission::DocumentRead]
            )
            .await
            .is_err());
        drop(scoped);
        txn.discard().unwrap();
        assert!(nac
            .check_permission(&did("partner"), NodePermission::DocumentRead)
            .await
            .unwrap());
        assert!(!nac
            .check_permission(&did("user"), NodePermission::DocumentRead)
            .await
            .unwrap());
    })
    .unwrap()
}
