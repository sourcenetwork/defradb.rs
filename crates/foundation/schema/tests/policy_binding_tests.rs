//! `@policy(relation:)` bindings: where they live, what validation refuses,
//! and what a collection's identity commits to.

use schema::{
    check_policy_bindings, check_synced_policy_bindings, generate_field_cid_with_priority, CType,
    CollectionVersion, FieldDescription, FieldKind, PolicyDescription, SchemaError,
};

fn doc_id_field() -> FieldDescription {
    FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::None)
}

fn author_relation() -> FieldDescription {
    FieldDescription::new(
        "2",
        "author",
        FieldKind::relation("authors", false),
        CType::None,
    )
    .with_relation_name("author_book")
    .as_primary()
}

/// `Book` after relation finalization, its `author` field bound to the
/// policy relation `author` and immutable, governed and under policy `p1`.
fn bound_book() -> CollectionVersion {
    let mut book = CollectionVersion::new(
        "Book",
        "v-book",
        "c-book",
        vec![
            doc_id_field(),
            author_relation()
                .as_immutable()
                .with_policy_relation("author"),
        ],
    )
    .with_policy(PolicyDescription::new("p1", "book"));
    book.governance_root = Some("root-a".into());
    let mut next = 10;
    book.add_relation_id_fields(|| {
        next += 1;
        next.to_string()
    })
    .unwrap();
    book
}

fn author_collection(policy_id: &str) -> CollectionVersion {
    CollectionVersion::new("Author", "v-author", "authors", vec![doc_id_field()])
        .with_policy(PolicyDescription::new(policy_id, "author"))
}

fn refusal(collection: &CollectionVersion, others: &[CollectionVersion]) -> String {
    let mut known = others.to_vec();
    known.push(collection.clone());
    match check_policy_bindings(collection, &known) {
        Err(SchemaError::InvalidPolicyRelation { field_name, reason }) => {
            assert_eq!(field_name, "Book.author");
            reason
        }
        other => panic!("expected a refused binding, got {other:?}"),
    }
}

#[test]
fn relation_finalization_moves_immutable_and_binding_to_the_id_field() {
    let book = bound_book();
    let id_field = book.field_by_name("_authorID").unwrap();
    assert!(id_field.immutable);
    assert_eq!(id_field.policy_relation.as_deref(), Some("author"));
    assert!(id_field.is_primary);
    let object = book.field_by_name("author").unwrap();
    assert!(!object.immutable);
    assert_eq!(object.policy_relation, None);
    for field in &book.fields {
        field.validate().unwrap();
    }
}

#[test]
fn relation_finalization_carries_to_a_declared_id_field() {
    let mut book = CollectionVersion::new(
        "Book",
        "v",
        "c",
        vec![
            doc_id_field(),
            author_relation()
                .as_immutable()
                .with_policy_relation("author"),
            FieldDescription::new("3", "_authorID", FieldKind::doc_id(), CType::LwwRegister)
                .with_relation_name("author_book")
                .as_primary(),
        ],
    );
    book.add_relation_id_fields(|| "99".into()).unwrap();
    assert_eq!(book.fields.len(), 3);
    let id_field = book.field_by_name("_authorID").unwrap();
    assert!(id_field.immutable);
    assert_eq!(id_field.policy_relation.as_deref(), Some("author"));
    assert!(!book.field_by_name("author").unwrap().immutable);
}

#[test]
fn an_immutable_array_relation_is_still_refused() {
    let mut authors = CollectionVersion::new(
        "Author",
        "v",
        "c",
        vec![
            doc_id_field(),
            FieldDescription::new(
                "2",
                "books",
                FieldKind::relation("books", true),
                CType::None,
            )
            .with_relation_name("author_book")
            .as_immutable(),
        ],
    );
    authors.add_relation_id_fields(|| "9".into()).unwrap();
    let books = authors.field_by_name("books").unwrap();
    assert!(matches!(
        books.validate(),
        Err(SchemaError::InvalidImmutableField { .. })
    ));
}

#[test]
fn an_immutable_secondary_relation_id_is_refused() {
    let field = FieldDescription::new("", "_bookID", FieldKind::doc_id(), CType::LwwRegister)
        .with_relation_name("author_book")
        .as_immutable();
    let err = field.validate().unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid immutable field _bookID: only the primary side of a relation can be immutable"
    );
}

