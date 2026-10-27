use super::push_docs_replay::ConnectedPeerTransport;
use crate::common::failing_write_store::FailingWriteStore;
use db::merge::{document_heads_acknowledged, push_existing_docs_by_id, PushExistingDocsSeOptions};
use db::{AutoCommitMutator, DB};
use document::Document;
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use storage::corekv::Store;

async fn fixture<S>(store: Arc<S>) -> (Arc<DB<S>>, Vec<(String, String)>, PeerId)
where
    S: Store + 'static,
{
    let db = Arc::new(DB::from_arc(store).unwrap());
    db.set_network(edge::testing::edge());
    db.create_collection(CollectionVersion::new(
        "Note",
        "v1",
        "notes",
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
    let mutator = AutoCommitMutator::new(db.clone());
    let schema = db.get_collection("Note").unwrap().unwrap().schema_arc();
    let mut docs = Vec::new();
    for body in ["one", "two"] {
        let mut doc = Document::new(schema.clone());
        doc.set("body", body.to_owned()).unwrap();
        docs.push((
            "Note".into(),
            mutator
                .create("Note", doc)
                .await
                .unwrap()
                .doc_id
                .to_string(),
        ));
    }
    (db, docs, PeerId::new("receiver".into()))
}

async fn select<S>(
    db: &DB<S>,
    transport: &ConnectedPeerTransport,
    peer: &PeerId,
    docs: &[(String, String)],
    filters: &p2p::ReplicationFilters,
) -> Result<(), String>
where
    S: Store + 'static,
{
    push_existing_docs_by_id(
        transport,
        db,
        None,
        peer,
        docs,
        filters,
        PushExistingDocsSeOptions::default(),
        Default::default(),
        &p2p::EqOnlyFilterMatcher,
        &p2p::sync::HeadHintCarAuthority::default(),
    )
    .await
}

async fn subscribe<S>(db: &DB<S>, peer: &PeerId)
where
    S: Store + 'static,
{
    let info =
        p2p::ReplicatorInfo::from_raw(peer.as_str().to_owned(), vec!["notes".into()], vec![]);
    db.peerstore()
        .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
        .await
        .unwrap();
}

#[test]
fn exact_selection_ignores_subscription_filter_and_backoff() {
    workers::block_on(async {
        let (db, docs, peer) =
            fixture(Arc::new(storage::RegolithStore::in_memory().unwrap())).await;
        subscribe(&db, &peer).await;
        db.peerstore()
            .record_retry_after(peer.as_str(), Duration::from_secs(600))
            .await
            .unwrap();
        let before = db.peerstore().get_retry_info(peer.as_str()).await.unwrap();
        let mut filters = p2p::ReplicationFilters::new();
        filters.insert(
            "notes".into(),
            p2p::ReplicationFilter::eq("body", serde_json::json!("excluded")),
        );
        let (sent, received) = regolith::sync::bounded(8);
        let transport = ConnectedPeerTransport::new("sender", peer.as_str())
            .with_replies(true, sent)
            .requiring_selected_authority(Arc::new(db.peerstore()));
        select(&db, &transport, &peer, &docs, &filters)
            .await
            .unwrap();
        assert!(received.try_recv().is_some());
        assert_eq!(
            before,
            db.peerstore().get_retry_info(peer.as_str()).await.unwrap()
        );
        assert!(db
            .peerstore()
            .get_retry_documents(peer.as_str())
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap();
}

#[test]
fn selected_receiver_hint_does_not_change_replication_schedule() {
    workers::block_on(async {
        let (db, docs, peer) =
            fixture(Arc::new(storage::RegolithStore::in_memory().unwrap())).await;
        subscribe(&db, &peer).await;
        let before = db.peerstore().get_retry_info(peer.as_str()).await.unwrap();
        let (sent, _received) = regolith::sync::bounded(8);
        let transport = ConnectedPeerTransport::new("sender", peer.as_str())
            .with_replies(false, sent)
            .with_retry_after(45_000);
        let error = select(&db, &transport, &peer, &docs[..1], &Default::default())
            .await
            .unwrap_err();
        assert!(error.contains(&docs[0].1), "{error}");
        assert!(error.contains("45000"), "{error}");
        assert_eq!(
            before,
            db.peerstore().get_retry_info(peer.as_str()).await.unwrap()
        );
        assert!(db
            .peerstore()
            .get_retry_documents(peer.as_str())
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap();
}

#[test]
fn unavailable_selection_identifies_document_and_collection() {
    workers::block_on(async {
        let (db, _, peer) = fixture(Arc::new(storage::RegolithStore::in_memory().unwrap())).await;
        let (sent, received) = regolith::sync::bounded(8);
        let transport =
            ConnectedPeerTransport::new("sender", peer.as_str()).with_replies(true, sent);
        for (collection, reason) in [
            ("Note", "document not found"),
            ("Missing", "collection not found"),
        ] {
            let error = select(
                &db,
                &transport,
                &peer,
                &[(collection.into(), "missing-id".into())],
                &Default::default(),
            )
            .await
            .unwrap_err();
            assert!(
                error.contains(&format!("{collection}/missing-id")) && error.contains(reason),
                "{error}"
            );
        }
        assert!(received.try_recv().is_none());
    })
    .unwrap();
}

#[test]
fn authority_write_failure_prevents_any_head_hint() {
    workers::block_on(async {
        let store = Arc::new(FailingWriteStore::in_memory(Arc::new(|key, _| {
            if key.starts_with(b"p/selected-car/v1/") {
                return Err(storage::Error::Other("authority unavailable".into()));
            }
            Ok(())
        })));
        let (db, docs, peer) = fixture(store).await;
        let (sent, received) = regolith::sync::bounded(8);
        let transport =
            ConnectedPeerTransport::new("sender", peer.as_str()).with_replies(true, sent);
        let error = select(&db, &transport, &peer, &docs, &Default::default())
            .await
            .unwrap_err();
        assert!(error.contains("authority unavailable"), "{error}");
        assert!(received.try_recv().is_none());
    })
    .unwrap();
}

#[test]
fn acknowledgement_failure_still_joins_and_records_other_started_send() {
    workers::block_on(async {
        let (failed, failure_seen) = regolith::sync::bounded(1);
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let store = Arc::new(FailingWriteStore::in_memory(Arc::new(move |key, _| {
            if key.starts_with(b"p/rep/acked/") && count.fetch_add(1, Ordering::SeqCst) == 0 {
                failed.try_send(()).unwrap();
                return Err(storage::Error::Other("first acknowledgement failed".into()));
            }
            Ok(())
        })));
        let (db, docs, peer) = fixture(store).await;
        subscribe(&db, &peer).await;
        let (started, second_started) = regolith::sync::bounded(1);
        let first_doc = docs[0].1.clone();
        let (sent, _received) = regolith::sync::bounded(8);
        let transport = ConnectedPeerTransport::new("sender", peer.as_str())
            .with_replies(true, sent)
            .with_reply_hook(Arc::new(move |doc_id| {
                let first = doc_id == first_doc;
                let started = started.clone();
                let second_started = second_started.clone();
                let failure_seen = failure_seen.clone();
                Box::pin(async move {
                    if first {
                        second_started.recv_async().await.unwrap();
                    } else {
                        started.try_send(()).unwrap();
                        failure_seen.recv_async().await.unwrap();
                    }
                })
            }));
        let result = select(&db, &transport, &peer, &docs, &Default::default()).await;
        assert!(result.unwrap_err().contains("first acknowledgement failed"));
        assert_eq!(writes.load(Ordering::SeqCst), 2);
        let ids: Vec<_> = docs.iter().map(|(_, id)| id.clone()).collect();
        let observed = document_heads_acknowledged(&db, peer.as_str(), "Note", &ids)
            .await
            .unwrap();
        assert_eq!(
            observed
                .iter()
                .filter(|&&acknowledged| acknowledged)
                .count(),
            1
        );
    })
    .unwrap();
}
