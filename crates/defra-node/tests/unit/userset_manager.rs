use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use identity::Identity as _;
use serde_json::{json, Value};

use crate::{EmbeddedSigner, Node};

const POLICY: &str = r#"
name: Userset manager fixture
resources:
  - name: nodes
    relations:
      - name: administrator
        types: [actor]
    permissions:
      - name: read
        expr: administrator
      - name: update
      - name: delete
  - name: configuration
    relations:
      - name: selected_by
        types: [actor]
      - name: reader
        types: [actor]
      - name: selection_manager
        types: [actor, nodes->owner, nodes->administrator, configuration->selection_manager]
        manages: [selected_by]
    permissions:
      - name: read
        expr: selected_by + reader
      - name: update
      - name: delete
"#;

fn actor() -> (identity::RawIdentity, EmbeddedSigner) {
    let key = crypto::generate_ed25519().unwrap();
    let public = key.public_key();
    let config = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: key.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    (
        identity::RawIdentity::from_private_key(key).unwrap(),
        EmbeddedSigner::from_config(config).unwrap(),
    )
}

fn context(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

async fn create(
    node: &Node,
    signer: &EmbeddedSigner,
    did: &str,
    collection: &str,
    name: &str,
) -> String {
    let query = format!("mutation {{ create_{collection}(input:{{name:\"{name}\"}}) {{_docID}} }}");
    let result = node
        .execute_request(
            signer
                .attach(query::QueryRequest::new(query).with_identity(Some(did.parse().unwrap()))),
        )
        .await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    result.data.unwrap()[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn attempt(node: &Node, did: &str, doc: &str, relation: &str, target: &str) -> bool {
    let context = context(did);
    let handle = node.runner().begin_txn_as(&context, false).await.unwrap();
    let tx = context
        .clone()
        .with_txn(Some(handle.as_str().parse().unwrap()));
    let result = node
        .add_dac_actor_relationship_as(&tx, "ManagedConfiguration", doc, relation, target)
        .await
        .is_ok();
    node.runner()
        .discard_txn_as(&context, &handle)
        .await
        .unwrap();
    result
}

fn exercise_userset_manager() -> Vec<bool> {
    edge::testing::edge().block_on(async {
        let (nac_owner, _) = actor();
        let (node_owner, node_signer) = actor();
        let (other_owner, other_signer) = actor();
        let (config_owner, config_signer) = actor();
        let (administrator, _) = actor();
        let (sibling, _) = actor();
        let (outsider, _) = actor();
        let did = |identity: &identity::RawIdentity| identity.did().unwrap().to_string();
        let root = did(&nac_owner);
        let owner = did(&node_owner);
        let other = did(&other_owner);
        let config = did(&config_owner);
        let admin = did(&administrator);
        let sibling = did(&sibling);
        let outsider = did(&outsider);
        let node = Node::builder().enable_signing().with_node_acp_owner(root.clone()).build().await.unwrap();
        for subject in [&owner, &other, &config, &admin, &sibling, &outsider] {
            for permission in [acp::NodePermission::DocumentRead, acp::NodePermission::DocumentUpdate,
                acp::NodePermission::CollectionGet, acp::NodePermission::DacRelationAdd,
                acp::NodePermission::DacRelationDelete] {
                node.add_nac_actor_relationship_as(&context(&root), permission.as_str(), subject).await.unwrap();
            }
        }
        let policy = node.add_dac_policy_as(&context(&root), POLICY).await.unwrap();
        node.add_collection(&format!("type ManagedNode @policy(id: \"{policy}\", resource: \"nodes\") {{name:String}}\ntype ManagedConfiguration @policy(id: \"{policy}\", resource: \"configuration\") {{name:String}}")) .await.unwrap();
        let node_doc = create(&node, &node_signer, &owner, "ManagedNode", "primary").await;
        let other_doc = create(&node, &other_signer, &other, "ManagedNode", "other").await;
        let doc = create(&node, &config_signer, &config, "ManagedConfiguration", "target").await;
        node.add_dac_actor_relationship_as(&context(&owner), "ManagedNode", &node_doc, "administrator", &admin).await.unwrap();
        node.add_dac_actor_relationship_as(&context(&other), "ManagedNode", &other_doc, "administrator", &sibling).await.unwrap();
        for target in [format!("nodes:\"{node_doc}\"#owner"), format!("nodes:\"{node_doc}\"#administrator"), format!("configuration:\"{doc}\"#selection_manager")] {
            node.add_dac_actor_relationship_as(&context(&config), "ManagedConfiguration", &doc, "selection_manager", &target).await.unwrap();
        }
        let mut decisions = Vec::new();
        for (actor, relation) in [(&config, "selected_by"), (&owner, "selected_by"), (&admin, "selected_by"),
            (&outsider, "selected_by"), (&sibling, "selected_by"), (&admin, "reader")] {
            decisions.push(attempt(&node, actor, &doc, relation, &outsider).await);
        }
        node.delete_dac_actor_relationship_as(&context(&owner), "ManagedNode", &node_doc, "administrator", &admin).await.unwrap();
        decisions.push(attempt(&node, &admin, &doc, "selected_by", &outsider).await);
        node.add_dac_actor_relationship_as(&context(&owner), "ManagedNode", &node_doc, "administrator", &admin).await.unwrap();
        decisions.push(attempt(&node, &admin, &doc, "selected_by", &outsider).await);
        node.add_dac_actor_relationship_as(&context(&config), "ManagedConfiguration", &doc, "selection_manager", &outsider).await.unwrap();
        decisions.push(attempt(&node, &outsider, &doc, "selected_by", &sibling).await);
        node.delete_dac_actor_relationship_as(&context(&config), "ManagedConfiguration", &doc, "selection_manager", &outsider).await.unwrap();
        node.add_dac_actor_relationship_as(&context(&config), "ManagedConfiguration", &doc, "selection_manager", "*").await.unwrap();
        decisions.push(attempt(&node, &outsider, &doc, "selected_by", &sibling).await);
        node.delete_dac_actor_relationship_as(&context(&config), "ManagedConfiguration", &doc, "selection_manager", "*").await.unwrap();
        assert_eq!(decisions, vec![true, true, true, false, false, false, false, true, true, true]);
        let admin_context = context(&admin);
        let handle = node.runner().begin_txn_as(&admin_context, false).await.unwrap();
        let tx = admin_context.clone().with_txn(Some(handle.as_str().parse().unwrap()));
        node.add_dac_actor_relationship_as(&tx, "ManagedConfiguration", &doc, "selected_by", &outsider).await.unwrap();
        node.delete_dac_actor_relationship_as(&context(&owner), "ManagedNode", &node_doc, "administrator", &admin).await.unwrap();
        assert!(node.runner().commit_txn_as(&admin_context, &handle).await.is_err(), "revocation must conflict with a grant whose userset traversal read the old membership");
        let observed = node.execute_request(query::QueryRequest::new("{ ManagedConfiguration {_docID} }").with_identity(Some(outsider.parse().unwrap()))).await;
        assert!(!observed.has_errors(), "{:?}", observed.errors);
        assert!(observed.data.unwrap()["ManagedConfiguration"].as_array().unwrap().is_empty(), "stale grant must not commit");
        node.shutdown().await.unwrap();
        decisions
    }).unwrap()
}

#[test]
fn transactional_userset_managers_respect_owner_admin_scope_and_revocation() {
    exercise_userset_manager();
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn userset_manager_admission_matches_executed_lean_model() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let actual = exercise_userset_manager();
    let facts = [
        (true, true, false, false, false, false, false),
        (false, true, false, false, true, true, false),
        (false, true, false, false, true, false, true),
        (false, true, false, false, true, false, false),
        (false, true, false, false, false, false, true),
        (false, false, false, false, true, false, true),
        (false, true, false, false, true, false, false),
        (false, true, false, false, true, false, true),
        (false, true, true, false, false, false, false),
        (false, true, false, true, false, false, false),
    ]
    .into_iter()
    .map(
        |(owner, declared, direct, wildcard, same_node, node_owner, node_admin)| {
            json!({
                "owner":owner,"declared":declared,"direct":direct,"wildcard":wildcard,
                "sameNode":same_node,"nodeOwner":node_owner,"nodeAdmin":node_admin
            })
        },
    )
    .collect::<Vec<Value>>();
    let mut child = Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .args(["env", "lean", "--run", "UsersetRelationManager.lean"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!(facts)).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, expected);
}
