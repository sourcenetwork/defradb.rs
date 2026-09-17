#![cfg(feature = "iroh")]

//! An app replication policy on iroh embedded nodes narrows what is pushed
//! and served to one peer, and what one peer may push, without touching
//! other peers or collections.

mod iroh_peers;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use blockstore::Blockstore;
use defra_http::router::P2pDocumentRequest;
use embedded::{AccessHooks, NodeBuilder};
use iroh_peers::{connect, create, doc_ids, iroh_config, listen_addr, sync, wait_for_docs, Node};
use p2p::replication_policy::{
    InboundRequest, OutboundBlock, OutboundPath, PolicyPeer, ReplicationPolicy,
};
use tokio::time::{sleep, Duration};

const SDL: &str = "type Grant { label: String } type Note { body: String }";

/// Only the listed documents may leave for `peer`.
#[derive(Default)]
struct AllowlistForPeer {
    peer: Mutex<String>,
    allowed: Mutex<HashSet<String>>,
    withheld: Mutex<Vec<(OutboundPath, String)>>,
}

#[async_trait]
impl ReplicationPolicy for AllowlistForPeer {
    async fn may_send(
        &self,
        peer: &PolicyPeer<'_>,
        path: OutboundPath,
        block: &OutboundBlock<'_>,
    ) -> Result<bool, String> {
        if peer.peer_id != *self.peer.lock().unwrap() {
            return Ok(true);
        }
        let allowed = self.allowed.lock().unwrap();
        let withheld: Vec<&String> = block
            .doc_ids
            .iter()
            .filter(|doc_id| !allowed.contains(*doc_id))
            .collect();
        for doc_id in &withheld {
            self.withheld
                .lock()
                .unwrap()
                .push((path, (*doc_id).clone()));
        }
        Ok(withheld.is_empty())
    }
}

/// `peer` may not push into `collection_id`; the policy's collections are
/// joined as topics.
#[derive(Default)]
struct RefusePushes {
    peer: Mutex<String>,
    collection_id: Mutex<String>,
}

#[async_trait]
impl ReplicationPolicy for RefusePushes {
    async fn may_accept(
        &self,
        peer: &PolicyPeer<'_>,
        request: InboundRequest,
        collection_id: &str,
    ) -> Result<bool, String> {
        Ok(!(request == InboundRequest::Push
            && peer.peer_id == *self.peer.lock().unwrap()
            && collection_id == *self.collection_id.lock().unwrap()))
    }

