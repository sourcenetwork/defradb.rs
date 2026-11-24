//! A document queried at a merge commit lists every version of its merged
//! history once in `_version`, as Go's DAG scan does
//! (`go.query.simple.cid.doc-version`).

use integration_test::actions::Request;
use integration_test::m;
use integration_test::scenario::{Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use crate::query::commits::with_depth_negative::merged_history;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_version_cid_on_merged_history_lists_each_version_once() {
    let mut actions = merged_history();
    actions.push(
        Request {
            request: Text::concat([
                Text::from(r#"query { Users(cid: ""#),
                Text::reference("m"),
                Text::from(r#"") { name _version { cid } } }"#),
            ]),
            // Unordered: versions are sorted by height, which is not Go's DAG
            // preorder on a merged history.
            expected_data: json!({
                "Users": [{
                    "name": "Addo",
                    "_version": m::unordered(json!([
                        { "cid": m::r("m") },
                        { "cid": m::r("mh1") },
                        { "cid": m::r("p1") },
                        { "cid": m::r("c0") },
                        { "cid": m::r("mh0") },
                        { "cid": m::r("p0") },
                    ])),
                }],
            }),
            common: Common {
                node: Some("db0".into()),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
