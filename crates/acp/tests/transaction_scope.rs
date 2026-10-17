use acp::{
    DocumentACP, DocumentPermission, Identity, PersistentZanzibarStore, ZanzibarDocumentACP,
};
use datastore::BasicTxn;
use identity::Did;
use std::sync::Arc;
use storage::{RegolithStore, RegolithStoreOptions};
use zanzibar::store::ZanzibarStore;
use zanzibar::types::Relationship;

fn did(name: &str) -> Did {
    Did::new(format!("did:key:{name}")).unwrap()
}

#[test]
fn registrations_share_the_document_commit_and_have_one_winner() {
    workers::block_on(async {
    for encrypted in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let opts = RegolithStoreOptions::default();
        let opts = if encrypted {
            opts.with_encryption_key([42; 32])
        } else {
            opts
        };
        let store = Arc::new(RegolithStore::open_with_options(dir.path(), opts).unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        persistent.store_policy(&ZanzibarDocumentACP::<PersistentZanzibarStore<RegolithStore>>::create_default_policy("p", "items")).await.unwrap();
        let acp = ZanzibarDocumentACP::new(persistent);
        let first = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let second = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        let first_acp = acp.bind_transaction(first.acpstore()).unwrap();
        let second_acp = acp.bind_transaction(second.acpstore()).unwrap();
        first_acp
            .register_doc_object(&did("alice"), "p", "items", "doc")
            .await
            .unwrap();
        second_acp
            .register_doc_object(&did("bob"), "p", "items", "doc")
            .await
            .unwrap();
        first.datastore().set(b"document", b"body").await.unwrap();
        assert!(!acp.is_doc_registered("p", "items", "doc").await.unwrap());
        drop(first_acp);
        first.commit().await.unwrap();
        drop(second_acp);
        assert!(second.commit().await.is_err());
        assert_eq!(
            acp.get_doc_owner("p", "items", "doc").await.unwrap(),
            Some(did("alice"))
        );
        let retry = BasicTxn::new(store.as_ref(), 3, false).await.unwrap();
        let retry_acp = acp.bind_transaction(retry.acpstore()).unwrap();
        retry_acp
            .register_doc_object(&did("alice"), "p", "items", "doc")
            .await
            .unwrap();
        assert!(matches!(
            retry_acp
                .register_doc_object(&did("bob"), "p", "items", "doc")
                .await,
            Err(acp::Error::DocumentAlreadyRegistered(_))
        ));
        drop(retry_acp);
        retry.commit().await.unwrap();
    }
})
    .unwrap()
}

#[test]
fn revocation_invalidates_writer_but_read_only_authorization_keeps_its_snapshot() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        let acp = ZanzibarDocumentACP::new(persistent.clone());
        acp.register_doc_object(&did("alice"), "p", "items", "doc")
            .await
            .unwrap();
        let grant = Relationship::with_entity("items", "doc", "updater", did("bob"));
        persistent.store_relationship("p", &grant).await.unwrap();
        let write = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let read = BasicTxn::new(store.as_ref(), 2, true).await.unwrap();
        let writer_acp = acp.bind_transaction(write.acpstore()).unwrap();
        let reader_acp = acp.bind_transaction(read.acpstore()).unwrap();
        let bob = Identity::Authenticated(did("bob"));
        assert!(writer_acp
            .check_doc_access(&bob, DocumentPermission::Update, "p", "items", "doc")
            .await
            .unwrap());
        assert!(reader_acp
            .check_doc_access(&bob, DocumentPermission::Update, "p", "items", "doc")
            .await
            .unwrap());
        persistent.delete_relationship("p", &grant).await.unwrap();
        write
            .datastore()
            .set(b"document", b"changed")
            .await
            .unwrap();
        drop(writer_acp);
        assert!(write.commit().await.is_err());
        assert!(reader_acp
            .check_doc_access(&bob, DocumentPermission::Update, "p", "items", "doc")
            .await
            .unwrap());
        drop(reader_acp);
        read.commit().await.unwrap();
    })
    .unwrap()
}

