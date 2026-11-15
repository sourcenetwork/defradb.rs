use chunk_split::split::SplitterVersion;
use db::chunk::chunk_config;
use db::error::Error;

const SDL: &str = "type Article { body: String @chunk(maxChars: 64, overlap: 1) }";

fn body() -> schema::ChunkedField {
    let parsed = query::parse_sdl(SDL).unwrap();
    parsed[0].chunked_field("body").unwrap().clone()
}

#[test]
fn the_default_a_delta_implies_is_the_first_splitter_version() {
    assert_eq!(
        defra_core::DEFAULT_SPLITTER_VERSION,
        SplitterVersion::V1.number()
    );
}

#[test]
fn a_parsed_field_cuts_with_its_settings() {
    let config = chunk_config("Article", &body()).unwrap();
    assert_eq!(config.version, SplitterVersion::CURRENT);
    assert_eq!((config.max_chars, config.overlap), (64, 1));
}

/// A version newer than any this build can split with.
fn unknown_splitter() -> u32 {
    SplitterVersion::ALL
        .iter()
        .map(|known| known.number())
        .max()
        .unwrap()
        + 1
}

#[test]
fn a_splitter_version_this_node_lacks_fails_loud() {
    let unknown = unknown_splitter();
    let mut field = body();
    field.splitter_version = unknown;
    let error = chunk_config("Article", &field).unwrap_err();
    assert!(
        matches!(&error, Error::UnknownSplitterVersion { field, source }
            if field == "Article.body" && source.version == unknown),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("Article.body"), "{message}");
    assert!(
        message.contains(&format!("splitter version {unknown}")),
        "{message}"
    );
    assert!(message.contains("Upgrade the node"), "{message}");
}
