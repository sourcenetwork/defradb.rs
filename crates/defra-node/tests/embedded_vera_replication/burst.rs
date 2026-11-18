//! More refused changes to one document than the receiver indexes for it,
//! arriving over the network at once.

use std::time::Duration;

use super::actor::Actor;
use super::peer::{Blocks, Peer};
use super::wait::{eventually_within, steady};

/// Past the per-document bound of the index of parked changes.
const BURST: i64 = 1_100;

/// A writer the creator granted on its own deployment only makes more
/// changes to one document than the receiver indexes for it. None is applied
/// while the receiver refuses the writer; once the receiver grants it, every
/// one is applied, each exactly once, those indexed and those past the bound
/// alike.
#[test]
fn a_burst_past_the_per_document_bound_is_applied_once_when_its_grant_lands() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let creator = Actor::new(41);
        let writer = Actor::new(43);
        let a = Peer::start(&directory.path().join("a"), 9441, &creator, Blocks::Signed).await;
        let b = Peer::start(
            &directory.path().join("b"),
            9442,
            &Actor::new(42),
            Blocks::Signed,
        )
        .await;
        a.replicate_to(&b).await;
        let doc = a.create(&creator, "Alice").await;
        eventually_within(
            "the creator's document reaches the receiver",
            Duration::from_secs(90),
            || async { b.points(&creator.did, &doc).await == Some(0) },
        )
        .await;
        a.grant(&doc, "writer", &writer.did).await;

        for _ in 0..BURST {
            a.write_while_unavailable(writer.signed(Peer::add_points(&doc, 1)), "update_Users")
                .await;
        }
        assert_eq!(a.points(&creator.did, &doc).await, Some(BURST));
        steady(
            "no refused change is applied",
            Duration::from_secs(5),
            Some(0),
            || b.points(&creator.did, &doc),
        )
        .await;

        b.grant(&doc, "writer", &writer.did).await;
        eventually_within(
            "every change of the burst is applied",
            Duration::from_secs(600),
            || async { b.points(&creator.did, &doc).await == Some(BURST) },
        )
        .await;
        steady(
            "nothing is applied twice",
            Duration::from_secs(5),
            Some(BURST),
            || b.points(&creator.did, &doc),
        )
        .await;
        assert_eq!(
            b.commits(&creator.did, &doc).await,
            a.commits(&creator.did, &doc).await
        );

        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
