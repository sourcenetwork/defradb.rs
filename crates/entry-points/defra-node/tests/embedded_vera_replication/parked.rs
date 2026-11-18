//! A change the receiving node's provider refuses waits there, parked by
//! its document, and is applied exactly once when a grant lets its signer
//! write, across a restart of the receiver too.

use std::time::Duration;

use defra_node::DocumentPermission;

use super::actor::Actor;
use super::peer::{Blocks, Peer};
use super::wait::{eventually, throughout};

/// The creator's node and a receiver of another deployment holding the
/// creator's document, and a writer the creator granted on its own
/// deployment only.
struct Parked {
    a: Peer,
    b: Peer,
    creator: Actor,
    writer: Actor,
    doc: String,
}

impl Parked {
    async fn new(root: &std::path::Path, chains: (u64, u64)) -> Self {
        let creator = Actor::new(21);
        let writer = Actor::new(23);
        let a = Peer::start(&root.join("a"), chains.0, &creator, Blocks::Signed).await;
        let b = Peer::start(&root.join("b"), chains.1, &Actor::new(22), Blocks::Signed).await;
        a.replicate_to(&b).await;
        let doc = a.create(&creator, "Alice").await;
        eventually("the creator's document reaches the receiver", || async {
            b.points(&creator.did, &doc).await == Some(0)
        })
        .await;
        a.grant(&doc, "writer", &writer.did).await;
        Self {
            a,
            b,
            creator,
            writer,
            doc,
        }
    }

    /// The writer adds a point on the creator's node; the CID of its
    /// change.
    async fn writer_adds_a_point(&self) -> String {
        let before = self.a.commits(&self.creator.did, &self.doc).await;
        self.a
            .write_while_unavailable(
                self.writer.signed(Peer::add_points(&self.doc, 1)),
                "update_Users",
            )
            .await;
        let mut change = self
            .a
            .commits_since(&self.creator.did, &self.doc, &before)
            .await;
        assert_eq!(change.len(), 1, "one change: {change:?}");
        change.remove(0)
    }

    /// The receiver holds the change `cid` and does not apply it: the
    /// change has arrived, so a grant lets it in from where it waits rather
    /// than on its first delivery.
    async fn refuses(&self, cid: &str, what: &str) {
        eventually("the receiver holds the writer's change", || async {
            self.b
                .holds_unapplied(&self.creator.did, &self.doc, cid)
                .await
        })
        .await;
        throughout(what, Duration::from_secs(5), || async {
            self.b.visible_points(&self.creator.did, &self.doc).await == 0
                && self
                    .b
                    .holds_unapplied(&self.creator.did, &self.doc, cid)
                    .await
        })
        .await;
    }

    /// The receiver applies its changes until it shows `points`, and then
    /// holds them: nothing is applied a second time.
    async fn settles_at(&self, points: i64) {
        eventually("the receiver applies the change", || async {
            self.b.points(&self.creator.did, &self.doc).await == Some(points)
        })
        .await;
        throughout(
            "the receiver applies nothing twice",
            Duration::from_secs(3),
            || async { self.b.visible_points(&self.creator.did, &self.doc).await == points },
        )
        .await;
        assert_eq!(
            self.b.commits(&self.creator.did, &self.doc).await,
            self.a.commits(&self.creator.did, &self.doc).await,
            "the receiver holds each change of the history once"
        );
    }

    async fn stop(self) {
        self.b.stop().await;
        self.a.stop().await;
    }
}

/// A writer's change arrives at the receiver before the receiver's
/// deployment lets it write: it waits unapplied, the receiver's grant lets
/// it in once, and the writer's later change follows.
#[test]
fn a_parked_change_is_applied_once_when_the_receivers_grant_lands() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let parked = Parked::new(directory.path(), (9421, 9422)).await;
        let change = parked.writer_adds_a_point().await;
        assert_eq!(
            parked
                .a
                .visible_points(&parked.creator.did, &parked.doc)
                .await,
            1
        );
        assert!(
            !parked
                .b
                .answer(&parked.writer.did, DocumentPermission::Update, &parked.doc)
                .await
        );
        parked
            .refuses(&change, "a refused change is not applied")
            .await;

        parked
            .b
            .grant(&parked.doc, "writer", &parked.writer.did)
            .await;
        parked.settles_at(1).await;
        assert_eq!(
            parked.b.held_unmerged(&[change]).await,
            0,
            "the applied change is merged"
        );
        parked.writer_adds_a_point().await;
        parked.settles_at(2).await;
        parked.stop().await;
    })
    .unwrap();
}

/// The same with the receiver restarted while the change is parked: the
/// index of parked changes is gone, the change is still neither lost nor
/// let in before the grant, and the grant lets it in once.
#[test]
fn a_change_parked_across_a_restart_of_the_receiver_is_applied_once_when_its_grant_lands() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let mut parked = Parked::new(directory.path(), (9423, 9424)).await;
        let change = parked.writer_adds_a_point().await;
        parked
            .refuses(&change, "a refused change is not applied")
            .await;

        parked.b = parked.b.restart().await;
        parked
            .refuses(&change, "a restart does not let a refused change in")
            .await;

        parked
            .b
            .grant(&parked.doc, "writer", &parked.writer.did)
            .await;
        parked.settles_at(1).await;
        assert_eq!(
            parked.b.held_unmerged(&[change]).await,
            0,
            "the applied change is merged"
        );
        parked.writer_adds_a_point().await;
        parked.settles_at(2).await;
        parked.stop().await;
    })
    .unwrap();
}
