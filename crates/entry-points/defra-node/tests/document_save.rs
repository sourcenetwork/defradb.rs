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
        let context = node.anonymous();
        let options = AddOptions::default();
        let doc_id = context
            .save_doc("Users", None, &json!({ "name": "John" }), &options)
            .await
            .unwrap();
        context.delete_doc("Users", &doc_id).await.unwrap();

        let error = context
            .save_doc("Users", Some(&doc_id), &json!({ "name": "Fred" }), &options)
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
        let context = node.anonymous();
        let options = AddOptions::default();
        let doc_id = context
            .add_docs("Users", &[json!({ "name": "John" })], &options)
            .await
            .unwrap()
            .remove(0);
        context.delete_doc("Users", &doc_id).await.unwrap();

        let error = context
            .save_docs(
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
