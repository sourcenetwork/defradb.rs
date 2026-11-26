//! A data directory where Go DefraDB keeps its Badger datastore is refused at
//! start, with what it holds and what to do, rather than failing inside the
//! storage engine.

use integration_test::actions::NewNode;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.node.start.go_datastore")]
fn a_go_badger_datastore_is_refused_with_what_to_do() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![NewNode {
            go_datastore: true,
            expected_error: Some(
                "holds a Badger datastore written by Go DefraDB, which defradb.rs cannot read; \
                 start this node on an empty data directory (another --rootdir or \
                 datastore.path), and move the documents with Go's `defradb client backup \
                 export` and this node's `defradb client backup import`"
                    .into(),
            ),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
