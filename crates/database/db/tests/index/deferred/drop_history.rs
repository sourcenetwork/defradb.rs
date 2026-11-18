//! Dropping a deferred index whose log has a long history and more owed
//! entries than one page holds.

use super::{count_keys, Node, COLLECTION};

/// The most items one drop page takes (`PAGE_ITEMS`).
const PAGE_ITEMS: usize = 256;

#[test]
fn a_drop_clears_a_log_longer_than_a_page() {
    workers::block_on(async {
        let node = Node::open().await;
        let id = node.create("a", 0).await;
        for _ in 0..PAGE_ITEMS {
            node.increment(&id, 1).await;
        }
        node.drain().await;
        let owed = PAGE_ITEMS + PAGE_ITEMS / 2;
        for _ in 0..owed {
            node.increment(&id, 1).await;
        }
        let short_id = node
            .db
            .require_collection(COLLECTION)
            .unwrap()
            .schema()
            .resolved_root_id();
        let records = storage::keys::index_position_prefix(short_id, node.index_id);
        let log = storage::keys::index_pending_log(short_id, node.index_id)
            .entry_prefix()
            .to_vec();
        assert_eq!(count_keys(&node.db, records.clone()).await, 1);
        assert_eq!(count_keys(&node.db, log.clone()).await, owed);

        node.db.drop_index(COLLECTION, "by_count").await.unwrap();

        assert_eq!(count_keys(&node.db, records).await, 0);
        assert_eq!(count_keys(&node.db, log).await, 0);
    })
    .unwrap();
}
