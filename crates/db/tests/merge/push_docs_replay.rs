use db::merge::push_docs_replay::ReplayDocumentFailure;
use db::merge::push_docs_replay::*;
use p2p::message::PushLogReply;
use p2p::transport::PeerId;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

#[test]
fn replay_push_gate_caps_concurrent_sends() {
    workers::block_on(async {
        let gate = Arc::new(
            ReplayPushGate::new(ReplayPushConfig {
                max_concurrent_document_tasks: 8,
                max_concurrent_outbound_pushes: 2,
                per_peer_rate_limit_burst: 100,
                per_peer_rate_limit_rate: 100.0,
                send_timeout: Duration::from_secs(1),
                ..Default::default()
            })
            .on(p2p::network::Network::on(edge::testing::edge())),
        );
        let current = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let peer = PeerId::new("peer-1".to_string());

        let mut handles = Vec::new();
        for _ in 0..8 {
            let gate = gate.clone();
            let current = current.clone();
            let max_seen = max_seen.clone();
            let peer = peer.clone();
            handles.push(
                workers::spawn(
                    Some(crate::common::worker_io::pool()),
                    "test",
                    move || async move {
                        gate.send_pushlog(&peer, async move {
                            let active = current.fetch_add(1, Ordering::SeqCst) + 1;
                            record_max(&max_seen, active);
                            workers::delay(Duration::from_millis(50))
                                .unwrap()
                                .await
                                .unwrap();
                            current.fetch_sub(1, Ordering::SeqCst);
                            Ok(PushLogReply::success("message"))
                        })
                        .await
                        .unwrap();
                    },
                )
                .unwrap(),
            );
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(max_seen.load(Ordering::SeqCst), 2);
    })
    .unwrap()
}

#[test]
fn replay_push_gate_paces_after_peer_burst() {
    workers::block_on(async {
        let gate = ReplayPushGate::new(ReplayPushConfig {
            max_concurrent_document_tasks: 1,
            max_concurrent_outbound_pushes: 1,
            per_peer_rate_limit_burst: 1,
            per_peer_rate_limit_rate: 10.0,
            send_timeout: Duration::from_secs(1),
            ..Default::default()
        })
        .on(p2p::network::Network::on(edge::testing::edge()));
        let peer = PeerId::new("peer-1".to_string());

        let start = Instant::now();
        for _ in 0..3 {
            gate.send_pushlog(&peer, async { Ok(PushLogReply::success("message")) })
                .await
                .unwrap();
        }

        assert!(start.elapsed() >= Duration::from_millis(150));
    })
    .unwrap()
}

fn record_max(max_seen: &AtomicUsize, value: usize) {
    let mut observed = max_seen.load(Ordering::SeqCst);
    while value > observed {
        match max_seen.compare_exchange(observed, value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return,
            Err(current) => observed = current,
        }
    }
}

#[test]
fn replay_hint_defers_following_sends_without_polling_transport() {
    workers::block_on(async {
        let gate = ReplayPushGate::new(ReplayPushConfig::default())
            .on(p2p::network::Network::on(edge::testing::edge()));
        let peer = PeerId::new("limited".into());
        let mut reply = PushLogReply::error("message", p2p::error::RATE_LIMITED_MESSAGE);
        reply.retry_after_ms = Some(45_000);
        gate.send_pushlog(&peer, async { Ok(reply) }).await.unwrap();
        let result = gate
            .send_pushlog(&peer, async { panic!("must defer to durable owner") })
            .await;
        let Err(ReplayPushSendError::Backpressure { retry_after }) = result else {
            panic!("expected backpressure, got {result:?}");
        };
        assert!(retry_after > Duration::from_secs(44));
        gate.send_pushlog(&PeerId::new("healthy".into()), async {
            Ok(PushLogReply::success("ok"))
        })
        .await
        .unwrap();
    })
    .unwrap()
}

#[test]
fn replay_hint_is_durable_while_another_send_is_still_pending() {
    workers::block_on(async {
        let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store);
        let peer = PeerId::new("peer".into());
        peerstore
            .create_replicator(peer.as_str(), b"replicator")
            .await
            .unwrap();
        for doc in ["slow", "limited"] {
            peerstore
                .observe_push_head(peer.as_str(), doc, "collection")
                .await
                .unwrap();
        }
        let gate = Arc::new(
            ReplayPushGate::new(ReplayPushConfig::default())
                .on(p2p::network::Network::on(edge::testing::edge())),
        );
        let (started, ready) = regolith::sync::bounded(1);
        let (release, wait) = regolith::sync::bounded(1);
        let slow_gate = gate.clone();
        let slow_peer = peer.clone();
        let slow = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                slow_gate
                    .send_pushlog(&slow_peer, async {
                        started.try_send(()).ok().unwrap();
                        wait.recv_async().await.unwrap();
                        Ok(PushLogReply::success("slow"))
                    })
                    .await
                    .unwrap();
            },
        )
        .unwrap();
        ready.recv_async().await.unwrap();
        let mut reply = PushLogReply::error("limited", p2p::error::RATE_LIMITED_MESSAGE);
        reply.retry_after_ms = Some(45_000);
        let reply = gate.send_pushlog(&peer, async { Ok(reply) }).await.unwrap();
        persist_retry_after(&peerstore, &peer, &reply)
            .await
            .unwrap();
        assert!(!slow.is_finished());
        assert!(
            remaining_retry_after(&peerstore, &peer)
                .await
                .unwrap()
                .unwrap()
                > Duration::from_secs(44)
        );
        assert_eq!(
            peerstore
                .get_retry_documents(peer.as_str())
                .await
                .unwrap()
                .len(),
            2
        );
        release.try_send(()).ok().unwrap();
        slow.await.unwrap();
    })
    .unwrap()
}

