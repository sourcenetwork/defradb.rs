//! A document that names its one-to-one partner from the primary side
//! commits, on a plain node and on a node that signs its documents.

use defra_node::{EmbeddedNode, NodeBuilder};
use serde_json::Value as JsonValue;

type Node = EmbeddedNode;

const SDL: &str = r#"
type Author {
    name: String
    published: [Book]
}

type Book {
    title: String
    author: Author
    publisher: Publisher
}

type Publisher {
    name: String
    establishedYear: Int @index
    book: Book @primary
}
"#;

async fn add(node: &Node, collection: &str, input: &str) -> String {
    let response = node
        .execute(&format!(
            "mutation {{ add_{collection}(input: {{{input}}}) {{ _docID }} }}"
        ))
        .await;
    assert!(
        !response.has_errors(),
        "add_{collection} failed: {:?}",
        response.errors
    );
    response
        .data
        .as_ref()
        .and_then(|data| data.get(format!("add_{collection}")))
        .and_then(JsonValue::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("_docID"))
        .and_then(JsonValue::as_str)
        .expect("the created document's ID")
        .to_string()
}

async fn primary_side_commits(signing: bool) {
    let builder = NodeBuilder::default();
    let node = if signing {
        builder.enable_signing()
    } else {
        builder
    }
    .build()
    .await
    .expect("node starts");
    node.add_collection(SDL).await.expect("schema added");
    if let Some(did) = node.node_identity_did() {
        defra_core::signing::set_signing_config(defra_core::signing::get_identity(did));
    }

    let author = add(&node, "Author", r#"name: "Cornelia Funke""#).await;
    let book = add(
        &node,
        "Book",
        &format!(r#"title: "Inkheart", author: "{author}""#),
    )
    .await;
    add(
        &node,
        "Publisher",
        &format!(r#"name: "Chicken House", establishedYear: 2000, book: "{book}""#),
    )
    .await;

    let response = node
        .execute("query { Book { title publisher { name } } }")
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    assert_eq!(
        response.data.as_ref().and_then(|data| data.get("Book")),
        Some(&serde_json::json!([{"title": "Inkheart", "publisher": {"name": "Chicken House"}}]))
    );
    if signing {
        defra_core::signing::set_signing_config(None);
    }
}

#[test]
fn a_primary_side_create_commits() {
    workers::block_on(async {
        primary_side_commits(false).await;
    })
    .unwrap()
}

#[test]
fn a_signed_primary_side_create_commits() {
    workers::block_on(async {
        primary_side_commits(true).await;
    })
    .unwrap()
}
