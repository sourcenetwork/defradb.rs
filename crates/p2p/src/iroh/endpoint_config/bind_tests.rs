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
    port_mapping_allowed: bool,
}

#[test]
#[ignore = "requires the prepared executable Lean model"]
fn native_port_mapping_configuration_matches_executed_model() {
    let mut inputs = Vec::new();
    for address in [
        None,
        Some("0.0.0.0"),
        Some("::"),
        Some("127.0.0.1"),
        Some("::1"),
        Some("192.168.1.10"),
        Some("198.51.100.10"),
        Some("2001:db8::10"),
    ] {
        for port in [None, Some(0), Some(9512)] {
            inputs.push(json!({
                "addr": address.map(|ip| {
                    let ip: IpAddr = ip.parse().unwrap();
                    json!({"ip":ip.to_string(),"ipv6":ip.is_ipv6()})
                }),
                "port": port,
            }));
        }
    }
    for case in model_cases(&inputs) {
        let address = case.input["addr"]["ip"]
            .as_str()
            .map(|ip| ip.parse::<IpAddr>().unwrap());
        let port = case.input["port"]
            .as_u64()
            .map(|port| u16::try_from(port).unwrap());
        let builder = super::apply_bind_config(
            iroh::Endpoint::builder(iroh::endpoint::presets::Minimal),
            address,
            port,
        )
        .unwrap();
        assert_eq!(
            format!("{builder:?}").contains("portmapper_config: Disabled"),
            !case.port_mapping_allowed,
            "native mapper configuration disagrees with the model for {}",
            case.input,
        );
    }
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

/// Only bound wildcard UDP listeners are gateway-discovery leaks here. A
/// connected, short-lived socket used to inspect routing is not such a listener.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires native UDP sockets and the owned-process lsof observation"]
fn loopback_endpoint_does_not_open_wildcard_gateway_discovery_listeners() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            let builder = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
                .relay_mode(iroh::RelayMode::Disabled);
            let endpoint =
                super::apply_bind_config(builder, Some(IpAddr::from([127, 0, 0, 1])), Some(0))
                    .unwrap()
                    .bind()
                    .await
                    .unwrap();
            let pid = std::process::id().to_string();
            let deadline = crate::rt::time::Instant::now() + std::time::Duration::from_secs(2);
            let mut observations = 0;
            loop {
                let process = pid.clone();
                let output = nuclei::spawn_blocking(move || {
                    Command::new("/usr/sbin/lsof")
                        .args(["-a", "-p", &process, "-iUDP", "-nP", "-Ftn"])
                        .output()
                })
                .join()
                .await
                .unwrap()
                .unwrap();
                assert!(
                    output.status.success(),
                    "owned UDP socket observation failed"
                );
                let listing = String::from_utf8(output.stdout).unwrap();
                assert!(listing.lines().any(|line| line.starts_with("n127.0.0.1:")));
                let wildcard = listing.lines().find(|line| {
                    ["n*:", "n0.0.0.0:", "n[::]:"].iter().any(|prefix| {
                        line.strip_prefix(prefix)
                            .and_then(|port| port.parse::<u16>().ok())
                            .is_some_and(|port| port != 0)
                    })
                });
                assert!(
                    wildcard.is_none(),
                    "unscoped gateway listener: {wildcard:?}"
                );
                observations += 1;
                if crate::rt::time::Instant::now() >= deadline {
                    break;
                }
                crate::rt::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            endpoint.close().await;
            assert!(observations > 1);
        }))
        .expect("native loopback gateway scope");
}
