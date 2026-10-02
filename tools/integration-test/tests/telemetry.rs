//! OTLP export is observable from the integration harness.
//!
//! The harness passes `--no-telemetry` to every Rust node unconditionally. It
//! also exposes `with_extra_rust_args`, documented as landing after the managed
//! flags "so they win under clap's last-one-wins parsing" — which only holds
//! because the CLI's `--no-telemetry` now `overrides_with` itself instead of
//! rejecting a repeat. Together those need no change to the harness crate.
//!
//! Requires `DEFRA_RUST_BINARY` to point at a node built with
//! `--features otel`. Whether it was is a property of the binary, not of this
//! crate, and nothing in `defra version` reports it — so the caller declares it
//! with `DEFRA_TEST_OTEL=1` and the test skips otherwise. Without that it would
//! hang for the timeout and fail on every default-feature build.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use integration_test::TestCluster;

/// Minimal OTLP/HTTP receiver: accepts one export and returns its body.
/// Deliberately not a full server — just enough to prove spans arrive.
fn otlp_sink() -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sink");
    let port = listener.local_addr().expect("sink addr").port();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            // Read headers, then exactly Content-Length bytes.
            let mut content_length = None;
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if content_length.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        content_length = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .map(|len| (pos + 4, len));
                    }
                }
                if let Some((start, len)) = content_length {
                    if buf.len() >= start + len {
                        let _ = tx.send(buf[start..start + len].to_vec());
                        break;
                    }
                }
            }
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            let _ = stream.flush();
        }
    });

    (port, rx)
}

#[tokio::test]
async fn harness_can_observe_otlp_span_export() {
    if std::env::var_os("DEFRA_TEST_OTEL").is_none() {
        eprintln!(
            "skipping: set DEFRA_TEST_OTEL=1 with an `--features otel` binary in \
             DEFRA_RUST_BINARY to run this"
        );
        return;
    }

    let (port, rx) = otlp_sink();
    // Scoped to this process, so a parallel test cannot see it.
    std::env::set_var(
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        format!("http://127.0.0.1:{port}"),
    );

    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_extra_rust_args(["--no-telemetry=false"])
        .build()
        .await
        .expect("cluster with telemetry enabled");

    let client = cluster.client(0);
    client
        .schema_add("type Traced { body: String }")
        .expect("schema");
    client
        .query(r#"mutation { add_Traced(input: {body: "x"}) { _docID } }"#)
        .expect("mutation");

    // The exporter batches on fastrace's collector interval (1 s default).
    // Batched on fastrace's collector interval (1 s default), so this normally
    // returns in about a second. The common cause of a timeout is a
    // DEFRA_RUST_BINARY built without `--features otel`, which silently makes
    // the whole exporter a no-op.
    let body = rx.recv_timeout(std::time::Duration::from_secs(15)).expect(
        "no OTLP export arrived within 15s — is DEFRA_RUST_BINARY built with \
             `--features otel`? (a default build makes telemetry a no-op); \
             otherwise the --no-telemetry override failed",
    );

    assert!(!body.is_empty(), "OTLP export body was empty");
    // Span names appear as plain UTF-8 in the protobuf payload.
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("query.parse") || text.contains("POST /api/v0/graphql"),
        "export carried no recognisable span names"
    );
}
