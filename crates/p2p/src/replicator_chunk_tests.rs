use serde_json::json;

use crate::replicator::{EqOnlyFilterMatcher, ReplicationFilter, ReplicatorInfo};

pub(crate) const ARTICLES: &str = "articles";
pub(crate) const CHUNKS: &str = "articles__body_chunk";
const PEER: &str = "12D3KooWGjMkcMy5PM9iSbgWWgUnH5dQhvzhNu7w3Gk4kHZBsxnJ";

/// A peer that takes only public articles, with their chunks.
pub(crate) fn public_articles_peer() -> ReplicatorInfo {
    ReplicatorInfo::from_raw_with_filters(
        PEER.to_string(),
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

#[test]
fn chunk_sources_round_trip_beside_the_filters() {
    let info = public_articles_peer();

    let bytes = info.to_bytes().unwrap();
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        r#"{"ID":"12D3KooWGjMkcMy5PM9iSbgWWgUnH5dQhvzhNu7w3Gk4kHZBsxnJ","Addresses":[],"CollectionIDs":["articles","articles__body_chunk"],"Status":0,"LastStatusChange":"0001-01-01T00:00:00Z","Filters":{"articles":{"predicate":{"kind":{"_eq":"public"}}}},"ChunkSources":{"articles__body_chunk":"articles"}}"#
    );
    assert_eq!(ReplicatorInfo::from_bytes(&bytes).unwrap(), info);
}

#[test]
fn only_the_chunks_of_a_filtered_parent_get_a_source() {
    let info = ReplicatorInfo::from_raw(
        PEER.to_string(),
        vec![ARTICLES.to_string(), CHUNKS.to_string()],
        Vec::new(),
    )
    .with_chunk_sources([(CHUNKS.to_string(), ARTICLES.to_string())]);

    assert!(info.chunk_sources.is_empty());
    assert!(!info.is_filtered_for_collection(CHUNKS));
}

#[test]
fn a_chunk_of_a_filtered_parent_counts_as_filtered() {
    assert!(public_articles_peer().is_filtered_for_collection(CHUNKS));
}

#[test]
fn a_chunk_never_passes_on_its_own_document() {
    let own_document = json!({"kind": "public", "text": "t"});

    assert!(!public_articles_peer().matches_filter(&EqOnlyFilterMatcher, CHUNKS, &own_document));
}