#[test]
fn durable_hint_arriving_while_waiting_for_send_permit_prevents_dispatch() {
    workers::block_on(async {
        let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
        let peerstore = Arc::new(storage::stores::Peerstore::new(store));
        let peer = PeerId::new("peer".into());
        peerstore
            .create_replicator(peer.as_str(), b"replicator")
            .await
            .unwrap();
        peerstore
            .observe_push_head(peer.as_str(), "doc", "collection")
            .await
            .unwrap();
        let gate = Arc::new(
            ReplayPushGate::new(ReplayPushConfig {
                max_concurrent_outbound_pushes: 1,
                ..Default::default()
            })
            .on(p2p::network::Network::on(edge::testing::edge())),
        );
        let (started, ready) = regolith::sync::bounded(1);
        let (release, wait) = regolith::sync::bounded(1);
        let first_gate = gate.clone();
        let first_peer = peer.clone();
        let first = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                first_gate
                    .send_pushlog(&first_peer, async {
                        started.try_send(()).ok().unwrap();
                        wait.recv_async().await.unwrap();
                        Ok(PushLogReply::success("first"))
                    })
                    .await
                    .unwrap();
            },
        )
        .unwrap();
        ready.recv_async().await.unwrap();
        let waiting_gate = gate.clone();
        let waiting_store = peerstore.clone();
        let waiting_peer = peer.clone();
        let (entered, entered_rx) = regolith::sync::bounded(1);
        let waiting = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                let attempt = waiting_gate.send_pushlog_with_admission(
                    &waiting_peer,
                    check_retry_admission(&waiting_store, &waiting_peer),
                    async { panic!("hint installed while queued must prevent transport dispatch") },
                );
                let mut attempt = std::pin::pin!(attempt);
                assert!(futures::poll!(&mut attempt).is_pending());
                entered.try_send(()).ok().unwrap();
                attempt.await
            },
        )
        .unwrap();
        entered_rx.recv_async().await.unwrap();
        let mut reply = PushLogReply::error("limited", p2p::error::RATE_LIMITED_MESSAGE);
        reply.retry_after_ms = Some(45_000);
        persist_retry_after(&peerstore, &peer, &reply)
            .await
            .unwrap();
        release.try_send(()).ok().unwrap();
        first.await.unwrap();
        assert!(matches!(
            waiting.await.unwrap(),
            Err(ReplayPushSendError::Backpressure { .. })
        ));
    })
    .unwrap()
}

