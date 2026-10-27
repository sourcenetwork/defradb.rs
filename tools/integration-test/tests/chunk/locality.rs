//! A typo fix in one paragraph of a long text writes one new chunk; every
//! other chunk document stays as it was.

use integration_test::TestCluster;

use super::support::{create, update, visible};

const SCHEMA: &str = r#"type Article {
    title: String
    body: String @chunk(maxChars: 160)
}"#;

fn book(typo: bool) -> String {
    (0..40)
        .map(|i| {
            let verb = if typo && i == 20 { "takls" } else { "talks" };
            format!("Paragraph number {i} {verb} about its own small topic.")
        })
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

#[tokio::test]
async fn editing_one_paragraph_changes_one_chunk() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();

    let doc_id = create(&client, &book(false));
    let before: Vec<String> = visible(&client, None)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    update(&client, &doc_id, &book(true));
    let after: Vec<String> = visible(&client, None)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let gone: Vec<&String> = before.iter().filter(|id| !after.contains(id)).collect();
    let new: Vec<&String> = after.iter().filter(|id| !before.contains(id)).collect();
    assert_eq!((gone.len(), new.len()), (1, 1), "{gone:?} {new:?}");
    assert_eq!(before.len(), after.len());
}
