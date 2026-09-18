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

/// Which commitments reach the identity, and which do not.
///
/// A commitment is a promise to writers — who governs, what may never change,
/// whether history is verifiable. A representation or performance choice a
/// node can make and unmake is not: an index is added to a live collection and
/// backfilled under the same version ID, so it cannot be part of an identity
/// it would have to change.
#[test]
fn a_governed_identity_commits_to_immutability_and_branchability() {
    let id_for = |sdl: &str| parse_sdl(sdl).unwrap()[0].collection_id.clone();
    let governed = id_for(r#"type Agent @governed(root: "root-a") { did: String, body: String }"#);

    for sdl in [
        r#"type Agent @governed(root: "root-a") { did: String @immutable, body: String }"#,
        r#"type Agent @governed(root: "root-a") @branchable { did: String, body: String }"#,
    ] {
        assert_ne!(id_for(sdl), governed, "commitment did not move: {sdl}");
    }

    assert_eq!(
        id_for(r#"type Agent @governed(root: "root-a") { did: String @index, body: String }"#),
        governed,
        "an index is configuration and must not move the identity"
    );
}

/// The gate is the root. An ungoverned collection derives what it always
/// derived, whatever it declares about immutability or branchable history, so
/// nothing anyone has today changes identity. The opt-in is in the schema
/// because the identity is: an opt-in in node configuration would let two
/// nodes running the same schema disagree about what the collection is.
#[test]
fn an_ungoverned_identity_commits_to_neither() {
    let id_for = |sdl: &str| parse_sdl(sdl).unwrap()[0].collection_id.clone();

    for sdl in [
        r#"type Agent { did: String @immutable, body: String }"#,
        r#"type Agent @branchable { did: String, body: String }"#,
        r#"type Agent @branchable { did: String @immutable, body: String }"#,
    ] {
        assert_eq!(id_for(sdl), UNGOVERNED_AGENT_ID, "identity moved: {sdl}");
    }
}

/// Each commitment moves a governed identity on its own, and no two of them
/// collide.
#[test]
fn each_commitment_mints_a_distinct_governed_identity() {
    let id_for = |sdl: &str| parse_sdl(sdl).unwrap()[0].collection_id.clone();
    let mut ids = vec![
        id_for(r#"type Agent @governed(root: "root-a") { did: String, body: String }"#),
        id_for(r#"type Agent @governed(root: "root-b") { did: String, body: String }"#),
        id_for(r#"type Agent @governed(root: "root-a") { did: String @immutable, body: String }"#),
        id_for(r#"type Agent @governed(root: "root-a") @branchable { did: String, body: String }"#),
        id_for(
            r#"type Agent @governed(root: "root-a") @branchable { did: String @immutable, body: String }"#,
        ),
    ];
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "two commitments share an identity");
}

/// For a governed collection a policy reaches the version and not the
/// collection: attaching or amending one mints a new version of the same
/// collection, rather than a different collection whose documents the old
/// one's no longer belong to.
#[test]
fn a_policy_moves_a_governed_version_id_and_not_its_collection_id() {
    let parse = |sdl: &str| parse_sdl(sdl).unwrap().remove(0);

    let bare = parse(r#"type Agent @governed(root: "root-a") { did: String, body: String }"#);
    let policied = parse(
        r#"type Agent @governed(root: "root-a") @policy(id: "p1", resource: "agents") { did: String, body: String }"#,
    );
    let other = parse(
        r#"type Agent @governed(root: "root-a") @policy(id: "p2", resource: "agents") { did: String, body: String }"#,
    );

    assert_eq!(bare.version_id, bare.collection_id);
    assert_eq!(policied.collection_id, bare.collection_id);
    assert_eq!(other.collection_id, bare.collection_id);

    assert_ne!(policied.version_id, policied.collection_id);
    assert_ne!(policied.version_id, bare.version_id);
    assert_ne!(policied.version_id, other.version_id);
}

/// An ungoverned collection commits to no policy: both its identities are
/// where they were, so a policy on an existing collection changes neither.
#[test]
fn an_ungoverned_policy_moves_neither_identity() {
    let policied = parse_sdl(
        r#"type Agent @policy(id: "p1", resource: "agents") { did: String, body: String }"#,
    )
    .unwrap()
    .remove(0);

    assert_eq!(policied.collection_id, UNGOVERNED_AGENT_ID);
    assert_eq!(policied.version_id, UNGOVERNED_AGENT_ID);
}
