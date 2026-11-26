//! `db::create_gate`: the relationships a new document carries through its
//! `@policy(relation:)` fields, and whether its collection gates creates.

use acp::policy_yaml::{build_policy, parse_policy_yaml};
use db::{create_gate, CreateGate, CreateGateError, DerivedRelationship};
use document::Document;
use schema::CollectionVersion;

const AUTHOR_ID: &str = "bae-0a24b4e4-c1f7-5d2b-9c0e-6c1e1b3c9f01";

fn policy(create: Option<&str>) -> acp::Policy {
    let collection = create
        .map(|create| format!("    collection:\n      create: {create}\n"))
        .unwrap_or_default();
    let yaml = format!(
        r#"
name: library
resources:
  - name: author
    relations:
      - name: publisher
        types: [actor]
    permissions:
      - name: publish
        expr: publisher
  - name: book
    relations:
      - name: author
        types: [author]
    permissions:
      - name: read
{collection}"#
    );
    build_policy(&parse_policy_yaml(&yaml).unwrap(), 1).unwrap()
}

fn book(policy_id: &str, governed: bool) -> CollectionVersion {
    let root = if governed {
        r#"@governed(root: "root-a")"#
    } else {
        ""
    };
    let sdl = format!(
        r#"type Book {root} @policy(id: "{policy_id}", resource: "book") {{
               title: String
               author: Author @immutable @policy(relation: "author")
           }}
           type Author @policy(id: "{policy_id}", resource: "author") {{ name: String }}"#
    );
    query::parse_sdl(&sdl)
        .unwrap()
        .into_iter()
        .find(|c| c.name == "Book")
        .unwrap()
}

fn doc(collection: &CollectionVersion, json: &str) -> Document {
    Document::from_json_str(json, collection.clone()).unwrap()
}

#[test]
fn a_bound_field_derives_its_relationship_and_a_gated_resource_gates() {
    let policy = policy(Some("author->publish"));
    let book = book(&policy.id, true);
    let doc = doc(
        &book,
        &format!(r#"{{"title": "Dune", "_authorID": "{AUTHOR_ID}"}}"#),
    );
    assert_eq!(
        create_gate(&book, &doc, Some(&policy)).unwrap(),
        CreateGate {
            gated: true,
            derived: vec![DerivedRelationship {
                relation: "author".into(),
                target_resource: "author".into(),
                target_doc_id: AUTHOR_ID.into(),
            }],
        }
    );
}

#[test]
fn an_unset_bound_field_derives_nothing() {
    let policy = policy(Some("author->publish"));
    let book = book(&policy.id, true);
    for json in [
        r#"{"title": "Dune"}"#,
        r#"{"title": "Dune", "_authorID": null}"#,
    ] {
        let gate = create_gate(&book, &doc(&book, json), Some(&policy)).unwrap();
        assert!(gate.gated);
        assert!(gate.derived.is_empty(), "{json}");
    }
}

#[test]
fn an_open_or_absent_create_does_not_gate() {
    for create in [None, Some(r#""*""#)] {
        let policy = policy(create);
        let book = book(&policy.id, true);
        let doc = doc(
            &book,
            &format!(r#"{{"title": "Dune", "_authorID": "{AUTHOR_ID}"}}"#),
        );
        let gate = create_gate(&book, &doc, Some(&policy)).unwrap();
        assert!(!gate.gated, "{create:?}");
        assert_eq!(gate.derived.len(), 1);
    }
}

#[test]
fn an_ungoverned_collection_is_never_gated() {
    let policy = policy(Some("author->publish"));
    let book = book(&policy.id, false);
    let doc = doc(&book, r#"{"title": "Dune"}"#);
    assert!(!create_gate(&book, &doc, Some(&policy)).unwrap().gated);
}

#[test]
fn no_policy_gates_nothing_and_another_policy_is_an_error() {
    let policy = policy(Some("author->publish"));
    let book = book(&policy.id, true);
    let doc = doc(
        &book,
        &format!(r#"{{"title": "Dune", "_authorID": "{AUTHOR_ID}"}}"#),
    );
    assert_eq!(
        create_gate(&book, &doc, None).unwrap(),
        CreateGate::default()
    );
    let other = book_with_other_policy(&book);
    assert!(matches!(
        create_gate(&other, &doc, Some(&policy)),
        Err(CreateGateError::WrongPolicy { .. })
    ));
}

fn book_with_other_policy(book: &CollectionVersion) -> CollectionVersion {
    let mut other = book.clone();
    if let Some(policy) = other.policy.as_mut() {
        policy.id = "another".into();
    }
    other
}
