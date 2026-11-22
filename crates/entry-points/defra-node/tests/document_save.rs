use defra_core::RequestContext;
use defra_node::Node;
use query::documents::AddOptions;
use serde_json::json;

async fn users() -> Node {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type Users { name: String }")
        .await
        .unwrap();
    node
}

#[test]
fn saving_a_deleted_document_id_refuses() {
    workers::block_on(async {
        let node = users().await;
        let context = RequestContext::ambient();
        let options = AddOptions::default();
        let doc_id = node
            .save_doc(
                &context,
                "Users",
                None,
                &json!({ "name": "John" }),
                &options,
            )
            .await
            .unwrap();
        node.delete_doc(&context, "Users", &doc_id).await.unwrap();

        let error = node
            .save_doc(
                &context,
                "Users",
                Some(&doc_id),
                &json!({ "name": "Fred" }),
                &options,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("a document with the given ID has been deleted. DocID: {doc_id}")
        );

        let users = node.execute("query { Users { name } }").await;
        assert!(!users.has_errors(), "{:?}", users.errors);
        assert_eq!(users.data.unwrap()["Users"], json!([]));
    })
    .unwrap()
}

#[test]
fn saving_many_to_a_deleted_document_id_refuses() {
    workers::block_on(async {
        let node = users().await;
        let context = RequestContext::ambient();
        let options = AddOptions::default();
        let doc_id = node
            .add_docs(&context, "Users", &[json!({ "name": "John" })], &options)
            .await
            .unwrap()
            .remove(0);
        node.delete_doc(&context, "Users", &doc_id).await.unwrap();

        let error = node
            .save_docs(
                &context,
                "Users",
                &[
                    json!({ "name": "Bob" }),
                    json!({ "_docID": doc_id, "name": "Fred" }),
                ],
                &options,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("a document with the given ID has been deleted. DocID: {doc_id}")
        );

        let users = node.execute("query { Users { name } }").await;
        assert!(!users.has_errors(), "{:?}", users.errors);
        assert_eq!(users.data.unwrap()["Users"], json!([]));
    })
    .unwrap()
}
