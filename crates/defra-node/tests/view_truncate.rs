//! Truncating a materialized view empties its cache, and a refresh fills it again.

use defra_core::RequestContext;
use defra_node::Node;

async fn names(node: &Node) -> serde_json::Value {
    let response = node.execute("{ UserView { name } }").await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["UserView"].clone()
}

#[test]
fn truncating_a_view_clears_its_cache_until_the_next_refresh() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let context = RequestContext::ambient();
        node.add_collection("type User { name: String }").await.unwrap();
        node.add_view("User { name }", "type UserView { name: String }")
            .await
            .unwrap();
        let response = node
            .execute(r#"mutation { add_User(input: {name: "Ada"}) { _docID } }"#)
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);

        node.refresh_views_as(&context, db::CollectionSelector::all())
            .await
            .unwrap();
        assert_eq!(names(&node).await, serde_json::json!([{ "name": "Ada" }]));

        node.truncate_collection_as(&context, "UserView", None)
            .await
            .unwrap();
        assert_eq!(names(&node).await, serde_json::json!([]));

        node.refresh_views_as(&context, db::CollectionSelector::all())
            .await
            .unwrap();
        assert_eq!(names(&node).await, serde_json::json!([{ "name": "Ada" }]));
    })
    .unwrap()
}
