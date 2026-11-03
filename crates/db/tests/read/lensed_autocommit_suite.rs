use db::read::lensed::autocommit::LensedAutoCommitFetcher;
use lens::TargetedHistoryLink;
use rapidhash::RapidHashMap;
use std::sync::Arc;

#[test]
fn unknown_document_version_passes_through() {
    workers::block_on(async {
        let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let fetcher = LensedAutoCommitFetcher::new(db.clone());

        let collection = db::Collection::new(
            schema::CollectionVersion::new(
                "Users",
                "v2",
                "users-collection",
                vec![schema::FieldDescription::new(
                    "1",
                    "name",
                    schema::FieldKind::string(),
                    schema::CType::LwwRegister,
                )],
            ),
            crdt::Crdts::default(),
        );
        let history = Some(RapidHashMap::from_iter([
            (
                "v1".to_string(),
                TargetedHistoryLink::new("v1", "users-collection").with_next("v2"),
            ),
            (
                "v2".to_string(),
                TargetedHistoryLink::new("v2", "users-collection")
                    .with_transform(Some("transform-v1-v2".to_string()))
                    .with_previous("v1"),
            ),
        ]));

        let mut doc = document::Document::new(collection.schema_arc());
        doc.set("name", "Alice").unwrap();
        doc.set_collection_version_id("foreign-v3");

        let returned = fetcher
            .process_document(doc, &collection, true, &history)
            .await
            .unwrap();
        assert_eq!(
            returned
                .document
                .get("name")
                .and_then(|value| value.as_str()),
            Some("Alice")
        );
        assert_eq!(
            returned.document.collection_version_id(),
            Some("foreign-v3")
        );
        assert!(returned.source_document.is_none());
    })
    .unwrap()
}
