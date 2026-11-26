//! A rootdir whose `config.yaml` Go DefraDB wrote starts a node with the
//! settings in it: the CLI's loader reads viper's lowercased keys into Rust's.

use integration_test::actions::{NewNode, PeerInfo, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

/// The `config.yaml` Go DefraDB's `start --no-keyring` writes into a new
/// rootdir, with `{p2paddress}` and `{p2pdisabled}` left to fill.
const GO_CONFIG: &str = r#""": ./root
acp:
    document:
        remote:
            address: ""
        type: local
api:
    address: 127.0.0.1:9181
    allowed-origins: []
    audience: ""
    privkeypath: ""
    pubkeypath: ""
datastore:
    badger:
        valuelogfilesize: 1073741824
    defaultkeytype: secp256k1
    maxtxnretries: 5
    noencryption: false
    nosearchableencryption: false
    nosigning: false
    path: data
    store: badger
development: false
keyring:
    backend: file
    disabled: true
    namespace: defradb
    path: keys
log:
    caller: false
    colordisabled: false
    format: text
    level: info
    output: stderr
    overrides: ""
    source: false
    stacktrace: false
net:
    p2paddresses:
        - {p2paddress}
    p2pblocksynctimeout: 30
    p2pdisabled: {p2pdisabled}
    peers: []
    pubsubenabled: true
    relay: false
replicator:
    retryintervals:
        - 30
        - 60
        - 120
        - 240
        - 480
        - 960
        - 1920
secretfile: .env
telemetry:
    disabled: false
"#;

fn go_config(p2paddress: &str, p2pdisabled: bool) -> String {
    GO_CONFIG
        .replace("{p2paddress}", p2paddress)
        .replace("{p2pdisabled}", &p2pdisabled.to_string())
}

/// A request that needs no collection, so the node's GraphQL endpoint is
/// shown to serve whatever the config's `development` says.
fn introspection() -> Request {
    Request {
        request: "query { __schema { queryType { name } } }".into(),
        expected_data: json!({ "__schema": { "queryType": { "name": "Query" } } }),
        ..Default::default()
    }
}

/// Go's `p2paddresses` are libp2p addresses, which a pinc node has no use
/// for: the node still runs P2P, on its own pinc address, which a config
/// naming no `net.pinc_bind_addr` binds on loopback.
#[test]
#[mantra_macros::req_test("defra.cli.config.go_layout")]
#[mantra_macros::req_test("defra.cli.pinc")]
fn a_go_config_runs_p2p_on_pinc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                go_config: Some(go_config("/ip4/127.0.0.1/tcp/9171", false)),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({
                    "addresses": [m::substring("@127.0.0.1:")],
                }),
                ..Default::default()
            }
            .into(),
            introspection().into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// `net.pinc_public_addrs` are listed ahead of the addresses the node binds.
#[test]
#[mantra_macros::req_test("defra.cli.pinc.public_addrs")]
fn a_configured_public_address_is_listed_first() {
    let config = go_config("/ip4/127.0.0.1/tcp/9171", false).replace(
        "    relay: false\n",
        "    relay: false\n    pinc_public_addrs:\n        - 203.0.113.7:9171\n",
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                go_config: Some(config),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({
                    "addresses": [
                        m::substring("@203.0.113.7:9171"),
                        m::substring("@127.0.0.1:"),
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.cli.config.go_layout")]
fn a_go_config_disables_p2p() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                go_config: Some(go_config("/ip4/127.0.0.1/tcp/0", true)),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({ "addresses": [] }),
                ..Default::default()
            }
            .into(),
            introspection().into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// A `net` key the node has no setting for fails the load, naming the key:
/// a stale `iroh_allowed_peers` or a misspelt allowlist would otherwise
/// leave the node accepting every peer.
#[test]
#[mantra_macros::req_test("defra.cli.config.unknown_net_key")]
fn an_unknown_net_key_fails_the_load() {
    for key in ["iroh_allowed_peers", "pinc_allowed_peer"] {
        let config = go_config("/ip4/127.0.0.1/tcp/9171", false).replace(
            "    relay: false\n",
            &format!("    relay: false\n    {key}:\n        - abc\n"),
        );
        TestCase {
            id: integration_test::scenario_id!(),
            actions: vec![NewNode {
                go_config: Some(config),
                expected_error: Some(format!("unknown field `{key}`, expected one of")),
                ..Default::default()
            }
            .into()],
            ..Default::default()
        }
        .execute();
    }
}
