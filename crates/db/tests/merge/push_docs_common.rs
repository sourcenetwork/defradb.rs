use db::merge::push_docs_common::*;
use db::DB;
use defra_core::block::generate_cid_from_bytes;
use defra_core::Block;
use defra_core::CompositeDeltaPayload;
use defra_core::CrdtDelta;
use std::sync::Arc;
use storage::corekv::Key;
use storage::keys::headstore::HeadstorePriorityKey;
use storage::RegolithStore;

#[test]
fn current_composite_frontier_retains_lower_priority_sibling() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        let doc_short_id = 7_u64;
        let first = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "schema-v1".to_string(),
                priority: 1,
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let first_bytes = first.to_dag_cbor().unwrap();
        let first_cid = generate_cid_from_bytes(&first_bytes).unwrap();

        let second = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "schema-v1".to_string(),
                priority: 2,
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let second_bytes = second.to_dag_cbor().unwrap();
        let second_cid = generate_cid_from_bytes(&second_bytes).unwrap();

        let txn = db.new_txn(false).await.unwrap();
        txn.blockstore()
            .unwrap()
            .set(&first_cid.to_bytes(), &first_bytes)
            .await
            .unwrap();
        txn.blockstore()
            .unwrap()
            .set(&second_cid.to_bytes(), &second_bytes)
            .await
            .unwrap();
        let mut heads = storage::merge::head_set::Write::default();
        heads.add(&db::block::heads::document::name("C", &first_cid), 1);
        heads.add(&db::block::heads::document::name("C", &second_cid), 2);
        db::block::heads::document::commit(&txn.headstore().unwrap(), doc_short_id, &heads)
            .await
            .unwrap();
        txn.headstore()
            .unwrap()
            .set(
                &HeadstorePriorityKey::new(doc_short_id, 1, first_cid).bytes(),
                &[],
            )
            .await
            .unwrap();
        txn.headstore()
            .unwrap()
            .set(
                &HeadstorePriorityKey::new(doc_short_id, 2, second_cid).bytes(),
                &[],
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let mut heads = load_latest_composite_head_cids(
            &txn.headstore().unwrap(),
            &txn.blockstore().unwrap(),
            doc_short_id,
        )
        .await;

        heads.sort_unstable();
        let mut expected = vec![first_cid, second_cid];
        expected.sort_unstable();
        assert_eq!(heads, expected);
    })
    .unwrap()
}

#[test]
fn stale_retry_heads_cannot_clear_a_newer_document_marker() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        let peerstore = storage::stores::Peerstore::new(store);
        let peer_id = "peer";
        let doc_id = "doc";
        let collection_id = "collection";
        let doc_short_id = 7_u64;
        peerstore
            .create_replicator(peer_id, b"replicator")
            .await
            .unwrap();
        let marker_generation = peerstore
            .observe_push_head_with_generation(peer_id, doc_id, collection_id)
            .await
            .unwrap()
            .unwrap();

        let old = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "schema-v1".to_string(),
                priority: 1,
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let old_bytes = old.to_dag_cbor().unwrap();
        let old_cid = generate_cid_from_bytes(&old_bytes).unwrap();
        let current = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "schema-v1".to_string(),
                priority: 2,
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let current_bytes = current.to_dag_cbor().unwrap();
        let current_cid = generate_cid_from_bytes(&current_bytes).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        for (cid, bytes, priority) in [
            (old_cid, old_bytes.as_slice(), 1),
            (current_cid, current_bytes.as_slice(), 2),
        ] {
            txn.blockstore()
                .unwrap()
                .set(&cid.to_bytes(), bytes)
                .await
                .unwrap();
            txn.headstore()
                .unwrap()
                .set(
                    &HeadstorePriorityKey::new(doc_short_id, priority, cid).bytes(),
                    &[],
                )
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();

        let error = complete_document_retry_if_current(
            &db,
            peer_id,
            doc_id,
            collection_id,
            doc_short_id,
            &[old_cid],
            marker_generation,
        )
        .await
        .unwrap_err();
        assert!(error.contains("heads changed"));
        assert_eq!(
            peerstore.get_retry_documents(peer_id).await.unwrap().len(),
            1
        );

        complete_document_retry_if_current(
            &db,
            peer_id,
            doc_id,
            collection_id,
            doc_short_id,
            &[current_cid],
            marker_generation,
        )
        .await
        .unwrap();
        assert!(peerstore
            .get_retry_documents(peer_id)
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}
