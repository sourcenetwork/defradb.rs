use crate::document::DocumentMapping;
use crate::limits::QueryLimits;
use crate::mapper::Select;

use super::{build_plan, ScanSource};

#[test]
fn a_chunk_collection_is_refused_off_the_planner() {
    let sdl = "type Article { body: String @chunk(maxChars: 64) }";
    let collections = crate::parse_sdl(sdl).unwrap();
    let chunks_col = collections
        .iter()
        .find(|collection| collection.chunk_of.is_some())
        .unwrap();

    let refused = build_plan(
        &Select::new(chunks_col.name.clone()),
        ScanSource::Docs(Vec::new()),
        DocumentMapping::default(),
        chunks_col,
        None,
        None,
        QueryLimits::default(),
    );

    let Err(error) = refused else {
        panic!("a chunk collection must not get a plan here");
    };
    assert!(
        error
            .to_string()
            .contains("must be read through the planner"),
        "{error}"
    );
}
