//! A collection added in a call of its own, after the collection it relates
//! to, is the collection it is when both are added in one call
//! (`defra.collection.id_added_alone`).

use integration_test::actions::{Actions, AddCollection, AddDoc, GetCollections, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

const AUTHOR: &str = "type Author { name: String }";
const BOOK: &str = "type Book { title: String author: Author }";

/// `Author`'s collection ID for the SDL above.
const AUTHOR_ID: &str = "bafyreibvcavbxqwimz5vdxe5q5href63g3skc6ytg45hm4fqh6wsx57wmq";
/// `Book`'s collection ID for the SDL above, however it is added.
const BOOK_ID: &str = "bafyreieifr5ciohlbzxlrqy4laqdz7nsmnuxzqya44i2d22isyr7nyomja";

fn add(sdl: &str) -> Action {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

/// What `Book` is, and what the node's GraphQL schema says of it, whichever
/// way `Author` and `Book` were added.
fn book_is_the_collection_of_one_call() -> Vec<Action> {
    vec![
        GetCollections {
            collection_name: Some("Book".into()),
            filtered: true,
            expected_data: json!({ "collections": [m::object(json!({
                "CollectionID": BOOK_ID,
                "VersionID": BOOK_ID,
                "Fields": [
                    m::object(json!({ "Name": "_docID" })),
                    m::object(json!({ "Name": "_authorID", "RelationName": "author_book" })),
                    m::object(json!({
                        "Name": "author",
                        "Kind": { "Array": false, "CollectionID": AUTHOR_ID },
                        "IsPrimary": true,
                        "RelationName": "author_book",
                    })),
                    m::object(json!({ "Name": "title" })),
                ],
            }))] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"{
                __type(name: "BookMutationInputArg") {
                    inputFields { name type { name kind } }
                }
            }"#
            .into(),
            expected_data: json!(m::object(json!({
                "__type": m::object(json!({
                    "inputFields": m::contains(json!([
                        { "name": "author", "type": { "name": "ID", "kind": "SCALAR" } },
                    ])),
                })),
            }))),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "Cornelia Funke" })),
            common: Common {
                capture: captures([("author", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({ "title": "Inkheart", "author": m::r("author") })),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Book { title author { name } } }".into(),
            expected_data: json!({
                "Book": [{ "title": "Inkheart", "author": { "name": "Cornelia Funke" } }],
            }),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.collection.id_added_alone")]
fn a_collection_added_after_its_target_has_the_id_of_one_call() {
    let together = vec![add(&format!("{AUTHOR}\n{BOOK}"))];
    let apart = vec![add(AUTHOR), add(BOOK)];
    for added in [together, apart] {
        TestCase {
            actions: added
                .into_iter()
                .chain(book_is_the_collection_of_one_call())
                .collect(),
            ..Default::default()
        }
        .execute();
    }
}

/// Read `name`'s definition and capture its collection ID as `capture`.
/// `relation` names a field and the capture holding the collection ID its
/// kind must carry.
fn collection_related_to(name: &str, capture: &str, relation: Option<(&str, &str)>) -> Action {
    let mut fields = vec![m::object(json!({ "Name": "_docID" }))];
    if let Some((field, target)) = relation {
        fields.push(m::object(json!({ "Name": format!("_{field}ID") })));
        fields.push(m::object(json!({
            "Name": field,
            "Kind": { "Array": false, "CollectionID": m::r(target) },
        })));
    }
    fields.push(m::object(json!({ "Name": "name" })));
    GetCollections {
        collection_name: Some(name.into()),
        filtered: true,
        expected_data: json!({ "collections": [m::object(json!({ "Fields": fields }))] }),
        common: Common {
            capture: captures([(capture, "/collections/0/CollectionID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection.id_added_alone")]
fn a_chain_added_one_collection_per_call_relates_by_collection_id() {
    TestCase {
        actions: vec![
            add("type Country { name: String }"),
            add("type City { name: String country: Country }"),
            add("type Street { name: String city: City }"),
            collection_related_to("Country", "country", None),
            collection_related_to("City", "city", Some(("country", "country"))),
            collection_related_to("Street", "street", Some(("city", "city"))),
            Request {
                request: r#"{ __type(name: "StreetMutationInputArg") { name } }"#.into(),
                expected_data: json!({ "__type": { "name": "StreetMutationInputArg" } }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// An index declared on a relation is the same index either way: it is read
/// from the collection's definition, which every client reports.
#[test]
#[mantra_macros::req_test("defra.collection.id_added_alone")]
fn an_indexed_relation_to_a_stored_collection_indexes_its_id_field() {
    const INDEXED_BOOK: &str = "type Book { title: String author: Author @index }";
    let together = vec![add(&format!("{AUTHOR}\n{INDEXED_BOOK}"))];
    let apart = vec![add(AUTHOR), add(INDEXED_BOOK)];
    for mut actions in [together, apart] {
        actions.push(
            GetCollections {
                collection_name: Some("Book".into()),
                filtered: true,
                expected_data: json!({ "collections": [m::object(json!({
                    "Indexes": [m::object(json!({
                        "Name": "Book__authorID_ASC",
                        "Fields": [{ "Name": "_authorID", "Descending": false }],
                    }))],
                }))] }),
                ..Default::default()
            }
            .into(),
        );
        TestCase {
            actions,
            ..Default::default()
        }
        .execute();
    }
}
