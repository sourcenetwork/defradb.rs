//! A run of one peer's scope records shares one commit and does what the
//! records' single calls do, in order (#3471).

use super::*;
use crate::backends::RegolithStore;

const PEER: &str = "peer";
const COLLECTION: &str = "collection";

async fn peerstore() -> Peerstore<RegolithStore> {
    let peerstore = Peerstore::new(Arc::new(RegolithStore::in_memory().unwrap()));
    peerstore
        .create_replicator(PEER, b"replicator")
        .await
        .unwrap();
    peerstore
}

fn observe(doc: &str) -> ScopeRecord {
    ScopeRecord::Observe {
        doc_id: doc.to_owned(),
        collection_id: COLLECTION.to_owned(),
    }
}

fn complete(doc: &str, generation: u64) -> ScopeRecord {
    ScopeRecord::Complete {
        doc_id: doc.to_owned(),
        collection_id: COLLECTION.to_owned(),
        is_collection: false,
        expected_generation: Some(generation),
        acked_heads: vec!["head".to_owned()],
    }
}

impl ScopeRun {
    /// The outcomes of a run its shared transaction recorded.
    fn together(self) -> Vec<ScopeOutcome> {
        assert!(
            self.shared_error.is_none(),
            "the shared transaction failed: {:?}",
            self.shared_error
        );
        self.outcomes
            .into_iter()
            .map(|outcome| outcome.expect("a recorded outcome"))
            .collect()
    }
}

fn commits(peerstore: &Peerstore<RegolithStore>) -> u64 {
    peerstore
        .transaction_stats_handle()
        .expect("a regolith store counts its commits")
        .snapshot()
        .commits
}

async fn marker_docs(peerstore: &Peerstore<RegolithStore>) -> Vec<String> {
    let mut docs: Vec<String> = peerstore
        .get_retry_documents(PEER)
        .await
        .unwrap()
        .into_iter()
        .map(|marker| marker.doc_id)
        .collect();
    docs.sort();
    docs
}

#[test]
fn a_run_of_registrations_commits_once_with_rising_generations() {
    workers::block_on(async {
        let peerstore = peerstore().await;
        let before = commits(&peerstore);
        let outcomes = peerstore
            .record_scopes(
                PEER,
                &[observe("doc-a"), observe("doc-b"), observe("doc-c")],
            )
            .await
            .together();

        assert_eq!(commits(&peerstore) - before, 1, "one commit for the run");
        let generations: Vec<u64> = outcomes
            .iter()
            .map(|outcome| match outcome {
                ScopeOutcome::Observed(Some(generation)) => *generation,
                other => panic!("every registration writes a marker, got {other:?}"),
            })
            .collect();
        assert!(generations.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(marker_docs(&peerstore).await, ["doc-a", "doc-b", "doc-c"]);

        let single = peerstore
            .observe_push_head_with_generation(PEER, "doc-d", COLLECTION)
            .await
            .unwrap()
            .unwrap();
        assert!(
            single > generations[2],
            "a later single registration takes a newer generation"
        );
    })
    .unwrap()
}

#[test]
fn a_record_sees_what_the_records_before_it_wrote() {
    workers::block_on(async {
        let peerstore = peerstore().await;
        let ScopeOutcome::Observed(Some(old)) = peerstore
            .record_scopes(PEER, &[observe("doc-a")])
            .await
            .together()[0]
        else {
            panic!("the registration writes a marker");
        };

        let outcomes = peerstore
            .record_scopes(
                PEER,
                &[
                    complete("doc-a", old),
                    observe("doc-b"),
                    complete("doc-b", old),
                ],
            )
            .await
            .together();

        assert_eq!(outcomes[0], ScopeOutcome::Completed(true));
        let ScopeOutcome::Observed(Some(new)) = outcomes[1] else {
            panic!("doc-b registers after doc-a completes: {outcomes:?}");
        };
        assert!(new > old);
        assert_eq!(
            outcomes[2],
            ScopeOutcome::Completed(false),
            "an ack at another generation leaves doc-b's marker"
        );
        assert_eq!(marker_docs(&peerstore).await, ["doc-b"]);
        assert!(
            peerstore
                .get_replicator_retry_peers()
                .await
                .unwrap()
                .iter()
                .any(|(peer, _)| peer == PEER),
            "doc-b's marker keeps the schedule row doc-a's ack would have deleted alone"
        );
    })
    .unwrap()
}

#[test]
fn a_run_for_an_unknown_peer_registers_nothing() {
    workers::block_on(async {
        let peerstore = peerstore().await;
        let outcomes = peerstore
            .record_scopes("gone", &[observe("doc-a"), complete("doc-a", 1)])
            .await
            .together();

        assert_eq!(
            outcomes,
            [ScopeOutcome::Observed(None), ScopeOutcome::Completed(false)]
        );
        assert!(peerstore
            .get_retry_documents("gone")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn an_empty_scope_is_decided_without_a_commit() {
    workers::block_on(async {
        let peerstore = peerstore().await;
        let before = commits(&peerstore);
        let outcomes = peerstore
            .record_scopes(
                PEER,
                &[ScopeRecord::Observe {
                    doc_id: String::new(),
                    collection_id: String::new(),
                }],
            )
            .await
            .together();

        assert_eq!(outcomes, [ScopeOutcome::Observed(None)]);
        assert_eq!(commits(&peerstore), before);
    })
    .unwrap()
}

/// One record's error costs only that record, as its single call would: a
/// schedule row an ack cannot read fails the ack, and the registration beside
/// it still writes its marker (#3494 review).
#[test]
fn an_error_costs_only_its_own_record() {
    workers::block_on(async {
        let peerstore = peerstore().await;
        let ScopeOutcome::Observed(Some(old)) = peerstore
            .record_scopes(PEER, &[observe("doc-a")])
            .await
            .together()[0]
        else {
            panic!("the registration writes a marker");
        };
        let mut txn = peerstore.store.new_txn(false).await.unwrap();
        txn.set(
            &ReplicatorRetryIDKey::new(PEER).bytes(),
            b"not a retry schedule",
        )
        .await
        .unwrap();
        txn.commit().await.unwrap();
        assert!(
            peerstore
                .complete_retry_scope(PEER, "doc-a", COLLECTION, false, old, &["head".to_owned()])
                .await
                .is_err(),
            "the ack's single call fails on the unreadable schedule row"
        );

        let run = peerstore
            .record_scopes(PEER, &[observe("doc-c"), complete("doc-a", old)])
            .await;

        assert!(run.shared_error.is_some(), "the shared transaction fails");
        let [observed, completed] = <[_; 2]>::try_from(run.outcomes).unwrap();
        assert!(
            matches!(observed, Ok(ScopeOutcome::Observed(Some(generation))) if generation > old),
            "doc-c registers as its single call does: {observed:?}"
        );
        assert!(
            completed.is_err(),
            "doc-a's ack fails as its single call does"
        );
        assert_eq!(marker_docs(&peerstore).await, ["doc-a", "doc-c"]);
    })
    .unwrap()
}
