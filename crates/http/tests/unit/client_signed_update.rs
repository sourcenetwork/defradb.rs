use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn single_document_filter_admission_matches_model() {
    let requests = [
        r#"mutation { update_Item(docID: "doc", input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(docID: "other", input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(filter: {_docID: {_eq: "doc"}, status: {_eq: "open"}}, input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(filter: {_docID: {_eq: "other"}}, input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(filter: {_docID: {_in: ["doc"]}}, input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(filter: {_or: [{_docID: {_eq: "doc"}}]}, input: {status: "done"}) { _docID } }"#,
        r#"mutation { update_Item(docID: "doc", filter: {_docID: {_eq: "doc"}}, input: {status: "done"}) { _docID } }"#,
    ];
    let mutations = requests
        .iter()
        .map(|source| {
            let query::query_parse::ParsedOperation::Mutation { mut mutations, .. } =
                query::query_parse::parse_request_with_limits(
                    source,
                    None,
                    None,
                    query::QueryLimits::default(),
                )
                .unwrap()
            else {
                panic!("expected mutation")
            };
            mutations.remove(0)
        })
        .collect::<Vec<_>>();
    let inputs = mutations
        .iter()
        .map(|mutation| {
            let direct = mutation.doc_ids.as_ref().map(|ids| {
                if ids.len() == 1 {
                    ids[0].as_str()
                } else {
                    "multiple"
                }
            });
            json!({"expected": "doc", "direct": direct,
            "filter": mutation.filter.as_ref().map(|filter| filter.conditions())})
        })
        .collect::<Vec<_>>();
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "ClientSignedUpdateFilter.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (mutation, modeled) in mutations.iter().zip(expected) {
        assert_eq!(
            super::single_document_target(mutation) == Some("doc"),
            modeled["accepted"].as_bool().unwrap()
        );
    }
}
