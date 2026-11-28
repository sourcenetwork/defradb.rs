//! Document relationships through the HTTP server `defradb start` runs.

mod common;

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use cli::commands::client::http_client::{GraphQLResponse, HttpClient};
use futures::StreamExt;
use serde_json::Value;

const POLICY: &str = r#"name: Users
resources:
  - name: users
    relations:
      - name: reader
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;

struct RunningNode {
    child: Child,
    url: String,
}

impl Drop for RunningNode {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl RunningNode {
    async fn start(root: &Path, owner_hex: &str) -> Self {
        let log_path = root.join("node.log");
        let (log, output) = common::NodeLog::create(log_path.clone());
        let child = Command::new(env!("CARGO_BIN_EXE_defradb"))
            .arg("--rootdir")
            .arg(root)
            .args(["--url", common::ANY_PORT])
            .args(common::LOG_FLAGS)
            .args([
                "start",
                "--store",
                "memory",
                "--no-p2p",
                "--no-keyring",
                "--identity",
                owner_hex,
                "--document-acp-type",
                "local",
            ])
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(output)
            .spawn()
            .expect("start node");
        let mut node = Self {
            child,
            url: String::new(),
        };
        let address = log.http_address(Duration::from_secs(15));
        node.url = format!("http://{address}");
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("health client");
        nuclei::time::timeout(Duration::from_secs(15), async {
            loop {
                assert!(
                    node.child.try_wait().unwrap().is_none(),
                    "node exited during startup: {}",
                    std::fs::read_to_string(&log_path).unwrap_or_default()
                );
                if client
                    .get(format!("{}/health-check", node.url))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
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

    fn client(&self, token: &str) -> HttpClient {
        HttpClient::new(&self.url, &edge::testing::edge())
            .expect("HTTP client")
            .with_auth_token(Some(token.to_owned()))
    }
}

fn token(key: &str, node: &RunningNode, name: &str) -> String {
    cli::commands::client::generate_auth_token(key, &node.url)
        .unwrap_or_else(|error| panic!("{name} token: {error}"))
}

fn data(response: GraphQLResponse) -> Value {
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.expect("GraphQL data")
}

async fn visible(client: &HttpClient) -> usize {
    let data = data(
        client
            .graphql("query { Users { name } }", None, None)
            .await
            .expect("query documents"),
    );
    data["Users"].as_array().expect("Users array").len()
}

async fn visible_commits(client: &HttpClient, doc: &str) -> usize {
    let data = data(
        client
            .graphql(
                &format!(r#"query {{ _commits(docID: "{doc}") {{ cid }} }}"#),
                None,
                None,
            )
            .await
            .expect("query commits"),
    );
    data["_commits"].as_array().expect("commits array").len()
}

#[nuclei::test]
async fn checks_and_transactional_grants_match_across_the_started_server() {
    use crypto::Key;
    use identity::Identity as _;

    let root = tempfile::tempdir().unwrap();
    let owner_key = hex::encode(crypto::generate_ed25519().unwrap().raw());
    let reader_key = crypto::generate_ed25519().unwrap();
    let reader_hex = hex::encode(reader_key.raw());
    let reader_did = identity::RawIdentity::from_private_key(reader_key)
        .unwrap()
        .did()
        .unwrap()
        .to_string();
    let node = RunningNode::start(root.path(), &owner_key).await;
    let owner_token = token(&owner_key, &node, "owner");
    let owner = node.client(&owner_token);
    let reader_token = token(&reader_hex, &node, "reader");
    let reader = node.client(&reader_token);

    let policy = owner
        .acp_add_policy(POLICY, None)
        .await
        .expect("add policy")
        .policy_id;
    owner
        .schema_add(
            &format!(
                "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
            ),
            None,
        )
        .await
        .expect("add policied collection");
    owner
        .schema_add("type Open { name: String }", None)
        .await
        .expect("add open collection");

    let add = owner
        .graphql(
            r#"mutation { add_Users(input: {name: "Ada"}) { _docID } }"#,
            None,
            None,
        )
        .await
        .expect("add document");
    let doc = data(add)["add_Users"][0]["_docID"]
        .as_str()
        .expect("document ID")
        .to_owned();

    for change in [
        owner
            .acp_doc_relationship_add("Open", "doc", "owner", "did:key:reader", None)
            .await,
        owner
            .acp_doc_relationship_delete("Open", "doc", "", "did:key:reader", None)
            .await,
    ] {
        let error = change
            .expect_err("the collection has no policy")
            .to_string();
        assert!(
            error.contains("operation requires ACP, but collection has no policy"),
            "the server used another check first: {error}"
        );
    }

    let http = net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("relationship test client");
    let events = http
        .get(format!("{}/api/v0/events?event=update", node.url))
        .bearer_auth(&reader_token)
        .send()
        .await
        .expect("subscribe to updates");
    assert!(events.status().is_success(), "events: {}", events.status());
    let mut events = events.bytes_stream();

    for commit in [false, true] {
        let txn = owner
            .tx_begin(false)
            .await
            .expect("begin grant")
            .id
            .to_string();
        owner
            .acp_doc_relationship_add("Users", &doc, "reader", &reader_did, Some(&txn))
            .await
            .expect("add grant in transaction");
        assert_eq!(
            visible(&reader).await,
            0,
            "an uncommitted grant was visible"
        );
        assert_eq!(visible_commits(&reader, &doc).await, 0);
        if commit {
            owner.tx_commit(&txn).await.expect("commit grant");
        } else {
            owner.tx_discard(&txn).await.expect("discard grant");
            assert_eq!(visible(&reader).await, 0, "a discarded grant was visible");
        }
    }
    assert_eq!(
        visible(&reader).await,
        1,
        "the committed grant was not visible"
    );
    assert!(visible_commits(&reader, &doc).await > 0);
    let mut event = Vec::new();
    loop {
        let chunk = events
            .next()
            .await
            .expect("grant update")
            .expect("event body");
        event.extend_from_slice(&chunk);
        if String::from_utf8_lossy(&event).lines().any(|line| {
            line.strip_prefix("data: ")
                .and_then(|json| serde_json::from_str::<Value>(json).ok())
                .is_some_and(|message| {
                    message["name"] == "update" && message["data"]["doc_id"] == doc
                })
        }) {
            break;
        }
    }
    drop(events);

    let txn = owner
        .tx_begin(false)
        .await
        .expect("begin delete")
        .id
        .to_string();
    owner
        .acp_doc_relationship_delete("Users", &doc, "reader", &reader_did, Some(&txn))
        .await
        .expect("delete grant in transaction");
    assert_eq!(
        visible(&reader).await,
        1,
        "an uncommitted delete changed access"
    );
    owner.tx_discard(&txn).await.expect("discard delete");
    assert_eq!(
        visible(&reader).await,
        1,
        "a discarded delete changed access"
    );

    let url = format!("{}/api/v0/acp/document", node.url);
    let listed = http
        .get(format!("{url}/relationships?collection=Users&docID={doc}"))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("list relationships");
    assert!(listed.status().is_success(), "list: {}", listed.status());
    let listed: Vec<acp::DocumentRelationship> = listed.json().await.expect("relationships");
    assert!(listed
        .iter()
        .any(|entry| entry.relation == "reader" && entry.subject == reader_did));

    let observed = http
        .get(format!(
            "{url}/relationship?collection=Users&docID={doc}&relation=reader&targetActor={reader_did}"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("observe relationship");
    assert!(
        observed.status().is_success(),
        "observe: {}",
        observed.status()
    );
    assert!(observed.json::<bool>().await.expect("exact relationship"));

    let txn = owner
        .tx_begin(false)
        .await
        .expect("begin refused grant")
        .id
        .to_string();
    let error = owner
        .acp_doc_relationship_add("Users", &doc, "NotOnPolicy", &reader_did, Some(&txn))
        .await
        .expect_err("the relation is not in the policy")
        .to_string();
    assert!(
        error.contains(
            "failed to add document actor relationship with acp: relation 'NotOnPolicy' not found"
        ),
        "the transaction route used another error: {error}"
    );
    owner.tx_discard(&txn).await.expect("discard refused grant");

    let anonymous = HttpClient::new(&node.url, &edge::testing::edge()).expect("anonymous client");
    let txn = anonymous
        .tx_begin(false)
        .await
        .expect("begin anonymous transaction")
        .id
        .to_string();
    for txn in [None, Some(txn.as_str())] {
        let error = anonymous
            .acp_doc_relationship_add("Users", &doc, "reader", &reader_did, txn)
            .await
            .expect_err("a grant requires a requester")
            .to_string();
        assert!(
            error.contains("missing a required argument needed to add doc actor relationship."),
            "{error}"
        );
    }
    anonymous
        .tx_discard(&txn)
        .await
        .expect("discard anonymous transaction");
}
