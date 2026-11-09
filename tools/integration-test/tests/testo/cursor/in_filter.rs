use super::{aged, ages, capture_end, page, rows, USERS};
use integration_test::actions::Request;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

const IN: &str = "filter: {age: {_in: [55, 25, 45, 35]}}";

#[test]
#[mantra_macros::req_test("defra.cursor.in_filter_order")]
fn an_unsorted_in_filter_pages_in_order() {
    let args = format!("first: 2, {IN}, order: {{age: ASC}}");
    let mut actions = aged(USERS, &[25, 35, 45, 55, 65]);
    actions.extend([
        capture_end(
            page(&args, None, "age", rows(ages(&[25, 35]), true, false)),
            "first_end",
        ),
        page(
            &args,
            Some(Text::reference("first_end")),
            "age",
            rows(ages(&[45, 55]), false, true),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.in_filter_order")]
fn an_unsorted_in_filter_keeps_the_order_without_a_cursor() {
    for sdl in [
        USERS,
        "type User { name: String age: Int @index(direction: DESC) }",
        "type User { name: String age: Int }",
    ] {
        for (order, want) in [("ASC", [25, 35, 45, 55]), ("DESC", [55, 45, 35, 25])] {
            let mut actions = aged(sdl, &[25, 35, 45, 55, 65]);
            actions.push(
                Request {
                    request: format!("query {{ User({IN}, order: {{age: {order}}}) {{ age }} }}")
                        .into(),
                    expected_data: json!({ "User": ages(&want) }),
                    ..Default::default()
                }
                .into(),
            );
            TestCase::from_actions(actions).execute();
        }
    }
}
