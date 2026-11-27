use chunk_split::split::ChunkConfig;
use defra_core::vector::Metric;
use schema::ChunkedField;
use serde_json::json;

use super::{listed_in_row, ListedInRow};
use crate::plan::chunk_similarity::target::ChunkTarget;
use crate::planner::Doc;

const AT: ListedInRow = ListedInRow { text: 0 };

fn target() -> ChunkTarget {
    ChunkTarget {
        parent: "Article".into(),
        chunked: ChunkedField {
            field_name: "body".into(),
            max_chars: 64,
            overlap: 0,
            splitter_version: defra_core::DEFAULT_SPLITTER_VERSION,
        },
        chunks: "Article__body_chunk".into(),
        source_index: None,
        vector_indexes: Vec::new(),
        field: "body_v".into(),
        vector: vec![1.0],
        metric: Metric::Cosine,
        app: None,
        splitter: ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).ok(),
    }
}

fn row(text: &str) -> Doc {
    let mut row = Doc::new(2);
    row.set(0, json!(text));
    row
}

#[test]
fn a_live_row_lists_the_chunks_of_its_text() {
    assert!(listed_in_row(&row("Some text."), AT, &target()).is_some_and(|l| l.len() == 1));
}

#[test]
fn a_deleted_row_lists_no_chunks() {
    let mut deleted = row("Some text.");
    deleted.mark_deleted();
    assert!(listed_in_row(&deleted, AT, &target()).is_none());
}

#[test]
fn a_row_lists_nothing_where_the_splitter_is_unknown() {
    let target = ChunkTarget {
        splitter: None,
        ..target()
    };
    assert!(listed_in_row(&row("Some text."), AT, &target).is_none());
}
