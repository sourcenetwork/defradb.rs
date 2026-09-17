//! The view's document lookups read what they need, not the collection.

use super::*;

impl Node {
    /// Overwrite a document's stored blob so that any read of it fails, which
    /// makes a collection scan fail and leaves reads of other documents alone.
    async fn corrupt(&self, collection: &str, doc_id: &str) {
        let collection = self.db.get_collection(collection).unwrap().unwrap();
        let txn = self.db.new_txn(false).await.unwrap();
        {
            let short_id = collection
                .resolve_doc_short_id(&txn.systemstore().unwrap(), &doc_id.parse().unwrap())
                .await
                .unwrap()
                .unwrap();
            txn.datastore()
                .unwrap()
                .set(&collection.doc_key(short_id), b"not a document")
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
    }
}

#[tokio::test]
async fn immutable_fields_reads_one_document_not_the_collection() {
    let validator = Arc::new(ReadImmutable {
        collection: "Grants",
        doc_id: Mutex::new(String::new()),
        read: Mutex::new(None),
    });
    let node = Node::with_immutable_grants(validator.clone()).await;
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    let bystander = genesis("col-grants", "writer", "someone else", &writer);
    assert_eq!(
        bystander.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );
    node.corrupt("Grants", &bystander.doc_id).await;
    assert!(AutoCommitFetcher::new(node.db.clone())
        .get_all("Grants")
        .await
        .is_err());

    *validator.doc_id.lock().unwrap() = grant.doc_id.clone();
    let note = genesis("col-notes", "grant", "anything", &writer);
    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(
        validator.read.lock().unwrap().clone().unwrap(),
        Some(vec![(
            "writer".to_string(),
            NormalValue::String(writer.did.clone())
        )])
    );
}
