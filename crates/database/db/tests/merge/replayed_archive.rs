//! A document its owner deleted on a Vera deployment, whose object the delete
//! archived (`proofs/tla/ArchiveOnDelete.tla`), replayed on another node of
//! that deployment: the delete itself, commits that reach the node only after
//! the archive, and a genesis the node first sees after it.
//!
//! The deployment shows an archived object as unregistered, the same answer
//! it gives for a registration that has not landed yet. The node cannot tell
//! the two apart, so every frame of the document is put back as unregistered
//! and tried again without end: no delete lands, nothing is owed, nothing is
//! parked. These tests pin that behaviour, which sourcenetwork/defradb.rs#2980
//! (rule 6) tracks.

use std::sync::Arc;

use crate::replica_support::{
    is_parked, key, Chain, Composite, Embedder, Key, Receiver, Source, POLICY, RESOURCE,
};
use db::merge::ReplicaOwnership;
use defra_core::merge::MergeOutcome;

const LISTED: &str =
    "replicated protected document is not yet registered; its genesis registers it first";
const UNLISTED: &str = "replicated protected document is not yet registered in local ACP";

struct Deployment {
    chain: Arc<Chain>,
    owner: Key,
    peer: Receiver,
    source: Source,
}

/// The owner's node created and registered a document, and granted the
/// peer's actor read. The peer lists the collection for replica
/// registration, with the owner as one of the deployment's own actors, or
/// does not list it.
async fn deployment(listed: bool) -> (Deployment, String, Composite) {
    let chain = Chain::new();
    let (owner, peer_actor) = (key(), key().did);
    let replicas = listed.then(|| {
        Arc::new(Embedder {
            own: vec![owner.did.clone()],
            nodes: vec![],
        }) as Arc<dyn ReplicaOwnership>
    });
    let peer = Receiver::open(&chain, &peer_actor, replicas).await;
    let source = Source::open().await;
    let (doc, genesis) = source.create(&owner, "Alice").await;
    chain
        .as_node(&owner.did)
        .register_doc_object(&owner.did, POLICY, RESOURCE, &doc)
        .await
        .unwrap();
    chain.grant(&doc, "reader", &peer_actor);
    (
        Deployment {
            chain,
            owner,
            peer,
            source,
        },
        doc,
        genesis,
    )
}

/// `composite` is put back as unregistered, again after a re-drive, and
/// leaves nothing owed or parked.
async fn retried_as_unregistered(d: &Deployment, doc: &str, composite: &Composite, reason: &str) {
    for _ in 0..2 {
        let outcome = d.peer.receive(&d.source, doc, composite).await;
        assert_eq!(outcome, MergeOutcome::retryable_skip(reason));
        assert!(!is_parked(&outcome));
        d.peer.redrive().await;
    }
    assert_eq!(d.peer.owed(doc).await, acp::ReplicaOwed::default());
    assert!(!d
        .peer
        .acp()
        .is_doc_registered(POLICY, RESOURCE, doc)
        .await
        .unwrap());
}

async fn replayed_delete(listed: bool, reason: &str) {
    let (d, doc, genesis) = deployment(listed).await;
    assert_eq!(
        d.peer.receive(&d.source, &doc, &genesis).await,
        MergeOutcome::Merged
    );
    let delete = d.source.delete(&d.owner, &doc).await;
    d.chain.archive(&doc);
    retried_as_unregistered(&d, &doc, &delete, reason).await;
}

async fn update_after_the_archive(listed: bool, reason: &str) {
    let (d, doc, genesis) = deployment(listed).await;
    assert_eq!(
        d.peer.receive(&d.source, &doc, &genesis).await,
        MergeOutcome::Merged
    );
    let first = d.source.update(&d.owner, &doc, "Alicia").await;
    assert_eq!(
        d.peer.receive(&d.source, &doc, &first).await,
        MergeOutcome::Merged
    );
    let late = d.source.update(&d.owner, &doc, "Ali").await;
    let delete = d.source.delete(&d.owner, &doc).await;
    d.chain.archive(&doc);
    retried_as_unregistered(&d, &doc, &late, reason).await;
    retried_as_unregistered(&d, &doc, &delete, reason).await;
}

async fn genesis_after_the_archive(listed: bool) {
    let (d, doc, _) = deployment(listed).await;
    let delete = d.source.delete(&d.owner, &doc).await;
    d.chain.archive(&doc);
    retried_as_unregistered(&d, &doc, &delete, UNLISTED).await;
}

#[test]
fn a_listed_peer_retries_a_replayed_delete_as_unregistered_once_the_owner_archived() {
    workers::block_on(replayed_delete(true, LISTED)).unwrap()
}

#[test]
fn an_unlisted_peer_retries_a_replayed_delete_as_unregistered_once_the_owner_archived() {
    workers::block_on(replayed_delete(false, UNLISTED)).unwrap()
}

#[test]
fn a_listed_peer_retries_commits_reaching_it_after_the_archive_as_unregistered() {
    workers::block_on(update_after_the_archive(true, LISTED)).unwrap()
}

#[test]
fn an_unlisted_peer_retries_commits_reaching_it_after_the_archive_as_unregistered() {
    workers::block_on(update_after_the_archive(false, UNLISTED)).unwrap()
}

#[test]
fn a_listed_peer_retries_a_genesis_first_seen_after_the_archive_as_unregistered() {
    workers::block_on(genesis_after_the_archive(true)).unwrap()
}

#[test]
fn an_unlisted_peer_retries_a_genesis_first_seen_after_the_archive_as_unregistered() {
    workers::block_on(genesis_after_the_archive(false)).unwrap()
}
