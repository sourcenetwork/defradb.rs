//! A node given a retention rule judges replicated documents with that rule,
//! whatever kind it is: a fixed map or one whose floor moves with the chain.

#[path = "support/outcome.rs"]
mod outcome;

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use db::retention::{RetentionFloor, RetentionRule};
use defra_node::NodeBuilder;
use outcome::Result;

/// A floor the embedder raises as its chain advances.
struct ChainFloor(AtomicI64);

impl RetentionRule for ChainFloor {
    fn retention_floor(&self, _collection_id: &str) -> Option<RetentionFloor> {
        Some(RetentionFloor {
            field: "blockNumber".to_string(),
            floor: self.0.load(Ordering::SeqCst),
        })
    }
}

#[test]
fn a_node_judges_with_the_rule_it_was_given() -> Result<()> {
    workers::block_on(async {
        let rule = Arc::new(ChainFloor(AtomicI64::new(100)));
        let node = NodeBuilder::default()
            .with_retention(rule.clone())
            .build()
            .await?;
        let logs = schema::CollectionVersion::new("Log", "v1", "col-log", vec![]);
        let options = node.database().options();
        assert_eq!(
            options.retention_floor(&logs).map(|floor| floor.floor),
            Some(100)
        );
        rule.0.store(250, Ordering::SeqCst);
        assert_eq!(
            options.retention_floor(&logs).map(|floor| floor.floor),
            Some(250)
        );
        Ok(())
    })
    .unwrap()
}
