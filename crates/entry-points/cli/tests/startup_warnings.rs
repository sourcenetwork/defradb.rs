#![cfg(unix)]

//! A peerless `defradb start` serves a write and stops without warning.

mod common;

use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

const SHUTDOWN_COMPLETE: &str = "DefraDB node shutdown complete";
const HOST_FALLBACK: &str = "a worker setting fell back to a weaker one";
const KADEMLIA_THROTTLE: Duration = Duration::from_millis(500);

struct RunningNode {
    child: ChildGuard,
    url: String,
    client: net::http::Client,
    log: Option<common::NodeLog>,
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn id(&self) -> u32 {
        self.0.as_ref().expect("child is running").id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let status = self.0.as_mut().expect("child is running").try_wait()?;
        if status.is_some() {
            self.0.take();
        }
        Ok(status)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl RunningNode {
    async fn start(root: &Path) -> Self {
        let (log, output) = common::NodeLog::create(root.join("node.log"));
        let child = ChildGuard::new(
            Command::new(env!("CARGO_BIN_EXE_defradb"))
                .arg("--rootdir")
                .arg(root)
                .args(["--url", common::ANY_PORT])
                .args(common::LOG_FLAGS)
                .arg("--no-log-color")
                .args(["start", "--store", "memory", "--no-keyring"])
                .env("RUST_LOG", "info")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(output)
                .spawn()
                .expect("start node"),
        );
        let address = log.http_address(Duration::from_secs(30));
        let url = format!("http://{address}");
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let mut node = Self {
            child,
            url,
            client,
            log: Some(log),
        };
        nuclei::time::timeout(Duration::from_secs(30), async {
            loop {
                assert!(
                    node.child.try_wait().unwrap().is_none(),
                    "node exited during startup: {}",
                    node.log.as_ref().unwrap().text()
                );
                if let Ok(response) = node
                    .client
                    .get(format!("{}/health-check", node.url))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        break;
                    }
                }
                nuclei::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("node did not become healthy");
        node
    }

    async fn graphql(&self, query: &str) -> Value {
        let response: Value = self
            .client
            .post(format!("{}/api/v0/graphql", self.url))
            .json(&json!({ "query": query }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(response["errors"].is_null(), "query failed: {response}");
        response["data"].clone()
    }

    async fn stop(mut self) -> String {
        let pid = self.child.id().to_string();
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .expect("kill");
        assert!(sent.success());
        nuclei::time::timeout(Duration::from_secs(30), async {
            loop {
                if self.child.try_wait().unwrap().is_some() {
                    break;
                }
                nuclei::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("node did not shut down");
        self.log.take().unwrap().closed()
    }
}

fn unexpected_logs(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|line| line.contains(" WARN ") || line.contains(" ERROR "))
        .filter(|line| !line.contains(HOST_FALLBACK))
        .filter(|line| !line.contains("a thread setting fell back to a weaker one"))
        .collect()
}

#[nuclei::test]
async fn peerless_node_starts_and_stops_without_warnings() {
    let root = tempfile::tempdir().unwrap();
    let node = RunningNode::start(root.path()).await;
    node.client
        .post(format!("{}/api/v0/collections", node.url))
        .body("type User { name: String age: Int }")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    node.graphql(r#"mutation { add_User(input: {name: "Ada", age: 36}) { _docID } }"#)
        .await;

    nuclei::time::sleep(KADEMLIA_THROTTLE * 2).await;
    let log = node.stop().await;
    assert!(
        log.contains(SHUTDOWN_COMPLETE),
        "shutdown did not complete:\n{log}"
    );
    let unexpected = unexpected_logs(&log);
    assert!(
        unexpected.is_empty(),
        "a peerless node warned on start or stop:\n{}",
        unexpected.join("\n")
    );
}
