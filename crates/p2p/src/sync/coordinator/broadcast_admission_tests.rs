use bytes::Bytes;
use cid::Cid;
use serde_json::json;

use super::{Admission, PendingPush};
use crate::replicator::{EqOnlyFilterMatcher, ReplicationFilter, ReplicatorInfo};

const ARTICLES: &str = "articles";
const CHUNKS: &str = "articles__body_chunk";

// A peer that takes only public articles, with their chunks.
fn public_articles_peer() -> ReplicatorInfo {
    ReplicatorInfo::from_raw_with_filters(
        "peer".to_string(),
        vec![ARTICLES.to_string(), CHUNKS.to_string()],
        Vec::new(),
        [(
            ARTICLES.to_string(),
            ReplicationFilter::eq("kind", json!("public")),
        )]
        .into(),
    )
    .with_chunk_sources([(CHUNKS.to_string(), ARTICLES.to_string())])
}

fn chunk_push(
    document: Option<serde_json::Value>,
    source: Option<serde_json::Value>,
) -> PendingPush {
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
