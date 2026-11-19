//! A transaction that commits while its node shuts down keeps its
//! replicator's durable head marker, or is refused uncommitted: a commit never
//! returns Ok after P2P stopped recording markers.
#![cfg(feature = "p2p")]

#[path = "support/clock.rs"]
mod clock;
#[path = "iroh_peers/mod.rs"]
mod iroh_peers;
#[path = "support/outcome.rs"]
mod outcome;

use std::sync::Arc;
use std::time::{Duration, Instant};

use clock::sleep;
use defra_core::RequestContext;
use defra_node::{NodeBuilder, NodeConfig, QueryRequest, TransportConfig};
use iroh_peers::{create, iroh_config, listen_addr};
use outcome::Result;
use serde_json::Value as JsonValue;

const NOTE_SDL: &str = "type Note { body: String }";

/// How long after shutdown starts the commit lands: P2P is then stopping.
const COMMIT_DELAY: Duration = Duration::from_millis(200);

/// How long regolith's close waits for the transactions still open.
const STORE_CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

/// The sender's replicator is down, so no acknowledgement clears a marker: a
/// note written before shutdown keeps one, and a note whose transaction
/// commits during shutdown either keeps one too or was never committed.
#[test]
fn a_transaction_committed_during_shutdown_is_refused_or_keeps_its_head_marker() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("data.regolith");
            let sender = NodeBuilder::default()
                .with_config(NodeConfig {
                    edge: Some(edge::testing::edge()),
                    transport: TransportConfig::Iroh(iroh_config()),
                    ..Default::default()
                })
                .data_path(&path)
                .build()
                .await?;
            let receiver = NodeBuilder::default()
                .with_p2p(iroh_config())
                .build()
                .await?;
            sender.add_collection(NOTE_SDL).await?;
            receiver.add_collection(NOTE_SDL).await?;
            let receiver_peer = receiver
                .p2p_system()
                .expect("p2p")
                .ops()
                .local_peer_id()
                .await?;
            let receiver_addr = listen_addr(&receiver).await?;
            sender
                .p2p_system()
                .expect("p2p")
                .ops()
                .add_replicator(
                    vec!["Note".to_string()],
                    Some(&receiver_addr),
                    Default::default(),
                    Vec::new(),
                    None,
                )
                .await?;
            receiver.shutdown().await?;

            let (before, _) = create(&sender, "Note", r#"body: "before shutdown""#).await?;
            let txn = sender
                .begin_transaction(false)
                .await
                .map_err(|e| e.to_string())?;
            let written = sender
                .execute_request_in_txn(
                    QueryRequest::new(
                        r#"mutation { add_Note(input: {body: "at shutdown"}) { _docID } }"#,
                    ),
                    &txn,
                )
                .await;
            assert!(written.errors.is_empty(), "{:?}", written.errors);
            let during = doc_id(written.data.as_ref(), "add_Note");

            let sender = Arc::new(sender);
            let stopping = edge::testing::edge().spawn({
                let sender = Arc::clone(&sender);
                async move { sender.shutdown().await }
            });
            sleep(COMMIT_DELAY).await;
            let committed = sender.commit_transaction(&txn).await.is_ok();
            stopping.await.expect("shutdown task")?;
            drop(sender);

            let store = Arc::new(storage::RegolithStore::open(&path)?);
            let markers = storage::stores::Peerstore::new(Arc::clone(&store))
                .get_retry_documents(&receiver_peer)
                .await
                .map_err(|e| e.to_string())?;
            storage::Store::close(&*store).await?;
            drop(store);
            let marked = |doc: &str| markers.iter().any(|marker| marker.doc_id == doc);

            assert!(
                marked(&before),
                "{before}, committed before shutdown, has no head marker for {receiver_peer}"
            );
            if committed {
                assert!(
                    marked(&during),
                    "a commit returned Ok during shutdown but {during} has no durable head marker for {receiver_peer}"
                );
            } else {
                let reopened = NodeBuilder::default().data_path(&path).build().await?;
                let notes = reopened.execute("query { Note { _docID } }").await;
                let ids: Vec<&str> = notes
                    .data
                    .as_ref()
                    .and_then(|data| data.get("Note"))
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|note| note.get("_docID").and_then(JsonValue::as_str))
                    .collect();
                assert!(
                    !ids.contains(&during.as_str()),
                    "a refused commit left {during} committed"
                );
                reopened.shutdown().await?;
            }
            Ok(())
        })
        .unwrap()
}

/// A commit refused at shutdown, through `commit_transaction` or the ops
/// API's `txn_commit`, leaves its transaction open, and its caller may never
/// discard it: an HTTP client's discard is refused too. Shutdown ends it
/// rather than have the store's close wait out its timeout for it.
#[test]
fn a_transaction_left_open_by_a_refused_commit_does_not_stall_shutdown() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let node = NodeBuilder::default()
                .with_config(NodeConfig {
                    edge: Some(edge::testing::edge()),
                    transport: TransportConfig::Iroh(iroh_config()),
                    ..Default::default()
                })
                .data_path(dir.path().join("data.regolith"))
                .build()
                .await?;
            node.add_collection(NOTE_SDL).await?;
            let txn = node
                .begin_transaction(false)
                .await
                .map_err(|e| e.to_string())?;
            let written = node
                .execute_request_in_txn(
                    QueryRequest::new(
                        r#"mutation { add_Note(input: {body: "left open"}) { _docID } }"#,
                    ),
                    &txn,
                )
                .await;
            assert!(written.errors.is_empty(), "{:?}", written.errors);
            let ops_txn = node.txn_begin(&RequestContext::default(), false).await?;
            let ops_context = RequestContext::default().with_txn(Some(ops_txn));
            let written = node
                .execute_request_as(
                    &ops_context,
                    QueryRequest::new(
                        r#"mutation { add_Note(input: {body: "left open by txn_commit"}) { _docID } }"#,
                    ),
                )
                .await?;
            assert!(written.errors.is_empty(), "{:?}", written.errors);

            let node = Arc::new(node);
            let started = Instant::now();
            let stopping = edge::testing::edge().spawn({
                let node = Arc::clone(&node);
                async move { node.shutdown().await }
            });
            while node.execute("query { Note { _docID } }").await.errors.is_empty() {
                assert!(
                    started.elapsed() < STORE_CLOSE_TIMEOUT,
                    "shutdown never stopped admitting requests"
                );
                sleep(Duration::from_millis(10)).await;
            }
            assert!(
                node.commit_transaction(&txn).await.is_err(),
                "commit_transaction was admitted after shutdown stopped admitting requests"
            );
            assert!(
                node.txn_commit(&ops_context, ops_txn).await.is_err(),
                "txn_commit was admitted after shutdown stopped admitting requests"
            );
            let stopped = stopping.await.expect("shutdown task");
            let elapsed = started.elapsed();
            assert!(
                stopped.is_ok() && elapsed < STORE_CLOSE_TIMEOUT,
                "shutdown with a refused commit's transaction open returned {stopped:?} after {elapsed:?}"
            );
            Ok(())
        })
        .unwrap()
}

fn doc_id(data: Option<&JsonValue>, field: &str) -> String {
    data.and_then(|data| data.get(field))
        .and_then(JsonValue::as_array)
        .and_then(|docs| docs.first())
        .and_then(|doc| doc.get("_docID"))
        .and_then(JsonValue::as_str)
        .expect("_docID")
        .to_string()
}
