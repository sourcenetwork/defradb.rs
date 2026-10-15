use crate::common::fixture::make_test_db_with_bus;
use document::Document;
use query::mutator::DocMutator;
use std::sync::Arc;

const SDL: &str = r#"
    type Users {
        name: String
        numbers: [Int!] @constraints(size: 2)
    }
"#;

async fn mutator() -> (db::AutoCommitMutator<storage::RegolithStore>, impl Sized) {
    let (db, bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .expect("schema");
    (db::AutoCommitMutator::new(Arc::clone(&db)), (db, bus))
}

#[tokio::test]
async fn create_rejects_an_array_of_the_wrong_size() {
    let (mutator, _keep) = mutator().await;
    let doc = Document::from_json_str(r#"{"name": "John", "numbers": [27, 28, 29]}"#).unwrap();

    let error = mutator.create("Users", doc).await.unwrap_err();

    assert!(error.to_string().contains("array size mismatch"), "{error}");
}

#[tokio::test]
async fn update_rejects_an_array_of_the_wrong_size() {
    let (mutator, _keep) = mutator().await;
    let doc = Document::from_json_str(r#"{"name": "John", "numbers": [27, 28]}"#).unwrap();
    let created = mutator.create("Users", doc).await.expect("create");

    let mut doc = Document::from_json_str(r#"{"numbers": [27, 28, 29]}"#).unwrap();
    doc.set_id(created.doc_id.clone());
    let error = mutator
        .update("Users", doc, ["numbers".to_string()].into_iter().collect())
        .await
        .unwrap_err();

    assert!(error.to_string().contains("array size mismatch"), "{error}");
}

#[tokio::test]
async fn create_and_update_accept_an_array_of_the_right_size() {
    let (mutator, _keep) = mutator().await;
    let doc = Document::from_json_str(r#"{"name": "John", "numbers": [27, 28]}"#).unwrap();
    let created = mutator.create("Users", doc).await.expect("create");

    let mut doc = Document::from_json_str(r#"{"numbers": [22, 23]}"#).unwrap();
    doc.set_id(created.doc_id.clone());
    mutator
        .update("Users", doc, ["numbers".to_string()].into_iter().collect())
        .await
        .expect("update");
}

#[tokio::test]
async fn create_accepts_a_null_array() {
    let (mutator, _keep) = mutator().await;
    let doc = Document::from_json_str(r#"{"name": "John", "numbers": null}"#).unwrap();

    mutator.create("Users", doc).await.expect("create");
}

#[tokio::test]
async fn create_ignores_size_on_a_json_field() {
    let (db, bus) = make_test_db_with_bus().await;
    let sdl = r#"
        type Users {
            data: JSON @constraints(size: 2)
        }
    "#;
    db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
        .await
        .expect("schema");
    let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
    let doc = Document::from_json_str(r#"{"data": [27, 28, 29]}"#).unwrap();

    mutator.create("Users", doc).await.expect("create");
    drop(bus);
}
