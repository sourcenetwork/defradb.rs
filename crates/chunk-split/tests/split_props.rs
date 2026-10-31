//! Property tests mirroring the Lean theorems in `proofs/lean/ChunkSplit`.

use chunk_split::split::{cut_sentences, cut_words, fence_before, paragraphs, split, ChunkConfig};
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

    // Mirrors `local_edit`: editing one paragraph without moving a boundary leaves
    // every chunk outside its segment unchanged.
    #[test]
    fn local_edit(text in text(), config in config(), pick in any::<prop::sample::Index>(), insert in "[a-z]{1,6}") {
        let max = config.max_chars;
        let paras = paragraphs(&text);
        prop_assume!(!paras.is_empty());
        let at = pick.index(paras.len());
        let p = &paras[at];
        let trimmed = p.text.trim_end().len();
        prop_assume!(trimmed > 0);
        let before_p: usize = paras[..at].iter().map(|q| q.text.len()).sum();
        let mut edited = text.clone();
        edited.insert_str(before_p + trimmed, &format!(" {insert}"));

        let new_paras = paragraphs(&edited);
        prop_assume!(new_paras.len() == paras.len());
        let q = &new_paras[at];
        prop_assume!(q.heading == p.heading);
        let keeps_fence_before = at == 0 || fence_before(&paras[at - 1], p, max) == fence_before(&new_paras[at - 1], q, max);
        let keeps_fence_after = at + 1 == paras.len() || fence_before(p, &paras[at + 1], max) == fence_before(q, &new_paras[at + 1], max);
        prop_assume!(keeps_fence_before && keeps_fence_after);

        let seg_start = (1..=at).rev().find(|&i| fence_before(&paras[i - 1], &paras[i], max)).unwrap_or(0);
        let seg_end = (at + 1..paras.len()).find(|&i| fence_before(&paras[i - 1], &paras[i], max)).unwrap_or(paras.len());
        let start_char = paras[seg_start].start;
        let end_char = paras.get(seg_end).map_or(text.chars().count(), |q| q.start);
        let shift = edited.chars().count() - text.chars().count();

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
