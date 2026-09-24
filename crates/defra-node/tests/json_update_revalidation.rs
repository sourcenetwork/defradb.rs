use anyhow::{Context, Result};
use defra_node::{EmbeddedNode, ExecuteRetryPolicy, QueryRequest};
use serde_json::{json, Value};

#[tokio::test]
async fn retained_json_survives_unrelated_updates() -> Result<()> {
    let node = EmbeddedNode::builder().build().await?;
    node.add_schema(
        "type JsonProbe { key: String @index(unique: true) payload: JSON sibling: String }",
    )
    .await?;
    let payloads = [
        json!({"arbitrary-key": []}),
        json!([{"name":"model", "efforts":null}]),
        json!([]),
        json!([1, 2]),
        json!(["one", "two"]),
        json!(true),
        json!(42),
        json!("literal"),
        json!([1, "two", false, null, {"nested":[]}]),
        json!([true, null, false]),
        json!([1, null, 2]),
        json!([1.5, null, 2.5]),
        json!(["one", null]),
        Value::Null,
    ];
    for (index, payload) in payloads.into_iter().enumerate() {
        let created = node.execute_request_with_retry(
            QueryRequest::new("mutation($key:String!,$payload:JSON){create_JsonProbe(input:{key:$key,payload:$payload,sibling:\"before\"}){_docID}}")
                .with_variables(json!({"key":index.to_string(),"payload":payload})),
            ExecuteRetryPolicy::default(),
        ).await;
        assert!(
            !created.has_errors(),
            "create {payload}: {:?}",
            created.errors
        );
        let query = format!(
            "{{ JsonProbe(filter:{{key:{{_eq:\"{index}\"}}}}){{_docID payload sibling}} }}"
        );
        let before = node.execute(&query).await;
        assert!(!before.has_errors(), "read {payload}: {:?}", before.errors);
        let row = &before.data.as_ref().context("missing data")?["JsonProbe"][0];
        let id = row["_docID"].as_str().context("missing document ID")?;
        let changed = node
            .execute(&format!(
                "mutation{{update_JsonProbe(docID:\"{id}\",input:{{sibling:\"after\"}}){{_docID}}}}"
            ))
            .await;
        assert!(
            !changed.has_errors(),
            "update retaining {payload}: {:?}",
            changed.errors
        );
        let after = node.execute(&query).await;
        assert!(
            !after.has_errors(),
            "read updated {payload}: {:?}",
            after.errors
        );
        let after = &after.data.as_ref().context("missing updated data")?["JsonProbe"][0];
        assert_eq!(
            after["payload"], row["payload"],
            "changed retained JSON {payload}"
        );
        assert_eq!(after["sibling"], "after");
    }
    node.shutdown().await;
    Ok(())
}
