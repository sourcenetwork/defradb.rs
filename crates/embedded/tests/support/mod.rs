//! A DAC-policy collection, identities, and grants shared by the peer tests.

use std::time::Duration;

use acp::StorePolicyOptions;
use anyhow::{anyhow, bail, Result};
use defra_core::current_identity::with_scoped_identity;
use embedded::EmbeddedNode;
use identity::{Did, Identity};
use query::{QueryRequest, QueryResponse};
use tokio::time::{sleep, Instant};

const POLICY: &str = r#"name: test-user-policy
description: A test policy for user document access control

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
          - actor"#;

pub type Node = EmbeddedNode<storage::RegolithStore>;

pub fn new_identity() -> Result<Did> {
    let raw = identity::RawIdentity::from_ed25519(crypto::generate_ed25519()?)?;
    let did = raw.did()?;
    defra_core::signing::store_identity(
        did.as_ref(),
        defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: defra_core::signing::SigningConfig::private_key_bytes_from_vec(
                raw.private_key_bytes().to_vec(),
            ),
            public_key_bytes: raw.public_key_bytes().to_vec(),
            public_key_hex: hex::encode(raw.public_key_bytes()),
            remote_signer: None,
            signing_authorization: None,
        },
    );
    Ok(did)
}

pub async fn add_policy(node: &Node) -> Result<String> {
    let store = node
        .local_zanzibar_store
        .as_ref()
        .ok_or_else(|| anyhow!("local zanzibar store missing"))?;
    let parsed = acp::policy_yaml::parse_policy_yaml(POLICY).map_err(|e| anyhow!(e))?;
    let counter = store.next_policy_counter().await?;
    let policy = acp::policy_yaml::build_policy(&parsed, counter).map_err(|e| anyhow!(e))?;
    let options = StorePolicyOptions::new()
        .with_validation()
        .with_dpi_enforcement();
    store.store_policy_with_options(&policy, &options).await?;
    Ok(policy.id)
}

pub async fn add_schema(node: &Node, sdl: &str, creator: &Did) -> Result<()> {
    let collections = query::parse_sdl(sdl).map_err(|e| anyhow!("SDL parse error: {e}"))?;
    schema::definition_validation::validate_new_collections(&collections)
        .map_err(|e| anyhow!("schema validation error: {e}"))?;
    with_scoped_identity(Some(creator.to_string()), async {
        node.database
            .create_collections_atomic_with_acp_registration(
                collections,
                node.document_acp.clone(),
                Some(creator.clone()),
            )
            .await
    })
    .await?;
    Ok(())
}

pub async fn grant_collection_reader(
    node: &Node,
    requestor: &Did,
    target: &Did,
    policy_id: &str,
    collection_id: &str,
) -> Result<()> {
    with_scoped_identity(Some(requestor.to_string()), async {
        node.document_acp
            .add_actor_relationship(
                requestor,
                target,
                policy_id,
                "users",
                collection_id,
                "reader",
                &[],
            )
            .await
    })
    .await?;
    Ok(())
}

pub async fn users_as(node: &Node, reader: &Did, selection: &str) -> QueryResponse {
    let request = format!("query {{ Users {{ {selection} }} }}");
    with_scoped_identity(Some(reader.to_string()), async {
        node.query_runner
            .execute(QueryRequest::new(&request).with_identity(Some(reader.clone())))
            .await
    })
    .await
}

pub async fn wait_for_fred(node: &Node, reader: &Did) -> Result<()> {
    let expected = serde_json::json!([{ "name": "Fred" }]);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = users_as(node, reader, "name").await;
        let users = response.data.as_ref().and_then(|data| data.get("Users"));
        if users == Some(&expected) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "peer never returned Fred as the reader; last response: data={:?} errors={:?}",
                response.data,
                response.errors
            );
        }
        sleep(Duration::from_millis(250)).await;
    }
}