#[test]
fn policy_identity_cannot_be_overwritten() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = PersistentZanzibarStore::from_store(store.clone());
        let policy =
            ZanzibarDocumentACP::<PersistentZanzibarStore<RegolithStore>>::create_default_policy(
                "p", "items",
            );
        persistent.store_policy(&policy).await.unwrap();
        persistent.store_policy(&policy).await.unwrap();
        let different =
            ZanzibarDocumentACP::<PersistentZanzibarStore<RegolithStore>>::create_default_policy(
                "p", "other",
            );
        assert!(matches!(
            persistent.store_policy(&different).await,
            Err(zanzibar::error::Error::PolicyAlreadyExists(_))
        ));
        let txn = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        let scoped = acp::zanzibar::store::TransactionZanzibarStore::new(txn.acpstore());
        assert!(matches!(
            scoped.store_policy(&different).await,
            Err(zanzibar::error::Error::PolicyAlreadyExists(_))
        ));
        drop(scoped);
        txn.discard().unwrap();
    })
    .unwrap()
}

/// Every transaction bound from one ACP decides with one shared policy
/// snapshot: the policy is read and parsed once, not per bound transaction. A
/// body that no longer parses proves no later transaction re-reads it.
#[test]
fn bound_transactions_share_one_policy_snapshot() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        persistent
        .store_policy(
            &ZanzibarDocumentACP::<PersistentZanzibarStore<RegolithStore>>::create_default_policy(
                "p", "items",
            ),
        )
        .await
        .unwrap();
        let acp = ZanzibarDocumentACP::new(persistent);
        acp.register_doc_object(&did("alice"), "p", "items", "doc")
            .await
            .unwrap();
        let alice = Identity::Authenticated(did("alice"));
        let first = BasicTxn::new(store.as_ref(), 1, true).await.unwrap();
        let first_acp = acp.bind_transaction(first.acpstore()).unwrap();
        assert!(first_acp
            .check_doc_access(&alice, DocumentPermission::Update, "p", "items", "doc")
            .await
            .unwrap());
        drop(first_acp);
        first.discard().unwrap();

        let corrupt = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        corrupt
            .acpstore()
            .set(b"/zanzibar/policy/p", b"not a policy")
            .await
            .unwrap();
        corrupt.commit().await.unwrap();

        for id in 3..5 {
            let txn = BasicTxn::new(store.as_ref(), id, true).await.unwrap();
            let bound = acp.bind_transaction(txn.acpstore()).unwrap();
            assert!(bound
                .check_doc_access(&alice, DocumentPermission::Update, "p", "items", "doc")
                .await
                .unwrap());
            assert!(!bound
                .check_doc_access(
                    &Identity::Authenticated(did("mallory")),
                    DocumentPermission::Update,
                    "p",
                    "items",
                    "doc",
                )
                .await
                .unwrap());
            drop(bound);
            txn.discard().unwrap();
        }
    })
    .unwrap()
}

fn exclusion_policy() -> zanzibar::types::Policy {
    use zanzibar::types::{Policy, Relation, Resource};
    use zanzibar::RelationExpression;
    Policy::new("p", "items with an exclusion")
        .with_resource(Resource::new("group").with_relation(Relation::direct("member")))
        .with_resource(
            Resource::new("items")
                .with_relation(Relation::direct("owner"))
                .with_relation(Relation::direct("updater"))
                .with_relation(Relation::direct("banned"))
                .with_relation(Relation::computed(
                    "update",
                    RelationExpression::difference(
                        RelationExpression::union(vec![
                            RelationExpression::computed_userset("owner"),
                            RelationExpression::computed_userset("updater"),
                        ]),
                        RelationExpression::computed_userset("banned"),
                    ),
                )),
        )
}

async fn exclusion_fixture() -> (
    Arc<RegolithStore>,
    Arc<PersistentZanzibarStore<RegolithStore>>,
    ZanzibarDocumentACP<PersistentZanzibarStore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
    persistent.store_policy(&exclusion_policy()).await.unwrap();
    let acp = ZanzibarDocumentACP::new(persistent.clone());
    acp.register_doc_object(&did("alice"), "p", "items", "doc")
        .await
        .unwrap();
    (store, persistent, acp)
}

