#![cfg(unix)]

mod common;

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::json;

struct RunningNode {
    child: Child,
    url: String,
    client: net::http::Client,
}

impl Drop for RunningNode {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl RunningNode {
    async fn start(root: &Path) -> Self {
        let log_path = root.join("node.log");
        let (log, output) = common::NodeLog::create(log_path.clone());
        let child = Command::new(env!("CARGO_BIN_EXE_defradb"))
            .arg("--rootdir")
            .arg(root)
            .args(["--url", common::ANY_PORT])
            .args(common::LOG_FLAGS)
            .args(["start", "--store", "memory", "--no-p2p"])
            .env("DEFRA_KEYRING_SECRET", "view-server-check-secret")
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(output)
            .spawn()
            .expect("start node");
        let address = log.http_address(Duration::from_secs(15));
        let mut node = Self {
            child,
            url: format!("http://{address}"),
            client: net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        };
        nuclei::time::timeout(Duration::from_secs(15), async {
            loop {
                assert!(
                    node.child.try_wait().unwrap().is_none(),
                    "node exited during startup: {}",
                    std::fs::read_to_string(&log_path).unwrap_or_default()
                );
                if matches!(node.client.get(format!("{}/health-check", node.url)).send().await, Ok(response) if response.status().is_success())
                {
                    break;
                }
                nuclei::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("node did not become healthy");
        node
    }

    async fn add_view(&self, body: serde_json::Value) -> (net::http::StatusCode, String) {
        let response = self
            .client
            .post(format!("{}/api/v0/view", self.url))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.text().await.unwrap())
    }
}

#[nuclei::test]
async fn defradb_start_uses_the_core_view_checks() {
    let root = tempfile::tempdir().unwrap();
    let node = RunningNode::start(root.path()).await;

    for (body, expected) in [
        (
            json!({
                "Query": "User { name } Other { name }",
                "SDL": "type UserView { name: String }"
            }),
            "invalid view query: multiple selections found",
        ),
        (
            json!({
                "Query": "User { name }",
                "SDL": "type UserView { name: String }",
                "TransformCID": ""
            }),
            "cid too short",
        ),
    ] {
        let (status, text) = node.add_view(body).await;
        assert_eq!(status, net::http::StatusCode::BAD_REQUEST, "{text}");
        assert!(text.contains(expected), "expected {expected:?} in {text}");
    }
}
