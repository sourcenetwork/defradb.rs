//! Pins where each splitter version cuts a fixed set of texts. A failure here
//! means peers on this build and on older ones would cut one text two ways.

use std::fmt::Write as _;
use std::path::PathBuf;

use chunk_split::list::text_hash;
use chunk_split::split::{split, ChunkConfig, SplitterVersion};

const SETTINGS: [(usize, usize); 6] = [(16, 0), (40, 0), (40, 1), (120, 2), (400, 1), (1000, 3)];

fn samples() -> Vec<(&'static str, String)> {
    let mut long_prose = String::new();
    for i in 0..60 {
        write!(
            long_prose,
            "Sentence number {i} talks about item {} and its owner. ",
            i * 7 % 13
        )
        .unwrap();
        if i % 5 == 4 {
            long_prose.push_str("\n\n");
        }
    }
    let mut long_doc = String::new();
    for section in 0..12 {
        write!(long_doc, "## Section {section}\n\n").unwrap();
        for para in 0..3 {
            write!(
                long_doc,
                "Paragraph {para} of section {section} has some words. It ends here. Then one more.\n\n"
            )
            .unwrap();
        }
    }
    vec![
        ("empty", String::new()),
        ("one_word", "Hello".into()),
        (
            "prose",
            "Cats sleep a lot. Dogs bark at night! Do birds sing? Fish swim quietly in the pond.".into(),
        ),
        (
            "paragraphs",
            "First paragraph here. It has two sentences.\n\nSecond paragraph is short.\n\n\n\nThird after extra blank lines.".into(),
        ),
        (
            "headings",
            "# Title\n\nIntro text under the title.\n\n## Part one\n\nBody of part one. More body.\n\n### Detail\n\nA small detail.".into(),
        ),
        (
            "list",
            "Shopping:\n- apples\n- bread and butter\n- milk\n\n1. first step\n2. second step\n3. third step".into(),
        ),
        (
            "code",
            "Run this:\n\n```rust\nfn main() {\n    println!(\"hi. there\");\n}\n```\n\nThen check the output.".into(),
        ),
        (
            "abbreviations",
            "Dr. Smith met Mr. Jones at 3.14 p.m. on the U.S. coast. See https://example.com/a.b?c=d. Done.".into(),
        ),
        (
            "no_terminators",
            "a long run of words with no sentence end at all that keeps going and going well past any chunk size we use here".into(),
        ),
        ("long_word", "x".repeat(130)),
        (
            "cjk",
            "猫はよく寝る。犬は夜に吠える！鳥は歌うか？魚は静かに泳ぐ．終わり｡".into(),
        ),
        (
            "graphemes",
            "Family 👨‍👩‍👧‍👦 trip. Flags 🇩🇪🇯🇵 wave. Accents e\u{301}e\u{301}e\u{301} stay whole. Skin 👋🏽 tone.".into(),
        ),
        (
            "rtl",
            "مرحبا بالعالم. هذا نص قصير. שלום עולם. זה טקסט.".into(),
        ),
        (
            "crlf",
            "Windows line one.\r\nLine two.\r\n\r\nNew paragraph.\r\n".into(),
        ),
        ("whitespace", "   \n\n \t \n".into()),
        ("long_prose", long_prose),
        ("long_doc", long_doc),
    ]
}

/// One line per sample and setting: a digest of every chunk's range and hash.
fn render(version: SplitterVersion) -> String {
    let mut out = String::new();
    for (name, text) in samples() {
        for (max_chars, overlap) in SETTINGS {
            let config = ChunkConfig {
                version,
                max_chars,
                overlap,
            };
            let chunks = split(&text, &config);
            let mut cuts = String::new();
            for chunk in &chunks {
                writeln!(cuts, "{}:{}:{}", chunk.start, chunk.end, chunk.hash).unwrap();
            }
            writeln!(
                out,
                "{name} {max_chars} {overlap} {} {}",
                chunks.len(),
                text_hash(&cuts)
            )
            .unwrap();
        }
    }
    out
}

fn golden_path(version: SplitterVersion) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("v{}.txt", version.number()))
}

#[test]
fn every_version_cuts_as_it_was_released() {
    for version in SplitterVersion::ALL {
        let path = golden_path(version);
        let actual = render(version);
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "{} is missing. A new splitter version freezes its cuts there:\n{actual}",
                path.display()
            )
        });
        let changed: Vec<&str> = expected
            .lines()
            .filter(|line| !actual.lines().any(|new| new == *line))
            .chain(
                actual
                    .lines()
                    .filter(|line| !expected.lines().any(|old| old == *line)),
            )
            .collect();
        assert!(
            expected == actual,
            "splitter version {} now cuts these samples differently: {changed:?}. \
             Released versions never change: keep their rules and add a new SplitterVersion instead.",
            version.number()
        );
    }
}
