//! Nodes of one home share one Vera deployment, which the creator's node
//! runs. A row whose genesis names a node as its reader is granted to that
//! node from the create's own transaction, so the named node receives and
//! reads it; a node the row does not name never merges it, and a revoked
//! node merges nothing more of it.

use std::time::Duration;

use defra_node::{DocumentPermission, QueryRequest};
use identity::Did;

use super::actor::Actor;
use super::peer::Peer;
use super::wait::{eventually, throughout};

impl Peer {
    /// Add the `Notes` collection, which names its reader in `addressee`.
    async fn with_notes(self) -> Self {
        if self
            .node
            .get_collection("Notes")
            .expect("collections")
            .is_none()
        {
            self.node
                .add_collection(
                    &defra_core::RequestContext::ambient(),
                    &format!(
                        "type Notes @policy(id: \"{}\", resource: \"users\") {{ \
                     text: String addressee: String @immutable }}",
                        self.policy
                    ),
                )
                .await
                .expect("collection");
        }
        self
    }

    /// The text of the `Notes` row `doc` as `reader` sees it here.
    async fn note(&self, reader: &Did, doc: &str) -> Option<String> {
        let request = QueryRequest::new(format!("{{ Notes(docID: \"{doc}\") {{ _docID text }} }}"))
            .with_identity(Some(reader.clone()));
        let response = self.node.execute_request(request).await;
        if response.has_errors() {
            return None;
        }
        response.data.expect("data")["Notes"]
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|row| row["text"].as_str().map(str::to_owned))
    }
}

/// The creator's node writes a note addressed to the receiver's node; the
/// grant is staged in the create's transaction and settles behind the
/// note's registration. The receiver merges and reads it, a bystander on
/// the same deployment never merges it, and once the receiver's light
/// client sees the creator revoke it, a later change does not reach it.
#[test]
fn a_note_addressed_to_a_node_of_the_home_reaches_that_node_only() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let creator = Actor::new(61);
        let receiver = Actor::new(62);
        let bystander = Actor::new(63);
        let a = Peer::start_serving(&directory.path().join("a"), 9461, &creator)
            .await
            .with_notes()
            .await;
        let b = Peer::start_sharing(&directory.path().join("b"), &a, &receiver)
            .await
            .with_notes()
            .await;
        let c = Peer::start_sharing(&directory.path().join("c"), &a, &bystander)
            .await
            .with_notes()
            .await;
        a.replicate_collection_to(&b, "Notes").await;
        a.replicate_collection_to(&c, "Notes").await;

        let doc = a
            .write_while_unavailable(
                creator.signed(format!(
                    "mutation {{ add_Notes(input: {{text: \"hello\", addressee: \"{}\"}}) \
                     {{ _docID }} }}",
                    receiver.did
                )),
                "add_Notes",
            )
            .await;

        eventually("the addressed node receives and reads the note", || async {
            b.note(&receiver.did, &doc).await.as_deref() == Some("hello")
        })
        .await;
        assert!(
            b.answer(&receiver.did, DocumentPermission::Read, &doc)
                .await,
            "the provider grants the addressed node"
        );
        assert!(
            !c.answer(&bystander.did, DocumentPermission::Read, &doc)
                .await,
            "the provider grants nobody else"
        );
        throughout(
            "the bystander's node never merges the note",
            Duration::from_secs(10),
            || async { c.note(&creator.did, &doc).await.is_none() },
        )
        .await;

        a.node
            .delete_dac_actor_relationship(
                creator.did.as_str(),
                "Notes",
                &doc,
                "reader",
                receiver.did.as_str(),
            )
            .await
            .expect("revoke");
        eventually(
            "the revocation reaches the receiver's light client",
            || async {
                matches!(
                    b.may(&receiver.did, DocumentPermission::Read, &doc).await,
                    Ok(false)
                )
            },
        )
        .await;
        a.write_while_unavailable(
            creator.signed(format!(
                "mutation {{ update_Notes(docID: \"{doc}\", input: {{text: \"again\"}}) \
                 {{ _docID }} }}"
            )),
            "update_Notes",
        )
        .await;
        throughout(
            "the revoked node merges nothing more of the note",
            Duration::from_secs(10),
            || async { b.note(&creator.did, &doc).await.as_deref() == Some("hello") },
        )
        .await;

        c.stop().await;
        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
