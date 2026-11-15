use db::chunk::window::{derive_search_chunks, SearchChunkingConfig};

#[test]
fn derive_search_chunks_returns_single_chunk_for_small_content() {
    let chunks = derive_search_chunks(
        "message-1",
        "content",
        "short assistant reply",
        &SearchChunkingConfig::default(),
    );

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_index, 0);
    assert_eq!(chunks[0].chunk_count, 1);
    assert_eq!(chunks[0].content, "short assistant reply");
}

#[test]
fn derive_search_chunks_splits_large_content_deterministically() {
    let content = (0..40)
        .map(|index| format!("chunk-token-{index:02}"))
        .collect::<Vec<_>>()
        .join(" ");
    let config = SearchChunkingConfig {
        max_chars: 80,
        overlap_chars: 16,
    };

    let first = derive_search_chunks("message-1", "content", &content, &config);
    let second = derive_search_chunks("message-1", "content", &content, &config);

    assert_eq!(first, second);
    assert!(first.len() > 1);
    assert_eq!(first[0].chunk_index, 0);
    assert_eq!(first.last().unwrap().chunk_count, first.len());
}