async fn may_update(
    acp: &ZanzibarDocumentACP<PersistentZanzibarStore<RegolithStore>>,
    txn: &BasicTxn,
    who: &str,
) -> bool {
    acp.bind_transaction(txn.acpstore())
        .unwrap()
        .check_doc_access(
            &Identity::Authenticated(did(who)),
            DocumentPermission::Update,
            "p",
            "items",
            "doc",
        )
        .await
        .unwrap()
}

/// A grant that reaches the writer only through a userset is found by a scan
/// of the relation's subjects; revoking it must abort the writer all the same.
#[test]
fn revocation_through_the_scan_path_invalidates_the_writer() {
    workers::block_on(async {
        let (store, persistent, acp) = exclusion_fixture().await;
        let userset = Relationship::new(
            "items",
            "doc",
            "updater",
            zanzibar::types::Subject::entity_set("group", "eng", "member"),
        );
        persistent.store_relationship("p", &userset).await.unwrap();
        persistent
            .store_relationship(
                "p",
                &Relationship::with_entity("group", "eng", "member", did("bob")),
            )
            .await
            .unwrap();

        let write = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        assert!(may_update(&acp, &write, "bob").await);
        persistent.delete_relationship("p", &userset).await.unwrap();
        write
            .datastore()
            .set(b"document", b"bob's edit")
            .await
            .unwrap();
        assert!(write.commit().await.is_err());

        let retry = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        assert!(!may_update(&acp, &retry, "bob").await);
        retry.discard().unwrap();
    })
    .unwrap()
}

/// The writer decided that bob is not excluded; a grant that excludes him and
/// commits first must abort the writer, and the re-run denies.
#[test]
fn a_grant_into_an_exclusion_invalidates_the_writer() {
    workers::block_on(async {
        let (store, persistent, acp) = exclusion_fixture().await;
        persistent
            .store_relationship(
                "p",
                &Relationship::with_entity("items", "doc", "updater", did("bob")),
            )
            .await
            .unwrap();

        let write = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
        assert!(may_update(&acp, &write, "bob").await);
        persistent
            .store_relationship(
                "p",
                &Relationship::with_entity("items", "doc", "banned", did("bob")),
            )
            .await
            .unwrap();
        write
            .datastore()
            .set(b"document", b"bob's edit")
            .await
            .unwrap();
        assert!(write.commit().await.is_err());

        let retry = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
        assert!(!may_update(&acp, &retry, "bob").await);
        retry.discard().unwrap();
    })
    .unwrap()
}

async fn relations_left(
    persistent: &PersistentZanzibarStore<RegolithStore>,
) -> Vec<zanzibar::types::Subject> {
    let mut left = Vec::new();
    for relation in ["owner", "updater"] {
        left.extend(
            persistent
                .get_relation_subjects("p", "items", "doc", relation)
                .await
                .unwrap(),
        );
    }
    left
}

