#[tokio::test]
#[should_panic(expected = "not a final successful write")]
async fn verification_rejects_an_acknowledged_update_that_was_not_persisted() {
    use super::{verify, Fixture, Scenario};

    let fixture = Fixture::new(Scenario::SharedUpdates).await;
    verify::check(&fixture, Scenario::SharedUpdates, &[(0, 0)]).await;
}

#[tokio::test]
#[should_panic(expected = "not a final successful write")]
async fn verification_rejects_an_increment_without_its_label_update() {
    use super::{verify, Fixture, Scenario};
    use query::QueryRequest;

    let fixture = Fixture::new(Scenario::CountersIndexes).await;
    let response = fixture
        .state
        .executor
        .execute(QueryRequest::new(format!(
            r#"mutation {{ update_Sample(docID: "{}", input: {{count: 1}}) {{ _docID }} }}"#,
            fixture.ids[0],
        )))
        .await;
    assert!(response.errors.is_empty());
    verify::check(&fixture, Scenario::CountersIndexes, &[(0, 0)]).await;
}

#[tokio::test]
#[should_panic(expected = "no obsolete label entries remain")]
async fn verification_rejects_an_obsolete_index_entry() {
    use document::{DocID, NormalValue};

    use super::{verify, Fixture, Scenario, COLLECTION};

    let fixture = Fixture::new(Scenario::CountersIndexes).await;
    let collection = fixture.db.get_collection(COLLECTION).unwrap().unwrap();
    let indexes = db::index::IndexManager::from_collection(
        collection.resolved_root_id(),
        collection.schema(),
    )
    .unwrap();
    let txn = fixture.db.new_txn(false).await.unwrap();
    {
        let systemstore = txn.systemstore().unwrap();
        let id = DocID::from_string(&fixture.ids[0]).unwrap();
        let short_id = collection
            .resolve_doc_short_id(&systemstore, &id)
            .await
            .unwrap()
            .unwrap();
        let mut datastore = txn.datastore().unwrap();
        indexes
            .get_index("by_label")
            .unwrap()
            .save(
                &mut datastore,
                short_id,
                &[NormalValue::String("obsolete".into())],
            )
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    verify::check(&fixture, Scenario::CountersIndexes, &[]).await;
}
