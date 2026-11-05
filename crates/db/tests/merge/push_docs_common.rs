use super::push_docs_replay::ConnectedPeerTransport;
use db::merge::push_docs_common::*;
use db::merge::{replication_acknowledged, retry_doc};
use db::{AutoCommitMutator, DB};
use defra_core::block::generate_cid_from_bytes;
use defra_core::Block;
use defra_core::CompositeDeltaPayload;
use defra_core::CrdtDelta;
use document::Document;
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
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

/// A hub holding two sibling heads of a document reports no live ack for
/// either, so the retry ladder completes the scope: it sends both heads and
/// records both as acknowledged, and the document reads as acknowledged. The
/// read stays an equality with the whole frontier: one head acknowledged
/// alone, as a generation-less live ack would record it, reads as not.
#[test]
fn multi_head_doc_reads_acknowledged_after_ladder() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        db.set_network(edge::testing::edge());
        db.create_collection(CollectionVersion::new(
            "PushNote",
            "v1",
            "col-push-note",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
            ],
        ))
        .await
        .unwrap();
        let schema = db.get_collection("PushNote").unwrap().unwrap().schema_arc();
        let mut doc = Document::new(schema);
        doc.set("body", "local".to_owned()).unwrap();
        let doc_id = AutoCommitMutator::new(db.clone())
            .create("PushNote", doc)
            .await
            .unwrap()
            .doc_id
            .to_string();

        let txn = db.new_txn(false).await.unwrap();
        let doc_short_id = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &doc_id)
            .await
            .unwrap()
            .unwrap()
            .doc_short_id;
        let local = load_latest_composite_head_cids(
            &txn.headstore().unwrap(),
            &txn.blockstore().unwrap(),
            doc_short_id,
        )
        .await;
        let remote = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "v1".to_string(),
                priority: 1,
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let remote_bytes = remote.to_dag_cbor().unwrap();
        let remote_cid = generate_cid_from_bytes(&remote_bytes).unwrap();
        txn.blockstore()
            .unwrap()
            .set(&remote_cid.to_bytes(), &remote_bytes)
            .await
            .unwrap();
        let mut heads = storage::merge::head_set::Write::default();
        heads.add(&db::block::heads::document::name("C", &remote_cid), 1);
        db::block::heads::document::commit(&txn.headstore().unwrap(), doc_short_id, &heads)
            .await
            .unwrap();
        txn.headstore()
            .unwrap()
            .set(
                &HeadstorePriorityKey::new(doc_short_id, 1, remote_cid).bytes(),
                &[],
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let mut frontier = [local.as_slice(), &[remote_cid]].concat();
        frontier.sort_unstable();
        assert_eq!(frontier.len(), 2, "the document has two sibling heads");

        let peer = PeerId::new("receiver".into());
        let peerstore = db.peerstore();
        let info = p2p::ReplicatorInfo::from_raw(
            peer.as_str().to_owned(),
            vec!["col-push-note".into()],
            vec![],
        );
        peerstore
            .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        let generation = peerstore
            .observe_push_head_with_generation(peer.as_str(), &doc_id, "col-push-note")
            .await
            .unwrap()
            .unwrap();
        let (requests, received) = regolith::sync::bounded(16);
        let transport =
            ConnectedPeerTransport::new("hub", peer.as_str()).with_replies(true, requests);
        retry_doc(
            &transport,
            &db,
            None,
            &peer,
            &doc_id,
            "col-push-note",
            generation,
            &p2p::ReplicationFilters::new(),
            &p2p::EqOnlyFilterMatcher,
            &p2p::sync::HeadHintCarAuthority::default(),
        )
        .await
        .unwrap();
        let mut sent = Vec::new();
        while let Some(request) = received.try_recv() {
            sent.push(cid::Cid::try_from(request.cid.as_ref()).unwrap());
        }
        sent.sort_unstable();
        assert_eq!(sent, frontier, "the ladder sends every current head");
        assert_eq!(
            replication_acknowledged(
                &db,
                peer.as_str(),
                "PushNote",
                std::slice::from_ref(&doc_id)
            )
            .await
            .unwrap(),
            [true],
            "the ladder records the whole frontier as acknowledged"
        );

        peerstore
            .record_acknowledged_heads(
                peer.as_str(),
                &doc_id,
                "col-push-note",
                false,
                &[remote_cid.to_string()],
            )
            .await
            .unwrap();
        assert_eq!(
            replication_acknowledged(
                &db,
                peer.as_str(),
                "PushNote",
                std::slice::from_ref(&doc_id)
            )
            .await
            .unwrap(),
            [false],
            "one sibling acknowledged alone is not the frontier"
        );
    })
    .unwrap()
}
