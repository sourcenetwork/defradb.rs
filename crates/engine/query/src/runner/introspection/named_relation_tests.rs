//! A relation whose kind kept only its target's name is an `ID` in mutation
//! inputs, so the schema builds for every collection of the node.

use std::sync::Arc;

use schema::{CType, CollectionVersion, FieldDescription, FieldKind, ScalarKind};

use super::execute_introspection;

fn collection(name: &str, id: &str, fields: Vec<FieldDescription>) -> Arc<CollectionVersion> {
    Arc::new(CollectionVersion::new(name, id, id, fields))
}

fn collections(author_kind: FieldKind) -> Vec<Arc<CollectionVersion>> {
    let name = FieldDescription::new(
        "name-id",
        "name",
        FieldKind::Scalar(ScalarKind::NillableString),
        CType::LwwRegister,
    );
    vec![
        collection("Author", "author-id", vec![name.clone()]),
        collection(
            "Book",
            "book-id",
            vec![
                name,
                FieldDescription::new("", "author", author_kind, CType::None),
            ],
        ),
    ]
}

const INPUTS: &str = r#"{
    book: __type(name: "BookMutationInputArg") { inputFields { name type { kind name ofType { name } } } }
    author: __type(name: "AuthorMutationInputArg") { name }
}"#;

fn author_input(introspected: &serde_json::Value) -> serde_json::Value {
    introspected["book"]["inputFields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["name"] == "author")
        .unwrap()["type"]
        .clone()
}

#[test]
fn a_named_relation_is_an_id_in_a_mutation_input() {
    workers::block_on(async {
        let named = execute_introspection(&collections(FieldKind::named("Author", false)), INPUTS)
            .await
            .expect("the schema builds with a named relation");
        let resolved = execute_introspection(
            &collections(FieldKind::relation("author-id", false)),
            INPUTS,
        )
        .await
        .unwrap();

        assert_eq!(
            author_input(&named),
            serde_json::json!({ "kind": "SCALAR", "name": "ID", "ofType": null })
        );
        assert_eq!(author_input(&named), author_input(&resolved));
        assert_eq!(named["author"]["name"], "AuthorMutationInputArg");
    })
    .unwrap()
}

#[test]
fn a_named_relation_to_many_is_an_id_too() {
    workers::block_on(async {
        let named = execute_introspection(&collections(FieldKind::named("Author", true)), INPUTS)
            .await
            .expect("the schema builds with a named array relation");
        let resolved =
            execute_introspection(&collections(FieldKind::relation("author-id", true)), INPUTS)
                .await
                .unwrap();

        assert_eq!(author_input(&named), author_input(&resolved));
    })
    .unwrap()
}
