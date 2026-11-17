//! A backlog larger than the smallest plan drains in plans that grow with it
//! (`proofs/tla/DeferredIndex.tla`, INV_PlanBounded).

use std::sync::atomic::Ordering;

use super::{migrated, seed_owed_at, Node};

/// The smallest plan a drain takes (`PlanSize::SMALLEST`).
const SMALLEST: usize = 256;

impl Node {
    /// Creates `count` documents at 0, then increments each once. A create
    /// places its entry, so each is owed once.
    async fn owe_many(&self, count: usize) {
        let mut ids = Vec::new();
        for batch in 0..count.div_ceil(500) {
            let input = (0..500.min(count - batch * 500))
                .map(|i| format!(r#"{{name: "t{batch}-{i}", count: 0}}"#))
                .collect::<Vec<_>>()
                .join(",");
            let data = self
                .run(&format!(
                    "mutation {{ add_Tallies(input: [{input}]) {{ _docID }} }}"
                ))
                .await;
            for doc in data["add_Tallies"].as_array().unwrap() {
                ids.push(format!("\"{}\"", doc["_docID"].as_str().unwrap()));
            }
        }
        for chunk in ids.chunks(500) {
            self.run(&format!(
                "mutation {{ update_Tallies(docID: [{}], input: {{count: 1}}) {{ _docID }} }}",
                chunk.join(",")
            ))
            .await;
        }
    }
}

fn commits() -> u64 {
    telemetry::storage_event_count(telemetry::StorageEvent::Commit)
}

#[test]
fn a_backlog_drains_in_fewer_commits_than_plans_of_the_smallest_size() {
    workers::block_on(async {
        let node = Node::open().await;
        let owed = 3000;
        node.owe_many(owed).await;
        assert_eq!(node.owed_entries().await, owed);
        let before = commits();

        node.drain().await;

        let taken = commits() - before;
        // 256, 512, 1024 and the 1208 left, where plans of 256 take 12.
        assert!(
            (1..=4).contains(&taken),
            "{taken} commits, against {} at the smallest size",
            owed.div_ceil(SMALLEST)
        );
        assert_eq!(node.owed_entries().await, 0);
        assert_eq!(
            (node.stored_at(0).await, node.stored_at(1).await),
            (0, owed)
        );
        let found = node
            .run("query { Tallies(filter: {count: {_eq: 1}}) { _docID } }")
            .await;
        assert_eq!(found["Tallies"].as_array().unwrap().len(), owed);
    })
    .unwrap();
}

/// A document that fails to migrate at the head of a long backlog is carried
/// to the tail while the plans behind it grow, and the rest all drain.
#[test]
fn a_broken_document_is_carried_through_grown_plans() {
    workers::block_on(async {
        let (node, v1, lenses) = migrated(db::DbOptions::default()).await;
        seed_owed_at(&node.db, &v1, "poison", 1).await;
        let healthy = 4 * SMALLEST;
        node.owe_many(healthy).await;
        lenses.poison.store(true, Ordering::Relaxed);

        assert!(node.db.drain_deferred_indexes().await.is_err());

        assert_eq!(node.owed_entries().await, 1, "only the broken one is owed");
        assert_eq!(
            node.stored_at(1).await,
            healthy + 1,
            "its stale entry stays"
        );

        lenses.poison.store(false, Ordering::Relaxed);
        node.drain().await;
        assert_eq!(node.owed_entries().await, 0);
        assert_eq!(
            (node.stored_at(1).await, node.stored_at(10).await),
            (healthy, 1)
        );
    })
    .unwrap();
}
