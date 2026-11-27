use serde_json::json;

use crate::replicator::{Admission, EqOnlyFilterMatcher, ReplicationFilter, ReplicatorInfo};

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

fn chunk_admission(
    peer: &ReplicatorInfo,
    decided_in: &str,
    document: Option<serde_json::Value>,
) -> Admission {
    peer.admission(&EqOnlyFilterMatcher, CHUNKS, decided_in, document.as_ref())
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
fn a_chunk_pair_needs_both_collections_replicated() {
    let mut info = public_articles_peer();
    info.collections = vec![CHUNKS.to_string()];

    assert!(info
        .with_chunk_sources([(CHUNKS.to_string(), ARTICLES.to_string())])
        .chunk_sources
        .is_empty());
}

#[test]
fn a_chunk_of_a_filtered_parent_counts_as_filtered() {
    assert!(public_articles_peer().is_filtered_for_collection(CHUNKS));
}

#[test]
fn a_chunk_never_passes_on_its_own_document() {
    let own_document = json!({"kind": "public", "text": "t"});

    assert!(!public_articles_peer().matches_filter(&EqOnlyFilterMatcher, CHUNKS, &own_document));
    assert_eq!(
        chunk_admission(&public_articles_peer(), CHUNKS, Some(own_document)),
        Admission::NoDocument
    );
}

#[test]
fn a_chunk_of_a_matching_parent_is_admitted() {
    assert_eq!(
        chunk_admission(
            &public_articles_peer(),
            ARTICLES,
            Some(json!({"kind": "public"}))
        ),
        Admission::Admit
    );
}

#[test]
fn a_chunk_of_an_unmatched_parent_is_refused() {
    assert_eq!(
        chunk_admission(
            &public_articles_peer(),
            ARTICLES,
            Some(json!({"kind": "secret"}))
        ),
        Admission::FilterMiss
    );
}

#[test]
fn a_chunk_without_its_parents_document_is_refused() {
    assert_eq!(
        chunk_admission(&public_articles_peer(), ARTICLES, None),
        Admission::NoDocument
    );
}

#[test]
fn a_record_without_chunk_sources_still_decides_a_chunk_by_its_parent() {
    let mut info = public_articles_peer();
    info.chunk_sources.clear();

    assert_eq!(
        chunk_admission(&info, ARTICLES, Some(json!({"kind": "secret"}))),
        Admission::FilterMiss
    );
    assert_eq!(
        chunk_admission(&info, ARTICLES, Some(json!({"kind": "public"}))),
        Admission::Admit
    );
}

#[test]
fn a_filtered_document_without_its_document_is_refused() {
    assert_eq!(
        public_articles_peer().admission(&EqOnlyFilterMatcher, ARTICLES, ARTICLES, None),
        Admission::NoDocument
    );
}

#[test]
fn removing_the_parent_takes_its_chunk_collection() {
    let mut info = public_articles_peer();

    info.remove_collection(ARTICLES);

    assert!(info.collections.is_empty());
    assert!(info.filters.is_empty());
    assert!(info.chunk_sources.is_empty());
}
