//! An empty array reads back from storage without its element kind. A
//! document holding one must still pass validation when it is updated, and
//! rewriting an immutable field with the empty array it holds is not a change.

use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::RegolithStore;

#[test]
fn updating_a_document_that_holds_an_empty_string_array_succeeds() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let post = CollectionVersion::new(
            "Post",
            "v1",
            "col-post",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new(
                    "2",
                    "title",
                    FieldKind::string(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new(
                    "3",
                    "tags",
                    FieldKind::nillable_string_array(),
                    schema::CType::LwwRegister,
                ),
            ],
        );
        db.create_collection(post.clone()).await.unwrap();
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let mut doc = Document::new(post);
        doc.set("title", NormalValue::String("first".to_string()))
            .unwrap();
        doc.set("tags", NormalValue::NillableStringArray(Some(Vec::new())))
            .unwrap();
        let created = mutator.create("Post", doc).await.unwrap();

        // The plan's full read of the stored document, as an update that returns
        // the document does it, carries the stored empty array into validation.
        let expected = mutator
            .get_for_update("Post", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        let mut doc = expected.clone();
        doc.set("title", NormalValue::String("second".to_string()))
            .unwrap();
        mutator
            .update_if_unchanged(
                "Post",
                expected,
                doc,
                RapidHashSet::from_iter(["title".to_string()]),
            )
            .await
            .expect("an update of a document holding an empty array commits");

        let stored = mutator
            .get_for_update("Post", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.get("title"),
            Some(&NormalValue::String("second".to_string()))
        );
        let tags = stored.get("tags").expect("tags stays set");
        assert_eq!(
            document::encoding::normal_value_to_json(tags).unwrap(),
            serde_json::json!([])
        );
    })
    .unwrap()
}

#[test]
fn an_empty_array_read_from_storage_is_valid_for_every_array_kind() {
    for kind in [
        FieldKind::nillable_string_array(),
        FieldKind::string_array(),
        FieldKind::ScalarArray(schema::ScalarArrayKind::NillableIntArray),
        FieldKind::ScalarArray(schema::ScalarArrayKind::BoolArray),
    ] {
        let collection = db::Collection::new(
            CollectionVersion::new(
                "Post",
                "v1",
                "col-post",
                vec![
                    FieldDescription::new(
                        "1",
                        "_docID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new("2", "values", kind.clone(), schema::CType::LwwRegister),
                ],
            ),
            crdt::Crdts::default(),
        );
        let mut doc = Document::new(collection.schema_arc());
        doc.set("values", NormalValue::JsonArray(Vec::new()))
            .unwrap();
        collection
            .validate_document(&doc)
            .unwrap_or_else(|error| panic!("{kind:?}: {error}"));
    }
}

/// A document whose immutable JSON `tags` holds an empty array (only scalar
/// kinds can be immutable, and JSON is one), and a copy of it read for update
/// with `tags` set to `tags`.
async fn immutable_tags_update(
    tags: NormalValue,
) -> (AutoCommitMutator<RegolithStore>, document::DocID, Document) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let post = CollectionVersion::new(
        "Post",
        "v1",
        "col-post",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "title",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("3", "tags", FieldKind::json(), schema::CType::LwwRegister)
                .as_immutable(),
        ],
    );
    db.create_collection(post.clone()).await.unwrap();
    let mutator = AutoCommitMutator::new(Arc::clone(&db));
    let mut doc = Document::new(post);
    doc.set("title", NormalValue::String("first".to_string()))
        .unwrap();
    doc.set("tags", NormalValue::JsonArray(Vec::new())).unwrap();
    let created = mutator.create("Post", doc).await.unwrap();
    let mut doc = mutator
        .get_for_update("Post", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    doc.set("title", NormalValue::String("second".to_string()))
        .unwrap();
    doc.set("tags", tags).unwrap();
    (mutator, created.doc_id, doc)
}

#[test]
fn setting_an_immutable_empty_array_to_an_empty_array_is_not_a_change() {
    workers::block_on(async {
        let (mutator, doc_id, doc) =
            immutable_tags_update(NormalValue::JsonArray(Vec::new())).await;

        mutator
            .update(
                "Post",
                doc,
                RapidHashSet::from_iter(["title".to_string(), "tags".to_string()]),
            )
            .await
            .expect("rewriting an immutable field with the value it holds commits");

        let stored = mutator
            .get_for_update("Post", &doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.get("title"),
            Some(&NormalValue::String("second".to_string()))
        );
        assert_eq!(
            document::encoding::normal_value_to_json(stored.get("tags").unwrap()).unwrap(),
            serde_json::json!([])
        );
    })
    .unwrap()
}

#[test]
fn changing_an_immutable_empty_array_is_refused() {
    workers::block_on(async {
        let (mutator, doc_id, doc) =
            immutable_tags_update(NormalValue::JsonArray(vec![serde_json::json!("new")])).await;

        let error = mutator
            .update(
                "Post",
                doc,
                RapidHashSet::from_iter(["title".to_string(), "tags".to_string()]),
            )
            .await
            .expect_err("a real change of an immutable field is refused");
        assert!(
            error
                .to_string()
                .contains("immutable field 'tags' cannot be changed"),
            "{error}"
        );

        let stored = mutator
            .get_for_update("Post", &doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.get("title"),
            Some(&NormalValue::String("first".to_string()))
        );
    })
    .unwrap()
}
