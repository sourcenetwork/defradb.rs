//! A lens is found by a request that reads its version from storage between
//! the commit landing and that commit's ordered publication, which an earlier
//! commit still publishing holds back.

use std::sync::Arc;

use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use futures::TryStreamExt;
use lens::{LensConfig, LensDoc, LensDocStream, LensModule, TransformId, TransformStore};
use query::txn::TransactionRegistry;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use regolith::sync::Event;
use storage::corekv::Key;
use storage::corekv::{IterOptions, OnCommit};
use storage::keys::systemstore::{CollectionKey, LensConfigKey};
use storage::RegolithStore;

use crate::common::gated_lens_store::GatedStore;

type Db = db::DB<RegolithStore>;

/// A database with `Users { name }` over a gated lens store.
async fn node() -> (Arc<Db>, Arc<GatedStore>) {
    let store = GatedStore::new();
    let mut raw = Db::new(RegolithStore::in_memory().unwrap()).unwrap();
    raw.set_lens_store(store.clone());
    raw.add_collection(
        &defra_core::RequestContext::ambient(),
        "type Users { name: String }",
    )
    .await
    .unwrap();
    (Arc::new(raw), store)
}

fn runner(
    db: &Arc<Db>,
) -> QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>> {
    QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(db)),
        DbCollectionProvider::new_arc(Arc::clone(db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(db))))
}

fn config(source: &str, destination: &str) -> LensConfig {
    LensConfig::new(
        source,
        destination,
        LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
    )
}

/// An earlier commit whose ordered hook holds every later commit's ordered
/// publication until `release` is set; `entered` is set once it holds.
async fn earlier_commit(
    db: &Db,
    entered: &Arc<Event>,
    release: &Arc<Event>,
) -> db::txn::DbTxn<RegolithStore> {
    let earlier = db.new_txn(false).await.unwrap();
    earlier
        .systemstore()
        .unwrap()
        .set(b"earlier", b"commit")
        .await
        .unwrap();
    let (hook_entered, hook_release) = (Arc::clone(entered), Arc::clone(release));
    earlier
        .on_commit(OnCommit::OrderedAsync(Box::new(move || {
            Box::pin(async move {
                hook_entered.set();
                hook_release.wait().await;
                Ok(())
            })
        })))
        .unwrap();
    earlier
}

/// The lenses a fresh read of storage finds committed.
async fn committed_lens_ids(db: &Db) -> Vec<TransformId> {
    let txn = db.new_txn(true).await.unwrap();
    let mut ids = Vec::new();
    {
        let systemstore = txn.systemstore().unwrap();
        let options = IterOptions::new().with_prefix(LensConfigKey::prefix());
        let mut pairs = systemstore.iterator(options).await.unwrap();
        while let Some(pair) = pairs.next().await.unwrap() {
            let key = String::from_utf8_lossy(&pair.key).into_owned();
            ids.push(TransformId::new(
                key.strip_prefix("/lens/config/").unwrap_or(&key),
            ));
        }
        pairs.close().await.unwrap();
    }
    txn.discard().unwrap();
    ids
}

/// The transform a fresh read of storage finds on `version`.
async fn stored_transform(db: &Db, version: &str) -> Option<TransformId> {
    let txn = db.new_txn(true).await.unwrap();
    let stored = txn
        .systemstore()
        .unwrap()
        .get(&CollectionKey::new(version).bytes())
        .await
        .unwrap();
    txn.discard().unwrap();
    let schema: schema::CollectionVersion = serde_json::from_slice(&stored?).unwrap();
    schema
        .previous_version
        .and_then(|source| source.transform)
        .map(TransformId::new)
}

async fn run(db: &Db, id: &TransformId) -> lens::Result<Vec<LensDoc>> {
    let mut doc = LensDoc::new();
    doc.insert("name".to_string(), serde_json::json!("Ada"));
    let docs: LensDocStream = Box::pin(futures::stream::iter(vec![doc]));
    db.lens_store().transform(id, docs)?.try_collect().await
}

/// A query that reads the migration from storage after the explicit
/// transaction's commit lands, while that commit's ordered publication is held
/// back and its registration is still compiling, migrates the document.
#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn a_read_between_a_migration_commit_and_its_ordered_publication_finds_the_transform() {
    workers::block_on(async {
        let (db, store) = node().await;
        let runner = runner(&db);
        let created = runner
            .execute(QueryRequest::new(
                r#"mutation { add_Users(input: {name: "Ada"}) { _docID } }"#,
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let v1 = db
            .require_collection("Users")
            .unwrap()
            .version_id()
            .to_string();
        db.patch_collection(
            &defra_core::RequestContext::ambient(),
            r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"age","Kind":"Int"}}]"#,
            None,
        )
        .await
        .unwrap();
        let v2 = db
            .require_collection("Users")
            .unwrap()
            .version_id()
            .to_string();
        let registry = DbTransactionRegistry::new(db.clone());
        let txn_id = registry.begin(false).await.unwrap();
        let id = registry
            .set_migration_in_txn(&txn_id, config(&v1, &v2), None)
            .await
            .unwrap();

        let (entered, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let earlier = earlier_commit(&db, &entered, &release).await;
        let migration = async {
            entered.wait().await;
            registry.commit(&txn_id).await
        };
        let reader = async {
            entered.wait().await;
            while stored_transform(&db, &v2).await.as_ref() != Some(&id) {
                workers::yield_now().await;
            }
            assert!(
                !db.has_migration(&id),
                "the commit's ordered publication has not owed the lens yet"
            );
            let response = runner
                .execute(QueryRequest::new("query { Users { name } }"))
                .await;
            release.set();
            store.release();
            response
        };
        let (earlier, migrated, response) = futures::join!(earlier.commit(), migration, reader);
        earlier.unwrap();
        migrated.unwrap();
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(
            response.data,
            Some(serde_json::json!({ "Users": [{ "name": "Ada" }] }))
        );
        assert!(db.has_migration(&id));
    })
    .unwrap()
}

