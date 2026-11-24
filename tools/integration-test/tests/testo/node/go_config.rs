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

/// A port nothing listens on right now, for the node to bind.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("pick a free port")
        .port()
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

#[test]
#[mantra_macros::req_test("defra.cli.config.go_layout")]
fn a_go_config_sets_the_p2p_addresses() {
    let port = free_port();
    let address = format!("/ip4/127.0.0.1/tcp/{port}");
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                go_config: Some(go_config(&address, false)),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                expected_data: json!({
                    "addresses": [m::substring(&format!("{address}/p2p/"))],
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
