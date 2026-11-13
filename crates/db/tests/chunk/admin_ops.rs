use std::sync::Arc;

use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use db::IndexManager;
use defra_core::RequestContext;
use lens::{LensConfig, LensModule};
use query::txn::TransactionRegistry;
use schema::IndexedFieldDescription;
use storage::RegolithStore;

use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

const CHUNKS: &str = "Article__body_chunk";

const REFUSED: &str =
    "Article__body_chunk holds the chunks of Article.body and is changed only through Article";

async fn chunked_db() -> Arc<DB<RegolithStore>> {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .expect("schema");
    db
}

fn field(name: &str) -> IndexedFieldDescription {
    IndexedFieldDescription {
        name: name.to_string(),
        descending: false,
    }
}

fn chunk_version(db: &DB<RegolithStore>) -> schema::CollectionVersion {
    db.get_collection(CHUNKS).unwrap().unwrap().schema().clone()
}

fn assert_refused<T, E>(result: Result<T, E>)
where
    E: std::fmt::Display,
{
    let error = result.err().expect("refused").to_string();
    assert!(error.contains(REFUSED), "{error}");
}

#[test]
fn index_create_on_a_chunk_collection_is_refused() {
    workers::block_on(async {
        let db = chunked_db().await;
        let fields = vec![field("hash")];
        let kind = IndexManager::requested_kind(&fields, true, None).unwrap();
        assert_refused(db.create_index(CHUNKS, None, fields, kind).await);
        assert_refused(
            db.ensure_ordinary_index(CHUNKS, "by_text", vec![field("text")])
                .await,
        );
        assert_eq!(chunk_version(&db).indexes.len(), 1);
    })
    .unwrap()
}

#[test]
fn index_drop_on_a_chunk_collection_is_refused_and_keeps_the_index() {
    workers::block_on(async {
        let db = chunked_db().await;
        let generated = chunk_version(&db).indexes[0].name.clone();
        assert_refused(db.drop_index(CHUNKS, &generated).await);
        let indexes = chunk_version(&db).indexes;
        assert_eq!(indexes.len(), 1);
        assert_eq!(indexes[0].name, generated);
    })
    .unwrap()
}

#[test]
fn the_parent_still_takes_index_changes() {
    workers::block_on(async {
        let db = chunked_db().await;
        let fields = vec![field("title")];
        let kind = IndexManager::requested_kind(&fields, false, None).unwrap();
        let index = db
            .create_index("Article", None, fields, kind)
            .await
            .expect("an index on the parent");
        db.drop_index("Article", &index.name)
            .await
            .expect("a drop on the parent");
    })
    .unwrap()
}

#[test]
fn truncating_a_chunk_collection_is_refused() {
    workers::block_on(async {
        let db = chunked_db().await;
        assert_refused(
            db.truncate_collection_as(&RequestContext::ambient(), CHUNKS, None)
                .await,
        );
        db.truncate_collection_as(&RequestContext::ambient(), "Article", None)
            .await
            .expect("a truncate of the parent");
    })
    .unwrap()
}

#[test]
fn switching_a_chunk_collection_version_is_refused() {
    workers::block_on(async {
        let db = chunked_db().await;
        let version = chunk_version(&db).version_id;
        assert_refused(db.set_active_collection_version(&version).await);

        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin(false).await.unwrap();
        assert_refused(
            registry
                .set_collection_active_in_txn(handle.as_str(), &version, false)
                .await,
        );
        registry.discard(&handle).await.unwrap();
        assert!(db.get_collection(CHUNKS).unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn a_migration_from_or_to_a_chunk_collection_is_refused() {
    workers::block_on(async {
        let db = chunked_db().await;
        let chunks = chunk_version(&db).version_id;
        let article = db
            .get_collection("Article")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let lens = || LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec());
        assert_refused(
            db.set_migration(
                LensConfig::new(&chunks, "bafyreinotachunkversion", lens()),
                None,
            )
            .await,
        );
        assert_refused(
            db.set_migration(LensConfig::new(&article, &chunks, lens()), None)
                .await,
        );
    })
    .unwrap()
}

#[test]
fn a_chunk_collection_is_deleted_only_with_its_parent() {
    workers::block_on(async {
        let db = chunked_db().await;
        assert_refused(db.delete_collections(vec![CHUNKS.into()], false).await);
        assert_refused(db.delete_collection(CHUNKS).await);
        let version = chunk_version(&db).version_id;
        assert_refused(db.delete_collection_version(&version, &[]).await);

        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin(false).await.unwrap();
        assert_refused(
            registry
                .delete_collections_in_txn(handle.as_str(), vec![CHUNKS.into()], false)
                .await,
        );
        registry.discard(&handle).await.unwrap();
        assert!(db.get_collection(CHUNKS).unwrap().is_some());

        db.delete_collections(vec!["Article".into(), CHUNKS.into()], false)
            .await
            .expect("the parent and its chunks together");
        assert!(db.get_collection(CHUNKS).unwrap().is_none());
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .expect("the schema again");
    })
    .unwrap()
}
