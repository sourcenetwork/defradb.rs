use integration_test::actions::{Actions, AddCollection};

pub const SCHEMA: &str = r#"type Article {
    title: String
    body: String @chunk(maxChars: 64)
}"#;

pub const CHUNKS: &str = "Article__body_chunk";

pub fn add_article_collection() -> Actions {
    AddCollection {
        sdl: SCHEMA.into(),
        ..Default::default()
    }
    .into()
}
