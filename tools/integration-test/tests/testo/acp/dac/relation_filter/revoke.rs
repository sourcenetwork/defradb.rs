//! Bob read a secret once. After Alice revokes him, a filter through a public
//! note must not let him read the pin back one digit at a time.

use integration_test::actions::{AddDACActorRelationship, DeleteDACActorRelationship};
use integration_test::scenario::{Common, Text};
use serde_json::json;

use super::support::{query, run, Indexes, ALICE, BOB};

#[test]
#[mantra_macros::req_test("go.acp.dac.query.relations")]
fn relation_filter_on_a_revoked_policy_document() {
    let grant = |relation: &str| AddDACActorRelationship {
        target_identity_seed: Some(BOB),
        collection: "Secret".into(),
        doc_id: Text::reference("s1"),
        relation: relation.into(),
        expected_data: json!({ "existedAlready": false }),
        common: Common {
            identity_seed: Some(ALICE),
            ..Default::default()
        },
        ..Default::default()
    };
    let revoke = DeleteDACActorRelationship {
        target_identity_seed: Some(BOB),
        collection: "Secret".into(),
        doc_id: Text::reference("s1"),
        relation: "reader".into(),
        expected_data: json!({ "recordFound": true }),
        common: Common {
            identity_seed: Some(ALICE),
            ..Default::default()
        },
        ..Default::default()
    };
    let probe = |pattern: &str| {
        format!(r#"query {{ Note(filter: {{s: {{pin: {{_like: "{pattern}"}}}}}}) {{ name }} }}"#)
    };
    let joined = r#"query { Note(filter: {name: {_eq: "n1"}}) { name s { pin } } }"#;
    let n1 = json!({ "Note": [{ "name": "n1" }] });
    let none = json!({ "Note": [] });
    run(
        integration_test::scenario_id!(),
        Indexes::None,
        vec![
            grant("reader").into(),
            query(BOB, &probe("4%"), n1.clone()),
            revoke.into(),
            query(
                BOB,
                joined,
                json!({ "Note": [{ "name": "n1", "s": null }] }),
            ),
            query(BOB, &probe("4%"), none.clone()),
            query(BOB, &probe("47%"), none.clone()),
            query(BOB, &probe("4721"), none),
            query(ALICE, &probe("4721"), n1),
        ],
    );
}
