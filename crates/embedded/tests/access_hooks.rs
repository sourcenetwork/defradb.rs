#![cfg(feature = "iroh")]

//! An app's own access control on iroh embedded nodes: a merge validator that
//! accepts, rejects and defers replicated notes, and node-local read and
//! write validators.

mod iroh_peers;

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cid::Cid;
use db::merge::governance::{
    FieldValue, MergeCandidate, MergeValidator, MergeVerdict, MergeView, SignatureStatus,
};
use document::NormalValue;
use embedded::{AccessHooks, NodeBuilder};
use iroh_peers::{connect, create, doc_ids, iroh_config, sync, wait_for_docs};
use query::access_hooks::{ReadRequest, ReadValidator, WriteRequest, WriteValidator};
use serde_json::Value as JsonValue;
use tokio::time::{sleep, Duration};

const SDL: &str =
    "type Grant { writer: String label: String } type Note { grant: String body: String }";

/// A note merges when the grant it names is held and names the note's signer.
struct GrantValidator;

#[async_trait]
impl MergeValidator for GrantValidator {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let SignatureStatus::Verified(signer) = &candidate.signature else {
            return Ok(MergeVerdict::reject("notes must be signed"));
        };
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .unwrap_or_default();
        let grant = fields.iter().find_map(|(name, value)| match value {
            FieldValue::Value(NormalValue::String(grant)) if name == "grant" => Some(grant.clone()),
            _ => None,
        });
        let Some(grant) = grant.and_then(|grant| Cid::try_from(grant.as_str()).ok()) else {
            return Ok(MergeVerdict::reject("note names no grant"));
        };
        let Some(grant_fields) = view.composite_fields(&grant).await? else {
            return Ok(MergeVerdict::defer("grant not held", [grant]));
        };
        let names_signer = grant_fields.iter().any(|(name, value)| {
            name == "writer" && *value == FieldValue::Value(NormalValue::String(signer.clone()))
        });
        Ok(if names_signer {
            MergeVerdict::Accept
        } else {
            MergeVerdict::reject("grant does not name the signer")
        })
    }
}

struct HideDrafts;

#[async_trait]
impl ReadValidator for HideDrafts {
    fn governs(&self, collection: &schema::CollectionVersion) -> bool {
        collection.name == "Note"
    }

    async fn may_read(&self, request: &ReadRequest<'_>) -> Result<bool, String> {
        Ok(!request.doc_id.is_empty() && request.identity.is_some())
    }
}

struct RefuseEmptyBodies;

#[async_trait]
impl WriteValidator for RefuseEmptyBodies {
    async fn validate_write(&self, request: &WriteRequest<'_>) -> Result<(), String> {
        let empty = request
            .create_input
            .iter()
            .chain(std::iter::once(request.update_input))
            .any(|input| input.get("body") == Some(&JsonValue::String(String::new())));
        if empty {
            return Err("a note needs a body".to_string());
        }
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_validator_accepts_rejects_and_defers_replicated_notes() -> Result<()> {
    let author = NodeBuilder::default()
        .with_iroh(iroh_config())
        .enable_signing()
        .build()
        .await?;
    let receiver = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(["Note"]).with_merge_validator(Arc::new(GrantValidator)),
        )
        .build()
        .await?;
    author.add_schema(SDL).await?;
    receiver.add_schema(SDL).await?;
    let writer = author
        .node_identity_did
        .clone()
        .context("author has a signing identity")?;
    defra_core::signing::set_signing_config(defra_core::signing::get_identity(&writer));

    let (held_grant, held_grant_cid) = create(
        &author,
        "Grant",
        &format!(r#"writer: "{writer}", label: "held""#),
    )
    .await?;
    let (late_grant, late_grant_cid) = create(
        &author,
        "Grant",
        &format!(r#"writer: "{writer}", label: "late""#),
    )
    .await?;
    let (accepted, _) = create(
        &author,
        "Note",
        &format!(r#"grant: "{held_grant_cid}", body: "ok""#),
    )
    .await?;
    let (rejected, _) = create(&author, "Note", r#"grant: "nothing", body: "no grant""#).await?;
    let (deferred, _) = create(
        &author,
        "Note",
        &format!(r#"grant: "{late_grant_cid}", body: "later""#),
    )
    .await?;

    connect(&receiver, &author).await?;
    sync(&receiver, "Grant", vec![held_grant]).await?;
    sync(
        &receiver,
        "Note",
        vec![accepted.clone(), rejected.clone(), deferred.clone()],
    )
    .await?;

    wait_for_docs(&receiver, "Note", &[accepted.as_str()]).await?;
    sleep(Duration::from_secs(1)).await;
    assert_eq!(doc_ids(&receiver, "Note").await?, vec![accepted.clone()]);

    sync(&receiver, "Grant", vec![late_grant]).await?;
    wait_for_docs(&receiver, "Note", &[accepted.as_str(), deferred.as_str()]).await?;
    assert!(!doc_ids(&receiver, "Note").await?.contains(&rejected));

    author.shutdown().await;
    receiver.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_and_write_validators_narrow_local_clients() -> Result<()> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(Vec::<String>::new())
                .with_read_validator(Arc::new(HideDrafts))
                .with_write_validator(Arc::new(RefuseEmptyBodies)),
        )
        .build()
        .await?;
    node.add_schema(SDL).await?;

    let refused = node
        .execute(r#"mutation { add_Note(input: {body: ""}) { _docID } }"#)
        .await;
    assert!(
        format!("{:?}", refused.errors).contains("a note needs a body"),
        "{:?}",
        refused.errors
    );

    create(&node, "Note", r#"body: "kept""#).await?;
    let anonymous = node.execute("query { Note { _docID } }").await;
    assert_eq!(
        anonymous.data.as_ref().and_then(|data| data.get("Note")),
        Some(&serde_json::json!([]))
    );
    let reader = identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")?;
    let identified = node
        .query_runner
        .execute(query::QueryRequest::new("query { Note { body } }").with_identity(Some(reader)))
        .await;
    assert_eq!(
        identified.data.as_ref().and_then(|data| data.get("Note")),
        Some(&serde_json::json!([{ "body": "kept" }]))
    );
    let commits = node.execute("query { _commits { cid docID } }").await;
    let visible = commits
        .data
        .as_ref()
        .and_then(|data| data.get("_commits"))
        .and_then(JsonValue::as_array)
        .context("commits result")?;
    assert!(visible
        .iter()
        .all(
            |commit| commit.get("docID").and_then(JsonValue::as_str) == Some("")
                || commit.get("docID").is_none()
        ));

    node.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn access_hooks_require_iroh() {
    let error = NodeBuilder::default()
        .with_access_hooks(AccessHooks::new(["Note"]))
        .build()
        .await
        .err()
        .expect("a node without iroh refuses access hooks");
    assert!(error.to_string().contains("iroh"), "{error}");
}
