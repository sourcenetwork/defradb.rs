//! Property tests mirroring the Lean theorems in `proofs/lean/ChunkSplit`.

use chunk_split::split::{
    cut_sentences, cut_words, paragraphs, segments, split, units, ChunkConfig, SEGMENT_CHUNKS,
};
use proptest::prelude::*;

fn word() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,9}",
        "[A-Z][a-z]{0,6}",
        Just("é".to_string()),
        Just("日本".to_string()),
        Just("x".repeat(70)),
    ]
}

fn sentence() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(word(), 1..12),
        prop_oneof![Just("."), Just("!"), Just("?"), Just("")],
    )
        .prop_map(|(words, end)| format!("{}{end}", words.join(" ")))
}

fn paragraph() -> impl Strategy<Value = String> {
    prop_oneof![
        1 => prop::collection::vec(word(), 1..4).prop_map(|w| format!("# {}", w.join(" "))),
        6 => prop::collection::vec(sentence(), 1..8).prop_map(|s| s.join(" ")),
    ]
}

fn text() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(paragraph(), 0..14),
        prop_oneof![Just("\n\n"), Just("\n\n\n"), Just("\n \n")],
    )
        .prop_map(|(paras, sep)| paras.join(sep))
}

fn config() -> impl Strategy<Value = ChunkConfig> {
    (1usize..200, 0usize..3).prop_map(|(max_chars, overlap)| ChunkConfig { max_chars, overlap })
}

proptest! {
    #[test]
    fn paragraphs_join_back(text in text()) {
        let paras = paragraphs(&text);
        let joined: String = paras.iter().map(|p| p.text).collect();
        prop_assert_eq!(&joined, &text);
        let mut offset = 0;
        for p in &paras {
            prop_assert_eq!(p.start, offset);
            offset += p.chars;
        }
    }

    // Mirrors `LevelsJoin` in Covers.lean: cutting never adds or drops text.
    #[test]
    fn cutters_join_back(text in text()) {
        prop_assert_eq!(cut_sentences(&text).concat(), text.clone());
        prop_assert_eq!(cut_words(&text).concat(), text);
    }

    // Mirrors `covers`: the chunks' own texts cover the text, and each chunk ends
    // with its own text.
    #[test]
    fn covers(text in text(), config in config()) {
        let chunks = split(&text, &config);
        let chars: Vec<char> = text.chars().collect();
        let mut offset = 0;
        for chunk in &chunks {
            prop_assert_eq!(chunk.start, offset);
            prop_assert!(chunk.end > chunk.start);
            let core: String = chars[chunk.start..chunk.end].iter().collect();
            prop_assert!(chunk.text.ends_with(&core));
            offset = chunk.end;
        }
        prop_assert_eq!(offset, chars.len());
    }

    // Mirrors `bounded`: every chunk has 1 to `max_chars` characters, repeats included.
    #[test]
    fn bounded(text in text(), config in config()) {
        for chunk in split(&text, &config) {
            let len = chunk.text.chars().count();
            prop_assert!((1..=config.max_chars).contains(&len), "{len} chars");
        }
    }

    // Mirrors `units_edit`: an edit that keeps a paragraph a heading or text keeps
    // every unit's paragraphs.
    #[test]
    fn units_keep_their_shape_when_the_heading_flags_do(text in text(), pick in any::<prop::sample::Index>(), insert in "[a-z]{1,6}") {
        prop_assume!(!paragraphs(&text).is_empty());
        let Some(edited) = append_word(&text, pick, &insert) else { return Ok(()) };
        let shape = |t: &str| {
            let paras = paragraphs(t);
            units(&paras).iter().map(|u| u.paras.len()).collect::<Vec<_>>()
        };
        prop_assert_eq!(shape(&text), shape(&edited));
    }

    // Segments follow the text size on text where every paragraph sits under a heading.
    #[test]
    fn heading_led_segments_average_a_few_max_chars(
        bodies in prop::collection::vec(prop::collection::vec(sentence(), 1..6), 500..600),
        max in 64usize..=200,
    ) {
        let text = bodies
            .iter()
            .enumerate()
            .map(|(i, body)| format!("## Section {i}\n\n{}", body.join(" ")))
            .collect::<Vec<_>>()
            .join("\n\n");
        let paras = paragraphs(&text);
        let mean = text.chars().count() / segments(&paras, max).len();
        prop_assert!(mean <= 2 * SEGMENT_CHUNKS as usize * max, "mean {} at max {}", mean, max);
    }

    // Mirrors `local_edit`: editing one paragraph without moving the fence before
    // its unit leaves every chunk outside its segment unchanged.
    #[test]
    fn local_edit(text in text(), config in config(), pick in any::<prop::sample::Index>(), insert in "[a-z]{1,6}") {
        let max = config.max_chars;
        let paras = paragraphs(&text);
        prop_assume!(!paras.is_empty());
        let Some(edited) = append_word(&text, pick, &insert) else { return Ok(()) };
        let at = pick.index(paras.len());
        let starts = |t: &str| {
            let paras = paragraphs(t);
            segments(&paras, max).iter().map(|s| s[0].start).collect::<Vec<_>>()
        };
        let (old_starts, new_starts) = (starts(&text), starts(&edited));
        let p_start = paras[at].start;
        let shift = edited.chars().count() - text.chars().count();
        let moved = |s: usize| if s > p_start { s + shift } else { s };
        prop_assume!(old_starts.iter().map(|&s| moved(s)).collect::<Vec<_>>() == new_starts);

        let start_char = old_starts.iter().rev().copied().find(|&s| s <= p_start).unwrap_or(0);
        let end_char = old_starts.iter().copied().find(|&s| s > p_start).unwrap_or(text.chars().count());
        let old = split(&text, &config);
        let new = split(&edited, &config);
        let old_before: Vec<_> = old.iter().filter(|c| c.end <= start_char).collect();
        let new_before: Vec<_> = new.iter().filter(|c| c.end <= start_char).collect();
        prop_assert_eq!(old_before, new_before);
        let old_after: Vec<_> = old.iter().filter(|c| c.start >= end_char).map(|c| (c.start + shift, &c.hash)).collect();
        let new_after: Vec<_> = new.iter().filter(|c| c.start >= end_char + shift).map(|c| (c.start, &c.hash)).collect();
        prop_assert_eq!(old_after, new_after);
    }
}

// Appends ` insert` to the paragraph `pick`, or `None` when that changes a heading flag
// or the paragraph count.
fn append_word(text: &str, pick: prop::sample::Index, insert: &str) -> Option<String> {
    let paras = paragraphs(text);
    let at = pick.index(paras.len());
    let p = &paras[at];
    let trimmed = p.text.trim_end().len();
    if trimmed == 0 {
        return None;
    }
    let before_p: usize = paras[..at].iter().map(|q| q.text.len()).sum();
    let mut edited = text.to_string();
    edited.insert_str(before_p + trimmed, &format!(" {insert}"));
    let new_paras = paragraphs(&edited);
    let same = new_paras.len() == paras.len()
        && new_paras
            .iter()
            .zip(&paras)
            .all(|(a, b)| a.heading == b.heading);
    same.then_some(edited)
}
