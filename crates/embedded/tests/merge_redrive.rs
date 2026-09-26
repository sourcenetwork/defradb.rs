#![cfg(feature = "iroh")]

//! A composite a merge validator deferred, then merged by re-drive when what
//! it awaited arrived, reaches this node's own replicators like any composite
//! that merged on its first attempt.

mod iroh_peers;

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cid::Cid;
use db::merge::governance::{FieldValue, MergeCandidate, MergeValidator, MergeVerdict, MergeView};
use document::NormalValue;
use embedded::{AccessHooks, NodeBuilder};
use iroh_peers::{connect, create, doc_ids, iroh_config, replicate, wait_for_docs, Node};
use tokio::time::{sleep, Duration};

const SDL: &str = "type Grant { label: String } type Note { grant: String }";

/// A note names a grant by its genesis composite's CID; until that composite
/// is held the verdict awaits it.
struct AwaitGrant;

#[async_trait]
impl MergeValidator for AwaitGrant {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .unwrap_or_default();
        let grant = fields.iter().find_map(|(name, value)| match value {
            FieldValue::Value(NormalValue::String(grant)) if name == "grant" => {
                Cid::try_from(grant.as_str()).ok()
            }
            _ => None,
        });
        let Some(grant) = grant else {
            return Ok(MergeVerdict::reject("note names no grant"));
        };
        if view.composite_fields(&grant).await?.is_none() {
            return Ok(MergeVerdict::defer("grant not held", [grant]));
        }
        Ok(MergeVerdict::Accept)
    }
}

async fn plain_node() -> Result<Node> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .build()
        .await?;
    node.add_schema(SDL).await?;
    Ok(node)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_composite_merged_by_redrive_reaches_this_nodes_replicators() -> Result<()> {
    let author = plain_node().await?;
    let middle = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(AccessHooks::new(["Note"]).with_merge_validator(Arc::new(AwaitGrant)))
        .build()
        .await?;
    middle.add_schema(SDL).await?;
    let tail = plain_node().await?;

    connect(&author, &middle).await?;
    connect(&middle, &tail).await?;

    let (grant_doc, grant_cid) = create(&author, "Grant", r#"label: "grant""#).await?;
    let (note_doc, _) = create(&author, "Note", &format!(r#"grant: "{grant_cid}""#)).await?;

    replicate(&middle, &tail, &["Note"]).await?;
    replicate(&author, &middle, &["Note"]).await?;

    // The note reaches the middle node before the grant it names, so its
    // verdict defers and its first merge attempt does not merge it.
    sleep(Duration::from_secs(3)).await;
    assert!(
        doc_ids(&middle, "Note").await?.is_empty(),
        "note merged before the grant it awaits"
    );
    assert!(doc_ids(&tail, "Note").await?.is_empty());

    replicate(&author, &middle, &["Grant"]).await?;
    wait_for_docs(&middle, "Grant", &[&grant_doc]).await?;
    wait_for_docs(&middle, "Note", &[&note_doc]).await?;

    wait_for_docs(&tail, "Note", &[&note_doc])
        .await
        .context("the re-driven composite never reached the tail node")?;

    for node in [author, middle, tail] {
        node.shutdown().await;
    }
    Ok(())
}