#[test]
fn a_binding_on_a_scalar_or_with_an_empty_relation_is_refused() {
    let scalar = FieldDescription::new("1", "title", FieldKind::string(), CType::LwwRegister)
        .with_policy_relation("author");
    assert_eq!(
        scalar.validate().unwrap_err().to_string(),
        "invalid @policy(relation:) on field title: only a single-object relation field can be bound to a policy relation"
    );
    let empty = FieldDescription::new("1", "_authorID", FieldKind::doc_id(), CType::LwwRegister)
        .with_relation_name("author_book")
        .as_primary()
        .with_policy_relation(" ");
    assert_eq!(
        empty.validate().unwrap_err().to_string(),
        "invalid @policy(relation:) on field _authorID: the relation name must not be empty"
    );
}

#[test]
fn the_binding_serializes_only_when_present() {
    let unbound = FieldDescription::new("1", "_authorID", FieldKind::doc_id(), CType::LwwRegister);
    let json = serde_json::to_string(&unbound).unwrap();
    assert!(!json.contains("PolicyRelation"), "{json}");

    let bound = unbound.with_policy_relation("author");
    let json = serde_json::to_string(&bound).unwrap();
    assert!(json.contains(r#""PolicyRelation":"author""#), "{json}");
    let parsed: FieldDescription = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, bound);
}

#[test]
fn a_governed_primary_immutable_binding_under_one_policy_is_accepted() {
    let book = bound_book();
    let known = vec![book.clone(), author_collection("p1")];
    let bound = check_policy_bindings(&book, &known).unwrap();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].binding.policy_relation, "author");
    assert_eq!(bound[0].binding.declared_name(), "author");
    assert_eq!(bound[0].target.name, "Author");
}

#[test]
fn a_binding_outside_a_governed_collection_is_refused() {
    let mut book = bound_book();
    book.governance_root = None;
    assert_eq!(
        refusal(&book, &[author_collection("p1")]),
        "the collection must be governed (@governed(root:))"
    );
}

#[test]
fn a_binding_on_the_secondary_side_is_refused() {
    let mut book = bound_book();
    for field in &mut book.fields {
        field.is_primary = false;
    }
    let id_field = book
        .fields
        .iter_mut()
        .find(|f| f.name == "_authorID")
        .unwrap();
    id_field.immutable = false;
    assert_eq!(
        refusal(&book, &[author_collection("p1")]),
        "the binding must be on the primary side of the relation, which stores the related document ID"
    );
}

#[test]
fn a_binding_on_a_mutable_field_is_refused() {
    let mut book = bound_book();
    book.fields
        .iter_mut()
        .find(|f| f.name == "_authorID")
        .unwrap()
        .immutable = false;
    assert_eq!(
        refusal(&book, &[author_collection("p1")]),
        "the field must be @immutable"
    );
}

#[test]
fn a_binding_without_a_policy_is_refused() {
    let mut book = bound_book();
    book.policy = None;
    assert_eq!(
        refusal(&book, &[author_collection("p1")]),
        "the collection must have a @policy"
    );
}

#[test]
fn a_binding_to_a_collection_under_another_policy_is_refused() {
    let book = bound_book();
    assert_eq!(
        refusal(&book, &[author_collection("p2")]),
        "the related collection Author must have the same policy (p1)"
    );
    let unpolicied = CollectionVersion::new("Author", "v-author", "authors", vec![doc_id_field()]);
    assert_eq!(
        refusal(&book, &[unpolicied]),
        "the related collection Author must have the same policy (p1)"
    );
}

#[test]
fn the_binding_enters_a_governed_field_identity_only() {
    let book = bound_book();
    let bound = book.field_by_name("_authorID").unwrap().clone();
    let mut unbound = bound.clone();
    unbound.policy_relation = None;

    let governed = |field| generate_field_cid_with_priority(field, 1, true).unwrap();
    let ungoverned = |field| generate_field_cid_with_priority(field, 1, false).unwrap();

    assert_ne!(governed(&bound), governed(&unbound));
    assert_eq!(ungoverned(&bound), ungoverned(&unbound));
}

#[test]
fn a_synced_definition_needs_a_governed_collection_and_an_immutable_bound_field() {
    let book = bound_book();
    check_synced_policy_bindings(&book).unwrap();

    let mut mutable = book.clone();
    for field in &mut mutable.fields {
        if field.name == "_authorID" {
            field.immutable = false;
        }
    }
    assert!(matches!(
        check_synced_policy_bindings(&mutable),
        Err(SchemaError::InvalidPolicyRelation { reason, .. }) if reason.contains("@immutable")
    ));

    let mut ungoverned = book;
    ungoverned.governance_root = None;
    assert!(matches!(
        check_synced_policy_bindings(&ungoverned),
        Err(SchemaError::InvalidPolicyRelation { reason, .. }) if reason.contains("governed")
    ));
}
