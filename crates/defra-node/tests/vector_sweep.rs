//! An embedded node runs the vector sweep, so a DiskANN index's pending
//! inserts are merged into its graph without the CLI server.

use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "support/outcome.rs"]
mod outcome;

use db::index::vector::index::VectorIndex;
use defra_node::{EmbeddedNode, NodeBuilder};
use outcome::Result;

const TRAIN_THRESHOLD: usize = db::index::vector::engine::diskann::TRAIN_THRESHOLD as usize;

async fn pending<S: storage::Store + 'static>(node: &EmbeddedNode<S>) -> usize {
    let collection = node.database().get_collection("Note").unwrap().unwrap();
    let desc = collection
        .get_indexes()
        .iter()
        .find(|index| index.is_vector())
        .cloned()
        .unwrap();
    let index = VectorIndex::try_new(collection.resolved_root_id(), desc).unwrap();
    let txn = node.database().new_txn(true).await.unwrap();
    let plan = {
        let mut view = txn.datastore().unwrap();
        index.merge_plan(&mut view).await.unwrap()
    };
    let _ = txn.discard();
    plan.map_or(0, |plan| plan.pending.len())
}

/// Directions spread over the sphere, so the quantizer trained on the first
/// vectors also resolves the later ones.
fn spread(i: usize) -> String {
    let i = i as f32;
    format!(
        "[{:.6}, {:.6}, {:.6}, {:.6}]",
        i.sin(),
        (1.3 * i).cos(),
        (0.7 * i).sin(),
        (2.1 * i).cos()
    )
}

#[test]
fn an_embedded_node_merges_pending_diskann_inserts() -> Result<()> {
    workers::block_on(async { merges_pending_inserts("R: 8, m: 2").await }).unwrap()
}

#[test]
fn an_embedded_node_merges_pending_resident_code_inserts() -> Result<()> {
    workers::block_on(async { merges_pending_inserts("R: 8, m: 2, residentCodes: true").await })
        .unwrap()
}

async fn merges_pending_inserts(diskann: &str) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let corpus = TRAIN_THRESHOLD + 76;

    let node = NodeBuilder::default().data_path(dir.path()).build().await?;
    node.add_collection(&format!(
        "type Note {{ title: String embedding: [Float32!] @index(vector: {{dimensions: 4, diskann: {{{diskann}}}}}) }}"
    ))
    .await?;
    let input = (0..corpus)
        .map(|i| format!("{{title: \"n{i}\", embedding: {}}}", spread(i)))
        .collect::<Vec<_>>()
        .join(",");
    let created = node
        .execute(&format!(
            "mutation {{ add_Note(input: [{input}]) {{ _docID }} }}"
        ))
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let waiting = pending(&node).await;
    node.shutdown().await;
    drop(node);
    // Training indexed the first `TRAIN_THRESHOLD`; the rest wait for a merge.
    // The sweep's first tick may already have merged them.
    assert!(waiting <= corpus - TRAIN_THRESHOLD);

    // Reopened, the sweep's first tick runs at once and merges what waits.
    let node = Arc::new(NodeBuilder::default().data_path(dir.path()).build().await?);
    let deadline = Instant::now() + Duration::from_secs(30);
    while pending(&node).await > 0 {
        assert!(
            Instant::now() < deadline,
            "the sweep never merged the pending DiskANN inserts"
        );
        workers::delay(Duration::from_millis(100))
            .unwrap()
            .await
            .unwrap();
    }

    let found = node
        .execute(
            &format!("{{ Note(limit: 1, order: {{_alias: {{sim: DESC}}}}) {{ title sim: SIMILARITY(embedding: {{vector: {}}}) }} }}", spread(corpus - 1)),
        )
        .await;
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert_eq!(
        found
            .data
            .as_ref()
            .and_then(|data| data.pointer("/Note/0/title")),
        Some(&serde_json::json!(format!("n{}", corpus - 1))),
        "the last insert, pending until the merge, is reachable"
    );
    node.shutdown().await;
    Ok(())
}
