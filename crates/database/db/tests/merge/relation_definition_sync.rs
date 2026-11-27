//! A collection added after the collection it relates to reaches a peer as
//! the relation it is: its definition block names the target collection, so
//! the peer rebuilds the relation and both name the same collection.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::{database::DB, merge::merge_handler::DbMergeHandler};
use defra_core::block::{Block, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeOutcome};
use defra_core::RequestContext;
use schema::{CollectionVersion, FieldKind};
use storage::RegolithStore;

const AUTHOR: &str = "type Author { name: String }";
const BOOK: &str = "type Book { title: String author: Author }";

struct Peer {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
}

async fn peer_with(calls: &[&str]) -> Peer {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    for sdl in calls {
        db.add_collection(&RequestContext::default(), sdl)
            .await
            .unwrap();
    }
    Peer {
        db,
        blocks: Arc::new(DefraBlockstore::new(store, false)),
    }
}

fn definition(peer: &Peer, name: &str) -> CollectionVersion {
    peer.db
        .get_collection(name)
        .unwrap()
        .unwrap()
        .schema()
        .clone()
}

/// Hand `to` the definition block of `name` and its field blocks, as a sync
/// from `from` delivers them, and merge the definition.
async fn sync_definition(from: &Peer, to: &Peer, name: &str) -> CollectionVersion {
    let version: Cid = definition(from, name).version_id.parse().unwrap();
    let bytes = from.blocks.get(&version).await.unwrap().unwrap();
    let block = Block::from_dag_cbor(&bytes).unwrap();
    for link in block.links.iter().flatten() {
        let field = from.blocks.get(&link.link).await.unwrap().unwrap();
        to.blocks.put(&link.link, &field).await.unwrap();
    }
    to.blocks.put(&version, &bytes).await.unwrap();

    let CrdtDelta::CollectionDefinition(payload) = &block.delta else {
        panic!("{name} is not a collection definition block");
    };
    let outcome = DbMergeHandler::new(to.db.clone(), to.blocks.clone())
        .process_collection_definition_delta(
            &version,
            &block,
            payload,
            &BlockMetadata::normal("", "", "peer", None, false),
        )
        .await
        .unwrap();
    assert!(
        !matches!(outcome, MergeOutcome::Skipped { .. }),
        "the definition of {name} was not merged"
    );
    to.db
        .get_all_collection_versions()
        .await
        .unwrap()
        .into_iter()
        .find(|held| held.version_id == version.to_string())
        .unwrap()
}

#[test]
fn a_collection_added_alone_syncs_as_the_relation_it_is() {
    workers::block_on(async {
        let apart = peer_with(&[AUTHOR, BOOK]).await;
        let together = peer_with(&[&format!("{AUTHOR}\n{BOOK}")]).await;
        assert_eq!(
            definition(&apart, "Book").collection_id,
            definition(&together, "Book").collection_id
        );

        let receiver = peer_with(&[AUTHOR]).await;
        let received = sync_definition(&apart, &receiver, "Book").await;

        assert_eq!(
            received.collection_id,
            definition(&together, "Book").collection_id
        );
        let author = received
            .fields
            .iter()
            .find(|field| field.name == "author")
            .unwrap();
        assert_eq!(
            author.kind,
            FieldKind::relation(definition(&receiver, "Author").collection_id, false)
        );
    })
    .unwrap()
}

#[test]
fn a_bound_governed_collection_added_alone_syncs_with_its_binding() {
    const AUTHOR: &str = r#"type Author @policy(id: "p1", resource: "author") { name: String }"#;
    const BOOK: &str = r#"type Book
        @governed(root: "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
        @policy(id: "p1", resource: "book")
        { title: String author: Author @immutable @policy(relation: "author") }"#;
    workers::block_on(async {
        let apart = peer_with(&[AUTHOR, BOOK]).await;
        let together = peer_with(&[&format!("{AUTHOR}\n{BOOK}")]).await;
        assert_eq!(
            definition(&apart, "Book").collection_id,
            definition(&together, "Book").collection_id
        );
        assert_eq!(
            definition(&apart, "Book").version_id,
            definition(&together, "Book").version_id
        );

        let receiver = peer_with(&[AUTHOR]).await;
        let received = sync_definition(&apart, &receiver, "Book").await;

        assert_eq!(
            received.collection_id,
            definition(&together, "Book").collection_id
        );
        schema::check_synced_policy_bindings(&received).unwrap();
        let bindings: Vec<_> = received.policy_bindings().collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].policy_relation, "author");
        assert_eq!(
            bindings[0].relation_field.unwrap().kind,
            FieldKind::relation(definition(&receiver, "Author").collection_id, false)
        );
    })
    .unwrap()
}
