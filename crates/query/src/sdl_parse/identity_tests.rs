//! The identities an ungoverned collection has, pinned byte for byte.
//!
//! A collection ID is a CID over the type name and the CIDs of its field
//! definitions; a field definition CID covers a field's name, CRDT type and
//! kind. For a new schema the version ID equals the collection ID. Nothing
//! about who governs a collection reaches either: `@policy`, `@immutable`,
//! `@branchable` and `@index` all leave both exactly where the bare schema
//! left them.
//!
//! The literals are the identities this tree produced at `288f1ffe`, the head
//! of the interface PR, before governance entered the identities. They are
//! here so that a change to what an identity commits to has to rewrite this
//! file and say which identity it moved.

use super::parse_sdl;

/// A schema and the identity every one of its collections must keep.
struct Fixture {
    label: &'static str,
    sdl: &'static str,
    /// Collection name and collection ID, in declaration order.
    identities: &'static [(&'static str, &'static str)],
}

/// Every ungoverned `Agent` fixture below derives this, whatever governance
/// directives it carries.
const UNGOVERNED_AGENT_ID: &str = "bafyreibeq6rbxfwgvzuaow4alns74u3f4t6wv7udfheyul4l5ju2vyblfy";

const FIXTURES: &[Fixture] = &[
    Fixture {
        label: "plain",
        sdl: r#"type Agent { did: String, body: String }"#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "immutable field",
        sdl: r#"type Agent { did: String @immutable, body: String }"#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "immutable field with an index",
        sdl: r#"type Agent { did: String @immutable @index, body: String }"#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "branchable",
        sdl: r#"type Agent @branchable { did: String, body: String }"#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "policy",
        sdl: r#"type Agent @policy(id: "p1", resource: "agents") { did: String, body: String }"#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "policy, branchable and an immutable indexed field at once",
        sdl: r#"
            type Agent @branchable @policy(id: "p1", resource: "agents") {
                did: String @immutable @index
                body: String
            }
        "#,
        identities: &[("Agent", UNGOVERNED_AGENT_ID)],
    },
    Fixture {
        label: "a non-default CRDT",
        sdl: r#"
            type Counter {
                value: Int @crdt(type: "pncounter")
                label: String
            }
        "#,
        identities: &[(
            "Counter",
            "bafyreiawyq5yopgssitfyp5w27wqe7bkas7oyexp6pusg5nq5vphqvsxhi",
        )],
    },
    Fixture {
        label: "a one-to-many relation",
        sdl: r#"
            type Book {
                name: String
                rating: Float
                author: Author
            }
            type Author {
                name: String
                age: Int
                verified: Boolean
                published: [Book]
            }
        "#,
        identities: &[
            (
                "Book",
                "bafyreihpq2q7a7bgpmp54uwzpwomrmzar77qu4ncjrukumbj66pxomrlsq",
            ),
            (
                "Author",
                "bafyreibsjnlzaqfu6lq2njqjfgot2p4lwjhoxp63karkxzfu7flft4fohy",
            ),
        ],
    },
];

#[test]
fn ungoverned_identities_are_pinned() {
    for fixture in FIXTURES {
        let collections = parse_sdl(fixture.sdl).unwrap();
        assert_eq!(
            collections.len(),
            fixture.identities.len(),
            "collection count moved for: {}",
            fixture.label
        );
        for (collection, (name, collection_id)) in collections.iter().zip(fixture.identities) {
            assert_eq!(collection.name, *name, "order moved for: {}", fixture.label);
            assert_eq!(
                collection.collection_id, *collection_id,
                "collection ID moved for: {} ({})",
                fixture.label, name
            );
            assert_eq!(
                collection.version_id, collection.collection_id,
                "a new schema's version ID is its collection ID: {} ({})",
                fixture.label, name
            );
        }
    }
}

/// A governance root is part of the collection's identity: the same schema
/// under two roots is two collections, and a node that does not know the root
/// derives neither.
#[test]
fn a_governance_root_changes_the_collection_id() {
    let id_for = |sdl: &str| parse_sdl(sdl).unwrap()[0].collection_id.clone();

    let ungoverned = id_for(r#"type Agent { did: String, body: String }"#);
    let root_a = id_for(r#"type Agent @governed(root: "root-a") { did: String, body: String }"#);
    let root_b = id_for(r#"type Agent @governed(root: "root-b") { did: String, body: String }"#);

    assert_eq!(ungoverned, UNGOVERNED_AGENT_ID);
    assert_ne!(root_a, ungoverned);
    assert_ne!(root_b, ungoverned);
    assert_ne!(root_a, root_b);
}

/// The enforcement is structural rather than a handshake: a node that runs the
/// same schema without the root derives a different collection ID, so it is a
/// replica of a different collection and not of the governed one.
#[test]
fn a_node_that_does_not_know_the_root_is_not_a_replica() {
    let governed = r#"type Agent @governed(root: "root-a") { did: String, body: String }"#;
    let without_the_root = r#"type Agent { did: String, body: String }"#;

    assert_ne!(
        parse_sdl(governed).unwrap()[0].collection_id,
        parse_sdl(without_the_root).unwrap()[0].collection_id
    );
}

/// The derivation stays a function of the schema, so a second node parsing the
/// same governed SDL reaches the same identity.
#[test]
fn the_same_governed_schema_derives_the_same_identity() {
    let sdl = r#"type Agent @governed(root: "root-a") { did: String, body: String }"#;
    let once = &parse_sdl(sdl).unwrap()[0];
    let twice = &parse_sdl(sdl).unwrap()[0];

    assert_eq!(once.collection_id, twice.collection_id);
    assert_eq!(once.version_id, twice.version_id);
    assert_eq!(once.governance_root, twice.governance_root);
}
