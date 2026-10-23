use db::{BasicTxn, Collection};
use document::{DocID, Document, NormalValue};
use schema::{CollectionVersion, FieldDescription, FieldKind, QuerySource};
use storage::RegolithStore;

#[test]
fn concurrent_downsample_series_claims_have_one_winner() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut definition = CollectionVersion::new(
            "Target",
            "v1",
            "target",
            vec![FieldDescription::new(
                "1",
                "source_doc_id",
                FieldKind::string(),
            )],
        );
        definition.root_id = 1;
        definition.downsample_source = Some(QuerySource::new(serde_json::json!({})));
        let collection = Collection::new(definition);
        let make_doc = |seed: &str| {
            let mut doc = Document::new();
            doc.set_id(DocID::new_v0_from_seed(seed));
            doc.set("source_doc_id", NormalValue::String("shared-series".into()));
            doc
        };
        let first = BasicTxn::new(&store, 1, false).await.unwrap();
        let second = BasicTxn::new(&store, 2, false).await.unwrap();
        collection
            .create_with_datastore(&first.datastore(), &make_doc("first"), 1)
            .await
            .unwrap();
        collection
            .create_with_datastore(&second.datastore(), &make_doc("second"), 2)
            .await
            .unwrap();
        first.commit().await.unwrap();
        assert!(second.commit().await.is_err());
        let retry = BasicTxn::new(&store, 3, false).await.unwrap();
        assert!(matches!(
            collection
                .create_with_datastore(&retry.datastore(), &make_doc("second"), 2)
                .await,
            Err(db::Error::Storage(
                storage::Error::UniqueConstraintViolation
            ))
        ));
        retry.discard().unwrap();
    })
    .unwrap()
}
