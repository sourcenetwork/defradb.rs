use std::sync::Arc;

use db::DB;
use storage::RegolithStore;

const SDL: &str = "type Article { title: String body: String @chunk(maxChars: 64) }";
const CHUNKS: &str = "Article__body_chunk";

async fn fresh() -> Arc<DB<RegolithStore>> {
    let (db, _bus) = crate::common::fixture::make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    db
}

async fn version_id(db: &Arc<DB<RegolithStore>>, name: &str) -> String {
    db.get_all_collection_versions()
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.name == name && v.is_active)
        .expect("active version")
        .version_id
}

#[test]
fn a_chunk_version_is_refused_while_its_parent_lives() {
    workers::block_on(async {
        let db = fresh().await;
        let chunks = version_id(&db, CHUNKS).await;

        let err = db
            .delete_collection_versions_batch(vec![chunks.clone()])
            .await
            .unwrap_err();
        assert!(
            matches!(err, db::Error::DeleteGeneratedCollection(_)),
            "{err}"
        );

        let err = db
            .delete_collection_version(&chunks, &[])
            .await
            .unwrap_err();
        assert!(
            matches!(err, db::Error::DeleteGeneratedCollection(_)),
            "{err}"
        );
        assert!(db.get_collection(CHUNKS).unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn deleting_the_parent_version_removes_its_chunk_collection() {
    workers::block_on(async {
        let db = fresh().await;
        let article = version_id(&db, "Article").await;

        db.delete_collection_versions_batch(vec![article])
            .await
            .unwrap();
        assert!(db.get_collection("Article").unwrap().is_none());
        assert!(db.get_collection(CHUNKS).unwrap().is_none());

        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        assert!(db.get_collection(CHUNKS).unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn deleting_a_single_parent_version_removes_its_chunk_collection() {
    workers::block_on(async {
        let db = fresh().await;
        let article = version_id(&db, "Article").await;

        db.delete_collection_version(&article, &[]).await.unwrap();
        assert!(db.get_collection(CHUNKS).unwrap().is_none());
    })
    .unwrap()
}
