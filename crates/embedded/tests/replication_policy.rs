#![cfg(feature = "iroh")]

//! An app replication policy on iroh embedded nodes narrows what is pushed
//! and served to one peer, and what one peer may push, without touching
//! other peers or collections.

mod iroh_peers;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
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
