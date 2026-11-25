use chunk_split::hash::distinct;
use chunk_split::split::{split, ChunkConfig, SplitterVersion};

#[test]
fn repeated_text_needs_one_chunk_document() {
    // Each section is over half of max_chars, so every heading starts a new chunk.
    let chorus = format!("# Chorus\n\n{}\n\n", "La ".repeat(14));
    let text = format!("{chorus}{chorus}# Verse\n\nSomething else.\n");
    let chunks = split(
        &text,
        &ChunkConfig {
            version: SplitterVersion::CURRENT,
            max_chars: 64,
            overlap: 0,
        },
    );
    let needed = distinct(&chunks);
    assert!(needed.len() < chunks.len());
    let mut hashes: Vec<&str> = needed.iter().map(|c| c.hash.as_str()).collect();
    hashes.dedup();
    assert_eq!(hashes.len(), needed.len());
}
