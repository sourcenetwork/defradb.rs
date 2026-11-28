use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use defra_node::Node;
use integration_test::actions::shared::KeyType;
use integration_test::nodes::{signer_public_key, NodeSpec, TestNode};
use integration_test::test_case::State;
use integration_test::ClientType;
use serde::Serialize;
use serde_json::{json, Value};
use testo::prelude::*;

const QUERY: &str = r#"{
    Users { _docID name }
    _commits {
        cid
        fieldName
        height
        heads { cid }
        links { cid }
        signature { type identity }
    }
}"#;

#[derive(Clone, Debug, Serialize)]
struct CheckpointRoundTrip;

impl Action for CheckpointRoundTrip {
    type TState = State;

    fn execute(&self, state: &mut State) {
        let root = tempfile::tempdir().unwrap();
        let source = Arc::new(tempfile::tempdir_in(root.path()).unwrap());
        let spec = NodeSpec {
            dir: Some(source.clone()),
            signing: Some(KeyType::Secp256k1),
            ..Default::default()
        };
        let (key, _) = signer_public_key(&spec).unwrap();
        std::fs::write(
            root.path().join("config.yaml"),
            format!("datastore:\n  path: {}\n", source.path().display()),
        )
        .unwrap();
        let destination = root.path().join("destination");
        let missing = checkpoint(root.path(), &destination);
        assert!(!missing.status.success());
        assert!(String::from_utf8_lossy(&missing.stderr).contains(&format!(
            "storage checkpoint source has no manifest: {}",
            source.path().display()
        )));
        assert!(!destination.exists());

        let original = TestNode::start(ClientType::InProcess, spec);
        let node = original.node.as_ref().unwrap();
        let (expected, index) = state.block_on(async {
            node.add_collection("type Users { name: String @index }")
                .await
                .unwrap();
            let created = query(node, r#"mutation { add_Users(input: {name: "first"}) { _docID } }"#).await;
            let id = created["add_Users"][0]["_docID"].as_str().unwrap();
            query(node, &format!(
                r#"mutation {{ update_Users(docID: "{id}", input: {{name: "second"}}) {{ _docID }} }}"#
            )).await;
            let expected = contents(node).await;
            assert_eq!(expected["Users"], json!([{ "_docID": id, "name": "second" }]));
            let commits = expected["_commits"].as_array().unwrap();
            assert!(commits.iter().any(|commit| commit["height"] == 1));
            assert!(commits.iter().any(|commit| commit["height"] == 2));
            let indexes = node.list_indexes("Users").await.unwrap();
            assert_eq!(indexes.len(), 1);
            (expected, indexes[0].description.clone())
        });
        state.block_on(original.stop());
        drop(original);

        let occupied = tempfile::tempdir_in(root.path()).unwrap();
        let refused = checkpoint(root.path(), occupied.path());
        assert!(!refused.status.success());
        assert!(String::from_utf8_lossy(&refused.stderr).contains(&format!(
            "storage checkpoint destination already exists: {}",
            occupied.path().display()
        )));
        assert_eq!(std::fs::read_dir(occupied.path()).unwrap().count(), 0);

        let output = checkpoint(root.path(), &destination);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = destination.metadata().unwrap().permissions();
            assert_eq!(permissions.mode() & 0o077, 0);
        }

        drop(source);
        state.block_on(async {
            let restored = Node::builder()
                .data_path(&destination)
                .build()
                .await
                .unwrap();
            assert_eq!(contents(&restored).await, expected);
            let indexes = restored.list_indexes("Users").await.unwrap();
            assert_eq!(indexes.len(), 1);
            assert_eq!(indexes[0].description.name, index.name);
            assert_eq!(indexes[0].description.fields, index.fields);
            assert_eq!(indexes[0].description.unique, index.unique);
            assert_eq!(indexes[0].description.auto_generated, index.auto_generated);
            assert!(indexes[0].description.kind.is_some());
            let signed: Vec<_> = expected["_commits"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|commit| !commit["signature"].is_null())
                .collect();
            assert!(!signed.is_empty());
            for commit in signed {
                restored
                    .verify_block_signature(
                        commit["cid"].as_str().unwrap(),
                        &key,
                        crypto::KeyType::Secp256k1,
                    )
                    .await
                    .unwrap();
            }
            restored.shutdown().await.unwrap();
        });
    }
}

async fn query(node: &Node, request: &str) -> Value {
    let response = node.execute(request).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.expect("query result")
}

async fn contents(node: &Node) -> Value {
    let mut data = query(node, QUERY).await;
    data["_commits"]
        .as_array_mut()
        .unwrap()
        .sort_by(|a, b| a["cid"].as_str().cmp(&b["cid"].as_str()));
    data
}

fn checkpoint(root: &Path, destination: &Path) -> Output {
    Command::new(integration_test::rust_binary())
        .arg("--rootdir")
        .arg(root)
        .args(["storage", "checkpoint"])
        .arg(destination)
        .output()
        .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.backup.checkpoint")]
fn checkpoint_preserves_configuration_and_signed_history() {
    testo::action::execute::<State, CheckpointRoundTrip, CheckpointRoundTrip>(
        State::default(),
        &[ActionInstance::Sync(CheckpointRoundTrip)],
    );
}
