//! A lens whose config commits durably stays committed when registering it in
//! the lens store fails afterwards: it is owed, known as a migration, and
//! registered when it is next used.

use std::sync::Arc;

use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use futures::TryStreamExt;
use lens::{LensConfig, LensDoc, LensDocStream, LensModule, TransformId, TransformStore};
use query::txn::TransactionRegistry;
use storage::corekv::Key;
use storage::keys::systemstore::LensConfigKey;
use storage::RegolithStore;

use crate::common::refusing_lens_store::RefusingStore;

/// A database with a `Users` collection whose lens store refuses registration.
async fn node() -> (Arc<DB<RegolithStore>>, Arc<RefusingStore>) {
    let store = RefusingStore::refusing();
    let mut db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    db.set_lens_store(store.clone());
    db.create_collections_atomic(query::parse_sdl("type Users { name: String }").unwrap())
        .await
        .unwrap();
    (Arc::new(db), store)
}

fn config(source: &str, destination: &str) -> LensConfig {
    LensConfig::new(
        source,
        destination,
        LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
    )
}

async fn run(db: &DB<RegolithStore>, id: &TransformId) -> lens::Result<Vec<LensDoc>> {
    let mut doc = LensDoc::new();
    doc.insert("name".to_string(), serde_json::json!("Ada"));
    let docs: LensDocStream = Box::pin(futures::stream::iter(vec![doc]));
    db.lens_store().transform(id, docs)?.try_collect().await
}

/// The committed lens `id` is owed: known, refused while registration fails,
/// and registered by its first use once registration succeeds.
async fn assert_owed_until_used(db: &DB<RegolithStore>, store: &RefusingStore, id: &TransformId) {
    assert!(!store.inner.has_transform(id), "registration was refused");
    assert!(
        db.has_migration(id),
        "a committed migration is known while its registration is owed"
    );
    assert!(db
        .lens_store()
        .list()
        .await
        .unwrap()
        .contains_key(&id.to_string()));

    let error = run(db, id)
        .await
        .expect_err("a use while registration fails reports why");
    assert!(
        error.to_string().contains("registration refused"),
        "{error}"
    );
    assert!(!store.inner.has_transform(id));

    store.allow();
    let docs = run(db, id)
        .await
        .expect("the owed lens registers when it is used");
    assert_eq!(docs.len(), 1);
    assert!(store.inner.has_transform(id));
    assert!(db.has_migration(id));
}

#[test]
fn a_transaction_migration_commits_when_its_registration_fails() {
    workers::block_on(async {
        let (db, store) = node().await;
        let registry = DbTransactionRegistry::new(db.clone());
        let txn_id = registry.begin(false).await.unwrap();
        let id = registry
            .set_migration_in_txn(&txn_id, config("users-v1", "users-v2"), None)
            .await
            .unwrap();

        registry
            .commit(&txn_id)
            .await
            .expect("the commit stands when the registration after it fails");

        assert_owed_until_used(&db, &store, &id).await;
    })
    .unwrap()
}

#[test]
fn a_patch_migration_commits_when_its_registration_fails() {
    workers::block_on(async {
    let (db, store) = node().await;
    let patched = db
        .patch_collection_with_migration(
            "Users",
            r#"[{"op": "add", "path": "/Users/Fields/-", "value": {"Name": "age", "Kind": "Int"}}]"#,
            Some(config("", "")),
            None,
        )
        .await
        .expect("the patch stands when the registration after it fails");
    let id = TransformId::new(
        patched
            .previous_version
            .and_then(|previous| previous.transform)
            .unwrap(),
    );

    assert_owed_until_used(&db, &store, &id).await;
})
    .unwrap()
}

#[test]
fn an_added_lens_is_returned_when_its_registration_fails() {
    workers::block_on(async {
        let (db, store) = node().await;
        let id = db
            .add_lens(config("", ""))
            .await
            .expect("the lens is committed, so adding it succeeds");

        assert_owed_until_used(&db, &store, &id).await;
    })
    .unwrap()
}

/// A use of one document, the call every lensed read and migration makes,
/// also registers an owed lens first, and reports why while it cannot.
#[test]
fn a_one_document_use_registers_an_owed_lens() {
    workers::block_on(async {
        let (db, store) = node().await;
        let id = db
            .add_lens(config("", ""))
            .await
            .expect("the lens is committed, so adding it succeeds");
        let mut doc = LensDoc::new();
        doc.insert("name".to_string(), serde_json::json!("Ada"));

        let error = db
            .lens_store()
            .transform_doc(&id, doc.clone(), false)
            .await
            .expect_err("a use while registration fails reports why");
        assert!(
            error.to_string().contains("registration refused"),
            "{error}"
        );

        store.allow();
        let migrated = db
            .lens_store()
            .transform_doc(&id, doc.clone(), false)
            .await
            .expect("the owed lens registers when it is used");
        assert_eq!(migrated, doc);
        assert!(store.inner.has_transform(&id));
    })
    .unwrap()
}

/// A lens added in a transaction is written into it: its config and blocks are
/// durable with the commit, and nothing reads its module path afterwards.
#[test]
fn a_lens_added_in_a_transaction_is_durable_with_its_commit() {
    workers::block_on(async {
        let (db, store) = node().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens.wasm");
        std::fs::write(&path, b"\0asm\x01\0\0\0").unwrap();
        let registry = DbTransactionRegistry::new(db.clone());
        let txn_id = registry.begin(false).await.unwrap();
        let id = registry
            .add_lens_in_txn(
                &txn_id,
                LensConfig::new(
                    "",
                    "",
                    LensModule::from_path(format!("file://{}", path.display())),
                ),
            )
            .await
            .unwrap();

        std::fs::remove_file(&path).unwrap();
        registry.commit(&txn_id).await.unwrap();

        let txn = db.new_txn(true).await.unwrap();
        assert!(
            txn.systemstore()
                .unwrap()
                .get(&LensConfigKey::new(id.to_string()).bytes())
                .await
                .unwrap()
                .is_some(),
            "the lens config commits with the transaction"
        );
        let root = cid::Cid::try_from(id.to_string().as_str()).unwrap();
        assert!(
            txn.blockstore()
                .unwrap()
                .get(&root.to_bytes())
                .await
                .unwrap()
                .is_some(),
            "the lens blocks commit with the transaction"
        );
        txn.discard().unwrap();
        assert_owed_until_used(&db, &store, &id).await;
    })
    .unwrap()
}
