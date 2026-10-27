use db::chunk::split::{cut_sentences, cut_words, paragraphs, split, ChunkConfig};

fn config(max_chars: usize, overlap: usize) -> ChunkConfig {
    ChunkConfig { max_chars, overlap }
}

fn cores<'a>(text: &'a str, chunks: &[db::chunk::split::Chunk]) -> Vec<&'a str> {
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    chunks
        .iter()
        .map(|c| &text[offsets[c.start]..offsets[c.end]])
        .collect()
}

#[test]
fn empty_text_has_no_chunks() {
    assert!(split("", &config(100, 0)).is_empty());
}

#[test]
fn short_text_is_one_chunk() {
    let chunks = split("Hello there.", &config(100, 0));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, "Hello there.");
    assert_eq!((chunks[0].start, chunks[0].end), (0, 12));
}

#[test]
fn paragraphs_keep_their_blank_lines() {
    let text = "\n\nFirst line\nsecond line\n\n\nNext.\n";
    let paras: Vec<&str> = paragraphs(text).iter().map(|p| p.text).collect();
    assert_eq!(paras, vec!["\n\nFirst line\nsecond line\n\n\n", "Next.\n"]);
}

#[test]
fn headings_are_paragraphs_of_their_own() {
    let text = "Intro text\n# Title\nBody under it\nTitle two\n=========\nMore\n";
    let paras = paragraphs(text);
    let shape: Vec<(&str, bool)> = paras.iter().map(|p| (p.text, p.heading)).collect();
    assert_eq!(
        shape,
        vec![
            ("Intro text\n", false),
            ("# Title\n", true),
            ("Body under it\nTitle two\n=========\n", true),
            ("More\n", false),
        ]
    );
}

#[test]
fn hash_tag_without_space_is_not_a_heading() {
    let paras = paragraphs("#hashtag here\n");
    assert!(!paras[0].heading);
}

#[test]
fn heading_stays_with_the_text_under_it() {
    let text = "# Title\n\nA short paragraph.\n";
    let chunks = split(text, &config(200, 0));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, text);
}

#[test]
fn a_heading_starts_a_new_chunk() {
    let text = "Some intro.\n\n# Title\n\nUnder the title.\n";
    let chunks = split(text, &config(200, 0));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["Some intro.\n\n", "# Title\n\nUnder the title.\n"]
    );
}

#[test]
fn sentences_join_back() {
    let text = "One. Two!  Three? \"Four.\" Five";
    let pieces = cut_sentences(text);
    assert_eq!(
        pieces,
        vec!["One. ", "Two!  ", "Three? ", "\"Four.\" ", "Five"]
    );
    assert_eq!(pieces.concat(), text);
}

#[test]
fn a_dot_inside_a_word_is_not_a_sentence_end() {
    assert_eq!(
        cut_sentences("v1.2 is out. Yes"),
        vec!["v1.2 is out. ", "Yes"]
    );
}

#[test]
fn words_join_back() {
    let text = "  lead word  two\tthree ";
    let pieces = cut_words(text);
    assert_eq!(pieces, vec!["  ", "lead ", "word  ", "two\t", "three "]);
    assert_eq!(pieces.concat(), text);
}

#[test]
fn long_paragraph_cuts_by_sentences() {
    let text = "Alpha beta gamma. Delta epsilon zeta. Eta theta iota.";
    let chunks = split(text, &config(40, 0));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["Alpha beta gamma. Delta epsilon zeta. ", "Eta theta iota."]
    );
}

#[test]
fn long_word_cuts_by_characters() {
    let text = "x".repeat(10);
    let chunks = split(&text, &config(4, 0));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, vec!["xxxx", "xxxx", "xx"]);
}

#[test]
fn lengths_count_characters_not_bytes() {
    let text = "日本語のテキスト。".repeat(4);
    for chunk in split(&text, &config(10, 0)) {
        assert!(chunk.text.chars().count() <= 10);
    }
}

#[test]
fn overlap_borrows_trailing_sentences_that_fit() {
    let text = "A a. B b. C c. D d.";
    let chunks = split(text, &config(14, 1));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, vec!["A a. B b. ", "B b. C c. D d."]);
    assert_eq!(cores(text, &chunks), vec!["A a. B b. ", "C c. D d."]);
}

#[test]
fn overlap_is_dropped_when_it_would_not_fit() {
    let text = "Aaaaaaaaaa. Bbbbbbbbbbbbbbbbbbb.";
    let chunks = split(text, &config(20, 1));
    assert_eq!(chunks[1].text, "Bbbbbbbbbbbbbbbbbbb.");
}

#[test]
fn same_text_gives_same_chunks() {
    let text = "# A\n\nSome text. More text.\n\nAnother paragraph here.\n";
    assert_eq!(split(text, &config(30, 1)), split(text, &config(30, 1)));
}

#[test]
fn hash_is_the_sha256_of_the_stored_text() {
    let chunks = split("abc", &config(10, 0));
    assert_eq!(
        chunks[0].hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

fn forty_paragraphs() -> String {
    (0..40)
        .map(|i| format!("Paragraph number {i} talks about its own small topic."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn changed_chunks(before: &str, after: &str) -> usize {
    let old = split(before, &config(160, 0));
    let new = split(after, &config(160, 0));
    assert_eq!(old.len(), new.len());
    (0..old.len())
        .filter(|&i| old[i].hash != new[i].hash)
        .count()
}

#[test]
fn a_typo_fix_changes_only_its_own_chunk() {
    let before = forty_paragraphs();
    let after = before.replace("number 20 talks", "number 20 takls");
    assert_eq!(paragraphs(&before)[20].pick, paragraphs(&after)[20].pick);
    assert_eq!(changed_chunks(&before, &after), 1);
}

#[test]
fn a_typo_that_moves_a_fence_changes_only_its_neighbours() {
    let before = forty_paragraphs();
    let after = before.replace("number 20 talks", "number 20 tlaks");
    assert_ne!(paragraphs(&before)[20].pick, paragraphs(&after)[20].pick);
    assert_eq!(changed_chunks(&before, &after), 2);
}
