use crate::Node;

/// The node open path persists collections whose short IDs sit on the storage
/// byte boundary, and reloads their indexed rows separately after a restart.
#[test]
fn a_node_persists_collections_at_boundary_short_ids() {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let data_path = dir.path().join("node");

            let node = Node::builder()
                .data_path(&data_path)
                .build()
                .await
                .expect("build node");
            for number in 1..=118 {
                node.add_collection(
                    &defra_core::RequestContext::ambient(),
                    &format!("type Tally{number} {{ count: Int @index }}"),
                )
                .await
                .expect("create boundary collection");
            }
            node.execute("mutation { add_Tally100(input: {count: 100}) { _docID } }")
                .await
                .errors
                .is_empty()
                .then_some(())
                .expect("create collection 100 document");
            node.execute("mutation { add_Tally118(input: {count: 118}) { _docID } }")
                .await
                .errors
                .is_empty()
                .then_some(())
                .expect("create collection 118 document");
            node.shutdown().await.unwrap();
            drop(node);

            let node = Node::builder()
                .data_path(&data_path)
                .build()
                .await
                .expect("reopen node");
            for (name, count) in [("Tally100", 100), ("Tally118", 118)] {
                let response = node
                    .execute(&format!("query {{ {name} {{ count }} }}"))
                    .await;
                assert!(
                    response.errors.is_empty(),
                    "{name} restart query failed: {:?}",
                    response.errors
                );
                assert_eq!(
                    response.data,
                    Some(serde_json::json!({ name: [{ "count": count }] }))
                );
            }
            node.shutdown().await.unwrap();
            drop(node);
        })
        .unwrap()
}
