//! `defra start --governed … --rule-module …`: a relay whose collections are
//! judged by a wasm rule module, through the running binary.
//!
//! The guests are hand-written WAT, as in db's rules tests: they export
//! `memory`, `alloc` and `judge`, and answer one constant CBOR verdict.

use std::path::Path;
use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

/// `{"verdict":"reject","reason":"forged"}`
const REJECT: &str = r#"\a2\67verdict\66reject\66reason\66forged"#;
/// `{"verdict":"accept"}`
const ACCEPT: &str = r#"\a1\67verdict\66accept"#;

/// A guest answering `response`, CBOR in WAT string syntax, at every step.
fn guest(response: &str) -> Vec<u8> {
    let mut len = 0u32;
    let mut chars = response.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
            chars.next();
        }
        len += 1;
    }
    let prefix: String = len
        .to_le_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    wat::parse_str(format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 1024) "{prefix}{response}")
  (func (export "alloc") (param i32) (result i32) (i32.const 4096))
  (func (export "judge") (param i32) (param i32) (result i32) (i32.const 1024)))"#
    ))
    .expect("guest WAT")
}

fn module_cid(bytes: &[u8]) -> String {
    db::merge::governance::rule::module_cid(bytes).to_string()
}

fn schema(rule: &str) -> String {
    format!(
        r#"type Notes @governed(root: "arena", rule: "{rule}") {{ text: String }}
type Grants {{ writer: String }}"#
    )
}

struct RunningNode {
    child: Child,
    root: TempDir,
    url: String,
}

impl RunningNode {
    /// A node on a memory store with `args` added; P2P only when `p2p` names
    /// a listen port.
    async fn start(args: &[String], p2p: Option<u16>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let port = portpicker::pick_unused_port().expect("available port");
        let address = format!("127.0.0.1:{port}");
        let log = std::fs::File::create(root.path().join("node.log")).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_defra"));
        command
            .args(["start", "--no-keyring", "--store", "memory", "--url"])
            .arg(&address)
            .arg("--rootdir")
            .arg(root.path())
            .args(args)
            .env("RUST_LOG", "info")
            .env("TOKIO_WORKER_THREADS", "2")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .kill_on_drop(true);
        match p2p {
            Some(p2p_port) => {
                command
                    .arg("--p2paddr")
                    .arg(format!("/ip4/127.0.0.1/tcp/{p2p_port}"));
            }
            None => {
                command.arg("--no-p2p");
            }
        }
        let mut node = Self {
            child: command.spawn().unwrap(),
            root,
            url: format!("http://{address}/api/v0"),
        };
        node.healthy().await;
        node
    }

    fn logs(&self) -> String {
        std::fs::read_to_string(self.root.path().join("node.log")).unwrap_or_default()
    }

    async fn healthy(&mut self) {
        let client = client();
        let url = self.url.replace("/api/v0", "/health-check");
        let ready = timeout(Duration::from_secs(30), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return Err(status);
                }
                if client.get(&url).send().await.is_ok() {
                    return Ok(());
                }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        match ready {
            Ok(Ok(())) => {}
            Ok(Err(status)) => panic!("node exited with {status}:\n{}", self.logs()),
            Err(_) => panic!("node never became healthy:\n{}", self.logs()),
        }
    }

