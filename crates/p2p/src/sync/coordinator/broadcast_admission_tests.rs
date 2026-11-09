use bytes::Bytes;
use cid::Cid;
use serde_json::json;

use super::{Admission, PendingPush};
use crate::replicator::chunk_tests::{public_articles_peer, ARTICLES, CHUNKS};
use crate::replicator::EqOnlyFilterMatcher;

fn chunk_push(
    document: Option<serde_json::Value>,
    source: Option<serde_json::Value>,
) -> PendingPush {
    let source = source.map(std::sync::Arc::new);
    PendingPush {
        cid: Cid::default(),
        block: Bytes::new(),
        doc_id: "chunk".to_string(),
        collection_id: CHUNKS.to_string(),
        creator: String::new(),
        document,
        source,
    }
}

#[test]
fn a_chunk_of_a_matching_parent_is_pushed() {
    let push = chunk_push(None, Some(json!({"kind": "public"})));

    assert_eq!(
        push.admission(&public_articles_peer(), &EqOnlyFilterMatcher),
        Admission::Admit
    );
}

#[test]
fn a_chunk_of_an_unmatched_parent_gets_no_push_job() {
    let push = chunk_push(None, Some(json!({"kind": "secret"})));

    assert_eq!(
        push.admission(&public_articles_peer(), &EqOnlyFilterMatcher),
        Admission::FilterMiss
    );
}

#[test]
fn a_chunk_without_its_parents_document_is_not_pushed_even_if_its_own_would_match() {
    let push = chunk_push(Some(json!({"kind": "public", "text": "t"})), None);

    assert_eq!(
        push.admission(&public_articles_peer(), &EqOnlyFilterMatcher),
        Admission::NoDocument
    );
}

#[test]
fn a_filtered_document_whose_document_is_not_at_hand_is_not_pushed() {
    let push = PendingPush {
        collection_id: ARTICLES.to_string(),
        ..chunk_push(None, None)
    };

    assert_eq!(
        push.admission(&public_articles_peer(), &EqOnlyFilterMatcher),
        Admission::NoDocument
    );
}
