use chunk_split::split::{
    cut_sentences, cut_words, fence_before, paragraphs, segments, split, units, Chunk, ChunkConfig,
    SEGMENT_CHUNKS,
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
fn a_heading_and_the_text_under_it_are_one_unit() {
    let max_chars = 64;
    let body = "x".repeat(SEGMENT_CHUNKS as usize * max_chars);
    let text = format!("Intro.\n\n# Title\n\n{body}");
    let paras = paragraphs(&text);
    let shape: Vec<usize> = units(&paras).iter().map(|u| u.paras.len()).collect();
    assert_eq!(shape, vec![1, 2]);
}

#[test]
fn consecutive_headings_are_units_of_their_own_but_the_last_keeps_its_text() {
    let paras = paragraphs("# A\n\n## B\n\nText.\n");
    let shape: Vec<Vec<&str>> = units(&paras)
        .iter()
        .map(|u| u.paras.iter().map(|p| p.text).collect())
        .collect();
    assert_eq!(shape, vec![vec!["# A\n\n"], vec!["## B\n\n", "Text.\n"]]);
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

fn segment_starts(text: &str, max_chars: usize) -> Vec<usize> {
    let paras = paragraphs(text);
    segments(&paras, max_chars)
        .iter()
        .map(|segment| segment[0].start)
        .collect()
}

fn one_segment(text: &str, max_chars: usize) -> bool {
    segment_starts(text, max_chars).len() == 1
}

fn fence_before_paragraph(text: &str, at: usize) -> bool {
    let start = paragraphs(text)[at].start;
    segment_starts(text, TYPO_MAX_CHARS).contains(&start)
}

// The characters of the segment holding paragraph `at`.
fn segment_around(text: &str, at: usize) -> std::ops::Range<usize> {
    let start = paragraphs(text)[at].start;
    let starts = segment_starts(text, TYPO_MAX_CHARS);
    let first = starts
        .iter()
        .rev()
        .find(|&&s| s <= start)
        .copied()
        .unwrap_or(0);
    let end = starts
        .iter()
        .find(|&&s| s > start)
        .copied()
        .unwrap_or(text.chars().count());
    first..end
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
fn a_longer_word_changes_only_its_own_chunk_when_the_packing_has_room() {
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
    // Picked so that paragraph 20 starts a segment after the edit.
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
fn a_unit_as_heavy_as_a_segment_always_starts_one() {
    let max_chars = 64;
    let limit = SEGMENT_CHUNKS as usize * max_chars;
    let heading = "# T";
    let body = "x".repeat(limit - heading.len());
    let text = format!("Before.\n\n{heading}\n\n{body}");
    let paras = paragraphs(&text);
    let units = units(&paras);
    assert_eq!(units[1].weight, limit);
    assert!(fence_before(&units[1], max_chars));
}

const PARAGRAPHS: usize = 2000;

fn short_paragraphs() -> String {
    (0..PARAGRAPHS)
        .map(|i| format!("Short paragraph {i} with a few words."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn short_paragraphs_fill_chunks_to_at_least_half_of_max_chars() {
    let text = short_paragraphs();
    for max_chars in [1000, 4000] {
        let chunks = split(&text, &config(max_chars, 0));
        let mean = text.chars().count() / chunks.len();
        assert!(2 * mean >= max_chars, "max {max_chars}: mean {mean}");
    }
}

fn sections(count: usize, body: &str) -> String {
    (0..count)
        .map(|i| format!("## Question {i}\n\n{body} {i}."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn mean_segment_chars(text: &str, max_chars: usize) -> usize {
    text.chars().count() / segment_starts(text, max_chars).len()
}

#[test]
fn heading_led_text_gets_segments_of_a_few_max_chars() {
    let max_chars = 200;
    let target = SEGMENT_CHUNKS as usize * max_chars;
    let body = "The answer has some words in it and then a number".repeat(5);
    for text in [
        sections(400, &body),
        (0..4000)
            .map(|i| format!("# Heading {i}"))
            .collect::<Vec<_>>()
            .join("\n\n"),
    ] {
        let mean = mean_segment_chars(&text, max_chars);
        assert!((target / 2..=2 * target).contains(&mean), "mean {mean}");
    }
}

#[test]
fn a_heading_joins_a_chunk_that_is_less_than_half_full() {
    let text = "Some intro.\n\n# Title\n\nUnder the title.\n";
    assert!(one_segment(text, 200));
    assert_eq!(split(text, &config(200, 0)).len(), 1);
}

#[test]
fn a_heading_starts_a_new_chunk_once_the_chunk_is_half_full() {
    let intro = "Intro words. ".repeat(8);
    let text = format!("{intro}\n\n# Title\n\nUnder the title.\n");
    assert!(one_segment(&text, 200));
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
    assert!(one_segment(&text, 160));
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

const PACK_MAX_CHARS: usize = 100;

fn chunk_texts(text: &str) -> Vec<String> {
    assert!(one_segment(text, PACK_MAX_CHARS), "{text:?} has a fence");
    split(text, &config(PACK_MAX_CHARS, 0))
        .into_iter()
        .map(|c| c.text)
        .collect()
}

// `s` padded with dots so it is `chars` long, blank lines included.
fn sized(s: &str, chars: usize) -> String {
    format!("{s}{}\n\n", ".".repeat(chars - s.chars().count() - 2))
}

#[test]
fn a_heading_right_after_a_half_full_heading_stays_with_it() {
    let first = sized("# A long first heading", 60);
    let text = format!("{first}## B\n\nText.\n");
    assert_eq!(chunk_texts(&text), vec![text.clone()]);
}

#[test]
fn a_tail_of_headings_moves_together_when_it_would_not_fit() {
    let intro = sized("Intro", 30);
    let (a, b) = (sized("# A", 40), sized("# B", 40));
    let text = format!("{intro}{a}{b}");
    assert_eq!(chunk_texts(&text), vec![intro, format!("{a}{b}")]);
}

#[test]
fn only_the_first_piece_of_a_long_heading_counts_as_the_heading() {
    let intro = sized("Intro", 20);
    let heading = format!("# {}", "word ".repeat(30));
    let text = format!("{intro}{heading}\n\nText.\n");
    let chunks = chunk_texts(&text);
    assert!(
        chunks[0].starts_with(&format!("{intro}# word")),
        "{chunks:?}"
    );
}

#[test]
fn a_heading_starts_a_new_chunk_at_exactly_half_full() {
    let intro = sized("Intro", PACK_MAX_CHARS / 2);
    let text = format!("{intro}# T\n\nText.\n");
    assert_eq!(
        chunk_texts(&text),
        vec![intro, "# T\n\nText.\n".to_string()]
    );
}

#[test]
fn a_heading_whose_text_fits_exactly_stays_in_the_chunk() {
    let intro = sized("Intro", 30);
    let section = format!("# T\n\n{}", ".".repeat(PACK_MAX_CHARS - 30 - 5));
    let text = format!("{intro}{section}");
    assert_eq!(text.chars().count(), PACK_MAX_CHARS);
    assert_eq!(chunk_texts(&text), vec![text.clone()]);
}
