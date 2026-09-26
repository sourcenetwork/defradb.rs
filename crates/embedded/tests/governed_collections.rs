#![cfg(feature = "iroh")]

//! Governed collections across iroh embedded nodes: the properties the
//! models state about two replicas, over a real transport.
//!
//! Everything else that judges writes is single-node, driving the merge
//! handler directly (`crates/db/tests/merge/governance/`). What those cannot
//! show is the thing the interface exists for: that two nodes running the
//! same rule, holding different bytes, never disagree, and that what one
//! node refuses does not reach another. Each test here names the property it
//! exercises and, where there is one, the model run that states it
//! (`proofs/tla/GovernanceContract.tla`, `GovernanceMerge.tla`).

mod iroh_peers;

use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use db::merge::governance::FieldValue;
use db::merge::governance::{
    Awaited, Emission, Judged, MergeCandidate, MergeValidator, MergeVerdict, MergeView,
};
use document::NormalValue;
use embedded::{AccessHooks, NodeBuilder};
use iroh_peers::{connect, create, doc_ids, iroh_config, listen_addr, Node};
use tokio::time::{sleep, Duration, Instant};

/// `writer` is `@immutable`, so a verdict may look a grant up by it and two
/// replicas holding the same grants answer the same way.
const SDL: &str = "type Grant { writer: String @immutable } \
                   type Note { grant: String } \
                   type Record { of: String }";

/// A note is accepted when a grant names its `grant` value as a writer;
/// `forged` is refused on the note's own bytes, and an unknown writer defers
/// on the immutable-field key that would release it.
///
/// With `emits`, an accepted note also emits a record of it, which is the
/// host's to write (`Judged { verdict, emit }`).
struct NotesNeedGrant {
    emits: bool,
}

impl NotesNeedGrant {
    fn new(emits: bool) -> Arc<Self> {
        Arc::new(Self { emits })
    }

    async fn verdict(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        if candidate.collection.name != "Note" {
            return Ok(MergeVerdict::Accept);
        }
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .unwrap_or_default();
        let Some((_, FieldValue::Value(NormalValue::String(grant)))) =
            fields.into_iter().find(|(name, _)| name == "grant")
        else {
            // A delete links no fields; nothing to judge it by here.
            return Ok(MergeVerdict::Accept);
        };
        if grant == "forged" {
            return Ok(MergeVerdict::reject("forged grant"));
        }
        let writer = NormalValue::String(grant);
        if view
            .find_documents("Grant", "writer", &writer)
            .await?
            .is_empty()
        {
            return Ok(MergeVerdict::defer(
                "no grant for this writer",
                [Awaited::immutable_field("Grant", "writer", writer)],
            ));
        }
        Ok(MergeVerdict::Accept)
    }
}

#[async_trait]
impl MergeValidator for NotesNeedGrant {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        self.verdict(candidate, view).await
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<Judged, String> {
        let verdict = self.verdict(candidate, view).await?;
        if !self.emits || !matches!(verdict, MergeVerdict::Accept) {
            return Ok(Judged::new(verdict));
        }
        if candidate.collection.name != "Note" {
            return Ok(Judged::new(verdict));
        }
        // The record is a fact about this composite: the same fact on every
        // replica that judges it, so the same bytes and the same document.
        Ok(Judged::new(verdict)
            .emitting(Emission::new("Record").field("of", candidate.cid.to_string())))
    }
}

/// Claims `Note` and judges it by `NotesNeedGrant`.
async fn governed(validator: Arc<NotesNeedGrant>) -> Result<Node> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(AccessHooks::new(["Note"]).with_merge_validator(validator))
        .build()
        .await?;
    node.add_schema(SDL).await?;
    Ok(node)
}

/// Claims `Note` and installs no validator: fail-closed (H-6).
async fn claimed_without_validator() -> Result<Node> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(AccessHooks::new(["Note"]))
        .build()
        .await?;
    node.add_schema(SDL).await?;
    Ok(node)
}

/// Runs the same schema with no governance at all: the node a governed peer
/// must not trust.
async fn ungoverned() -> Result<Node> {
    let node = NodeBuilder::default()
        .with_iroh(iroh_config())
        .build()
        .await?;
    node.add_schema(SDL).await?;
    Ok(node)
}

async fn replicate(from: &Node, to: &Node, collections: &[&str]) -> Result<()> {
    let collections: Vec<String> = collections.iter().map(|name| name.to_string()).collect();
    to.p2p()
        .context("p2p")?
        .ops()
        .add_collections(collections.clone())
        .await
        .map_err(|error| anyhow!(error))?;
    let addr = listen_addr(to).await?;
    from.p2p()
        .context("p2p")?
        .ops()
        .add_replicator(
            collections,
            Some(&addr),
            Default::default(),
            Vec::new(),
            None,
        )
        .await
        .map_err(|error| anyhow!(error))
}