    fn collections(&self) -> Vec<String> {
        vec!["Grant".to_string(), "Note".to_string()]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_document_is_withheld_from_one_peer_on_push_and_serve() -> Result<()> {
    let policy = Arc::new(AllowlistForPeer::default());
    let author = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(Vec::<String>::new()).with_replication_policy(policy.clone()),
        )
        .build()
        .await?;
    author.add_schema(SDL).await?;
    let trusted = plain_node().await?;
    let restricted = plain_node().await?;

    connect(&author, &trusted).await?;
    *policy.peer.lock().unwrap() = connect(&author, &restricted).await?;

    let (public, _) = create(&author, "Note", r#"body: "public""#).await?;
    policy.allowed.lock().unwrap().insert(public.clone());
    let (replayed, _) = create(&author, "Note", r#"body: "before replicators""#).await?;

    replicate(&author, &trusted, &["Note"]).await?;
    replicate(&author, &restricted, &["Note"]).await?;
    let (pushed, _) = create(&author, "Note", r#"body: "after replicators""#).await?;

    wait_for_docs(&trusted, "Note", &[&public, &replayed, &pushed]).await?;
    wait_for_docs(&restricted, "Note", &[&public]).await?;

    connect(&restricted, &author).await?;
    sync(&restricted, "Note", vec![replayed.clone(), pushed.clone()])
        .await
        .ok();
    sleep(Duration::from_secs(3)).await;
    assert_eq!(doc_ids(&restricted, "Note").await?, vec![public.clone()]);

    let withheld = policy.withheld.lock().unwrap().clone();
    for path in [OutboundPath::Push, OutboundPath::Serve] {
        assert!(
            withheld.iter().any(|(seen, _)| *seen == path),
            "no {path:?} withheld in {withheld:?}"
        );
    }

    // A withheld push keeps its retry marker, so relaxing the policy lets
    // the next retry pass deliver what was withheld.
    policy
        .allowed
        .lock()
        .unwrap()
        .extend([replayed.clone(), pushed.clone()]);
    author
        .p2p()
        .context("p2p")?
        .retry_replicators()
        .await
        .map_err(|error| anyhow!(error))?;
    wait_for_docs(&restricted, "Note", &[&public, &replayed, &pushed]).await?;

    for node in [author, trusted, restricted] {
        node.shutdown().await;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pushes_from_one_peer_are_refused_for_one_collection() -> Result<()> {
    let policy = Arc::new(RefusePushes::default());
    let receiver = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(Vec::<String>::new()).with_replication_policy(policy.clone()),
        )
        .build()
        .await?;
    receiver.add_schema(SDL).await?;
    let author = plain_node().await?;

    let subscribed = receiver
        .p2p()
        .context("p2p")?
        .ops()
        .get_collections()
        .await
        .map_err(|error| anyhow!(error))?;
    assert_eq!(subscribed.len(), 2, "policy topics joined: {subscribed:?}");

    *policy.collection_id.lock().unwrap() = receiver
        .database
        .get_collection("Note")?
        .context("Note")?
        .collection_id()
        .to_string();
    *policy.peer.lock().unwrap() = author
        .p2p()
        .context("p2p")?
        .ops()
        .local_peer_id()
        .await
        .map_err(|error| anyhow!(error))?;

    connect(&author, &receiver).await?;
    replicate(&author, &receiver, &["Grant", "Note"]).await?;
    let (note, _) = create(&author, "Note", r#"body: "refused""#).await?;
    let (grant, _) = create(&author, "Grant", r#"label: "accepted""#).await?;

    wait_for_docs(&receiver, "Grant", &[&grant]).await?;
    sleep(Duration::from_secs(3)).await;
    assert!(!doc_ids(&receiver, "Note").await?.contains(&note));

    author.shutdown().await;
    receiver.shutdown().await;
    Ok(())
}

/// T writes notes before any link, then replicates both ways with D, which
/// it restricts to `allowed` of them. Returns the notes D holds after the
/// allowed ones should have arrived, and the allowed notes.
async fn restricted_peer_after_link(shared_field: bool) -> Result<(Vec<String>, Vec<String>)> {
    const SDL: &str = "type Entry { kind: String body: String }";
    let policy = Arc::new(AllowlistForPeer::default());
    let tower = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(Vec::<String>::new()).with_replication_policy(policy.clone()),
        )
        .build()
        .await?;
    tower.add_schema(SDL).await?;
    let device = NodeBuilder::default()
        .with_iroh(iroh_config())
        .build()
        .await?;
    device.add_schema(SDL).await?;

    let mut written = Vec::new();
    for index in 0..6 {
        let kind = if shared_field {
            "entry".to_string()
        } else {
            format!("entry-{index}")
        };
        let (doc_id, _) = create(
            &tower,
            "Entry",
            &format!(r#"kind: "{kind}", body: "body-{index}""#),
        )
        .await?;
        written.push(doc_id);
    }
    let allowed: Vec<String> = written[..2].to_vec();
    *policy.peer.lock().unwrap() = connect(&tower, &device).await?;
    policy
        .allowed
        .lock()
        .unwrap()
        .extend(allowed.iter().cloned());

    replicate(&tower, &device, &["Entry"]).await?;
    replicate(&device, &tower, &["Entry"]).await?;

    let expected: Vec<&str> = allowed.iter().map(String::as_str).collect();
    let arrived = wait_for_docs(&device, "Entry", &expected).await;
    let held = doc_ids(&device, "Entry").await?;
    tower.shutdown().await;
    device.shutdown().await;
    arrived.map(|()| (held, allowed))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allowed_documents_reach_a_restricted_peer() -> Result<()> {
    let (held, mut allowed) = restricted_peer_after_link(false).await?;
    allowed.sort();
    assert_eq!(held, allowed);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allowed_documents_sharing_a_field_value_reach_a_restricted_peer() -> Result<()> {
    let (held, mut allowed) = restricted_peer_after_link(true).await?;
    allowed.sort();
    assert_eq!(held, allowed);
    Ok(())
}

/// Every commit CID the node holds for `doc_id`.
async fn commit_cids(node: &Node, doc_id: &str) -> Result<Vec<String>> {
    let response = node
        .execute(&format!(
            r#"query {{ _commits(docID: ["{doc_id}"]) {{ cid }} }}"#
        ))
        .await;
    if response.has_errors() {
        bail!("_commits query failed: {:?}", response.errors);
    }
    Ok(response
        .data
        .as_ref()
        .and_then(|data| data.get("_commits"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|commit| commit.get("cid").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect())
}

async fn holds_block(node: &Node, cid: &str) -> Result<bool> {
    let cid = cid::Cid::try_from(cid).context("block cid")?;
    blockstore::DefraBlockstore::new(node.database.store().clone(), true)
        .has(&cid)
        .await
        .map_err(Into::into)
}

/// The explicit push path is gated by the same policy as live and replay
/// pushes: a withheld document leaves no block behind at the peer, while a
/// block it shares with an allowed document still arrives.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_explicit_push_withholds_a_refused_document() -> Result<()> {
    const SDL: &str = "type Entry { kind: String body: String }";
    let policy = Arc::new(AllowlistForPeer::default());
    let author = NodeBuilder::default()
        .with_iroh(iroh_config())
        .with_access_hooks(
            AccessHooks::new(Vec::<String>::new()).with_replication_policy(policy.clone()),
        )
        .build()
        .await?;
    author.add_schema(SDL).await?;
    let peer = NodeBuilder::default()
        .with_iroh(iroh_config())
        .build()
        .await?;
    peer.add_schema(SDL).await?;

    let peer_id = connect(&author, &peer).await?;
    *policy.peer.lock().unwrap() = peer_id.clone();

    // The allowlist starts empty, so the replicator's own replay and live
    // pushes deliver nothing and the explicit call is the only sender.
    replicate(&author, &peer, &["Entry"]).await?;
    let (refused, refused_cid) = create(&author, "Entry", r#"kind: "shared", body: "x""#).await?;
    let (allowed, _) = create(&author, "Entry", r#"kind: "shared", body: "y""#).await?;
    sleep(Duration::from_secs(3)).await;
    assert!(doc_ids(&peer, "Entry").await?.is_empty());

    let refused_blocks = commit_cids(&author, &refused).await?;
    let allowed_blocks = commit_cids(&author, &allowed).await?;
    let shared: Vec<String> = refused_blocks
        .iter()
        .filter(|cid| allowed_blocks.contains(cid))
        .cloned()
        .collect();
    assert!(
        !shared.is_empty(),
        "no block shared by {refused_blocks:?} and {allowed_blocks:?}"
    );

    policy.allowed.lock().unwrap().insert(allowed.clone());
    policy.withheld.lock().unwrap().clear();
    author
        .p2p()
        .context("p2p")?
        .ops()
        .push_documents_to_peer(
            &peer_id,
            vec![
                P2pDocumentRequest {
                    collection: "Entry".to_string(),
                    doc_id: refused.clone(),
                },
                P2pDocumentRequest {
                    collection: "Entry".to_string(),
                    doc_id: allowed.clone(),
                },
            ],
        )
        .await
        .map_err(|error| anyhow!(error))?;

    wait_for_docs(&peer, "Entry", &[&allowed]).await?;
    sleep(Duration::from_secs(3)).await;
    assert_eq!(doc_ids(&peer, "Entry").await?, vec![allowed.clone()]);
    assert!(
        !holds_block(&peer, &refused_cid).await?,
        "refused document's composite block {refused_cid} reached the peer"
    );
    for cid in &shared {
        assert!(
            holds_block(&peer, cid).await?,
            "shared block {cid} was withheld from the peer"
        );
    }
    let withheld = policy.withheld.lock().unwrap().clone();
    assert!(
        withheld
            .iter()
            .any(|(path, doc_id)| *path == OutboundPath::Push && *doc_id == refused),
        "explicit push never asked the policy about {refused}: {withheld:?}"
    );

    // The withheld document kept its durable retry marker, so a later, more
    // permissive policy delivers it on the next retry pass.
    policy.allowed.lock().unwrap().insert(refused.clone());
    author
        .p2p()
        .context("p2p")?
        .retry_replicators()
        .await
        .map_err(|error| anyhow!(error))?;
    wait_for_docs(&peer, "Entry", &[&allowed, &refused]).await?;

    author.shutdown().await;
    peer.shutdown().await;
    Ok(())
}
