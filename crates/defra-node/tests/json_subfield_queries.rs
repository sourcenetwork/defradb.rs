use defra_node::Result;
use defra_node::{Node, QueryRequest};
use serde_json::json;

async fn json_blob_node(sdl: &str) -> Result<Node> {
    let node = Node::builder().build().await?;
    node.add_collection(sdl).await?;
    for payload in [
        json!({"i": {"love": "eating my family and commas"}}),
        json!({"i": {"love": "nothing"}}),
        json!({"other": 1}),
    ] {
        let created = node
            .execute_request(
                QueryRequest::new(
                    "mutation($payload:JSON){add_jsonBlob(input:{jsonField:$payload}){_docID}}",
                )
                .with_variables(json!({"payload": payload})),
            )
            .await;
        assert!(!created.has_errors(), "create: {:?}", created.errors);
    }
    Ok(node)
}

#[test]
fn json_sub_path_filter_matches_go() -> Result<()> {
    workers::block_on(async {
        for sdl in [
            "type jsonBlob { jsonField: JSON @index }",
            "type jsonBlob { jsonField: JSON }",
        ] {
            let node = json_blob_node(sdl).await?;
            let response = node
                .execute(
                    r#"{ jsonBlob(filter: {jsonField: {i: {love: {_like: "%family%"}}}}) { jsonField } }"#,
                )
                .await;
            assert!(!response.has_errors(), "{sdl}: {:?}", response.errors);
            assert_eq!(
                response.data.expect("missing data")["jsonBlob"],
                json!([{"jsonField": {"i": {"love": "eating my family and commas"}}}]),
                "{sdl}"
            );
            node.shutdown().await?;
        }
        Ok(())
    })
    .unwrap()
}

#[test]
fn json_sub_selection_is_rejected_like_go() -> Result<()> {
    workers::block_on(async {
        let node = json_blob_node("type jsonBlob { jsonField: JSON @index }").await?;
        let response = node
            .execute("{ jsonBlob { jsonField { i { love } } } }")
            .await;
        let messages: Vec<&str> = response
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect();
        assert_eq!(
            messages,
            ["Field \"jsonField\" of type \"JSON\" must not have a sub selection."]
        );
        assert!(response.data.is_none());
        node.shutdown().await?;
        Ok(())
    })
    .unwrap()
}