async fn wait_for(node: &Node, collection: &str, expected: &[&str]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let ids = doc_ids(node, collection).await?;
        if expected.iter().all(|id| ids.iter().any(|held| held == id)) {
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("{collection} never merged {expected:?}, holds {ids:?}");
        }
        sleep(Duration::from_millis(200)).await;
    }
}

/// Nothing in `collection` arrives, given time to.
async fn stays_empty(node: &Node, collection: &str) -> Result<()> {
    sleep(Duration::from_secs(3)).await;
    let ids = doc_ids(node, collection).await?;
    if !ids.is_empty() {
        bail!("{collection} merged {ids:?}, which no verdict accepted");
    }
    Ok(())
}

/// A grant and a note under it replicate and merge on a peer running the
/// same rule: governance does not break replication.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_note_under_a_grant_merges_on_a_peer() -> Result<()> {
    let author = governed(NotesNeedGrant::new(false)).await?;
    let peer = governed(NotesNeedGrant::new(false)).await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Grant", "Note"]).await?;

    let (grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (note, _) = create(&author, "Note", r#"grant: "alice""#).await?;

    wait_for(&peer, "Grant", &[&grant]).await?;
    wait_for(&peer, "Note", &[&note]).await?;
    Ok(())
}

/// The note arrives before the grant it needs. The peer defers it, and
/// never rejects it, then merges it when the grant arrives: the contract's
/// L1 and `INV_RejectIntrinsic` over a transport, with the arrival order
/// reversed by joining the collections one at a time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_note_that_arrives_before_its_grant_merges_when_the_grant_arrives() -> Result<()> {
    let author = governed(NotesNeedGrant::new(false)).await?;
    let peer = governed(NotesNeedGrant::new(false)).await?;
    connect(&author, &peer).await?;

    let (_grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (note, _) = create(&author, "Note", r#"grant: "alice""#).await?;

    // Only Note is replicated, so the peer judges it with no grant in reach.
    replicate(&author, &peer, &["Note"]).await?;
    stays_empty(&peer, "Note").await?;

    // The grant's collection joins, the grant arrives, and the note's
    // deferral is released by the immutable-field key it named.
    replicate(&author, &peer, &["Grant"]).await?;
    wait_for(&peer, "Note", &[&note]).await?;
    Ok(())
}

/// A node that runs no validator writes a note its rule would refuse and
/// pushes it. The governed peer refuses it, which is the whole point of
/// judging at merge: an honest peer cannot be made to accept it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_forged_note_from_an_ungoverned_node_is_refused() -> Result<()> {
    let rogue = ungoverned().await?;
    let peer = governed(NotesNeedGrant::new(false)).await?;
    connect(&rogue, &peer).await?;
    replicate(&rogue, &peer, &["Grant", "Note"]).await?;

    let (grant, _) = create(&rogue, "Grant", r#"writer: "alice""#).await?;
    let (_forged, _) = create(&rogue, "Note", r#"grant: "forged""#).await?;
    let (good, _) = create(&rogue, "Note", r#"grant: "alice""#).await?;

    // The grant and the acceptable note arrive; the forged one never does.
    wait_for(&peer, "Grant", &[&grant]).await?;
    wait_for(&peer, "Note", &[&good]).await?;
    let notes = doc_ids(&peer, "Note").await?;
    if notes.len() != 1 {
        bail!("the peer merged {notes:?}; the forged note was accepted");
    }
    Ok(())
}

/// A write this node's own rule refuses never becomes a block, so no peer
/// ever sees it: the local write path judged by the merge validator.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_local_write_the_rule_refuses_never_reaches_a_peer() -> Result<()> {
    let author = governed(NotesNeedGrant::new(false)).await?;
    let peer = governed(NotesNeedGrant::new(false)).await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Grant", "Note"]).await?;

    let response = author
        .execute(r#"mutation { add_Note(input: {grant: "forged"}) { _docID } }"#)
        .await;
    if !response.has_errors() {
        bail!("the author committed a write its own rule refuses");
    }

    stays_empty(&peer, "Note").await?;
    // And it is not on the author either: the transaction was dropped.
    let own = doc_ids(&author, "Note").await?;
    if !own.is_empty() {
        bail!("the refused write was committed locally: {own:?}");
    }
    Ok(())
}

/// A grant and a note under it written in one request: the judge reads the
/// transaction's own documents, so the note is accepted although the grant
/// is not committed yet, and both replicate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_grant_and_a_note_in_one_request_merge_on_a_peer() -> Result<()> {
    let author = governed(NotesNeedGrant::new(false)).await?;
    let peer = governed(NotesNeedGrant::new(false)).await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Grant", "Note"]).await?;

    let response = author
        .execute(
            r#"mutation {
                 add_Grant(input: {writer: "bob"}) { _docID }
                 add_Note(input: {grant: "bob"}) { _docID }
               }"#,
        )
        .await;
    if response.has_errors() {
        bail!(
            "one request writing a grant and a note under it failed: {:?}",
            response.errors
        );
    }

    let grants = doc_ids(&author, "Grant").await?;
    let notes = doc_ids(&author, "Note").await?;
    if grants.len() != 1 || notes.len() != 1 {
        bail!("the author holds {grants:?} and {notes:?}");
    }
    wait_for(&peer, "Grant", &[&grants[0]]).await?;
    wait_for(&peer, "Note", &[&notes[0]]).await?;
    Ok(())
}

/// A claimed collection with no validator defers every composite a peer
/// pushes, rather than merging it as ungoverned (H-6, fail closed).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claimed_collection_with_no_validator_defers_a_peers_writes() -> Result<()> {
    let author = ungoverned().await?;
    let peer = claimed_without_validator().await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Grant", "Note"]).await?;

    let (grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (_note, _) = create(&author, "Note", r#"grant: "alice""#).await?;

    // Grant is not claimed, so it merges; Note is claimed with no validator.
    wait_for(&peer, "Grant", &[&grant]).await?;
    stays_empty(&peer, "Note").await?;
    Ok(())
}

/// The peer that judges a note emits a record of it, and the record reaches
/// the author like any merge: emission is forwarded to replicators under
/// its own collection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_record_a_peer_emitted_reaches_the_author() -> Result<()> {
    let author = governed(NotesNeedGrant::new(true)).await?;
    let peer = governed(NotesNeedGrant::new(true)).await?;
    connect(&author, &peer).await?;
    connect(&peer, &author).await?;
    replicate(&author, &peer, &["Grant", "Note", "Record"]).await?;
    replicate(&peer, &author, &["Record"]).await?;

    let (_grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (note, _) = create(&author, "Note", r#"grant: "alice""#).await?;
    wait_for(&peer, "Note", &[&note]).await?;

    // The author's own write is not judged by the merge validator's emitting
    // path, so the record originates on the peer and travels back.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let here = doc_ids(&author, "Record").await?;
        if !here.is_empty() {
            let there = doc_ids(&peer, "Record").await?;
            if there != here {
                bail!("the record differs across replicas: {here:?} vs {there:?}");
            }
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!("the record the peer emitted never reached the author");
        }
        sleep(Duration::from_millis(200)).await;
    }
}

/// Two peers judge the same note and emit the same fact. The record is the
/// same bytes on both, so when they meet there is one document, not two:
/// idempotence over a transport rather than by construction.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_peers_that_judge_one_note_emit_one_record() -> Result<()> {
    let author = ungoverned().await?;
    let left = governed(NotesNeedGrant::new(true)).await?;
    let right = governed(NotesNeedGrant::new(true)).await?;

    connect(&author, &left).await?;
    connect(&author, &right).await?;
    replicate(&author, &left, &["Grant", "Note"]).await?;
    replicate(&author, &right, &["Grant", "Note"]).await?;

    let (_grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (note, _) = create(&author, "Note", r#"grant: "alice""#).await?;
    wait_for(&left, "Note", &[&note]).await?;
    wait_for(&right, "Note", &[&note]).await?;

    let one = doc_ids(&left, "Record").await?;
    let two = doc_ids(&right, "Record").await?;
    if one.len() != 1 || one != two {
        bail!("two replicas emitted {one:?} and {two:?} for one fact");
    }

    // They meet: the record is one document, since the bytes are identical.
    connect(&left, &right).await?;
    replicate(&left, &right, &["Record"]).await?;
    sleep(Duration::from_secs(3)).await;
    let merged = doc_ids(&right, "Record").await?;
    if merged != two {
        bail!("meeting turned one record into {merged:?}");
    }
    Ok(())
}

/// A deferred note survives a restart: the defer index is in memory, and
/// the sweep over the unmerged set is what merges it once the grant is
/// held (`MC_GovernanceMerge_Today`, the restart route).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_deferred_note_merges_after_the_peer_restarts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("peer");

    let author = governed(NotesNeedGrant::new(false)).await?;
    let (_grant, _) = create(&author, "Grant", r#"writer: "alice""#).await?;
    let (note, _) = create(&author, "Note", r#"grant: "alice""#).await?;

    let peer = NodeBuilder::default()
        .data_path(&path)
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(["Note"]).with_merge_validator(NotesNeedGrant::new(false)),
        )
        .build()
        .await?;
    peer.add_schema(SDL).await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Note"]).await?;
    stays_empty(&peer, "Note").await?;
    peer.shutdown().await;
    // The store's lock goes with the node, so it has to be dropped before
    // the same path is opened again.
    drop(peer);

    // The grant is held before the restart, so nothing arrives to release
    // the deferral: only a pass over the unmerged set reaches it.
    let peer = NodeBuilder::default()
        .data_path(&path)
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(["Note"]).with_merge_validator(NotesNeedGrant::new(false)),
        )
        .build()
        .await?;
    connect(&author, &peer).await?;
    replicate(&author, &peer, &["Grant", "Note"]).await?;
    wait_for(&peer, "Note", &[&note]).await?;
    Ok(())
}
