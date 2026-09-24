//! A policy-bound collection whose documents an owner created on the node, so
//! the node's local ACP registers that owner and no one else.

use serde_json::json;

use crate::key::Key;
use crate::node;

pub struct Protected {
    pub owner: Key,
    pub collection: String,
    pub sdl: String,
}

impl Protected {
    pub async fn start(collection: &str) -> Self {
        let owner = Key::generate();
        let token = owner.token();
        let policy = node::http(
            "POST",
            "/api/v0/acp/document/policy",
            "text/plain",
            &policy(collection),
            Some(&token),
        )
        .await;
        let policy_id = policy["PolicyID"].as_str().expect("PolicyID");
        let sdl = format!(
            r#"type {collection} @policy(id: "{policy_id}", resource: "users") {{ name: String age: Int }}"#
        );
        node::add_schema(&sdl, Some(&token)).await;
        node::subscribe(&[collection]).await;
        Self {
            owner,
            collection: collection.to_string(),
            sdl,
        }
    }

    pub async fn create(&self, name: &str, age: i64) -> String {
        let collection = &self.collection;
        let created = node::graphql(
            &format!(
                r#"mutation {{ create_{collection}(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
            ),
            Some(&self.owner.token()),
        )
        .await;
        node::created_doc_id(&created)
    }

    pub async fn grant_writer(&self, doc_id: &str, did: &str) {
        node::post_json(
            "/api/v0/acp/document/relationship",
            json!({
                "collection": self.collection,
                "docID": doc_id,
                "relation": "writer",
                "actor": did,
            }),
            Some(&self.owner.token()),
        )
        .await;
    }

    /// The document's age as its owner reads it on the node.
    pub async fn age_on_node(&self, doc_id: &str) -> Option<i64> {
        let collection = &self.collection;
        let rows = node::graphql(
            &format!(r#"{{ {collection}(docID: "{doc_id}") {{ age }} }}"#),
            Some(&self.owner.token()),
        )
        .await;
        rows[collection.as_str()][0]["age"].as_i64()
    }

    pub async fn exists_on_node(&self, doc_id: &str) -> bool {
        let collection = &self.collection;
        let rows = node::graphql(
            &format!("{{ {collection} {{ _docID }} }}"),
            Some(&self.owner.token()),
        )
        .await;
        node::doc_ids(&rows[collection.as_str()])
            .iter()
            .any(|id| id == doc_id)
    }

    pub fn set_age(&self, doc_id: &str, age: i64) -> String {
        format!(
            r#"mutation {{ update_{}(docID: "{doc_id}", input: {{age: {age}}}) {{ _docID }} }}"#,
            self.collection
        )
    }
}

/// Updates need `writer`, which only the owner can grant. Named per collection
/// so each test's owner adds a policy of its own.
fn policy(collection: &str) -> String {
    format!(
        r#"name: {collection}-policy
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
"#
    )
}
