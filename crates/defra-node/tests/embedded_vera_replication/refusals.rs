//! Changes the receiver refuses for good, or until a grant that may never
//! come, do not hold up the valid changes that do not build on them.

use std::time::Duration;

use super::actor::Actor;
use super::peer::{Blocks, Peer};
use super::wait::{eventually, throughout};

/// Three deployments. The creator's node replicates to a receiver and to a
/// third node, whose owner then changes both documents there: one without a
/// signature, one signed with a key the receiver's deployment never
/// granted, and pushes them to the receiver. The unsigned change is refused
/// for good: no grant lets it in later. The signed one waits. Neither holds
/// up the creator's later change to the same document, which builds on
/// neither, and the signed one is applied once its signer is granted.
#[test]
fn a_refused_change_does_not_hold_up_a_later_valid_change_to_its_document() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let creator = Actor::new(31);
        let receiver_owner = Actor::new(32);
        let rogue = Actor::new(33);
        let a = Peer::start(&directory.path().join("a"), 9431, &creator, Blocks::Signed).await;
        let b = Peer::start(
            &directory.path().join("b"),
            9432,
            &receiver_owner,
            Blocks::Signed,
        )
        .await;
        let c = Peer::start(&directory.path().join("c"), 9433, &rogue, Blocks::Unsigned).await;
        a.replicate_to(&b).await;
        a.replicate_to(&c).await;
        let unsigned = a.create(&creator, "unsigned").await;
        let signed = a.create(&creator, "signed").await;
        for doc in [&unsigned, &signed] {
            for peer in [&b, &c] {
                eventually("the creator's documents reach both nodes", || async {
                    peer.row(&peer.owner().did, doc).await.is_some()
                })
                .await;
            }
        }

        c.write(
            rogue.unsigned(Peer::add_points(&unsigned, 100)),
            "update_Users",
        )
        .await;
        c.write(rogue.signed(Peer::add_points(&signed, 100)), "update_Users")
            .await;
        for doc in [&unsigned, &signed] {
            assert_eq!(
                c.row(&rogue.did, doc).await.and_then(|row| row.points),
                Some(100),
                "the third node's owner may change what it registered"
            );
        }
        c.replicate_to(&b).await;
        throughout(
            "the receiver applies neither refused change",
            Duration::from_secs(5),
            || async {
                b.points(&receiver_owner.did, &unsigned).await == Some(0)
                    && b.points(&receiver_owner.did, &signed).await == Some(0)
            },
        )
        .await;

        for doc in [&unsigned, &signed] {
            a.write(creator.signed(Peer::add_points(doc, 1)), "update_Users")
                .await;
        }
        for doc in [&unsigned, &signed] {
            eventually(
                "the creator's later change is applied past a refused one",
                || async { b.points(&receiver_owner.did, doc).await == Some(1) },
            )
            .await;
        }

        // A grant changes the answer for the signed change, never for the
        // unsigned one, which names nobody a grant could cover.
        for doc in [&unsigned, &signed] {
            b.grant(doc, "writer", &rogue.did).await;
        }
        eventually("the granted signer's parked change is applied", || async {
            b.points(&receiver_owner.did, &signed).await == Some(101)
        })
        .await;
        throughout(
            "the unsigned change stays refused, and nothing is applied twice",
            Duration::from_secs(5),
            || async {
                b.points(&receiver_owner.did, &unsigned).await == Some(1)
                    && b.points(&receiver_owner.did, &signed).await == Some(101)
            },
        )
        .await;

        c.stop().await;
        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
