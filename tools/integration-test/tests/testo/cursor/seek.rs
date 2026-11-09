use super::{around, capture_end, collection, doc_in, names, query};
use integration_test::test_case::TestCase;
use serde_json::json;

const STAMPS: [(&str, &str); 4] = [
    ("jan", "2026-01-05T18:05:40Z"),
    ("feb", "2026-02-10T09:00:00Z"),
    ("mar", "2026-03-15T12:30:00Z"),
    ("apr", "2026-04-20T08:15:00Z"),
];

/// `collection` with `stamp` of `kind`, indexed, holding the four `STAMPS`,
/// paged two at a time by `stamp`: [jan, feb], then [mar, apr].
fn pages_by_stamp(collection_name: &str, kind: &str) {
    let mut actions = vec![collection(&format!(
        "type {collection_name} {{ name: String stamp: {kind} @index }}"
    ))];
    actions.extend(
        STAMPS
            .map(|(name, stamp)| doc_in(collection_name, json!({ "name": name, "stamp": stamp }))),
    );
    actions.extend([
        capture_end(
            query(
                format!("query {{ _cursor {{ {collection_name}(first: 2, order: [{{stamp: ASC}}]) {{ name }} _pageInfo {{ endCursor hasNext }} }} }}"),
                json!({ "_cursor": {
                    collection_name: names(&["jan", "feb"]),
                    "_pageInfo": { "endCursor": { "$match": "nonemptyString" }, "hasNext": true },
                } }),
            ),
            "feb",
        ),
        query(
            around(
                &format!("query {{ _cursor {{ {collection_name}(first: 2, after: \""),
                "feb",
                "\", order: [{stamp: ASC}]) { name } } }",
            ),
            json!({ "_cursor": { collection_name: names(&["mar", "apr"]) } }),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.seek")]
fn a_date_like_string_pages_as_a_string() {
    pages_by_stamp("Event", "String");
}

#[test]
#[mantra_macros::req_test("defra.cursor.seek")]
fn a_datetime_pages_across_the_boundary() {
    pages_by_stamp("Reading", "DateTime");
}
