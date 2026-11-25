//! The HTTP client-signing session asks for each wave of a create once and
//! refuses an answer that re-signs a wave or signs outside the approved graph
//! (`defra.signature.client_session`).

use integration_test::actions::{
    Actions, AddCollection, ClientSigningSession, Request, SigningMisstep,
};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const CREATE: &str = r#"mutation { add_Users(input: {name: "John"}) { name } }"#;

fn session(
    id: &'static str,
    misstep: Option<SigningMisstep>,
    expected: Result<serde_json::Value, &str>,
) {
    let (expected_data, expected_error) = match expected {
        Ok(data) => (data, None),
        Err(error) => (serde_json::Value::Null, Some(error.to_owned())),
    };
    let users = if expected_error.is_some() {
        json!({ "Users": [] })
    } else {
        json!({ "Users": [{ "name": "John" }] })
    };
    let actions: Vec<Actions> = vec![
        AddCollection {
            sdl: "type Users { name: String }".into(),
            ..Default::default()
        }
        .into(),
        ClientSigningSession {
            request: CREATE.into(),
            misstep,
            expected_data,
            expected_error,
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Users { name } }".into(),
            expected_data: users,
            ..Default::default()
        }
        .into(),
    ];
    TestCase {
        id,
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::RustHttp]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.signature.client_session.waves")]
fn a_client_that_signs_each_wave_once_creates_the_document() {
    session(
        integration_test::scenario_id!(),
        None,
        Ok(json!({ "add_Users": [{ "name": "John" }] })),
    );
}

#[test]
#[mantra_macros::req_test("defra.signature.client_session.resigned")]
fn a_client_that_answers_a_wave_twice_is_refused() {
    session(
        integration_test::scenario_id!(),
        Some(SigningMisstep::AnswerTwice),
        Err("signing wave 1 was already signed"),
    );
}

#[test]
#[mantra_macros::req_test("defra.signature.client_session.outside_graph")]
fn a_client_that_answers_an_unrequested_wave_is_refused() {
    session(
        integration_test::scenario_id!(),
        Some(SigningMisstep::UnrequestedWave),
        Err("signing wave 9 is outside the approved graph"),
    );
}

#[test]
#[mantra_macros::req_test("defra.signature.client_session.outside_graph")]
fn a_client_that_signs_a_block_outside_the_graph_is_refused() {
    session(
        integration_test::scenario_id!(),
        Some(SigningMisstep::ForeignBlock),
        Err("signature 0 of signing wave 1 does not sign a block of the approved graph"),
    );
}
