use chunk_split::split::{
    cut_sentences, cut_words, fence_before, paragraphs, split, Chunk, ChunkConfig, SEGMENT_CHUNKS,
};

fn config(max_chars: usize, overlap: usize) -> ChunkConfig {
    ChunkConfig { max_chars, overlap }
}

fn cores<'a>(text: &'a str, chunks: &[Chunk]) -> Vec<&'a str> {
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
fn heading_keeps_its_text_whatever_the_hash_picks() {
    for i in 0..200 {
        let text = format!("# Title {i}\n\nBody number {i}.\n");
        assert_eq!(split(&text, &config(200, 0)).len(), 1, "{text}");
    }
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

const TYPO_MAX_CHARS: usize = 160;
const EDITED: usize = 20;

fn fence_before_paragraph(text: &str, at: usize) -> bool {
    let paras = paragraphs(text);
    fence_before(&paras[at - 1], &paras[at], TYPO_MAX_CHARS)
}

// The characters of the segment holding paragraph `at`.
fn segment_around(text: &str, at: usize) -> std::ops::Range<usize> {
    let paras = paragraphs(text);
    let fence = |i: usize| fence_before(&paras[i - 1], &paras[i], TYPO_MAX_CHARS);
    let first = (1..=at).rev().find(|&i| fence(i)).unwrap_or(0);
    let end = (at + 1..paras.len())
        .find(|&i| fence(i))
        .map_or(text.chars().count(), |i| paras[i].start);
    paras[first].start..end
}

// The chunks of `after` whose text `before` did not have.
fn new_chunks(before: &str, after: &str) -> Vec<Chunk> {
    let old = split(before, &config(TYPO_MAX_CHARS, 0));
    split(after, &config(TYPO_MAX_CHARS, 0))
        .into_iter()
        .filter(|c| !old.iter().any(|o| o.hash == c.hash))
        .collect()
}

#[test]
fn a_typo_fix_changes_only_its_own_chunk() {
    let before = forty_paragraphs();
    let after = before.replace("number 20 talks", "number 20 takls");
    assert_eq!(
        fence_before_paragraph(&before, EDITED),
        fence_before_paragraph(&after, EDITED)
    );
    assert_eq!(new_chunks(&before, &after).len(), 1);
}

#[test]
fn a_word_that_changes_the_length_changes_only_its_own_chunk() {
    let before = forty_paragraphs();
    let after = before.replace("number 20 talks", "number 20 talks loudly");
    assert_eq!(
        fence_before_paragraph(&before, EDITED),
        fence_before_paragraph(&after, EDITED)
    );
    assert_eq!(new_chunks(&before, &after).len(), 1);
}

#[test]
fn a_typo_that_moves_a_fence_changes_only_the_segment_it_splits() {
    let before = forty_paragraphs();
    let after = before.replace("number 20 talks", "number 20 atlks");
    assert!(!fence_before_paragraph(&before, EDITED));
    assert!(fence_before_paragraph(&after, EDITED));
    let segment = segment_around(&before, EDITED);
    let changed = new_chunks(&before, &after);
    assert!(!changed.is_empty());
    for chunk in changed {
        assert!(
            segment.contains(&chunk.start) && chunk.end <= segment.end,
            "{chunk:?}"
        );
    }
}

#[test]
fn a_paragraph_as_long_as_a_segment_always_starts_one() {
    let max_chars = 64;
    let long = "word ".repeat(SEGMENT_CHUNKS as usize * max_chars);
    let text = format!("Before.\n\n{long}");
    let paras = paragraphs(&text);
    assert!(fence_before(&paras[0], &paras[1], max_chars));
}

fn short_paragraphs(count: usize) -> String {
    (0..count)
        .map(|i| format!("Short paragraph {i} with a few words."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn short_paragraphs_fill_chunks_close_to_max_chars() {
    for max_chars in [200, 1000, 4000] {
        let text = short_paragraphs(max_chars);
        let chunks = split(&text, &config(max_chars, 0));
        let mean = text.chars().count() / chunks.len();
        assert!(2 * mean >= max_chars, "max {max_chars}: mean {mean}");
    }
}

#[test]
fn a_heading_joins_a_chunk_that_is_less_than_half_full() {
    let text = "Some intro.\n\n# Title\n\nUnder the title.\n";
    assert_eq!(split(text, &config(200, 0)).len(), 1);
}

#[test]
fn a_heading_starts_a_new_chunk_once_the_chunk_is_half_full() {
    let intro = "Intro words. ".repeat(8);
    let text = format!("{intro}\n\n# Title\n\nUnder the title.\n");
    let chunks = split(&text, &config(200, 0));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            format!("{intro}\n\n").as_str(),
            "# Title\n\nUnder the title.\n"
        ]
    );
}

#[test]
fn a_heading_moves_to_the_next_chunk_when_its_text_would_not_fit() {
    let under = "Text under the title. ".repeat(6);
    let text = format!("Intro with a few more words.\n\n# Title\n\n{under}");
    let chunks = split(&text, &config(160, 0));
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "Intro with a few more words.\n\n",
            format!("# Title\n\n{under}").as_str()
        ]
    );
}