    async fn add_schema(&self, sdl: &str) {
        let response = client()
            .post(format!("{}/schema", self.url))
            .body(sdl.to_string())
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "schema refused: {}",
            response.text().await.unwrap()
        );
    }

    /// The GraphQL response, with an HTTP error folded into `errors`.
    async fn graphql(&self, query: &str) -> Value {
        let response = client()
            .post(format!("{}/graphql", self.url))
            .json(&json!({ "query": query }))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        match serde_json::from_str::<Value>(&text) {
            Ok(body) if status.is_success() => body,
            _ => json!({ "errors": [{ "message": text }] }),
        }
    }

    async fn texts(&self, collection: &str, field: &str) -> Vec<String> {
        let body = self
            .graphql(&format!("query {{ {collection} {{ {field} }} }}"))
            .await;
        body["data"][collection]
            .as_array()
            .map(|docs| {
                docs.iter()
                    .filter_map(|doc| doc[field].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn post(&self, path: &str, body: Value) {
        let response = client()
            .post(format!("{}{path}", self.url))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "POST {path} refused: {}",
            response.text().await.unwrap()
        );
    }

    async fn p2p_address(&self) -> String {
        timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(response) = client().get(format!("{}/p2p/info", self.url)).send().await {
                    if let Ok(addresses) = response.json::<Vec<String>>().await {
                        if let Some(address) = addresses
                            .into_iter()
                            .find(|address| address.starts_with("/ip4/127.0.0.1/"))
                        {
                            return address;
                        }
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the node never reported a P2P address")
    }
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

fn write_module(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn governed_args(module: &str) -> Vec<String> {
    [
        "--governed",
        "Notes",
        "--rule-module",
        module,
        "--rule-engine",
        "wasmi",
    ]
    .map(str::to_string)
    .to_vec()
}

/// A relay with no P2P still judges its HTTP mutations: a write its rule
/// rejects is refused with the rule's reason and not stored, and a write
/// into an unclaimed collection is untouched.
#[tokio::test]
async fn a_governed_relay_refuses_a_local_write_its_rule_rejects() {
    let modules = tempfile::tempdir().unwrap();
    let reject = guest(REJECT);
    let path = write_module(modules.path(), "reject.wasm", &reject);
    let node = RunningNode::start(&governed_args(&path), None).await;
    let cid = module_cid(&reject);
    assert!(
        node.logs().contains(&cid),
        "the held module's CID was not logged:\n{}",
        node.logs()
    );

    node.add_schema(&schema(&cid)).await;
    let refused = node
        .graphql(r#"mutation { add_Notes(input: {text: "forged"}) { _docID } }"#)
        .await;
    let errors = refused["errors"].to_string();
    assert!(
        errors.contains("forged"),
        "not refused by the rule: {refused}"
    );
    assert!(node.texts("Notes", "text").await.is_empty());

    let granted = node
        .graphql(r#"mutation { add_Grants(input: {writer: "alice"}) { _docID } }"#)
        .await;
    assert!(
        granted["errors"].as_array().is_none_or(Vec::is_empty),
        "{granted}"
    );
    assert_eq!(node.texts("Grants", "writer").await, ["alice"]);
}

/// A module that does not compile fails the start rather than leaving every
/// governed write unmerged.
#[tokio::test]
async fn a_rule_module_that_does_not_compile_stops_the_start() {
    let modules = tempfile::tempdir().unwrap();
    let path = write_module(modules.path(), "broken.wasm", b"not wasm");
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_defra"))
        .args([
            "start",
            "--no-keyring",
            "--no-p2p",
            "--store",
            "memory",
            "--url",
            "127.0.0.1:0",
        ])
        .arg("--rootdir")
        .arg(root.path())
        .args(governed_args(&path))
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("does not compile") || stdout.contains("does not compile"),
        "{stderr}{stdout}"
    );
}

/// A peer pushes a governed composite the relay's rule rejects: the relay
/// does not merge it, while an unclaimed document the same peer pushes
/// alongside it arrives. The peer runs no governance, so it holds its note.
#[tokio::test]
async fn a_governed_relay_does_not_merge_a_pushed_composite_its_rule_rejects() {
    let modules = tempfile::tempdir().unwrap();
    let reject = guest(REJECT);
    let path = write_module(modules.path(), "reject.wasm", &reject);
    let cid = module_cid(&reject);
    let relay_port = portpicker::pick_unused_port().expect("available port");
    let peer_port = portpicker::pick_unused_port().expect("available port");
    let relay = RunningNode::start(&governed_args(&path), Some(relay_port)).await;
    let peer = RunningNode::start(&[], Some(peer_port)).await;
    for node in [&relay, &peer] {
        node.add_schema(&schema(&cid)).await;
        node.post("/p2p/collections", json!(["Notes", "Grants"]))
            .await;
    }
    let relay_address = relay.p2p_address().await;
    peer.post(
        "/p2p/replicators",
        json!({ "Collections": ["Notes", "Grants"], "Addresses": [relay_address] }),
    )
    .await;

    let note = peer
        .graphql(r#"mutation { add_Notes(input: {text: "forged"}) { _docID } }"#)
        .await;
    assert!(
        note["errors"].as_array().is_none_or(Vec::is_empty),
        "{note}"
    );
    let grant = peer
        .graphql(r#"mutation { add_Grants(input: {writer: "alice"}) { _docID } }"#)
        .await;
    assert!(
        grant["errors"].as_array().is_none_or(Vec::is_empty),
        "{grant}"
    );

    timeout(Duration::from_secs(30), async {
        while relay.texts("Grants", "writer").await.is_empty() {
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the unclaimed grant never arrived:\n{}", relay.logs()));
    // The note was pushed first; give its merge the time the grant took.
    sleep(Duration::from_secs(2)).await;
    assert!(
        relay.texts("Notes", "text").await.is_empty(),
        "the relay merged a note its rule rejects"
    );
    assert_eq!(peer.texts("Notes", "text").await, ["forged"]);
}

/// The control: the same push under a rule that accepts merges.
#[tokio::test]
async fn a_governed_relay_merges_a_pushed_composite_its_rule_accepts() {
    let modules = tempfile::tempdir().unwrap();
    let accept = guest(ACCEPT);
    let path = write_module(modules.path(), "accept.wasm", &accept);
    let cid = module_cid(&accept);
    let relay_port = portpicker::pick_unused_port().expect("available port");
    let peer_port = portpicker::pick_unused_port().expect("available port");
    let relay = RunningNode::start(&governed_args(&path), Some(relay_port)).await;
    let peer = RunningNode::start(&[], Some(peer_port)).await;
    for node in [&relay, &peer] {
        node.add_schema(&schema(&cid)).await;
        node.post("/p2p/collections", json!(["Notes"])).await;
    }
    let relay_address = relay.p2p_address().await;
    peer.post(
        "/p2p/replicators",
        json!({ "Collections": ["Notes"], "Addresses": [relay_address] }),
    )
    .await;
    let note = peer
        .graphql(r#"mutation { add_Notes(input: {text: "honest"}) { _docID } }"#)
        .await;
    assert!(
        note["errors"].as_array().is_none_or(Vec::is_empty),
        "{note}"
    );

    timeout(Duration::from_secs(30), async {
        while relay.texts("Notes", "text").await.is_empty() {
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the accepted note never merged:\n{}", relay.logs()));
}
