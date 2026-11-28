//! The `collection` section of a policy resource: parsed, kept, never
//! evaluated by the local engine, and invisible to policies without it.

use acp::policy_yaml::{
    build_policy, generate_policy_id, parse_policy_yaml, validate_policy_expressions,
};
use acp::{validate_relation_binding, Policy};

const WITHOUT: &str = r#"
name: library
resources:
  - name: author
    relations:
      - name: publisher
        types: [actor]
    permissions:
      - name: publish
        expr: publisher
      - name: read
      - name: update
      - name: delete
  - name: book
    relations:
      - name: author
        types: [author]
      - name: editor
        types: [actor]
      - name: coauthor
        types: [author, actor]
    permissions:
      - name: read
        expr: author->publish
      - name: update
      - name: delete
"#;

fn with_create(create: &str) -> String {
    format!("{WITHOUT}    collection:\n      create: {create}\n")
}

fn built(yaml: &str) -> Policy {
    let parsed = parse_policy_yaml(yaml).unwrap();
    validate_policy_expressions(&parsed).unwrap();
    build_policy(&parsed, 1).unwrap()
}

#[test]
fn a_policy_without_the_section_parses_and_builds_as_before() {
    let parsed = parse_policy_yaml(WITHOUT).unwrap();
    assert!(parsed
        .resources
        .iter()
        .all(|resource| resource.collection.is_none()));
    let policy = build_policy(&parsed, 1).unwrap();
    for resource in &policy.resources {
        assert_eq!(resource.collection_create, None);
        assert!(!resource.is_create_gated());
    }
    let json = serde_json::to_string(&policy).unwrap();
    assert!(!json.contains("collection_create"), "{json}");
}

#[test]
fn the_section_round_trips_through_the_stored_policy() {
    let policy = built(&with_create("author->publish"));
    let json = serde_json::to_vec(&policy).unwrap();
    let stored: Policy = serde_json::from_slice(&json).unwrap();
    let book = stored.get_resource("book").unwrap();
    assert_eq!(book.collection_create.as_deref(), Some("author->publish"));
    assert!(book.is_create_gated());
    let author = stored.get_resource("author").unwrap();
    assert_eq!(author.collection_create, None);
    assert!(!author.is_create_gated());
}

#[test]
fn an_explicit_anyone_is_kept_and_not_gated() {
    let policy = built(&with_create(r#""*""#));
    let book = policy.get_resource("book").unwrap();
    assert_eq!(book.collection_create.as_deref(), Some("*"));
    assert!(!book.is_create_gated());
}

#[test]
fn an_empty_section_is_no_create() {
    let yaml = format!("{WITHOUT}    collection: {{}}\n");
    let policy = built(&yaml);
    assert!(!policy.get_resource("book").unwrap().is_create_gated());
}

#[test]
fn the_section_leaves_the_engine_model_unchanged() {
    let without = built(WITHOUT);
    let with = built(&with_create("author->publish"));
    for (a, b) in without.resources.iter().zip(&with.resources) {
        assert_eq!(
            serde_json::to_value(&a.relations).unwrap(),
            serde_json::to_value(&b.relations).unwrap()
        );
    }
}

#[test]
fn a_policy_id_moves_only_with_a_present_create() {
    let without = parse_policy_yaml(WITHOUT).unwrap();
    let empty = parse_policy_yaml(&format!("{WITHOUT}    collection: {{}}\n")).unwrap();
    let gated = parse_policy_yaml(&with_create("author->publish")).unwrap();
    let open = parse_policy_yaml(&with_create(r#""*""#)).unwrap();
    let id = |parsed| generate_policy_id(parsed, 1);
    assert_eq!(id(&without), id(&empty));
    assert_ne!(id(&without), id(&gated));
    assert_ne!(id(&without), id(&open));
    assert_ne!(id(&gated), id(&open));
}

#[test]
fn an_empty_create_and_unknown_fields_are_refused() {
    let empty = parse_policy_yaml(&with_create(r#""""#)).unwrap();
    let err = build_policy(&empty, 1).unwrap_err().to_string();
    assert!(
        err.contains(
            "collection create of resource 'book' must not be empty: omit it or write \"*\" to let anyone create"
        ),
        "{err}"
    );
    let unknown = format!("{WITHOUT}    collection:\n      delete: owner\n");
    assert!(parse_policy_yaml(&unknown).is_err());
}

#[test]
fn a_relation_binding_needs_a_declared_relation_of_exactly_the_target_resource() {
    let policy = built(WITHOUT);
    validate_relation_binding(&policy, "book", "author", "author").unwrap();

    let refused = |relation: &str, target: &str| {
        validate_relation_binding(&policy, "book", relation, target).unwrap_err()
    };
    assert!(refused("missing", "author").contains("relation does not exist on the resource"));
    assert!(refused("read", "author").contains("relation does not exist on the resource"));
    assert!(refused("owner", "author").contains("the owner relation cannot be bound"));
    for (relation, target) in [
        ("editor", "author"),
        ("coauthor", "author"),
        ("author", "book"),
    ] {
        assert!(
            refused(relation, target)
                .contains("relation types must be exactly the related collection's resource"),
            "{relation} -> {target}"
        );
    }
}
