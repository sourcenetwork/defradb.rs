//! Public notes pointing at documents only Alice (identity 1) may read.
//! Bob (identity 2) reads the notes but must learn nothing about the rest.

use integration_test::actions::{Actions, AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;

const POLICY: &str = r#"
description: A Policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#;

/// Which indexes the schema declares, so each join shape gets planned.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Indexes {
    None,
    Fields,
}

fn sdl(indexes: Indexes) -> Text {
    let index = if indexes == Indexes::Fields {
        "@index"
    } else {
        ""
    };
    let private = |name: &str| {
        [
            Text::from(format!("type {name} @policy(id: \"")),
            Text::reference("policy0"),
            Text::from("\", resource: \"users\") "),
        ]
    };
    let mut parts = Vec::new();
    parts.extend(private("Secret"));
    parts.push(Text::from(format!(
        "{{ pin: String {index} notes: [Note] }}\n"
    )));
    parts.extend(private("Vault"));
    parts.push(Text::from(format!(
        "{{ code: String {index} note: Note @primary }}\n"
    )));
    parts.extend(private("Tag"));
    parts.push(Text::from(format!(
        "{{ code: String {index} note: Note {index} }}\n"
    )));
    parts.push(Text::from(format!(
        "type Note {{ name: String s: Secret {index} vault: Vault tags: [Tag] }}\n"
    )));
    Text::concat(parts)
}

fn add(collection: &str, doc: Value, identity: Option<u64>, name: &str) -> ActionInstance<Actions> {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        common: Common {
            identity_seed: identity,
            retry: true,
            capture: captures([(name, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Alice's secrets s1 (pin 4721) and s2 (pin 1111), public notes n1 and n2
/// pointing at them, and Alice's vault v1 and tag t1 on n1.
pub fn setup(indexes: Indexes) -> Vec<ActionInstance<Actions>> {
    vec![
        AddDACPolicy {
            policy: POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("policy0", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: sdl(indexes),
            ..Default::default()
        }
        .into(),
        add("Secret", json!({ "pin": "4721" }), Some(ALICE), "s1"),
        add("Secret", json!({ "pin": "1111" }), Some(ALICE), "s2"),
        add("Note", json!({ "name": "n1", "s": m::r("s1") }), None, "n1"),
        add("Note", json!({ "name": "n2", "s": m::r("s2") }), None, "n2"),
        add(
            "Vault",
            json!({ "code": "v4721", "note": m::r("n1") }),
            Some(ALICE),
            "v1",
        ),
        add(
            "Tag",
            json!({ "code": "c4721", "note": m::r("n1") }),
            Some(ALICE),
            "t1",
        ),
    ]
}

pub fn query(identity: u64, request: &str, expected: Value) -> ActionInstance<Actions> {
    Request {
        request: request.into(),
        expected_data: expected,
        common: Common {
            identity_seed: Some(identity),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Run `queries` against a fresh [`setup`].
pub fn run(id: &'static str, indexes: Indexes, queries: Vec<ActionInstance<Actions>>) {
    let mut actions = setup(indexes);
    actions.extend(queries);
    TestCase {
        id,
        actions,
        ..Default::default()
    }
    .execute();
}

/// One test per shape in `shapes`, each against a fresh [`setup`].
macro_rules! shape_tests {
    ($indexes:expr; $($shape:ident),* $(,)?) => {
        $(
            #[test]
            #[mantra_macros::req_test("go.acp.dac.query.relations")]
            fn $shape() {
                super::support::run(
                    integration_test::scenario_id!(),
                    $indexes,
                    super::shapes::$shape(),
                );
            }
        )*
    };
}
pub(super) use shape_tests;