/// A lens committed by a patch runs for a request that reads it from storage
/// while the patch's ordered publication is held back.
#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn a_read_between_a_patch_commit_and_its_ordered_publication_finds_the_transform() {
    workers::block_on(async {
        let (db, store) = node().await;
        let (entered, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let earlier = earlier_commit(&db, &entered, &release).await;
        let patch = async {
            entered.wait().await;
            db.patch_collection(
                &defra_core::RequestContext::ambient(),
                r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"age","Kind":"Int"}}]"#,
                Some(config("", "")),
            )
            .await
        };
        let reader = async {
            entered.wait().await;
            let id = loop {
                if let Some(id) = committed_lens_ids(&db).await.pop() {
                    break id;
                }
                workers::yield_now().await;
            };
            assert!(
                !db.has_migration(&id),
                "the patch's ordered publication has not owed the lens yet"
            );
            let docs = run(&db, &id)
                .await
                .expect("a request that reads the version from storage finds its transform");
            assert_eq!(docs.len(), 1);
            release.set();
            store.release();
            id
        };
        let (earlier, patched, id) = futures::join!(earlier.commit(), patch, reader);
        earlier.unwrap();
        patched.unwrap();
        assert!(store.inner.has_transform(&id));
    })
    .unwrap()
}

/// A migration whose commit fails leaves no trace of its lens, and does not
/// take away the lens of the commit it lost to.
#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn a_failed_migration_commit_leaves_its_lens_unknown() {
    workers::block_on(async {
        let (db, store) = node().await;
        store.release();
        let v1 = db
            .require_collection("Users")
            .unwrap()
            .version_id()
            .to_string();
        let registry = DbTransactionRegistry::new(db.clone());
        let winner = registry.begin(false).await.unwrap();
        let loser = registry.begin(false).await.unwrap();
        let won = registry
            .set_migration_in_txn(&winner, config(&v1, "users-v2"), None)
            .await
            .unwrap();
        let mut other = config(&v1, "users-v2");
        other.lenses[0].arguments = Some(serde_json::json!({ "variant": 2 }));
        let lost = registry
            .set_migration_in_txn(&loser, other, None)
            .await
            .unwrap();
        assert_ne!(won, lost);

        registry.commit(&winner).await.unwrap();
        registry
            .commit(&loser)
            .await
            .expect_err("both wrote the destination version, so one commit loses");

        assert!(db.has_migration(&won));
        assert_eq!(run(&db, &won).await.unwrap().len(), 1);
        assert!(!db.has_migration(&lost));
        let error = run(&db, &lost)
            .await
            .expect_err("nothing committed the lens");
        assert!(error.to_string().contains("transform not found"), "{error}");
    })
    .unwrap()
}

/// Two open transactions carry the same lens; one commits and the other
/// fails while the winner's ordered publication is held. The loser drops only
/// its own hold, so a read still finds the winner's compiled copy.
#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.transform_with_version")]
fn a_failed_commit_of_one_lens_keeps_the_other_commits_hold() {
    workers::block_on(async {
        let (db, store) = node().await;
        let v1 = db
            .require_collection("Users")
            .unwrap()
            .version_id()
            .to_string();
        let registry = DbTransactionRegistry::new(db.clone());
        let winner = registry.begin(false).await.unwrap();
        let loser = registry.begin(false).await.unwrap();
        let won = registry
            .set_migration_in_txn(&winner, config(&v1, "users-v2"), None)
            .await
            .unwrap();
        let lost = registry
            .set_migration_in_txn(&loser, config(&v1, "users-v2"), None)
            .await
            .unwrap();
        assert_eq!(won, lost);

        let (entered, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let earlier = earlier_commit(&db, &entered, &release).await;
        let commits = async {
            entered.wait().await;
            futures::join!(registry.commit(&winner), registry.commit(&loser))
        };
        let reader = async {
            entered.wait().await;
            while stored_transform(&db, "users-v2").await.as_ref() != Some(&won) {
                workers::yield_now().await;
            }
            assert!(
                !db.has_migration(&won),
                "the winning commit's ordered publication is still held"
            );
            let docs = run(&db, &won)
                .await
                .expect("the failed commit must not drop the winner's hold");
            release.set();
            store.release();
            docs.len()
        };

        let (earlier, commits, docs) = futures::join!(earlier.commit(), commits, reader);
        earlier.unwrap();
        let outcomes = [commits.0.is_ok(), commits.1.is_ok()];
        assert_eq!(
            outcomes.into_iter().filter(|ok| *ok).count(),
            1,
            "{commits:?}"
        );
        assert_eq!(docs, 1);
        assert!(store.inner.has_transform(&won));
    })
    .unwrap()
}
