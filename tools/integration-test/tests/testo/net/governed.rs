//! A node that embeds a merge validator judges every collection that
//! declares a governance root, without being told its name. A node with no
//! merge validator replicates such a collection as it replicates any other.
//! `@governed` is defradb.rs only, so these run only against the Rust node.

use integration_test::actions::stream::IrohSpec;
use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewIrohNode, NewNode, Request, Wait,
    WaitForBlocks, WaitForPeersEvents, WaitForSync,
};
use integration_test::merge_validator::RejectValue;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const LEDGER: &str = r#"type Ledger @governed(root: "root-a") { entry: String }"#;

const SENDER: &str = "db0";
const RECEIVER: &str = "db1";

/// How long a block the receiver holds may take to be judged. The verdict
/// follows the block's arrival within milliseconds; a rejected document
/// merged by mistake would show up well inside this.
const JUDGED_WITHIN_MILLIS: u64 = 2_000;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// Both nodes define `Ledger`, after they started, and the sender replicates
/// it to the receiver.
fn replicating_ledger() -> Vec<Actions> {
    let mut actions: Vec<Actions> = Vec::new();
    for node in [SENDER, RECEIVER] {
        actions.push(
            AddCollection {
                sdl: LEDGER.into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in [SENDER, RECEIVER] {
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        AddReplicator {
            target: RECEIVER.into(),
            collections: vec!["Ledger".into()],
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
    );
    actions
}

/// The sender writes a `Ledger` entry.
fn written(entry: &str) -> Actions {
    AddDoc {
        collection: "Ledger".into(),
        doc_map: Some(json!({ "entry": entry })),
        common: Common {
            retry: true,
            ..on(SENDER)
        },
        ..Default::default()
    }
    .into()
}

/// The sender writes a `Ledger` entry and captures its head as `head`.
fn written_capturing_head(entry: &str, head: &str) -> Vec<Actions> {
    let doc = format!("{head}Doc");
    vec![
        AddDoc {
            collection: "Ledger".into(),
            doc_map: Some(json!({ "entry": entry })),
            common: Common {
                retry: true,
                capture: captures([(doc.as_str(), "/docID")]),
                ..on(SENDER)
            },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference(&doc)),
            common: Common {
                capture: captures([(head, "/cid")]),
                ..on(SENDER)
            },
            ..Default::default()
        }
        .into(),
    ]
}

/// `node` holds exactly `entries` in `Ledger`.
fn ledger_holds(node: &str, entries: Value, wait: bool) -> Actions {
    Request {
        request: "query { Ledger { entry } }".into(),
        expected_data: json!({ "Ledger": entries }),
        common: Common {
            eventually: wait.then(Eventually::default),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.net.governed.no_validator")]
fn a_node_with_no_merge_validator_merges_a_governed_collection_as_before() {
    let mut actions: Vec<Actions> = Vec::new();
    for node in [SENDER, RECEIVER] {
        actions.push(
            NewNode {
                network: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend(replicating_ledger());
    actions.extend([
        written("unjudged"),
        ledger_holds(RECEIVER, json!([{ "entry": "unjudged" }]), true),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

/// A merge validator is a Rust embedder API with no HTTP, CLI or Go surface,
/// so this runs under the in-process client only. The receiver claims no
/// collection by name, and `Ledger` is defined after it started. The
/// absence of the rejected document is checked only once its head is in
/// the receiver's blockstore and the accepted document has merged, and again
/// after a grace period, so it does not depend on the order the two
/// documents arrive in.
#[test]
#[mantra_macros::req_test("defra.net.governed.judged")]
fn a_merge_validator_judges_a_governed_collection_defined_after_it_started() {
    let mut actions: Vec<Actions> = vec![
        NewIrohNode {
            common: on(SENDER),
            ..Default::default()
        }
        .into(),
        NewIrohNode {
            iroh: IrohSpec {
                merge_validator: Some(RejectValue {
                    field: "entry".into(),
                    value: "forbidden".into(),
                }),
                ..Default::default()
            },
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
    ];
    actions.extend(replicating_ledger());
    actions.extend(written_capturing_head("forbidden", "forbiddenHead"));
    actions.extend([
        written("allowed"),
        WaitForBlocks {
            cids: vec![Text::reference("forbiddenHead")],
            common: on(RECEIVER),
            ..Default::default()
        }
        .into(),
        ledger_holds(RECEIVER, json!([{ "entry": "allowed" }]), true),
        Wait {
            millis: Some(JUDGED_WITHIN_MILLIS),
            ..Default::default()
        }
        .into(),
        ledger_holds(RECEIVER, json!([{ "entry": "allowed" }]), false),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}
