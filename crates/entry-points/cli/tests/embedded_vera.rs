//! `defradb start` with a Vera node of its own for document ACP.

#![cfg(unix)]

mod common;

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
    log: common::NodeLog,
}

impl RunningNode {
    /// Starts a node on `root` that logs to `node-{name}.log` there.
    async fn start(root: &Path, name: &str) -> Self {
        let (log, output) = common::NodeLog::create(root.join(format!("node-{name}.log")));
        let child = Command::new(env!("CARGO_BIN_EXE_defradb"))
            .arg("--rootdir")
            .arg(root)
            .args(["--url", common::ANY_PORT])
            .args(common::LOG_FLAGS)
            .args([
                "start",
                "--identity",
                IDENTITY,
                "--store",
                "memory",
                "--no-p2p",
                "--document-acp-type",
                "vera-embedded",
                "--vera-deployment-id",
                "9407",
            ])
            .env("DEFRA_KEYRING_SECRET", "embedded-vera-test-secret")
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(output)
            .spawn()
            .expect("start node");
        // Startup includes the Vera node's, so it gets the same deadline as
        // the health check below.
        let address = log.http_address(Duration::from_secs(90));
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
        self.log.text()
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
        let status = exited
            .unwrap_or_else(|_| panic!("node did not shut down: {}", self.logs()))
            .expect("wait");
        assert!(
            status.success(),
            "node exited with {status}: {}",
            self.logs()
        );
        // The node has exited, so this returns once the last line is copied.
        self.log.closed()
    }
}

/// Adds the policy through the node as its identity and returns the output.
async fn add_policy(node: &RunningNode, root: &Path) -> String {
    add_policy_with(node, root, &[]).await
}

/// [`add_policy`] with further arguments to `policy add`.
async fn add_policy_with(node: &RunningNode, root: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_defradb"))
        .arg("--rootdir")
        .arg(root)
        .args(["--url", &node.address])
        .args(["client", "--identity", IDENTITY])
        .args(["acp", "document", "policy", "add", POLICY])
        .args(args)
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

    let first = RunningNode::start(root.path(), "first").await;
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
    let second = RunningNode::start(root.path(), "second").await;
    let logs = second.stop().await;
    assert!(logs.contains("Embedded Vera node stopped"), "{logs}");
}

fn policy_id(output: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(output).expect("JSON output");
    parsed["PolicyID"].as_str().expect("PolicyID").to_owned()
}

/// `policy add --nonce` hands the nonce to vera.rs, which derives the ID from
/// it: added again with the same nonce the policy comes back under the same
/// ID and no second one is made, while an add without a nonce makes a new
/// policy each time.
#[nuclei::test]
async fn policy_add_with_a_nonce_names_one_policy_however_often_it_is_sent() {
    let root = tempfile::tempdir().unwrap();
    let node = RunningNode::start(root.path(), "node").await;
    let nonce = "5a".repeat(32);

    let first = policy_id(&add_policy_with(&node, root.path(), &["--nonce", &nonce]).await);
    let again = policy_id(&add_policy_with(&node, root.path(), &["--nonce", &nonce]).await);
    assert_eq!(again, first);
    let drawn = policy_id(&add_policy(&node, root.path()).await);
    let drawn_again = policy_id(&add_policy(&node, root.path()).await);
    assert_ne!(drawn, first);
    assert_ne!(drawn, drawn_again);
    node.stop().await;
}
