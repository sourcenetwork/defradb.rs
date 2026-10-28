//! The node's API calls run on the database's worker pool: none of the
//! storage calls they make is made off the pool, which the store counts.

use super::Node;

const POLICY_YAML: &str = r#"
name: Notes
resources:
  - name: notes
    relations:
      - name: reader
    permissions:
      - name: read
        expr: reader
      - name: update
      - name: delete
"#;
const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

fn off_pool_calls(node: &Node) -> u64 {
    node.transaction_stats()
        .expect("the node's store is regolith")
        .off_pool_calls
}

#[test]
fn api_calls_make_no_storage_call_off_the_pool() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        let before = off_pool_calls(&node);

        let policy_id = node.add_dac_policy(OWNER, POLICY_YAML).await.unwrap();
        node.add_collection(&format!(
            "type Notes @policy(id: \"{policy_id}\", resource: \"notes\") {{ title: String }}"
        ))
        .await
        .unwrap();
        node.add_collection("type Tag { label: String }")
            .await
            .unwrap();
        let versions = node.get_all_collection_versions().await.unwrap();
        let tag = versions
            .iter()
            .find(|version| version.name == "Tag")
            .expect("Tag was added");
        let found = node
            .get_collection_by_version_id(&tag.version_id)
            .await
            .unwrap();
        assert_eq!(found.map(|version| version.name), Some("Tag".to_owned()));

        assert_eq!(
            off_pool_calls(&node),
            before,
            "an API call reached the store from a thread off the pool"
        );
    })
    .unwrap()
}
