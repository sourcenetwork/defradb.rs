use chunk_split::list::{chunk_list, describes, distinct, text_hash};
use chunk_split::split::{split, ChunkConfig};

#[test]
fn list_starts_with_the_text_hash_then_chunks_in_order() {
    let text = "First part.\n\n# Next\n\nSecond part.\n";
    let chunks = split(
        text,
        &ChunkConfig {
            max_chars: 64,
            overlap: 0,
        },
    );
    let list = chunk_list(text, &chunks);
    assert_eq!(list[0], text_hash(text));
    let hashes: Vec<&String> = chunks.iter().map(|c| &c.hash).collect();
    assert_eq!(list[1..].iter().collect::<Vec<_>>(), hashes);
    assert!(describes(&list, text));
    assert!(!describes(&list, "another text"));
}

#[test]
fn repeated_text_needs_one_chunk_document() {
    let text = "# Chorus\n\nLa la la.\n\n# Chorus\n\nLa la la.\n\n# Verse\n\nSomething else.\n";
    let chunks = split(
        text,
        &ChunkConfig {
            max_chars: 64,
            overlap: 0,
        },
    );
    let list = chunk_list(text, &chunks);
    assert_eq!(list.len(), chunks.len() + 1);
    let needed = distinct(&chunks);
    assert!(needed.len() < chunks.len());
    let mut hashes: Vec<&str> = needed.iter().map(|c| c.hash.as_str()).collect();
    hashes.dedup();
    assert_eq!(hashes.len(), needed.len());
}

#[test]
fn empty_text_lists_only_its_hash() {
    assert_eq!(chunk_list("", &[]), vec![text_hash("")]);
}
