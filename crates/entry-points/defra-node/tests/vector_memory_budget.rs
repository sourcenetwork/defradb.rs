use defra_node::Node;

/// The budget a node is built with is the one its database charges vector
/// caches to; a node built without one gets the default.
#[test]
fn the_builder_sets_the_vector_memory_budget() {
    workers::block_on(async {
        let node = Node::builder()
            .with_vector_memory_budget(12_345)
            .build()
            .await
            .unwrap();
        assert_eq!(node.database().vector_memory_budget().limit(), 12_345);
        node.shutdown().await.unwrap();

        let node = Node::builder().build().await.unwrap();
        assert_eq!(
            node.database().vector_memory_budget().limit(),
            db::index::vector::budget::DEFAULT_VECTOR_MEMORY_BUDGET
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}
