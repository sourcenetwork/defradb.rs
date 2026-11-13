//! `defradb start` with a Vera node of its own for document ACP.

#![cfg(unix)]

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use nuclei::process::{Child, Command};

/// A secp256k1 key: the node identity, the policy owner and the deployment's
/// only operator.
const IDENTITY: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";
const POLICY: &str = "\
name: embedded
resources:
  - name: users
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
";

struct RunningNode {
    child: Child,
    address: String,
    log: std::path::PathBuf,
}

impl RunningNode {
    async fn start(root: &Path) -> Self {
        let port = portpicker::pick_unused_port().expect("unused HTTP port");
        let address = format!("127.0.0.1:{port}");
        let log = root.join(format!("node-{port}.log"));
        let child = Command::new(env!("CARGO_BIN_EXE_defradb"))
            .arg("--rootdir")
            .arg(root)
            .args(["--url", &address])
            .args([
                "start",
                "--identity",
                IDENTITY,
                "--store",
                "memory",
                "--no-p2p",
                "--document-acp-type",
                "verars-embedded",
                "--vera-deployment-id",
                "9407",
            ])
            .env("DEFRA_KEYRING_SECRET", "embedded-vera-test-secret")
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .expect("start node");
        let mut node = Self {
            child,
            address,
            log,
        };
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let healthy = nuclei::time::timeout(Duration::from_secs(90), async {
            loop {
                assert!(
                    node.child.try_wait().unwrap().is_none(),
                    "node exited during startup: {}",
                    node.logs()
                );
                let url = format!("http://{}/health-check", node.address);
                if matches!(client.get(&url).send().await, Ok(response) if response.status().is_success())
                {
                    break;
                }
                nuclei::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(
            healthy.is_ok(),
            "node did not become healthy: {}",
            node.logs()
        );
        node
    }

    fn logs(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Asks the node to shut down as an operator does and waits for it.
    async fn stop(mut self) -> String {
        let pid = self.child.id().to_string();
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .expect("kill");
        assert!(sent.success());
        let exited = nuclei::time::timeout(Duration::from_secs(60), self.child.wait()).await;
        assert!(exited.is_ok(), "node did not shut down: {}", self.logs());
        self.logs()
    }
}

/// Adds the policy through the node as its identity and returns the output.
async fn add_policy(node: &RunningNode, root: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_defradb"))
        .arg("--rootdir")
        .arg(root)
        .args(["--url", &node.address])
        .args(["client", "--identity", IDENTITY])
        .args(["acp", "document", "policy", "add", POLICY])
        .env("DEFRA_KEYRING_SECRET", "embedded-vera-test-secret")
        .stdin(Stdio::null())
        .output()
        .await
        .expect("client");
    assert!(
        output.status.success(),
        "policy add failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        node.logs()
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[nuclei::test]
async fn start_runs_its_own_vera_node_and_reopens_it_after_a_shutdown() {
    let root = tempfile::tempdir().unwrap();

    let first = RunningNode::start(root.path()).await;
    assert!(
        root.path().join("vera").is_dir(),
        "the Vera node's directory"
    );
    let added = add_policy(&first, root.path()).await;
    assert!(added.contains("PolicyID"), "{added}");
    let logs = first.stop().await;
    for line in [
        "Embedded Vera node started",
        "Document ACP configured (embedded vera.rs)",
        "Embedded Vera node stopped",
    ] {
        assert!(logs.contains(line), "missing {line:?} in: {logs}");
    }

    // The first node let go of the Vera directory, and the second reopens
    // the deployment it finds there.
    let second = RunningNode::start(root.path()).await;
    let logs = second.stop().await;
    assert!(logs.contains("Embedded Vera node stopped"), "{logs}");
}
