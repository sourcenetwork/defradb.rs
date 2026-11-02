//! Fragments that spread each other in a cycle fail validation instead of
//! recursing (Go #5323, `defra.request.cyclic_fragments`).

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;

fn rejects(request: &str, error: &str) {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type User { name: String }".into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: request.into(),
            expected_error: Some(error.into()),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.request.cyclic_fragments")]
fn fragments_spreading_each_other_fail_validation() {
    rejects(
        "query { User { ...A } } fragment A on User { name ...B } fragment B on User { name ...A }",
        r#"Cannot spread fragment "A" within itself via B."#,
    );
}

#[test]
#[mantra_macros::req_test("defra.request.cyclic_fragments")]
fn a_cycle_no_field_spreads_still_fails_validation() {
    rejects(
        "query { User { name } } fragment A on User { name ...B } fragment B on User { name ...A }",
        r#"Cannot spread fragment "A" within itself via B."#,
    );
}

#[test]
#[mantra_macros::req_test("defra.request.cyclic_fragments")]
fn a_fragment_spreading_itself_fails_validation() {
    rejects(
        "query { User { ...A } } fragment A on User { name ...A }",
        r#"Cannot spread fragment "A" within itself."#,
    );
}
