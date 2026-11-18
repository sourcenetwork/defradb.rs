//! A document created on one deployment's node reaches another
//! deployment's node, which registers it as its own actor and grants the
//! creator what its embedder names.

use defra_node::DocumentPermission;

use super::actor::Actor;
use super::peer::{Blocks, Peer};
use super::wait::eventually;

/// The creator's document and the changes it makes at once, before the
/// receiver has registered it, all arrive over the network; the receiver
/// registers the document as its own actor, grants the creator `writer`, and
/// applies every change exactly once. Its owner and the creator read the row
/// there, a stranger does not, and a merged delete leaves it registered.
#[test]
fn a_foreign_creators_document_is_registered_by_the_receiver_and_every_change_follows_once() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let creator = Actor::new(11);
        let receiver_owner = Actor::new(12);
        let stranger = Actor::new(13);
        let a = Peer::start(&directory.path().join("a"), 9411, &creator, Blocks::Signed).await;
        let b = Peer::start(
            &directory.path().join("b"),
            9412,
            &receiver_owner,
            Blocks::Signed,
        )
        .await;
        assert_eq!(a.policy, b.policy, "one policy ID on both deployments");
        a.replicate_to(&b).await;

        let doc = a.create(&creator, "Alice").await;
        for _ in 0..3 {
            a.write(creator.signed(Peer::add_points(&doc, 1)), "update_Users")
                .await;
        }
        assert_eq!(a.owner_of(&doc).await, Some(creator.did.clone()));

        eventually(
            "the receiver registers the document as its own actor",
            || async { b.owner_of(&doc).await == Some(receiver_owner.did.clone()) },
        )
        .await;
        for permission in [DocumentPermission::Update, DocumentPermission::Delete] {
            eventually("the creator's grant lands on the receiver", || async {
                b.may(&creator.did, permission, &doc).await
            })
            .await;
        }
        eventually("every change reaches the receiver", || async {
            b.row(&creator.did, &doc).await.and_then(|row| row.points) == Some(3)
        })
        .await;
        let row = b.row(&creator.did, &doc).await.expect("the creator reads");
        assert_eq!(row.name.as_deref(), Some("Alice"));
        assert_eq!(
            b.row(&receiver_owner.did, &doc).await,
            Some(row),
            "the receiver's owner reads what the creator reads"
        );
        assert!(
            b.rows(&stranger.did).await.is_empty(),
            "a stranger reads nothing"
        );
        assert!(!b.may(&stranger.did, DocumentPermission::Read, &doc).await);
        assert_eq!(
            b.commits(&creator.did, &doc).await,
            a.commits(&creator.did, &doc).await,
            "the receiver holds the creator's history, each change once"
        );

        a.write(
            creator.signed(format!(
                "mutation {{ delete_Users(docID: \"{doc}\") {{ _docID }} }}"
            )),
            "delete_Users",
        )
        .await;
        eventually("the delete reaches the receiver", || async {
            b.rows(&receiver_owner.did).await.is_empty()
        })
        .await;
        assert_eq!(
            b.owner_of(&doc).await,
            Some(receiver_owner.did.clone()),
            "a merged delete leaves the object registered"
        );

        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
