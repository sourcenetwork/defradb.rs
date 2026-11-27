//! A merge that builds a parent's missing chunks publishes an event for each one.

use std::sync::Arc;

use defra_core::merge::MergeOutcome;
use document::{Document, NormalValue};
use events::{Bus, ChannelBus, EventName};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;

use super::chunk_forged::{node, node_with_bus};

#[test]
fn a_subscriber_on_the_merging_node_sees_each_rebuilt_chunk() {
    workers::block_on(async {
        let author = node().await;
        let bus: Arc<dyn Bus> = Arc::new(ChannelBus::new());
        let peer = node_with_bus(Some(Arc::clone(&bus))).await;
        let articles = author.db.get_collection("Article").unwrap().unwrap();
        let mut doc = Document::new(articles.schema_arc());
        let body = (0..3)
            .map(|i| format!("Paragraph {i} tells a small story of its own."))
            .collect::<Vec<_>>()
            .join("\n\n");
        doc.set("body", NormalValue::String(body)).unwrap();
        let created = db::AutoCommitMutator::new(Arc::clone(&author.db))
            .create("Article", doc)
            .await
            .unwrap();
        let chunk_ids: RapidHashSet<String> = created
            .derived
            .iter()
            .map(|write| write.doc_id.to_string())
            .collect();
        assert_eq!(chunk_ids.len(), 3);

        let mut updates = bus.subscribe(&[EventName::Update]);
        let outcome = super::chunk_forged::merge(
            &author,
            &peer,
            &articles,
            &created.commit_cid.expect("commit cid"),
            &created.doc_id.to_string(),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, MergeOutcome::Merged), "{outcome:?}");

        let chunks = peer
            .db
            .get_collection("Article__body_chunk")
            .unwrap()
            .unwrap();
        let mut seen = RapidHashSet::default();
        while let Ok(message) = updates.try_recv() {
            let update = message.as_update().expect("an update event");
            if update.observation_collection_id() == chunks.collection_id() {
                assert!(update.is_relay);
                seen.insert(update.doc_id.clone());
            }
        }
        assert_eq!(seen, chunk_ids, "one event for each rebuilt chunk");
    })
    .unwrap()
}
