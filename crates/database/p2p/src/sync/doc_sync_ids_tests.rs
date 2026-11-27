use async_trait::async_trait;
use cid::Cid;

use super::*;
use crate::error::{Error, Result};

/// "book" has five chunks, "broken" fails its lookup, and the rest have none.
/// Counts the follower lookups it answers.
#[derive(Default)]
struct Followers {
    lookups: std::sync::atomic::AtomicUsize,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentHeadProvider for Followers {
    async fn get_document_heads(&self, _doc_id: &str) -> Result<Vec<Cid>> {
        Ok(Vec::new())
    }

    async fn get_collection_heads(&self, _collection_id: &str) -> Result<Vec<Cid>> {
        Ok(Vec::new())
    }

    async fn following_doc_ids(&self, doc_id: &str) -> Result<Vec<String>> {
        self.lookups
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match doc_id {
            "book" => Ok((0..5).map(|i| format!("chunk{i}")).collect()),
            "broken" => Err(Error::HeadProvider("lookup failed".into())),
            _ => Ok(Vec::new()),
        }
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn refusal(result: std::result::Result<Vec<String>, TooMany>) -> String {
    match result {
        Err(refusal) => refusal.to_string(),
        other => panic!("the request must be refused, got {other:?}"),
    }
}

#[nuclei::test]
async fn followers_come_after_the_requested_ids() {
    let requested = ids(&["book", "plain"]);
    let all = with_followers(&Followers::default(), &requested, 7)
        .await
        .unwrap();
    let mut expected = requested.clone();
    expected.extend((0..5).map(|i| format!("chunk{i}")));
    assert_eq!(all, expected);
}

#[nuclei::test]
async fn a_failed_follower_lookup_keeps_the_requested_id() {
    let requested = ids(&["broken", "book"]);
    let all = with_followers(&Followers::default(), &requested, 7)
        .await
        .unwrap();
    assert_eq!(all.len(), 2 + 5);
    assert_eq!(&all[..2], requested.as_slice());
}

#[nuclei::test]
async fn more_requested_ids_than_the_limit_are_refused_before_any_lookup() {
    let provider = Followers::default();
    let message = refusal(with_followers(&provider, &ids(&["a", "b", "c"]), 2).await);
    assert_eq!(
        message,
        "DocSyncRequest contains 3 doc IDs, exceeding the limit of 2"
    );
    assert_eq!(
        provider.lookups.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[nuclei::test]
async fn a_request_at_the_limit_is_refused_when_its_chunks_pass_it() {
    let provider = Followers::default();
    let requested = ids(&["book", "plain", "other"]);
    let message = refusal(with_followers(&provider, &requested, 3).await);
    assert_eq!(
        message,
        "DocSyncRequest with its documents' chunks exceeds the limit of 3 doc IDs"
    );
    // The first parent already passes the limit, so the others are never looked up.
    assert_eq!(
        provider.lookups.load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[nuclei::test]
async fn chunks_that_fit_the_limit_are_served() {
    let all = with_followers(&Followers::default(), &ids(&["book"]), 6)
        .await
        .unwrap();
    assert_eq!(all.len(), 6);
}

#[nuclei::test]
async fn an_id_listed_twice_or_among_the_chunks_counts_once() {
    let requested = ids(&["book", "book", "chunk1", "chunk3"]);
    let all = with_followers(&Followers::default(), &requested, 6)
        .await
        .unwrap();
    let mut expected = ids(&["book", "chunk1", "chunk3"]);
    expected.extend(["chunk0", "chunk2", "chunk4"].map(str::to_string));
    assert_eq!(all, expected);
}

#[nuclei::test]
async fn the_refusal_reports_the_count_that_went_over() {
    let refused = with_followers(&Followers::default(), &ids(&["book"]), 3)
        .await
        .unwrap_err();
    assert_eq!(refused, TooMany::WithChunks { count: 6, max: 3 });
    assert_eq!(refused.count(), 6);
}
