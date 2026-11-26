//! A store written when position records kept whole entry keys still drains
//! and reads right, and each record it rewrites keeps field values alone.

use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::{index_position_prefix, IndexDataStoreKey};
use storage::RegolithStore;

use super::{Node, COLLECTION};

/// Every record of the index, key and value.
async fn records(node: &Node, short_id: u32) -> Vec<(Vec<u8>, Vec<u8>)> {
    let txn = node.db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_prefix(index_position_prefix(short_id, node.index_id)))
        .await
        .unwrap();
    let mut records = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        records.push((pair.key, pair.value.to_vec()));
    }
    iter.close().await.unwrap();
    records
}

/// Rewrites each record as its entry keys, each behind a four-byte length.
async fn write_whole_keys(node: &Node, short_id: u32) {
    let records = records(node, short_id).await;
    assert_eq!(records.len(), 2, "both documents have a record");
    let txn = node.db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let mut iter = datastore
            .iterator(
                IterOptions::new()
                    .with_prefix(IndexDataStoreKey::index_prefix(short_id, node.index_id))
                    .with_keys_only(true),
            )
            .await
            .unwrap();
        let mut entries = Vec::new();
        while let Some(pair) = iter.next().await.unwrap() {
            entries.push(pair.key);
        }
        iter.close().await.unwrap();
        for (key, _) in &records {
            let doc = u64::from_be_bytes(key[key.len() - 8..].try_into().unwrap());
            let suffix = IndexDataStoreKey::doc_suffix(doc);
            let mut whole = Vec::new();
            for entry in entries.iter().filter(|entry| entry.ends_with(&suffix)) {
                whole.extend_from_slice(&u32::try_from(entry.len()).unwrap().to_be_bytes());
                whole.extend_from_slice(entry);
            }
            assert!(!whole.is_empty(), "the document has its entry");
            datastore.set(key, &whole).await.unwrap();
        }
    }
    txn.commit().await.unwrap();
}

#[test]
fn records_of_whole_keys_still_move_their_entries() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, short_id) = {
            let node = Node::on(RegolithStore::open(dir.path()).unwrap(), true).await;
            let a = node.create("a", 1).await;
            let b = node.create("b", 2).await;
            let short_id = node
                .db
                .require_collection(COLLECTION)
                .unwrap()
                .resolved_root_id();
            write_whole_keys(&node, short_id).await;
            node.db.close().await.unwrap();
            (a, b, short_id)
        };
        let node = Node::on(RegolithStore::open(dir.path()).unwrap(), false).await;
        let whole: Vec<usize> = records(&node, short_id)
            .await
            .iter()
            .map(|(_, value)| value.len())
            .collect();

        node.increment(&a, 4).await;
        assert_eq!(
            node.names("{count: {_eq: 1}}", "{count: ASC}").await,
            Vec::<String>::new(),
            "a read before the drain hides the entry a record of whole keys names"
        );
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
            "the entries the records of whole keys named moved and went"
        );
        assert_eq!(node.names("{count: {_eq: 5}}", "{count: ASC}").await, ["a"]);
        let rewritten = records(&node, short_id).await;
        assert_eq!(rewritten.len(), 1, "the deleted document lost its record");
        let (_, value) = &rewritten[0];
        assert_eq!(value[0], 1, "the rewritten record keeps field values");
        assert!(
            whole.iter().all(|&len| value.len() < len),
            "{} bytes against {whole:?}",
            value.len()
        );
    })
    .unwrap();
}
