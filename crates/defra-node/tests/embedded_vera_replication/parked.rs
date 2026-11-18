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
            b.row(&creator.did, &doc).await.is_some()
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

    /// The writer adds a point on the creator's node.
    async fn writer_adds_a_point(&self) {
        self.a
            .write(
                self.writer.signed(Peer::add_points(&self.doc, 1)),
                "update_Users",
            )
            .await;
    }

    async fn points_on(&self, peer: &Peer) -> Option<i64> {
        peer.row(&self.creator.did, &self.doc)
            .await
            .and_then(|row| row.points)
    }

    /// The receiver applies its changes until it shows `points`, and then
    /// holds them: nothing is applied a second time.
    async fn settles_at(&self, points: i64) {
        eventually("the receiver applies the change", || async {
            self.points_on(&self.b).await == Some(points)
        })
        .await;
        throughout(
            "the receiver applies nothing twice",
            Duration::from_secs(3),
            || async { self.points_on(&self.b).await == Some(points) },
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
        parked.writer_adds_a_point().await;
        assert_eq!(parked.points_on(&parked.a).await, Some(1));
        assert!(
            !parked
                .b
                .may(&parked.writer.did, DocumentPermission::Update, &parked.doc)
                .await
        );
        throughout(
            "a refused change is not applied",
            Duration::from_secs(5),
            || async { parked.points_on(&parked.b).await == Some(0) },
        )
        .await;

        parked
            .b
            .grant(&parked.doc, "writer", &parked.writer.did)
            .await;
        parked.settles_at(1).await;
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
        parked.writer_adds_a_point().await;
        throughout(
            "a refused change is not applied",
            Duration::from_secs(5),
            || async { parked.points_on(&parked.b).await == Some(0) },
        )
        .await;

        parked.b = parked.b.restart().await;
        throughout(
            "a restart does not let a refused change in",
            Duration::from_secs(5),
            || async { parked.points_on(&parked.b).await == Some(0) },
        )
        .await;

        parked
            .b
            .grant(&parked.doc, "writer", &parked.writer.did)
            .await;
        parked.settles_at(1).await;
        parked.writer_adds_a_point().await;
        parked.settles_at(2).await;
        parked.stop().await;
    })
    .unwrap();
}
