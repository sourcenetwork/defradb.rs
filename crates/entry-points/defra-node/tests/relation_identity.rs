//! A collection added in a call of its own, after the collection it relates
//! to, is the collection it is when both are added in one call, and one
//! definition that kept only its target's name does not take the node's
//! GraphQL schema down.

use defra_core::RequestContext;
use defra_node::{Node, NodeBuilder};
use schema::{CollectionVersion, FieldKind};
use serde_json::{json, Value as JsonValue};

const AUTHOR: &str = "type Author { name: String }";
const BOOK: &str = "type Book { title: String author: Author }";
/// The collection ID `Book` got when it was added after `Author`, in a call
/// of its own, before a relation to a stored collection committed to it.
const ID_BEFORE_THE_FIX: &str = "bafyreid5dnhrerno3npkaonkgu73gpggs7dv2n3ii254r2aspzoydruh5u";
const BOOK_INPUT: &str =
    r#"{ __type(name: "BookMutationInputArg") { inputFields { name type { name kind } } } }"#;

async fn node_with(calls: &[&str]) -> Node {
    let node = NodeBuilder::default().build().await.expect("node starts");
    for sdl in calls {
        node.add_collection(sdl).await.expect("schema added");
    }
    node
}

async fn data(node: &Node, request: &str) -> JsonValue {
    let response = node.execute(request).await;
    assert!(!response.has_errors(), "{request}: {:?}", response.errors);
    response.data.expect("a response with data")
}

async fn add(node: &Node, collection: &str, input: &str) -> String {
    let created = data(
        node,
        &format!("mutation {{ add_{collection}(input: {{{input}}}) {{ _docID }} }}"),
    )
    .await;
    created[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .expect("the created document's ID")
        .to_owned()
}

/// The type of `field` in `BookMutationInputArg`.
async fn book_input_type(node: &Node, field: &str) -> JsonValue {
    let introspected = data(node, BOOK_INPUT).await;
    introspected["__type"]["inputFields"]
        .as_array()
        .expect("the input type's fields")
        .iter()
        .find(|candidate| candidate["name"] == field)
        .unwrap_or_else(|| panic!("BookMutationInputArg has no {field}"))["type"]
        .clone()
}

async fn books_with_authors(node: &Node) -> JsonValue {
    let author = add(node, "Author", r#"name: "Cornelia Funke""#).await;
    add(
        node,
        "Book",
        &format!(r#"title: "Inkheart", author: "{author}""#),
    )
    .await;
    data(node, "query { Book { title author { name } } }").await
}

/// `Book` as the parser left it before a relation to a stored collection
/// committed to that collection: the kind names its target and nothing else,
/// and the field and collection IDs are derived from that.
fn book_left_named(book: &CollectionVersion) -> CollectionVersion {
    let mut legacy = book.clone();
    for field in &mut legacy.fields {
        if field.name == "author" {
            field.kind = FieldKind::named("Author", false);
            field.id = schema::generate_field_cid(field).unwrap().to_string();
        }
    }
    let mut identified: Vec<_> = legacy
        .fields
        .iter()
        .filter(|field| !field.id.is_empty())
        .collect();
    identified.sort_by_key(|field| (field.name != "_docID", field.name.clone()));
    let field_cids: Vec<_> = identified
        .iter()
        .map(|field| schema::generate_field_cid(field).unwrap())
        .collect();
    legacy.collection_id = schema::generate_collection_cid_governed(
        &legacy.name,
        &field_cids,
        1,
        &[],
        schema::Commitments::default(),
    )
    .unwrap()
    .to_string();
    legacy.version_id.clone_from(&legacy.collection_id);
    legacy
}

#[test]
fn a_collection_added_alone_is_the_collection_of_one_call() {
    workers::block_on(async {
        let together = node_with(&[&format!("{AUTHOR}\n{BOOK}")]).await;
        let apart = node_with(&[AUTHOR, BOOK]).await;

        let author = together.get_collection("Author").unwrap().unwrap();
        let expected = together.get_collection("Book").unwrap().unwrap();
        let alone = apart.get_collection("Book").unwrap().unwrap();
        assert_eq!(alone.collection_id, expected.collection_id);
        assert_eq!(alone.version_id, expected.version_id);
        assert_eq!(alone.fields, expected.fields);
        let relation = alone
            .fields
            .iter()
            .find(|field| field.name == "author")
            .unwrap();
        assert_eq!(
            relation.kind,
            FieldKind::relation(author.collection_id, false)
        );

        for node in [&together, &apart] {
            assert_eq!(
                book_input_type(node, "author").await,
                json!({ "name": "ID", "kind": "SCALAR" })
            );
            assert_eq!(
                books_with_authors(node).await,
                json!({ "Book": [{ "title": "Inkheart", "author": { "name": "Cornelia Funke" } }] })
            );
        }
    })
    .unwrap()
}

#[test]
fn a_stored_named_relation_keeps_its_identity_and_the_node_its_schema() {
    workers::block_on(async {
        let reference = node_with(&[AUTHOR, BOOK]).await;
        let resolved = reference.get_collection("Book").unwrap().unwrap();
        let legacy = book_left_named(&resolved);
        assert_eq!(legacy.collection_id, ID_BEFORE_THE_FIX);
        assert_ne!(legacy.collection_id, resolved.collection_id);

        let node = node_with(&[AUTHOR]).await;
        node.database()
            .create_view_collections_as(
                &RequestContext::default(),
                vec![legacy.clone()],
                None,
                None,
            )
            .await
            .expect("the legacy definition is stored");

        let stored = node.get_collection("Book").unwrap().unwrap();
        assert_eq!(stored.collection_id, legacy.collection_id);
        assert_eq!(
            stored
                .fields
                .iter()
                .find(|field| field.name == "author")
                .unwrap()
                .kind,
            FieldKind::named("Author", false)
        );

        assert_eq!(
            book_input_type(&node, "author").await,
            json!({ "name": "ID", "kind": "SCALAR" })
        );
        assert_eq!(
            data(
                &node,
                r#"{ __type(name: "AuthorMutationInputArg") { name } }"#
            )
            .await,
            json!({ "__type": { "name": "AuthorMutationInputArg" } })
        );
        assert_eq!(
            books_with_authors(&node).await,
            json!({ "Book": [{ "title": "Inkheart", "author": { "name": "Cornelia Funke" } }] })
        );
    })
    .unwrap()
}
