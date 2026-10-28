use std::io::Write;
use std::net::{IpAddr, UdpSocket};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Address {
    ip: IpAddr,
    ipv6: bool,
}

#[derive(Deserialize)]
struct ExpectedSocket {
    address: Address,
    port: u16,
    required: bool,
}

#[derive(Deserialize)]
struct Case {
    input: Value,
    expected: Vec<ExpectedSocket>,
}

fn model_cases(inputs: &[Value]) -> Vec<Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proofs/lean");
    let mut child = Command::new("lake")
        .current_dir(root)
        .args(["env", "lean", "--run", "EndpointBind.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run prepared EndpointBind model");
    writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
    let output = child.wait_with_output().expect("finish binding model");
    assert!(
        output.status.success(),
        "binding model failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).expect("model cases");
    assert_eq!(cases.len(), inputs.len());
    for (case, input) in cases.iter().zip(inputs) {
        assert_eq!(&case.input, input, "model must evaluate the actual request");
    }
    cases
}

/// A held UDP port makes concrete-port refusal deterministic without a
/// close-then-rebind race. IPv6 coverage is conditional on an actual OS probe;
/// unavailable hosts still exercise IPv4, optional-family and preset behavior.
#[test]
#[ignore = "requires the prepared executable Lean model and native UDP sockets"]
fn native_bindings_match_executed_model() {
    let occupied = UdpSocket::bind("127.0.0.1:0").expect("hold a concrete UDP port");
    let occupied_addr = occupied.local_addr().unwrap();
    let mut inputs = vec![
        json!({"addr": null, "port": null}),
        json!({"addr": null, "port": 0}),
    ];
    let mut addresses = vec![IpAddr::from([127, 0, 0, 1])];
    match UdpSocket::bind("[::1]:0") {
        Ok(probe) => addresses.push(probe.local_addr().unwrap().ip()),
        Err(error) => {
            tracing::warn!(%error, "IPv6 loopback unavailable; explicit IPv6 binding not qualified")
        }
    }
    for address in addresses {
        for port in [None, Some(0)] {
            inputs.push(json!({
                "addr": {"ip":address.to_string(), "ipv6":address.is_ipv6()}, "port":port,
            }));
        }
    }
    inputs.push(json!({
        "addr":{"ip":occupied_addr.ip().to_string(),"ipv6":false},
        "port":occupied_addr.port(),
    }));
    let cases = model_cases(&inputs);
    let count = cases.len();
    edge::testing::edge()
        .block_on(edge::compat(async {
            for case in cases {
                let address = case.input["addr"]["ip"]
                    .as_str()
                    .map(|ip| ip.parse::<IpAddr>().unwrap());
                let port = case.input["port"]
                    .as_u64()
                    .map(|port| u16::try_from(port).unwrap());
                let builder = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
                    .relay_mode(iroh::RelayMode::Disabled);
                let endpoint = super::apply_bind_config(builder, address, port)
                    .expect("valid native request")
                    .bind()
                    .await;
                if case.expected.iter().any(|socket| {
                    socket.required
                        && socket.address.ip == occupied_addr.ip()
                        && socket.port == occupied_addr.port()
                }) {
                    assert!(
                        endpoint.is_err(),
                        "a required occupied port must not be replaced"
                    );
                    continue;
                }
                let endpoint = endpoint.expect("bind model-selected sockets");
                let actual = endpoint.bound_sockets();
                let matches = |actual: &std::net::SocketAddr, expected: &ExpectedSocket| {
                    actual.ip() == expected.address.ip
                        && actual.is_ipv6() == expected.address.ipv6
                        && actual.port() != 0
                        && (expected.port == 0 || actual.port() == expected.port)
                };
                for socket in &actual {
                    assert!(
                        case.expected
                            .iter()
                            .any(|expected| matches(socket, expected)),
                        "unrequested socket {socket} for {}",
                        case.input,
                    );
                }
                for expected in case.expected.iter().filter(|socket| socket.required) {
                    assert!(
                        actual.iter().any(|socket| matches(socket, expected)),
                        "required socket missing"
                    );
                }
                endpoint.close().await;
            }
        }))
        .expect("native endpoint model binding");
    tracing::info!(
        cases = count,
        "native listener requests matched the executable model"
    );
}
