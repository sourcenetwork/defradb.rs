//! `defradb hub` runs a pinc hub that CLI nodes naming it in `--pinc-hubs`
//! replicate a subscribed collection through.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// A child process killed when the test lets go of it.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn defradb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_defradb"))
}

/// Starts a hub on a loopback port the OS picks and returns it with the
/// address it prints.
fn hub(root: &TempDir) -> (Running, String) {
    let mut child = defradb()
        .args([
            "hub",
            "--bind",
            "127.0.0.1:0",
            "--no-telemetry",
            "--rootdir",
        ])
        .arg(root.path().join("hub"))
        .args(common::LOG_FLAGS)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the hub");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("hub stdout"))
        .read_line(&mut line)
        .expect("read the hub address");
    (Running(child), line.trim().to_owned())
}

/// Starts a node naming `hub` and returns it with its HTTP address.
fn node(root: &TempDir, name: &str, hub: &str) -> (Running, String, common::NodeLog) {
    let (log, output) = common::NodeLog::create(root.path().join(format!("{name}.log")));
    let mut command = defradb();
    command
        .args([
            "start",
            "--no-keyring",
            "--no-telemetry",
            "--store",
            "memory",
            "--url",
            common::ANY_PORT,
            "--pinc-bind-addr",
            "127.0.0.1",
            "--pinc-hubs",
            hub,
            "--rootdir",
        ])
        .arg(root.path().join(name))
        .args(common::LOG_FLAGS)
        .env("RUST_LOG", "info")
        .stdout(output.try_clone().expect("log pipe"))
        .stderr(output);
    let child = command.spawn().expect("start the node");
    drop(command);
    let address = log.http_address(Duration::from_secs(30));
    (Running(child), address, log)
}

/// Runs `defradb client <args>` against the node at `url` and returns its
/// standard output, failing the test when the command fails.
fn client(url: &str, args: &[&str]) -> String {
    let output = defradb()
        .args(["client", "--no-keyring", "--url", url])
        .args(args)
        .output()
        .expect("run the client");
    assert!(
        output.status.success(),
        "defradb client {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 output")
}

#[test]
#[mantra_macros::req_test("defra.cli.hub")]
#[mantra_macros::req_test("defra.cli.pinc")]
fn nodes_replicate_a_subscribed_collection_through_a_cli_hub() {
    let root = TempDir::new().expect("temporary rootdir");
    let (_hub, hub) = hub(&root);
    assert!(hub.contains("@127.0.0.1:"), "a hub address: {hub}");
    let (_a, a, _a_log) = node(&root, "a", &hub);
    let (_b, b, _b_log) = node(&root, "b", &hub);

    for url in [&a, &b] {
        client(url, &["collection", "add", "type User { name: String }"]);
    }
    let info: Vec<String> =
        serde_json::from_str(&client(&a, &["p2p", "info"])).expect("p2p info is a list");
    client(&b, &["p2p", "connect", &info[0]]);
    for url in [&a, &b] {
        client(url, &["p2p", "collection", "add", "User"]);
    }
    client(
        &a,
        &[
            "query",
            r#"mutation { add_User(input: {name: "through the hub"}) { _docID } }"#,
        ],
    );

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if client(&b, &["query", "query { User { name } }"]).contains("through the hub") {
            break;
        }
        assert!(Instant::now() < deadline, "the document never reached b");
        std::thread::sleep(Duration::from_millis(200));
    }
}
