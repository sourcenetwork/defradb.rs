//! A collection added on its own has the identity it has in a batch.
//!
//! A relation to a collection the store already holds is the relation it is
//! when both are declared together: the same field kind, the same field IDs
//! and so the same collection and version IDs.

use db::{DbTransactionRegistry, DB};
use defra_core::RequestContext;
use query::txn::TransactionRegistry;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use std::collections::BTreeMap;
use std::sync::Arc;
use storage::RegolithStore;

const ROOT: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

/// What a collection's identity and shape are, without the store's counters.
#[derive(Debug, PartialEq)]
struct Shape {
    collection_id: String,
    version_id: String,
    fields: Vec<FieldDescription>,
    indexes: Vec<IndexDescription>,
}

async fn shapes(calls: &[&str]) -> BTreeMap<String, Shape> {
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    let context = RequestContext::default();
    let mut created = Vec::new();
    for sdl in calls {
        created.extend(db.add_collection(&context, sdl).await.unwrap());
    }
    created
        .iter()
        .map(|added| {
            let stored: CollectionVersion = db
                .get_collection(&added.name)
                .unwrap()
                .unwrap()
                .schema()
                .clone();
            assert_eq!(stored.collection_id, added.collection_id, "{}", added.name);
            (
                stored.name.clone(),
                Shape {
                    collection_id: stored.collection_id,
                    version_id: stored.version_id,
                    fields: stored.fields,
                    indexes: stored.indexes,
                },
            )
        })
        .collect()
}

/// Every way of splitting `declarations` into calls, in the order given,
/// yields the collections one call with all of them yields.
fn assert_same_as_one_call(declarations: &[&str]) {
    workers::block_on(async {
        let together = shapes(&[&declarations.join("\n")]).await;
        let apart = shapes(declarations).await;
        assert_eq!(apart, together);
        for shape in together.values() {
            for field in &shape.fields {
                assert!(
                    !matches!(field.kind, FieldKind::Named { .. }),
                    "{} is left unresolved",
                    field.name
                );
            }
        }
    })
    .unwrap()
}

#[test]
fn a_relation_to_a_stored_collection() {
    assert_same_as_one_call(&[
        "type Author { name: String }",
        "type Book { title: String author: Author }",
    ]);
}

#[test]
fn the_batch_order_does_not_matter() {
    workers::block_on(async {
        let author_first =
            shapes(&["type Author { name: String }\ntype Book { title: String author: Author }"])
                .await;
        let book_first =
            shapes(&["type Book { title: String author: Author }\ntype Author { name: String }"])
                .await;
        let apart = shapes(&[
            "type Author { name: String }",
            "type Book { title: String author: Author }",
        ])
        .await;
        assert_eq!(author_first, book_first);
        assert_eq!(apart, author_first);
    })
    .unwrap()
}

#[test]
fn a_chain_added_in_three_calls() {
    assert_same_as_one_call(&[
        "type Country { name: String }",
        "type City { name: String country: Country }",
        "type Street { name: String city: City }",
    ]);
}

#[test]
fn a_self_reference_beside_a_stored_target() {
    assert_same_as_one_call(&[
        "type City { name: String }",
        "type Person { name: String boss: Person home: City }",
    ]);
}

#[test]
fn a_one_to_many_pair_pointing_at_a_stored_collection() {
    assert_same_as_one_call(&[
        "type Publisher { name: String }",
        "type Author { name: String publisher: Publisher books: [Book] }
         type Book { title: String author: Author publisher: Publisher }",
    ]);
}

#[test]
fn an_indexed_relation_to_a_stored_collection() {
    assert_same_as_one_call(&[
        "type Author { name: String }",
        "type Book { title: String author: Author @index }",
    ]);
}

#[test]
fn a_governed_collection_relating_to_a_stored_one() {
    let book = format!(
        "type Book @governed(root: \"{ROOT}\") @policy(id: \"p1\", resource: \"books\") \
         {{ title: String author: Author }}"
    );
    assert_same_as_one_call(&["type Author { name: String }", &book]);
}

#[test]
fn an_array_relation_to_a_stored_collection_is_refused_as_in_a_batch() {
    workers::block_on(async {
        let context = RequestContext::default();
        let together = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let in_one_call = together
            .add_collection(
                &context,
                "type Author { name: String }\ntype Shelf { label: String authors: [Author] }",
            )
            .await
            .unwrap_err();

        let apart = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        apart
            .add_collection(&context, "type Author { name: String }")
            .await
            .unwrap();
        let on_its_own = apart
            .add_collection(&context, "type Shelf { label: String authors: [Author] }")
            .await
            .unwrap_err();

        assert_eq!(on_its_own.to_string(), in_one_call.to_string());
        assert_eq!(apart.list_collections().unwrap(), ["Author"]);
    })
    .unwrap()
}

#[test]
fn a_collection_added_in_a_transaction_after_its_target() {
    workers::block_on(async {
        let together =
            shapes(&["type Author { name: String }\ntype Book { title: String author: Author }"])
                .await;

        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let context = RequestContext::default();
        db.add_collection(&context, "type Author { name: String }")
            .await
            .unwrap();
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin_as(&context, false).await.unwrap();
        let book = registry
            .add_collection(
                &context,
                handle.as_str(),
                "type Book { title: String author: Author }",
            )
            .await
            .unwrap();
        // A second statement relates to the collection the first one staged.
        let review = registry
            .add_collection(
                &context,
                handle.as_str(),
                "type Review { text: String book: Book }",
            )
            .await
            .unwrap();
        registry.commit(&handle).await.unwrap();

        assert_eq!(book[0].collection_id, together["Book"].collection_id);
        assert_eq!(book[0].fields, together["Book"].fields);
        let all_at_once = shapes(&["type Author { name: String }
             type Book { title: String author: Author }
             type Review { text: String book: Book }"])
        .await;
        assert_eq!(review[0].collection_id, all_at_once["Review"].collection_id);
        assert_eq!(review[0].fields, all_at_once["Review"].fields);
    })
    .unwrap()
}

/// A governed collection whose relation field is bound to a policy relation,
/// added after the collection it relates to.
fn bound_book() -> String {
    format!(
        "type Book @governed(root: \"{ROOT}\") @policy(id: \"p1\", resource: \"book\") \
         {{ title: String author: Author @immutable @policy(relation: \"author\") }}"
    )
}

const POLICIED_AUTHOR: &str =
    r#"type Author @policy(id: "p1", resource: "author") { name: String }"#;

#[test]
fn a_governed_collection_with_a_bound_relation_to_a_stored_one() {
    assert_same_as_one_call(&[POLICIED_AUTHOR, &bound_book()]);
}

#[test]
fn a_bound_relation_added_alone_passes_the_binding_checks_as_a_relation() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let context = RequestContext::default();
        db.add_collection(&context, POLICIED_AUTHOR).await.unwrap();
        db.add_collection(&context, &bound_book()).await.unwrap();
        let author = db
            .get_collection("Author")
            .unwrap()
            .unwrap()
            .schema()
            .clone();
        let book = db.get_collection("Book").unwrap().unwrap().schema().clone();

        let known = [book.clone(), author.clone()];
        let bound = schema::check_policy_bindings(&book, &known).unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].target.collection_id, author.collection_id);
        assert_eq!(bound[0].binding.policy_relation, "author");
        assert_eq!(bound[0].binding.id_field.name, "_authorID");
        assert_eq!(
            bound[0].binding.relation_field.unwrap().kind,
            FieldKind::relation(author.collection_id.clone(), false)
        );
        schema::check_synced_policy_bindings(&book).unwrap();
    })
    .unwrap()
}
