use std::sync::Arc;

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{DagFetchLimiter, SyncDiagnostics, MAX_DOC_CLAIMS};

fn cid(seed: u8) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(&[seed]))
}

#[nuclei::test(threads = 0)]
async fn a_second_fetch_of_a_document_starts_when_the_first_ends_and_names_its_root() {
    let limiter = DagFetchLimiter::new(4);
    let diagnostics = Arc::new(SyncDiagnostics::default());
    let mut first = limiter
        .claim_doc("users", "doc", &diagnostics)
        .await
        .expect("first claim");
    assert!(limiter.doc_busy("users", "doc"));
    let other = limiter
        .claim_doc("users", "other", &diagnostics)
        .await
        .expect("other documents are independent");

    let waiting = {
        let limiter = limiter.clone();
        let diagnostics = Arc::clone(&diagnostics);
        crate::rt::spawn(async move {
            let claim = limiter.claim_doc("users", "doc", &diagnostics).await;
            claim.map(|claim| claim.handed().to_vec())
        })
    };
    crate::rt::yield_now().await;
    assert_eq!(diagnostics.snapshot().dag_fetch_doc_waits, 1);

    first.completed(cid(1));
    drop(first);
    assert_eq!(
        waiting.join().await.unwrap(),
        Some(vec![cid(1)]),
        "the waiter runs on the drop and names the completed root"
    );
    assert!(!limiter.doc_busy("users", "doc"));
    let later = limiter
        .claim_doc("users", "doc", &diagnostics)
        .await
        .expect("a later fetch");
    assert_eq!(
        later.handed(),
        [cid(1)],
        "a fetch that never waited still names the completed root"
    );
    drop(other);
}

#[nuclei::test(threads = 0)]
async fn a_full_completed_roots_memory_is_cleared_counted_and_fetches_go_on() {
    let limiter = DagFetchLimiter::new(4);
    let diagnostics = Arc::new(SyncDiagnostics::default());
    for doc in 0..MAX_DOC_CLAIMS {
        let mut claim = limiter
            .claim_doc("users", &doc.to_string(), &diagnostics)
            .await
            .expect("claim");
        claim.completed(cid(1));
    }
    assert_eq!(limiter.doc_completed.len(), MAX_DOC_CLAIMS);
    assert_eq!(diagnostics.snapshot().dag_fetch_doc_completed_cleared, 0);

    let mut past = limiter
        .claim_doc("users", "past-the-cap", &diagnostics)
        .await
        .expect("a full memory never blocks a fetch");
    assert!(past.handed().is_empty());
    past.completed(cid(2));
    drop(past);
    assert_eq!(diagnostics.snapshot().dag_fetch_doc_completed_cleared, 1);
    assert_eq!(limiter.doc_completed.len(), 1);
    assert_eq!(limiter.docs.len(), 0);

    let next = limiter
        .claim_doc("users", "past-the-cap", &diagnostics)
        .await
        .expect("the next fetch");
    assert_eq!(next.handed(), [cid(2)], "the clear keeps the newest roots");
    drop(next);
    let forgotten = limiter
        .claim_doc("users", "0", &diagnostics)
        .await
        .expect("a cleared document still fetches");
    assert!(forgotten.handed().is_empty());
    assert!(limiter.doc_busy("users", "0"));
}

#[nuclei::test(threads = 0)]
async fn a_full_table_lets_a_fetch_run_unguarded_and_counts_it() {
    let limiter = DagFetchLimiter::new(4);
    let diagnostics = Arc::new(SyncDiagnostics::default());
    let mut claims = Vec::with_capacity(MAX_DOC_CLAIMS);
    for doc in 0..MAX_DOC_CLAIMS {
        claims.push(
            limiter
                .claim_doc("users", &doc.to_string(), &diagnostics)
                .await
                .expect("claim"),
        );
    }

    let unguarded = limiter
        .claim_doc("users", "past-the-cap", &diagnostics)
        .await
        .expect("a full table never blocks a fetch");
    assert_eq!(diagnostics.snapshot().dag_fetch_doc_claims_full, 1);
    assert!(!limiter.doc_busy("users", "past-the-cap"));
    drop(unguarded);
    assert!(
        limiter.doc_busy("users", "0"),
        "an unguarded drop frees nothing"
    );

    drop(claims);
    let guarded = limiter
        .claim_doc("users", "past-the-cap", &diagnostics)
        .await;
    assert!(guarded.is_some() && limiter.doc_busy("users", "past-the-cap"));
}

#[nuclei::test(threads = 0)]
async fn closing_ends_a_waiting_claim() {
    let limiter = DagFetchLimiter::new(4);
    let diagnostics = Arc::new(SyncDiagnostics::default());
    let held = limiter.claim_doc("users", "doc", &diagnostics).await;
    let waiting = {
        let limiter = limiter.clone();
        let diagnostics = Arc::clone(&diagnostics);
        crate::rt::spawn(async move {
            limiter
                .claim_doc("users", "doc", &diagnostics)
                .await
                .is_none()
        })
    };
    crate::rt::yield_now().await;

    limiter.close();
    assert!(waiting.join().await.unwrap());
    drop(held);
}
