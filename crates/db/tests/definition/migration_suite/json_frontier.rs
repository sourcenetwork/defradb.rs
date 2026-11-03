//! A lens that rewrites a `@crdt(type: json)` field reaches the field's
//! frontier only through a block: the migration write-back leaves the
//! document at its old version, and the next write carries the change as ops.

use super::*;
use serde_json::json;

async fn json_node() -> (Arc<DB<RegolithStore>>, String) {
    let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    raw_db.set_lens_store(Arc::new(SetVerifiedStore {
        verified_value: Some(json!({"by": "lens"})),
        ..Default::default()
    }));
    raw_db
        .create_collections_atomic(
            query::parse_sdl("type Users { name: String verified: JSON @crdt(type: json) }")
                .unwrap(),
        )
        .await
        .unwrap();
    let db = Arc::new(raw_db);
    let v1 = db
        .get_collection("Users")
        .unwrap()
        .unwrap()
        .version_id()
        .to_owned();
    (db, v1)
}

fn verified(doc: &document::Document) -> Option<serde_json::Value> {
    match doc.get("verified") {
        Some(document::NormalValue::Json(value)) => Some(value.clone()),
        _ => None,
    }
}

#[test]
fn lens_rewrite_of_a_json_field_lands_through_the_next_write() {
    workers::block_on(async {
        let (db, v1) = json_node().await;
        let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
        let mut doc = document::Document::new(users);
        doc.set("name", document::NormalValue::String("Alice".to_string()))
            .unwrap();
        doc.set("verified", document::NormalValue::Json(json!({"a": 1})))
            .unwrap();
        let doc_id = AutoCommitMutator::new(db.clone())
            .create("Users", doc)
            .await
            .unwrap()
            .doc_id;
        let v2 = add_placeholder_version(&db, "note").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let before = load_user_blob(&db, &doc_id).await;
        assert_eq!(
            db.materialize_collection("Users").await.unwrap(),
            0,
            "a lens change to a tagged field is not cached"
        );
        assert_eq!(load_user_blob(&db, &doc_id).await, before);
        let stored = load_user(&db, &doc_id).await;
        assert_eq!(stored.collection_version_id(), Some(v1.as_str()));
        assert_eq!(verified(&stored), Some(json!({"a": 1})));

        let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
        let mut update = document::Document::new(users);
        update.set_id(doc_id.clone());
        update
            .set("name", document::NormalValue::String("Bob".into()))
            .unwrap();
        AutoCommitMutator::new(db.clone())
            .update(
                "Users",
                update,
                RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();

        let written = load_user(&db, &doc_id).await;
        assert_eq!(written.collection_version_id(), Some(v2.as_str()));
        assert_eq!(
            verified(&written),
            Some(json!({"by": "lens"})),
            "the write that moved the document carried the lens change into the frontier"
        );
        assert_eq!(
            written.get("name"),
            Some(&document::NormalValue::String("Bob".into()))
        );
    })
    .unwrap()
}
