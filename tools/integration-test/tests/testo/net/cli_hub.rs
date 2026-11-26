//! `defradb hub` as an operator runs it (`defra.cli.hub`): it exits 0 on
//! SIGINT and SIGTERM and keeps its peer id across a restart; and a node's
//! `defradb start --peers` dials them once started (`defra.cli.pinc`).

use integration_test::actions::{
    ActivePeers, HubSignal, NewNode, PeerInfo, StartCliHub, StopCliHub,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn start(expected_data: serde_json::Value, common: Common) -> StartCliHub {
    StartCliHub {
        name: "hub".into(),
        expected_data,
        common,
        ..Default::default()
    }
}

fn stop(signal: HubSignal) -> StopCliHub {
    StopCliHub {
        name: "hub".into(),
        signal,
        expected_data: json!({ "exitCode": 0 }),
        ..Default::default()
    }
}

fn exits_0_on(signal: HubSignal) {
    TestCase::from_actions(vec![
        start(
            json!({ "address": m::substring("@127.0.0.1:"), "peerId": m::nonempty_string() }),
            Common::default(),
        )
        .into(),
        stop(signal).into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.cli.hub")]
fn a_cli_hub_exits_0_on_sigint() {
    exits_0_on(HubSignal::Interrupt);
}

#[test]
#[mantra_macros::req_test("defra.cli.hub")]
fn a_cli_hub_exits_0_on_sigterm() {
    exits_0_on(HubSignal::Terminate);
}

/// The second start reads the key the first wrote to `<rootdir>/hub.key`.
#[test]
#[mantra_macros::req_test("defra.cli.hub")]
fn a_cli_hub_keeps_its_peer_id_across_a_restart() {
    TestCase::from_actions(vec![
        start(
            serde_json::Value::Null,
            Common {
                capture: captures([("hub_id", "/peerId")]),
                ..Default::default()
            },
        )
        .into(),
        stop(HubSignal::Terminate).into(),
        start(
            json!({ "address": m::substring("@127.0.0.1:"), "peerId": m::r("hub_id") }),
            Common::default(),
        )
        .into(),
        stop(HubSignal::Interrupt).into(),
    ])
    .execute();
}

/// db1 is configured as `defradb start --peers <db0>` is: started, it dials
/// db0 without being asked to connect.
#[test]
#[mantra_macros::req_test("defra.cli.pinc")]
fn a_node_started_with_peers_dials_them() {
    TestCase::from_actions(vec![
        NewNode {
            network: true,
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        PeerInfo {
            common: Common {
                capture: captures([("peer0", "/addresses/0")]),
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
        NewNode {
            start_args: vec!["--peers".into(), Text::reference("peer0")],
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0")])) }),
            common: Common {
                eventually: Some(Eventually {
                    timeout_millis: Some(15_000),
                    ..Default::default()
                }),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
