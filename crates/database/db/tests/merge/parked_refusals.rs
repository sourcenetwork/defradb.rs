//! Replicated changes a remote ACP provider refuses are parked by their
//! document, not rejected for good (`proofs/tla/ParkedMerge.tla`): they are
//! tried again when this node changes the document's relationships and on
//! the bounded sweep, a change built on a parked one waits with it, and a
//! change on another branch is not held up.

use std::sync::Arc;

use crate::replica_support::{
    is_parked, key, Chain, Composite, Embedder, Key, Receiver, Source, POLICY, RESOURCE,
};
use defra_core::merge::MergeOutcome;

struct Document {
    chain: Arc<Chain>,
    node: identity::Did,
    receiver: Receiver,
    source: Source,
    creator: Key,
    writer: Key,
    doc: String,
    genesis: Composite,
}

impl Document {
    /// A foreign creator's document the receiver registered, with the
    /// creator's grants landed.
    async fn new() -> Self {
        let chain = Chain::new();
        let node = key().did;
        let embedder = Arc::new(Embedder {
            own: vec![],
            nodes: vec![],
        });
        let receiver = Receiver::open(&chain, &node, Some(embedder)).await;
        let source = Source::open().await;
        let (creator, writer) = (key(), key());
        let (doc, genesis) = source.create(&creator, "Alice").await;
        assert_eq!(
            receiver.receive(&source, &doc, &genesis).await,
            MergeOutcome::Merged
        );
        receiver.settle(&doc).await.unwrap();
        Self {
            chain,
            node,
            receiver,
            source,
            creator,
            writer,
            doc,
            genesis,
        }
    }

    async fn receive(&self, composite: &Composite) -> MergeOutcome {
        self.receiver
            .receive(&self.source, &self.doc, composite)
            .await
    }

    /// Whether `composite` merged on the receiver: delivered again, it is
    /// found applied. A parked one is parked again.
    async fn merged(&self, composite: &Composite) -> bool {
        self.receive(composite).await == MergeOutcome::terminal_skip(MergeOutcome::ALREADY_MERGED)
    }

    /// Whether the re-drive a release started merges `composite`: what is
    /// ready is drained, then delivering it again finds it applied. A
    /// delivery that merges it itself means the re-drive did not.
    async fn redriven(&self, composite: &Composite) -> bool {
        for _ in 0..100 {
            // A release re-drives on a task of its own: let it finish before
            // delivering again, so the delivery does not race it.
            edge::testing::edge()
                .sleep(std::time::Duration::from_millis(20))
                .await;
            self.receiver.redrive().await;
            match self.receive(composite).await {
                outcome if outcome == MergeOutcome::terminal_skip(MergeOutcome::ALREADY_MERGED) => {
                    return true
                }
                MergeOutcome::Merged => return false,
                _ => {}
            }
        }
        false
    }

    /// Whether `composite` is no longer parked: it merged, through the
    /// re-drive or through this delivery.
    async fn released(&self, composite: &Composite) -> bool {
        matches!(
            self.receive(composite).await,
            MergeOutcome::Merged | MergeOutcome::Skipped { terminal: true, .. }
        )
    }

    /// The node itself grants the writer again, through its provider.
    async fn regrant(&self) {
        self.receiver
            .acp()
            .add_actor_relationship(
                &self.node,
                &self.writer.did,
                POLICY,
                RESOURCE,
                &self.doc,
                "writer",
                &[],
            )
            .await
            .unwrap();
    }
}

#[test]
fn a_revoked_writers_change_is_parked_and_merges_once_the_node_grants_it_again() {
    workers::block_on(async {
        let document = Document::new().await;
        let (doc, source) = (&document.doc, &document.source);
        document.chain.grant(doc, "writer", &document.writer.did);
        let first = source
            .composite(&document.writer, vec![document.genesis.0], 2, 1)
            .await;
        assert_eq!(document.receive(&first).await, MergeOutcome::Merged);

        document.chain.revoke(doc, "writer", &document.writer.did);
        let second = source
            .composite(&document.writer, vec![first.0], 3, 1)
            .await;
        let outcome = document.receive(&second).await;
        assert!(is_parked(&outcome), "{outcome:?}");
        assert!(!document.merged(&second).await);

        document.regrant().await;
        assert!(
            document.redriven(&second).await,
            "the grant re-drives the parked change"
        );
    })
    .unwrap()
}

#[test]
fn a_change_built_on_a_parked_one_waits_with_it_and_one_on_another_branch_merges() {
    workers::block_on(async {
        let document = Document::new().await;
        let source = &document.source;
        let parked = source
            .composite(&document.writer, vec![document.genesis.0], 2, 1)
            .await;
        assert!(is_parked(&document.receive(&parked).await));

        // The creator's change on top of the parked one: its history holds it.
        let descendant = source
            .composite(&document.creator, vec![parked.0], 3, 1)
            .await;
        let outcome = document.receive(&descendant).await;
        assert!(is_parked(&outcome), "{outcome:?}");

        // The creator's change on another branch is not held up.
        let other = source
            .composite(&document.creator, vec![document.genesis.0], 2, 1)
            .await;
        assert_eq!(document.receive(&other).await, MergeOutcome::Merged);
        assert!(!document.merged(&parked).await);

        document.regrant().await;
        assert!(document.redriven(&parked).await);
        assert!(document.released(&descendant).await);
    })
    .unwrap()
}

/// A grant made on the provider by someone else reaches this node through
/// the bounded sweep.
#[test]
fn the_sweep_re_drives_a_change_a_grant_made_elsewhere_lets_in() {
    workers::block_on(async {
        let document = Document::new().await;
        let parked = document
            .source
            .composite(&document.writer, vec![document.genesis.0], 2, 1)
            .await;
        assert!(is_parked(&document.receive(&parked).await));

        assert_eq!(
            document.receiver.sweep_parked().await,
            1,
            "still refused, parked again"
        );
        assert!(!document.merged(&parked).await);

        document
            .chain
            .grant(&document.doc, "writer", &document.writer.did);
        assert_eq!(document.receiver.sweep_parked().await, 1);
        assert!(document.redriven(&parked).await);
        assert_eq!(
            document.receiver.sweep_parked().await,
            0,
            "nothing is left parked"
        );
    })
    .unwrap()
}

/// An unsigned change names nobody a grant could cover: its refusal stays
/// final.
#[test]
fn an_unsigned_change_is_still_rejected() {
    workers::block_on(async {
        let document = Document::new().await;
        let unsigned = document.source.unsigned(vec![document.genesis.0], 2).await;
        assert!(document.receive(&unsigned).await.is_rejected());
    })
    .unwrap()
}
