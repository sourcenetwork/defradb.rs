//! A committed write keeps registering its sender markers after its caller
//! stops waiting, as a request past its deadline does.

use std::sync::Arc;
use std::time::Duration;

use document::DocID;
use futures::future::{select, Either};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::Atom;

use super::broadcast_chunk_filter::{node, with_parent, Node, MARKER_GENERATION};
use super::chunk_filter_support::PUBLIC;

/// Long enough for several chunks at the schema's `maxChars: 64`.
const BODY: &str = "First paragraph of the article, long enough for a chunk.\n\n\
    Second paragraph of the article, long enough for a chunk.\n\n\
    Third paragraph of the article, long enough for a chunk.\n\n\
    Fourth paragraph of the article, long enough for a chunk.";

/// How long the markers left after the caller stopped may take to arrive.
const HANDOFF_GRACE: Duration = Duration::from_secs(5);

/// Answers one coordinator event; true when it was a marker registration.
fn answer(pushed: &Atom<Vec<String>>, mut event: p2p::sync::PushFailure) -> bool {
    let Some(reply) = event.durable_tx.take() else {
        return false;
    };
    let registration = !event.admission_only && !event.acknowledged && !event.create_retry;
    if registration {
        pushed.rcu(|docs| {
            let mut next = docs.clone();
            next.push(event.doc_id.clone());
            next
        });
        let _ = reply.send(Some(MARKER_GENERATION));
    } else {
        let _ = reply.send(event.marker_generation);
    }
    registration
}

/// Runs `write` until its first marker is registered, then drops it.
async fn abandon_after_first_marker<F>(
    failures: &mut p2p::sync::PushFailureReceiver,
    pushed: &Atom<Vec<String>>,
    write: F,
) where
    F: std::future::Future,
{
    let first_marker = async {
        while let Some(event) = failures.recv().await {
            if answer(pushed, event) {
                return;
            }
        }
        panic!("the failure channel closed during the write");
    };
    if let Either::Left(_) = select(std::pin::pin!(write), std::pin::pin!(first_marker)).await {
        panic!("the write returned before registering a marker");
    }
}

impl Node {
    /// Answers registrations until `expected` are recorded or the grace ends.
    async fn registered_within_grace(&mut self, expected: usize) -> RapidHashSet<String> {
        let (failures, pushed) = (&mut self.failures, &self.pushed);
        let rest = async {
            while pushed.load().len() < expected {
                let Some(event) = failures.recv().await else {
                    return;
                };
                answer(pushed, event);
            }
        };
        let _ = workers::timeout(HANDOFF_GRACE, rest).await;
        self.take_pushed()
    }

    async fn only_article(&self) -> DocID {
        let articles = self.db.get_collection("Article").unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let docs = articles
            .get_all_with_datastore_include_deleted(
                &txn.datastore().unwrap(),
                &txn.systemstore().unwrap(),
                false,
            )
            .await
            .unwrap();
        txn.force_discard().unwrap();
        let [(doc, _)] = <[_; 1]>::try_from(docs).expect("exactly one article");
        doc.id().unwrap().clone()
    }
}

#[test]
fn a_create_registers_every_chunk_after_its_caller_stops_waiting() {
    workers::block_on(async {
        let mut node = node().await;
        let article = node.article(PUBLIC, BODY);
        abandon_after_first_marker(
            &mut node.failures,
            &node.pushed,
            node.mutator.create("Article", article),
        )
        .await;

        let parent = node.only_article().await;
        let expected = with_parent(&parent, node.chunks_of(&parent).await);
        assert!(
            expected.len() > 2,
            "the body must split into several chunks"
        );
        assert_eq!(node.registered_within_grace(expected.len()).await, expected);
    })
    .unwrap()
}

#[test]
fn a_batch_registers_every_chunk_after_its_caller_stops_waiting() {
    workers::block_on(async {
        let mut node = node().await;
        let article = node.article(PUBLIC, BODY);
        let batch = node.mutator.begin_batch().await.unwrap().unwrap();
        let operation: query::mutator::MutationBatchOperation = Arc::new(move |mutator, _| {
            let article = article.clone();
            Box::pin(async move {
                mutator.create("Article", article).await?;
                Ok(serde_json::Value::Null)
            })
        });
        abandon_after_first_marker(&mut node.failures, &node.pushed, batch.run(operation)).await;

        let parent = node.only_article().await;
        let expected = with_parent(&parent, node.chunks_of(&parent).await);
        assert!(
            expected.len() > 2,
            "the body must split into several chunks"
        );
        assert_eq!(node.registered_within_grace(expected.len()).await, expected);
    })
    .unwrap()
}