/// A grant and an unregistration of the same document race. Whichever
/// commits first, the other aborts, and its re-run leaves no grant on an
/// unregistered document.
#[test]
fn a_grant_racing_an_unregister_leaves_no_orphaned_grant() {
    workers::block_on(async {
        for grant_first in [false, true] {
            let (store, persistent, acp) = exclusion_fixture().await;
            let grant = BasicTxn::new(store.as_ref(), 1, false).await.unwrap();
            let unregister = BasicTxn::new(store.as_ref(), 2, false).await.unwrap();
            let grant_acp = acp.bind_transaction(grant.acpstore()).unwrap();
            let unregister_acp = acp.bind_transaction(unregister.acpstore()).unwrap();
            assert!(grant_acp
                .add_relationship(
                    &did("alice"),
                    zanzibar::types::Subject::Entity(did("bob")),
                    "p",
                    "items",
                    "doc",
                    "updater",
                    &[],
                )
                .await
                .unwrap());
            unregister_acp
                .unregister_doc_object("p", "items", "doc")
                .await
                .unwrap();
            drop(grant_acp);
            drop(unregister_acp);

            if grant_first {
                grant.commit().await.unwrap();
                assert!(unregister.commit().await.is_err());
                let retry = BasicTxn::new(store.as_ref(), 3, false).await.unwrap();
                acp.bind_transaction(retry.acpstore())
                    .unwrap()
                    .unregister_doc_object("p", "items", "doc")
                    .await
                    .unwrap();
                retry.commit().await.unwrap();
            } else {
                unregister.commit().await.unwrap();
                assert!(grant.commit().await.is_err());
                let retry = BasicTxn::new(store.as_ref(), 3, false).await.unwrap();
                assert!(matches!(
                    acp.bind_transaction(retry.acpstore())
                        .unwrap()
                        .add_relationship(
                            &did("alice"),
                            zanzibar::types::Subject::Entity(did("bob")),
                            "p",
                            "items",
                            "doc",
                            "updater",
                            &[],
                        )
                        .await,
                    Err(acp::Error::NotOwner { .. })
                ));
                retry.discard().unwrap();
            }
            assert!(
                relations_left(&persistent).await.is_empty(),
                "grant_first={grant_first}: a grant outlived the unregistration"
            );
        }
    })
    .unwrap()
}

/// While documents are created and registered, a concurrent stranger reads
/// every snapshot it can take: no snapshot ever shows a created document
/// without its owner, so the stranger never reads one.
#[test]
fn no_snapshot_ever_shows_a_created_document_without_its_owner() {
    workers::block_on(async {
        const DOCS: usize = 64;
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let persistent = Arc::new(PersistentZanzibarStore::from_store(store.clone()));
        persistent
        .store_policy(
            &ZanzibarDocumentACP::<PersistentZanzibarStore<RegolithStore>>::create_default_policy(
                "p", "items",
            ),
        )
        .await
        .unwrap();
        let acp = Arc::new(ZanzibarDocumentACP::new(persistent));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let writer = {
            let (store, acp, done) = (store.clone(), acp.clone(), done.clone());
            workers::spawn(None, "test", move || async move {
                for index in 0..DOCS {
                    let txn = BasicTxn::new(store.as_ref(), index as u64, false)
                        .await
                        .unwrap();
                    acp.bind_transaction(txn.acpstore())
                        .unwrap()
                        .register_doc_object(&did("alice"), "p", "items", &format!("doc-{index}"))
                        .await
                        .unwrap();
                    txn.datastore()
                        .set(format!("doc-{index}").as_bytes(), b"body")
                        .await
                        .unwrap();
                    txn.commit().await.unwrap();
                }
                done.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap()
        };

        let mut snapshots = 0usize;
        loop {
            let finished = done.load(std::sync::atomic::Ordering::SeqCst);
            let read = BasicTxn::new(store.as_ref(), 0, true).await.unwrap();
            let reader = acp.bind_transaction(read.acpstore()).unwrap();
            let mut iter = read
                .datastore()
                .iterator(storage::corekv::IterOptions::new().with_prefix(b"doc-".to_vec()))
                .await
                .unwrap();
            let mut seen = 0;
            while let Some(entry) = iter.next().await.unwrap() {
                let key = String::from_utf8_lossy(&entry.key);
                let doc_id = &key[key.find("doc-").unwrap()..];
                assert!(
                    !reader
                        .check_doc_access(
                            &Identity::Authenticated(did("mallory")),
                            DocumentPermission::Read,
                            "p",
                            "items",
                            doc_id,
                        )
                        .await
                        .unwrap(),
                    "a stranger read {doc_id} in a snapshot"
                );
                seen += 1;
            }
            drop(iter);
            drop(reader);
            read.discard().unwrap();
            snapshots += 1;
            if finished {
                assert_eq!(seen, DOCS);
                break;
            }
            workers::yield_now().await;
        }
        writer.await.unwrap();
        assert!(snapshots > 0);
    })
    .unwrap()
}