#[test]
fn unfinished_replay_uses_configured_schedule_and_marks_replicator_inactive() {
    workers::block_on(async {
        use storage::RegolithStore;

        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone())
            .with_retry_schedule(storage::stores::RetrySchedule::new(vec![3600]).unwrap());
        let peer = PeerId::new("peer-durable".to_string());
        let info = p2p::ReplicatorInfo::from_raw(
            peer.to_string(),
            vec!["collection".to_string()],
            Vec::new(),
        );
        peerstore
            .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();

        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        persist_replay_failures(
            &peerstore,
            &peer,
            &[ReplayDocumentFailure {
                doc_id: "doc-1".to_string(),
                collection_id: "collection".to_string(),
            }],
        )
        .await
        .unwrap();

        let retries = peerstore.get_retry_documents(peer.as_str()).await.unwrap();
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].doc_id, "doc-1");
        assert_eq!(retries[0].scope, storage::stores::RetryScope::Document);
        assert!(!retries[0].is_collection_commit());
        assert!(!retries[0].retry_info.is_due());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(retries[0].retry_info.next_retry_unix >= before + 1800);
        assert!(retries[0].retry_info.next_retry_unix <= now + 3600);
        let saved = peerstore
            .get_replicator(peer.as_str())
            .await
            .unwrap()
            .unwrap();
        let saved = p2p::ReplicatorInfo::from_bytes(&saved).unwrap();
        assert_eq!(saved.status, p2p::ReplicatorStatus::Inactive);
    })
    .unwrap()
}

#[test]
fn forgotten_peer_cannot_be_resurrected_by_unfinished_replay() {
    workers::block_on(async {
        use storage::RegolithStore;

        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone());
        let peer = PeerId::new("peer-durable".to_string());
        let info = p2p::ReplicatorInfo::from_raw(
            peer.to_string(),
            vec!["collection".to_string()],
            Vec::new(),
        );
        peerstore
            .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();

        let persistence_peer = peer.clone();
        let persistence_store = storage::stores::Peerstore::new(store.clone());
        let failures = [ReplayDocumentFailure {
            doc_id: "doc-1".to_string(),
            collection_id: "collection".to_string(),
        }];
        let (persisted, deleted) = futures::join!(
            persist_replay_failures(&persistence_store, &persistence_peer, &failures),
            peerstore.delete_replicator(peer.as_str()),
        );
        persisted.unwrap();
        deleted.unwrap();
        assert!(!peerstore.has_replicator(peer.as_str()).await.unwrap());
        assert!(peerstore
            .get_retry_documents(peer.as_str())
            .await
            .unwrap()
            .is_empty());
        assert!(peerstore
            .get_retry_info(peer.as_str())
            .await
            .unwrap()
            .is_none());
    })
    .unwrap()
}

/// The first history replay has a replicator row but no retry schedule yet.
/// A backpressure hint arriving then must still land: reschedule reports
/// "nothing written", and the hint is seeded as the initial schedule instead
/// of being dropped. A gone replicator stays a success with no row written.
#[test]
fn a_first_replay_hint_seeds_the_missing_retry_schedule() {
    workers::block_on(async {
        use storage::stores::Peerstore;
        use storage::RegolithStore;

        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = Peerstore::new(store.clone());
        let peer = PeerId::new("peer-seed".to_string());
        peerstore
            .create_replicator("peer-seed", b"replicator")
            .await
            .unwrap();

        // retry_after only fires on a backpressure reply, never on plain success.
        let mut reply = PushLogReply::success("message");
        reply.err_message = Some(p2p::error::RATE_LIMITED_MESSAGE.to_string());
        reply.retry_after_ms = Some(45_000);
        persist_retry_after(&peerstore, &peer, &reply)
            .await
            .unwrap();

        persist_replay_failures(
            &peerstore,
            &peer,
            &[ReplayDocumentFailure {
                doc_id: "doc-seed".into(),
                collection_id: "collection-seed".into(),
            }],
        )
        .await
        .unwrap();
        peerstore.activate_retry_peer(peer.as_str()).await.unwrap();
        let remaining = remaining_retry_after(&peerstore, &peer)
            .await
            .unwrap()
            .expect("the hint landed as a schedule");
        assert!(
            remaining > Duration::from_secs(40),
            "the seeded deadline honors the hint; got {remaining:?}"
        );

        // A replicator that is gone: no row written, still a success.
        let gone = PeerId::new("peer-gone".to_string());
        persist_retry_after(&peerstore, &gone, &reply)
            .await
            .unwrap();
        assert!(peerstore
            .get_retry_info("peer-gone")
            .await
            .unwrap()
            .is_none());
    })
    .unwrap()
}
