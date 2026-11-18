//! A store from before deferred indexes holds inline entries and no position
//! records. Opening it must give those entries records, or a drain never
//! finds the old entry to remove.

use db::definition::migration::format::INDEX_FORMAT_KEY;
use schema::{IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::IndexDataStoreKey;
use storage::RegolithStore;

use super::{count_keys, Node, COLLECTION};

/// Keeps the entries and drops every record, with the format stamp an older build wrote.
async fn forget_records(node: &Node) {
    let short_id = node
        .db
        .require_collection(COLLECTION)
        .unwrap()
        .schema()
        .resolved_root_id();
    let txn = node.db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let mut iter = datastore
            .iterator(
                IterOptions::new()
                    .with_prefix(storage::keys::index_position_prefix(
                        short_id,
                        node.index_id,
                    ))
                    .with_keys_only(true),
            )
            .await
            .unwrap();
        let mut records = Vec::new();
        while let Some(pair) = iter.next().await.unwrap() {
            records.push(pair.key);
        }
        iter.close().await.unwrap();
        assert_eq!(records.len(), 2, "both documents had a record to forget");
        for key in &records {
            datastore.delete(key).await.unwrap();
        }
        txn.systemstore()
            .unwrap()
            .set(INDEX_FORMAT_KEY, &2u32.to_be_bytes())
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
}

#[test]
fn entries_from_before_deferred_indexes_move_and_go() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = {
            let node = Node::on(RegolithStore::open(dir.path()).unwrap(), true).await;
            let a = node.create("a", 1).await;
            let b = node.create("b", 2).await;
            forget_records(&node).await;
            node.db.close().await.unwrap();
            (a, b)
        };
        let node = Node::on(RegolithStore::open(dir.path()).unwrap(), false).await;

        node.increment(&a, 4).await;
        node.run(&format!(
            r#"mutation {{ delete_Tallies(docID: "{b}") {{ _docID }} }}"#
        ))
        .await;
        node.drain().await;

        assert_eq!(
            (
                node.stored_at(1).await,
                node.stored_at(2).await,
                node.stored_at(5).await
            ),
            (0, 0, 1),
            "the entries from before the upgrade moved and went"
        );
        assert!(node
            .names("{count: {_eq: 1}}", "{count: ASC}")
            .await
            .is_empty());
        assert!(node
            .names("{count: {_eq: 2}}", "{count: ASC}")
            .await
            .is_empty());
    })
    .unwrap();
}

/// Removes every entry of `index_id`, which a rebuild of it would put back.
async fn forget_entries(node: &Node, short_id: u32, index_id: u32) {
    let txn = node.db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let mut iter = datastore
            .iterator(
                IterOptions::new()
                    .with_prefix(IndexDataStoreKey::index_prefix(short_id, index_id))
                    .with_keys_only(true),
            )
            .await
            .unwrap();
        let mut entries = Vec::new();
        while let Some(pair) = iter.next().await.unwrap() {
            entries.push(pair.key);
        }
        iter.close().await.unwrap();
        assert_eq!(entries.len(), 2, "both documents had an entry to forget");
        for key in &entries {
            datastore.delete(key).await.unwrap();
        }
    }
    txn.commit().await.unwrap();
}

#[test]
fn an_upgrade_rebuilds_only_the_deferred_index() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (short_id, by_name) = {
            let node = Node::on(RegolithStore::open(dir.path()).unwrap(), true).await;
            let by_name = node
                .db
                .create_index(
                    COLLECTION,
                    Some("by_name"),
                    vec![IndexedFieldDescription {
                        name: "name".to_string(),
                        descending: false,
                    }],
                    IndexKind::Ordered(OrderedIndexDescription { unique: false }),
                )
                .await
                .unwrap()
                .id;
            node.create("a", 1).await;
            node.create("b", 2).await;
            let short_id = node
                .db
                .require_collection(COLLECTION)
                .unwrap()
                .resolved_root_id();
            forget_records(&node).await;
            forget_entries(&node, short_id, by_name).await;
            node.db.close().await.unwrap();
            (short_id, by_name)
        };
        let node = Node::on(RegolithStore::open(dir.path()).unwrap(), false).await;

        assert_eq!(
            count_keys(
                &node.db,
                storage::keys::index_position_prefix(short_id, node.index_id)
            )
            .await,
            2,
            "the deferred index got its records"
        );
        assert_eq!(
            count_keys(&node.db, IndexDataStoreKey::index_prefix(short_id, by_name)).await,
            0,
            "the inline index was not rebuilt"
        );
    })
    .unwrap();
}
