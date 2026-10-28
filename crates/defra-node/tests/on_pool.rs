//! The node's API calls run on the database's worker pool: none of
//! the storage calls they make is made off the pool, which the store counts.

use defra_core::RequestContext;
use defra_node::{Node, NodeBuilder};
use storage::Store;

fn off_pool_calls(node: &Node) -> u64 {
    node.database()
        .store()
        .transaction_stats_handle()
        .expect("the embedded store is regolith")
        .snapshot()
        .off_pool_calls
}

#[test]
fn api_calls_make_no_storage_call_off_the_pool() {
    workers::block_on(async {
        let node = NodeBuilder::default().build().await.unwrap();
        let before = off_pool_calls(&node);

        node.add_collection("type Note { title: String }")
            .await
            .unwrap();
        let added = node
            .execute_as(
                &RequestContext::ambient(),
                r#"mutation { add_Note(input: { title: "first" }) { _docID } }"#,
            )
            .await;
        assert!(added.errors.is_empty(), "{:?}", added.errors);
        let read = node
            .execute_as(&RequestContext::ambient(), "query { Note { title } }")
            .await;
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        node.add_encrypted_index("Note", "title").await.unwrap();

        assert_eq!(
            off_pool_calls(&node),
            before,
            "an API call reached the store from a thread off the pool"
        );
        node.shutdown().await;
    })
    .unwrap()
}
