//! A revocation made on one node of a deployment stops the revoked actor's
//! changes on another node of it once that node's light client has seen it.
//!
//! Nodes of different deployments do not learn of each other's revocations:
//! the receiver's own deployment decides there, through grants its owner
//! makes. So the nodes here share one deployment, which the creator's node
//! runs.

use std::time::Duration;

use defra_node::DocumentPermission;

use super::actor::Actor;
use super::peer::Peer;
use super::wait::{eventually, throughout};

/// The creator's node runs the deployment; a receiver and the writer's own
/// node verify against it. The writer's change made through the creator's
/// node while it is granted reaches the receiver and is applied. The writer
/// then changes the document again on its own node, now cut off from the
/// others, and the creator revokes the writer. Once the
/// receiver sees the revocation, the writer's node connects and pushes the
/// change: the receiver does not apply it. The writer cannot write anywhere
/// any more, and a new grant lets the held-back change in, once.
#[test]
fn a_revocation_on_the_creators_node_stops_the_revoked_writer_on_the_receiver() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let creator = Actor::new(51);
        let receiver_owner = Actor::new(52);
        let writer = Actor::new(53);
        let a = Peer::start_serving(&directory.path().join("a"), 9451, &creator).await;
        let b = Peer::start_sharing(&directory.path().join("b"), &a, &receiver_owner).await;
        let c = Peer::start_sharing(&directory.path().join("c"), &a, &writer).await;
        assert_eq!(a.policy, b.policy);
        assert_eq!(a.policy, c.policy);
        a.replicate_to(&b).await;
        a.replicate_to(&c).await;

        let doc = a.create(&creator, "Alice").await;
        a.grant(&doc, "reader", &receiver_owner.did).await;
        a.grant(&doc, "writer", &writer.did).await;
        eventually("both nodes merge the creator's document", || async {
            b.points(&creator.did, &doc).await == Some(0)
                && c.points(&creator.did, &doc).await == Some(0)
        })
        .await;

        a.write_while_unavailable(writer.signed(Peer::add_points(&doc, 1)), "update_Users")
            .await;
        eventually("the granted writer's change reaches both nodes", || async {
            b.points(&creator.did, &doc).await == Some(1)
                && c.points(&creator.did, &doc).await == Some(1)
        })
        .await;

        a.part_from(&c).await;
        let before = c.commits(&creator.did, &doc).await;
        c.write_while_unavailable(writer.signed(Peer::add_points(&doc, 1)), "update_Users")
            .await;
        assert_eq!(c.visible_points(&creator.did, &doc).await, 2);
        let mut change = c.commits_since(&creator.did, &doc, &before).await;
        assert_eq!(change.len(), 1, "one change: {change:?}");
        let change = change.remove(0);
        throughout(
            "the writer's node has not reached the receiver yet",
            Duration::from_secs(3),
            || async { b.visible_points(&creator.did, &doc).await == 1 },
        )
        .await;
        a.revoke(&doc, "writer", &writer.did).await;
        for peer in [&b, &c] {
            eventually("the revocation reaches the node's light client", || async {
                matches!(
                    peer.may(&writer.did, DocumentPermission::Update, &doc)
                        .await,
                    Ok(false)
                )
            })
            .await;
        }

        c.replicate_to(&b).await;
        eventually("the receiver holds the revoked writer's change", || async {
            b.holds_unapplied(&creator.did, &doc, &change).await
        })
        .await;
        throughout(
            "the revoked writer's change is not applied",
            Duration::from_secs(10),
            || async {
                b.visible_points(&creator.did, &doc).await == 1
                    && b.holds_unapplied(&creator.did, &doc, &change).await
            },
        )
        .await;
        for (peer, points) in [(&a, 1), (&c, 2)] {
            let outcome = peer
                .attempt(writer.signed(Peer::add_points(&doc, 1)), "update_Users")
                .await;
            assert_eq!(
                outcome,
                Ok(Vec::new()),
                "a node writes for the revoked writer"
            );
            assert_eq!(
                peer.visible_points(&creator.did, &doc).await,
                points,
                "a refused write changes nothing"
            );
        }

        a.grant(&doc, "writer", &writer.did).await;
        eventually("a new grant lets the held-back change in", || async {
            b.points(&creator.did, &doc).await == Some(2)
        })
        .await;
        throughout(
            "the held-back change is applied once",
            Duration::from_secs(3),
            || async { b.visible_points(&creator.did, &doc).await == 2 },
        )
        .await;

        c.stop().await;
        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
